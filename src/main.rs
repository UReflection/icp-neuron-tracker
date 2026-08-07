use clap::{Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use std::path::PathBuf;
use directories::ProjectDirs;

mod domain;
mod infrastructure;
mod application;

use application::{IdentityService, PortfolioService, TrackingService, RetirementService, ImportService, ImportOptions, ExportOptions, ReportService, SetupService};
use infrastructure::{Config, IcClient, SqliteRepository, TerminalReportFormatter};
use domain::{NeuronId, Portfolio};
use domain::retirement::TargetIncome;

/// Get the config.toml path, checking config directory first, then current directory
fn get_config_path() -> PathBuf {
    // First, try config directory (where setup wizard puts it)
    if let Some(proj_dirs) = ProjectDirs::from("com", "u-reflection", "icp-neuron-tracker") {
        let config_path = proj_dirs.config_dir().join("config.toml");
        if config_path.exists() {
            return config_path;
        }
    }

    // Fallback to current directory (for backwards compatibility)
    PathBuf::from("config.toml")
}

/// Output format for report data
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// Human-readable terminal output (default)
    Terminal,
    /// JSON format for programmatic use
    Json,
    /// CSV format for spreadsheet analysis
    Csv,
}

#[derive(Parser)]
#[command(name = "icp-neuron-tracker")]
// Reads CARGO_PKG_VERSION, so `--version` cannot drift from Cargo.toml.
#[command(version)]
#[command(about = "ICP Neuron Portfolio Tracker - U Reflection Design & Build Inc.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard for first-time configuration
    Init,

    /// Track neurons (default if no command specified)
    Track,

    /// Identity management commands
    Identity {
        #[command(subcommand)]
        action: IdentityCommands,
    },

    /// Calculate retirement projection
    Project {
        /// Target daily income in ICP (e.g., 2.5)
        #[arg(short, long)]
        target: f64,

        /// Alternative targets to compare (comma-separated, e.g., "1.5,2.0,3.0")
        #[arg(short, long, value_delimiter = ',')]
        compare: Option<Vec<f64>>,

        /// Skip the network entirely and project from the newest stored snapshot.
        /// Without this flag a live query is attempted first, falling back to stored
        /// data if it fails. Either way the staleness of the data used is reported.
        #[arg(long)]
        offline: bool,
    },

    /// Import historical neuron data from CSV
    Import {
        /// Path to CSV file
        #[arg(short, long)]
        file: PathBuf,

        /// Perform dry run without importing (preview only)
        #[arg(long)]
        dry_run: bool,
    },

    /// Export neuron snapshots to CSV
    Export {
        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Filter by specific neuron IDs (comma-separated)
        #[arg(short, long, value_delimiter = ',')]
        neurons: Option<Vec<u64>>,

        /// Start date filter (YYYY-MM-DD)
        #[arg(long)]
        start_date: Option<String>,

        /// End date filter (YYYY-MM-DD)
        #[arg(long)]
        end_date: Option<String>,
    },

    /// Generate portfolio reports and analytics
    Report {
        #[command(subcommand)]
        report_type: ReportCommands,
    },
}

#[derive(Subcommand)]
enum ReportCommands {
    /// Portfolio summary report with current state
    Summary {
        /// Output format (terminal, json, csv)
        #[arg(short, long, value_enum, default_value = "terminal")]
        format: OutputFormat,
    },

    /// Historical maturity growth trend analysis
    History {
        /// Number of days to analyze (e.g., 7, 30, 90)
        #[arg(short, long, default_value = "30")]
        days: u32,

        /// Output format (terminal, json, csv)
        #[arg(short, long, value_enum, default_value = "terminal")]
        format: OutputFormat,
    },

    /// Reward analysis with neuron performance rankings
    Rewards {
        /// Number of days to analyze (e.g., 7, 30, 90)
        #[arg(short, long, default_value = "30")]
        days: u32,

        /// Output format (terminal, json, csv)
        #[arg(short, long, value_enum, default_value = "terminal")]
        format: OutputFormat,
    },

    /// Detailed stats for a specific neuron
    Neuron {
        /// Neuron ID to analyze
        #[arg(short, long)]
        neuron_id: u64,

        /// Output format (terminal, json, csv)
        #[arg(short, long, value_enum, default_value = "terminal")]
        format: OutputFormat,
    },
}

#[derive(Subcommand)]
enum IdentityCommands {
    /// Generate a new Secp256k1 identity
    Generate {
        /// Name for the identity file
        #[arg(short, long, default_value = "tracker-hotkey")]
        name: String,
    },

    /// Verify identity and check neuron authorization
    Verify,

    /// Display current identity information
    Info,
}

/// Render errors with Display and exit non-zero.
///
/// `main` used to return `Result<_, Box<dyn Error>>`, which makes Rust print the error with
/// **Debug**. For the ~19 `Err("...".into())` sites in this crate that meant the message
/// arrived wrapped in quotes — `Error: "No tracked neurons found."` — reading like a leaked
/// internal string rather than something addressed to the user. It was inconsistent, too:
/// paths that print their own `eprintln!` rendered the same class of message unquoted, so
/// two adjacent commands disagreed about how an error looks.
///
/// Fixing it here rather than at each `return Err` site means the convention holds for
/// anything added later, and no propagation site had to change.
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            handle_init_command().await?;
        }
        Some(Commands::Track) | None => {
            run_tracking().await?;
        }
        Some(Commands::Identity { action }) => {
            handle_identity_command(action).await?;
        }
        Some(Commands::Project { target, compare, offline }) => {
            handle_project_command(target, compare, offline).await?;
        }
        Some(Commands::Import { file, dry_run }) => {
            handle_import_command(file, dry_run)?;
        }
        Some(Commands::Export { output, neurons, start_date, end_date }) => {
            handle_export_command(output, neurons, start_date, end_date)?;
        }
        Some(Commands::Report { report_type }) => {
            handle_report_command(report_type)?;
        }
    }

    Ok(())
}

/// Load the configuration, or explain what is wrong and exit.
///
/// `Config::load` returns `std::io::Error` on a missing file. Propagating that with `?`
/// reaches `main`, which prints it with Debug — the user got
/// `Error: Os { code: 2, kind: NotFound, message: "No such file or directory" }`,
/// which names neither the file nor the remedy. Every command that reads config went
/// through that path.
fn load_config_or_exit() -> Config {
    let path = get_config_path();
    match Config::load(&path.to_string_lossy()) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error: could not read configuration.");
            eprintln!();
            eprintln!("  Path:  {}", path.display());
            eprintln!("  Cause: {}", e);
            eprintln!();
            if path.exists() {
                eprintln!("The file exists but could not be read or parsed. Check that it is valid");
                eprintln!("TOML and that you have permission to read it. `icp-neuron-tracker init`");
                eprintln!("will write a fresh one if you would rather start over.");
            } else {
                eprintln!("No configuration file exists at that path.");
                eprintln!();
                eprintln!("Run `icp-neuron-tracker init` to create one. It generates an identity,");
                eprintln!("collects your neuron IDs and writes the config for you.");
            }
            std::process::exit(1);
        }
    }
}

/// Open the snapshot database, or explain what is wrong and exit.
///
/// Same Debug-rendering problem as `load_config_or_exit`. Note that a missing file is not
/// normally an error here — SQLite creates it — so a failure usually means the containing
/// directory is absent, the file is not writable, or it is locked or not a database.
fn open_database_or_exit(db_path: &str) -> SqliteRepository {
    match SqliteRepository::new(db_path) {
        Ok(db) => db,
        Err(e) => {
            let path = std::path::Path::new(db_path);
            eprintln!("Error: could not open the snapshot database.");
            eprintln!();
            eprintln!("  Path:  {}", db_path);
            eprintln!("  Cause: {}", e);
            eprintln!();

            let parent_missing = path
                .parent()
                .map(|p| !p.as_os_str().is_empty() && !p.exists())
                .unwrap_or(false);

            if parent_missing {
                eprintln!("The containing directory does not exist:");
                eprintln!("  {}", path.parent().unwrap().display());
                eprintln!();
                eprintln!("Create it, or point `tracking.history_file` at a path that exists.");
            } else if path.exists() {
                eprintln!("The file exists but could not be opened or migrated. It may be locked by");
                eprintln!("another process — close any other tracker instance or SQLite browser —");
                eprintln!("or it may not be readable, or not a database file.");
            } else {
                eprintln!("The database would normally be created on first use, so this usually means");
                eprintln!("the location is not writable. Check permissions on the directory, then run");
                eprintln!("`icp-neuron-tracker track` to create the database and collect a snapshot.");
            }

            eprintln!();
            eprintln!("This path comes from `tracking.history_file` in your configuration.");
            std::process::exit(1);
        }
    }
}

async fn handle_init_command() -> Result<(), Box<dyn std::error::Error>> {
    // The wizard is built on dialoguer prompts, which require a terminal. Without one they
    // fail deep inside step 2 with `IO(Custom { kind: NotConnected, error: "not a terminal" })`
    // — Debug-rendered, after an identity has already been written, and identically on every
    // retry. Refuse up front instead, and name the manual route out.
    if !std::io::stdin().is_terminal() {
        let config_path = SetupService::default_config_path();
        eprintln!("Error: `init` needs an interactive terminal.");
        eprintln!();
        eprintln!("It is a wizard — it asks for your neuron IDs and waits while you add a hot");
        eprintln!("key in the NNS dapp — so it cannot run from a script, a cron job, a Docker");
        eprintln!("build, or `ssh host 'icp-neuron-tracker init'` without a TTY.");
        eprintln!();
        eprintln!("To configure without a terminal, write the config file by hand:");
        eprintln!();
        eprintln!("  1. Copy the template from the repository:");
        eprintln!("       cp config.toml.example \\");
        eprintln!("          {}", config_path.display());
        eprintln!("  2. Edit it: set `identity.pem_file` and list your neuron IDs under");
        eprintln!("     `neurons.ids`. Every field is documented in the template.");
        eprintln!("  3. Generate an identity if you do not have one:");
        eprintln!("       icp-neuron-tracker identity generate --name tracker-hotkey");
        eprintln!("  4. Add the printed principal as a hot key on each neuron in the NNS dapp,");
        eprintln!("     then run `icp-neuron-tracker track`.");
        eprintln!();
        eprintln!("If you do have a terminal, run `icp-neuron-tracker init` directly rather");
        eprintln!("than through a pipe or redirect.");
        std::process::exit(1);
    }

    let setup_service = SetupService::new();
    if let Err(e) = setup_service.run_interactive_setup().await {
        // Display, not Debug: this is the last thing a first-run user sees.
        eprintln!();
        eprintln!("Error: setup could not be completed.");
        eprintln!();
        eprintln!("  Cause: {}", e);
        eprintln!();
        eprintln!("Any configuration already written has been kept. Re-run");
        eprintln!("`icp-neuron-tracker init` to continue, or edit the config file directly.");
        std::process::exit(1);
    }
    Ok(())
}

async fn handle_identity_command(
    action: IdentityCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let identity_service = IdentityService::new();

    match action {
        IdentityCommands::Generate { name } => {
            println!("U Reflection Design & Build Inc. - ICP Neuron Tracker");
            println!("Identity Generator\n");

            let pem_path = PathBuf::from(format!("{}.pem", name));

            // Check if file already exists
            if pem_path.exists() {
                eprintln!("Error: File '{}' already exists", pem_path.display());
                eprintln!("\nOptions:");
                eprintln!("  1. Use different name: --name tracker-hotkey-2");
                eprintln!("  2. Remove existing file: rm {}", pem_path.display());
                eprintln!("     (Backup first if important!)");
                std::process::exit(1);
            }

            println!("Generating new Secp256k1 identity...");
            let info = identity_service.generate_identity(&pem_path)?;

            println!("\n{}", "=".repeat(60));
            println!("Identity Generated Successfully");
            println!("{}", "=".repeat(60));
            println!("\nKey Type: Secp256k1 (EC curve)");
            println!("\nPrincipal:");
            println!("  {}", info.principal);
            println!("\nPEM File:");
            println!("  {}", info.pem_path);
            println!("  Permissions: rw------- (owner only)");
            println!("\n{}", "=".repeat(60));
            println!("Next Steps");
            println!("{}", "=".repeat(60));
            println!("\n1. Add this principal as hot key to your neurons:");
            println!("   - Go to https://nns.ic0.app/neurons/");
            println!("   - Login with your controller identity");
            println!("   - Select each neuron you want to track");
            println!("   - Click 'Add Hotkey'");
            println!("   - Paste principal: {}", info.principal);
            println!("\n2. Update config.toml:");
            println!("   [identity]");
            println!("   pem_file = \"{}\"", info.pem_path);
            println!("\n3. Verify setup:");
            println!("   icp-neuron-tracker identity verify");
            println!("\n4. Start tracking:");
            println!("   icp-neuron-tracker track");
            println!("\n{}", "=".repeat(60));
            println!("Security Reminders");
            println!("{}", "=".repeat(60));
            println!("\n- Keep PEM file secure (never commit to git)");
            println!("- Backup: cp {} {}.backup", info.pem_path, info.pem_path);
            println!("- This is a hot key (read-only, cannot transfer stake)");
            println!("{}\n", "=".repeat(60));
        }

        IdentityCommands::Verify => {
            println!("U Reflection Design & Build Inc. - ICP Neuron Tracker");
            println!("Identity Verification\n");

            // Load config
            let config = load_config_or_exit();
            let pem_path = PathBuf::from(&config.identity.pem_file);

            // Check PEM file exists
            if !pem_path.exists() {
                eprintln!("Error: PEM file not found");
                eprintln!("  Path: {}", pem_path.display());
                eprintln!("\n{}", "=".repeat(60));
                eprintln!("Solutions:");
                eprintln!("{}", "=".repeat(60));
                eprintln!("\n1. Generate new identity:");
                eprintln!("   icp-neuron-tracker identity generate --name tracker-hotkey");
                eprintln!("\n2. Use existing dfx identity:");
                eprintln!("   dfx identity export <name> > tracker-identity.pem");
                eprintln!("\n3. Check config.toml path:");
                eprintln!("   [identity]");
                eprintln!("   pem_file = \"./correct-path.pem\"");
                eprintln!();
                std::process::exit(1);
            }

            println!("PEM File: {} ✓", pem_path.display());

            // Get principal
            let info = match identity_service.get_identity_info(&pem_path) {
                Ok(info) => info,
                Err(e) => {
                    eprintln!("\nError loading identity: {}", e);
                    std::process::exit(1);
                }
            };

            println!("Principal: {} ✓\n", info.principal);

            println!("Checking neuron authorization...");

            // Convert neuron IDs from u64 to String
            let neuron_id_strings: Vec<String> = config.neurons.ids
                .iter()
                .map(|id| id.to_string())
                .collect();

            let auth_results = identity_service
                .verify_neuron_access(
                    &pem_path,
                    &neuron_id_strings,
                    &config.ic.ic_url,
                    &config.ic.governance_canister,
                )
                .await?;

            let mut all_authorized = true;
            let mut unauthorized_neurons = Vec::new();

            for result in &auth_results {
                let status = if result.is_authorized { "✓" } else { "✗" };
                let message = if result.is_authorized {
                    "Authorized".to_string()
                } else {
                    result
                        .error
                        .clone()
                        .unwrap_or_else(|| "Unknown error".to_string())
                };

                println!("  Neuron {}... {} {}", result.neuron_id, status, message);

                if !result.is_authorized {
                    all_authorized = false;
                    unauthorized_neurons.push(result.neuron_id.clone());
                }
            }

            println!();

            if all_authorized {
                println!("{}", "=".repeat(60));
                println!("All Neurons Authorized! ✓");
                println!("{}", "=".repeat(60));
                println!("\nReady to track:");
                println!("  icp-neuron-tracker track");
                println!();
            } else {
                println!("{}", "=".repeat(60));
                println!("Action Required");
                println!("{}", "=".repeat(60));
                println!("\nAdd principal as hot key to these neurons:");
                for neuron_id in &unauthorized_neurons {
                    println!("  - https://nns.ic0.app/neuron/{}", neuron_id);
                }
                println!("\nPrincipal to add:");
                println!("  {}", info.principal);
                println!("\nSteps:");
                println!("  1. Login to NNS dapp with controller identity");
                println!("  2. Select neuron");
                println!("  3. Click 'Add Hotkey'");
                println!("  4. Paste principal above");
                println!("\nAfter adding, verify again:");
                println!("  icp-neuron-tracker identity verify");
                println!();
            }
        }

        IdentityCommands::Info => {
            println!("U Reflection Design & Build Inc. - ICP Neuron Tracker");
            println!("Identity Information\n");

            let config = load_config_or_exit();
            let pem_path = PathBuf::from(&config.identity.pem_file);

            if !pem_path.exists() {
                eprintln!("Error: PEM file not found: {}", pem_path.display());
                std::process::exit(1);
            }

            let info = identity_service.get_identity_info(&pem_path)?;

            println!("{}", "=".repeat(60));
            println!("Current Identity");
            println!("{}", "=".repeat(60));
            println!("\nKey Type: Secp256k1 (EC curve)");
            println!("\nPrincipal:");
            println!("  {}", info.principal);
            println!("\nPEM File:");
            println!("  {}", info.pem_path);
            println!("\nConfigured Neurons:");
            for neuron_id in &config.neurons.ids {
                println!("  - {}", neuron_id);
            }
            println!("\n{}", "=".repeat(60));
            println!("Commands");
            println!("{}", "=".repeat(60));
            println!("\nVerify authorization:");
            println!("  icp-neuron-tracker identity verify");
            println!("\nStart tracking:");
            println!("  icp-neuron-tracker track");
            println!();
        }
    }

    Ok(())
}

/// Where the current portfolio value used by a projection came from.
///
/// Only `portfolio.total_value()` is taken from the portfolio; every other input to the
/// projection (reward percentiles, 30-day average, data-day count) is read from the database
/// regardless. So the source affects exactly one number — and the banner says so.
enum PortfolioSource {
    /// Fetched live from the governance canister.
    Live,
    /// `--offline` was passed; the network was never contacted.
    OfflineRequested,
    /// A live fetch was attempted and failed.
    Fallback(String),
}

/// Build the staleness banner. Returns `None` only for a live fetch — every fallback and
/// every `--offline` run produces a banner. Never suppressed, never reduced to a footnote:
/// a retirement date computed from months-old data must not look like one computed today.
fn format_staleness_banner(
    source: &PortfolioSource,
    stored: &application::StoredPortfolio,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    let bar = "=".repeat(60);
    let mut out = String::new();

    match source {
        PortfolioSource::Live => return None,
        PortfolioSource::OfflineRequested => {
            out.push_str(&format!("{}\n", bar));
            out.push_str("OFFLINE PROJECTION - using stored data\n");
            out.push_str(&format!("{}\n", bar));
        }
        PortfolioSource::Fallback(reason) => {
            out.push_str(&format!("{}\n", bar));
            out.push_str("NETWORK UNAVAILABLE - fell back to stored data\n");
            out.push_str(&format!("{}\n", bar));
            out.push_str(&format!("\nReason: {}\n", reason));
        }
    }

    // 1. Which portfolio value was used.
    out.push_str("\nPortfolio value source:  newest stored snapshot (not a live query)\n");
    out.push_str(&format!(
        "Current portfolio value: {:.4} ICP\n",
        stored.portfolio.total_value().to_icp()
    ));

    // 2. When it was observed, and how old that makes it.
    if let Some(retrieved) = stored.retrieved_at {
        out.push_str(&format!(
            "Data retrieved:          {} UTC\n",
            retrieved.format("%Y-%m-%d %H:%M:%S")
        ));
        out.push_str(&format!(
            "Age of data:             {} days old\n",
            stored.age_days(now).unwrap_or(0)
        ));
    }
    if let Some(date) = stored.snapshot_date {
        out.push_str(&format!("Newest snapshot date:    {}\n", date));
    }

    // 3. The direction of the error, stated plainly rather than as a footnote.
    out.push_str("\nWHAT THIS MEANS FOR THE NUMBER BELOW\n");
    out.push_str(&format!("{}\n", "-".repeat(60)));
    out.push_str("Stake and maturity have almost certainly grown since that snapshot,\n");
    out.push_str("so this portfolio value UNDERSTATES what you actually hold today.\n");
    out.push_str("An understated portfolio makes the gap to your target look larger,\n");
    out.push_str("so the projected retirement date below is LATER than reality - not\n");
    out.push_str("earlier. Treat it as a conservative bound, not a precise date.\n");
    out.push_str("\nRun without --offline, with the network available, for a live figure.\n");
    out.push_str(&format!("{}\n", bar));

    Some(out)
}

/// Print the staleness banner, if the source warrants one.
fn print_staleness_banner(source: &PortfolioSource, stored: &application::StoredPortfolio) {
    if let Some(banner) = format_staleness_banner(source, stored, chrono::Utc::now()) {
        println!("{}", banner);
    }
}

/// Attempt a live portfolio query. Returns `Err(reason)` rather than propagating, so the
/// caller can fall back to stored data instead of aborting.
async fn try_fetch_live_portfolio(config: &Config) -> Result<Portfolio, String> {
    println!("🔑 Initializing IC client...");
    let ic_client = IcClient::new(
        &config.identity.pem_file,
        &config.ic.ic_url,
        &config.ic.governance_canister,
    )
    .map_err(|e| format!("could not initialise IC client: {}", e))?;

    let portfolio_service = PortfolioService::new(ic_client);

    let neuron_ids: Vec<NeuronId> = config.neurons.ids.iter().map(|&id| NeuronId::new(id)).collect();

    println!("🔍 Querying {} neurons...\n", neuron_ids.len());

    let fetch_result = portfolio_service
        .fetch_portfolio(&neuron_ids)
        .await
        .map_err(|e| format!("query failed: {}", e))?;

    if !fetch_result.errors.is_empty() {
        eprintln!("⚠️  Warning: Some neurons failed to fetch:");
        for (neuron_id, error) in &fetch_result.errors {
            eprintln!("  - Neuron {}: {}", neuron_id, error);
        }
        eprintln!();
    }

    if fetch_result.portfolio.neuron_count() == 0 {
        return Err("no neurons could be fetched".to_string());
    }

    Ok(fetch_result.portfolio)
}

async fn handle_project_command(target: f64, compare: Option<Vec<f64>>, offline: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("U Reflection Design & Build Inc. - ICP Neuron Tracker");
    println!("Retirement Income Projection\n");

    // Validate and create target income
    let target_income = match TargetIncome::new(target) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("\nTarget income must be:");
            eprintln!("  - Positive (> 0 ICP/day)");
            eprintln!("  - Reasonable (< 10,000 ICP/day)");
            eprintln!("\nExample usage:");
            eprintln!("  icp-neuron-tracker project --target 2.5");
            eprintln!("  icp-neuron-tracker project --target 2.5 --compare 1.5,2.0,3.0");
            std::process::exit(1);
        }
    };

    println!("Loading configuration...");
    let config = load_config_or_exit();

    println!("🗄️  Initializing database...");
    let db = open_database_or_exit(&config.tracking.history_file);

    // Resolve the current portfolio value: live if possible, stored otherwise. Everything
    // else the projection needs already comes from `db`.
    let (portfolio, source) = if offline {
        let stored = application::load_stored_portfolio(&db)?;
        print_staleness_banner(&PortfolioSource::OfflineRequested, &stored);
        (stored.portfolio, PortfolioSource::OfflineRequested)
    } else {
        match try_fetch_live_portfolio(&config).await {
            Ok(p) => {
                println!("📊 Calculating retirement projection...\n");
                (p, PortfolioSource::Live)
            }
            Err(reason) => {
                let stored = application::load_stored_portfolio(&db).map_err(|e| {
                    format!(
                        "Live query failed ({}) and no stored snapshots are available either: {}",
                        reason, e
                    )
                })?;
                let source = PortfolioSource::Fallback(reason);
                print_staleness_banner(&source, &stored);
                (stored.portfolio, source)
            }
        }
    };

    // `source` has already been reported via the banner; the projection itself is identical
    // regardless of where the portfolio came from.
    let _ = source;

    // Create retirement service and calculate projection
    let retirement_service = RetirementService::new(&db);

    // Check if we're doing what-if analysis
    let (projection, comparisons) = if let Some(compare_targets) = compare {
        match retirement_service.calculate_what_if_analysis(&portfolio, target_income, compare_targets) {
            Ok(result) => (result.0, Some(result.1)),
            Err(e) => {
                eprintln!("Error calculating what-if analysis: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        match retirement_service.calculate_basic_projection(&portfolio, target_income) {
            Ok(proj) => (proj, None),
            Err(e) => {
                // The service errors are specific and already say what to do; a generic
                // list of "possible reasons" beneath them only contradicts the message
                // (it named a 7-day floor that is not the binding constraint).
                eprintln!("Error calculating projection: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Display the projection
    println!("{}", "=".repeat(60));
    println!("Retirement Income Projection");
    println!("{}", "=".repeat(60));
    println!("\nTarget Daily Income:     {:.4} ICP/day", projection.target_daily_income().icp_per_day());
    println!("Current Daily Income:    {:.4} ICP/day (30-day average)", projection.current_daily_income());
    // Both axes, never collapsed into one label: a reader must be able to see WHICH is
    // failing, because the remedies differ — thin history is fixed by waiting, stale history
    // by running the tracker.
    match projection.quality_assessment() {
        Some(a) => {
            println!("Data Depth:              {}", a.depth.description());
            println!("Data Freshness:          {} ({} days since newest reward)",
                     a.freshness.description(), a.days_since_newest);
            match a.limiting_axis() {
                domain::retirement::LimitingAxis::Freshness => {
                    println!("  ⚠ Freshness is the limiting factor. History is deep but has stopped");
                    println!("    updating — run `track` to bring it current.");
                }
                domain::retirement::LimitingAxis::Depth => {
                    println!("  ⚠ Depth is the limiting factor. Data is current but there is not");
                    println!("    much of it yet — accuracy improves as history accumulates.");
                }
                domain::retirement::LimitingAxis::Neither => {}
            }
            if !a.overall_is_reliable() {
                println!("  → Treat the projection below as indicative, not decisive.");
            }
        }
        None => {
            println!("Data Quality:            {}", projection.data_quality().description());
        }
    }
    println!("\nCurrent Portfolio:       {:.4} ICP", projection.current_portfolio_value().to_icp());
    println!("Required Portfolio:      {:.4} ICP (with 20% safety margin)",
        projection.projected_timeline().required_portfolio_size.to_icp());

    if projection.is_already_feasible() {
        println!("\n{}", "=".repeat(60));
        println!("✓ RETIREMENT FEASIBLE NOW!");
        println!("{}", "=".repeat(60));
        println!("\nCongratulations! Your current daily income already meets");
        println!("or exceeds your target. You can retire today!");
    } else {
        // Display all three scenarios
        use domain::retirement::ProjectionScenario;

        println!("\n{}", "=".repeat(60));
        println!("Risk Scenario Analysis");
        println!("{}", "=".repeat(60));
        // Say what the bands were computed from. A band from 12 windows and a band from 40
        // are not the same claim, and the reader cannot tell them apart from the numbers.
        if let Some(p) = projection.percentiles() {
            println!(
                "Basis: {} non-overlapping {}-day windows over the last {} days",
                p.window_count, p.window_days, p.lookback_days
            );
        }

        for scenario in projection.scenarios() {
            match scenario {
                ProjectionScenario::Optimistic(timeline) => {
                    println!("\nOptimistic (90th %):     Retirement in {:.1} years ({})",
                        timeline.years_until_retirement, timeline.retirement_date);
                }
                ProjectionScenario::Realistic(timeline) => {
                    println!("Realistic (median):      Retirement in {:.1} years ({})",
                        timeline.years_until_retirement, timeline.retirement_date);
                }
                ProjectionScenario::Pessimistic(timeline) => {
                    println!("Pessimistic (10th %):    Retirement in {:.1} years ({})",
                        timeline.years_until_retirement, timeline.retirement_date);
                }
            }
        }

        let timeline = projection.projected_timeline();
        println!("\nPortfolio shortfall:     {:.4} ICP", projection.portfolio_shortfall());
        println!("Required Portfolio:      {:.4} ICP (realistic scenario)",
            timeline.required_portfolio_size.to_icp());
    }

    println!("\n{}", "=".repeat(60));
    println!("Assumptions");
    println!("{}", "=".repeat(60));
    let assumptions = projection.assumptions();
    println!("- Reward rate based on {}", assumptions.reward_rate_basis);
    println!("- Auto-stake maturity enabled (compounding)");
    println!("- No withdrawals during accumulation");
    println!("- {}% safety margin applied", (assumptions.safety_margin * 100.0) as u8);
    println!("- ICP/USD price not considered");

    if !projection.is_reliable() {
        println!("\n{}", "=".repeat(60));
        println!("⚠️  DATA QUALITY WARNING");
        println!("{}", "=".repeat(60));
        println!("\nProjection reliability: {}", projection.data_quality().description());
        println!("\nFor more reliable projections:");
        println!("  - Continue daily tracking to gather more data");
        println!("  - Target: 30+ days for 'Moderate' quality");
        println!("  - Target: 90+ days for 'Good' quality");
    }

    // Display what-if comparisons if present
    if let Some(comparisons) = comparisons {
        println!("\n{}", "=".repeat(60));
        println!("What-If Analysis: Alternative Targets");
        println!("{}", "=".repeat(60));
        println!("\n{:<12} | {:<20} | {}", "Target", "Timeline (Realistic)", "Impact vs Base");
        println!("{}", "-".repeat(60));

        for comp in &comparisons {
            let impact = if comp.is_earlier() {
                format!("⬆ {:.1} years EARLIER ✓", comp.years_delta.abs())
            } else if comp.is_later() {
                format!("⬇ {:.1} years LATER", comp.years_delta)
            } else {
                "Same timeline".to_string()
            };

            println!(
                "{:.1} ICP/day | {:.1} years ({})  | {}",
                comp.target_income.icp_per_day(),
                comp.realistic_timeline.years_until_retirement,
                comp.realistic_timeline.retirement_date.format("%Y-%m"),
                impact
            );
        }

        println!("\n{}", "=".repeat(60));
        println!("Required Portfolio (Realistic Scenario)");
        println!("{}", "=".repeat(60));
        println!("\n{:<12} | {:<10} | {:<10} | {}", "Target", "Required", "Current", "Shortfall");
        println!("{}", "-".repeat(60));

        for comp in &comparisons {
            let shortfall = comp.realistic_timeline.required_portfolio_size.to_icp()
                - projection.current_portfolio_value().to_icp();

            println!(
                "{:.1} ICP/day | {:.0} ICP | {:.0} ICP | {:.0} ICP",
                comp.target_income.icp_per_day(),
                comp.realistic_timeline.required_portfolio_size.to_icp(),
                projection.current_portfolio_value().to_icp(),
                shortfall
            );
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("Note: Projections are estimates only. Not financial advice.");
    println!("{}", "=".repeat(60));
    println!();

    // Deliberately NOT persisted to config.toml.
    //
    // `project` used to write --target back as retirement.default_target_income on every run.
    // Three reasons it is gone:
    //
    //  1. Nothing ever read it. --target is a required argument with no default, so the saved
    //     value could not act as one. It was written and never consulted.
    //  2. It erased provenance. A target set against a 209-day-old offline snapshot was
    //     recorded identically to one set against a live fetch, with nothing to tell them apart.
    //  3. Config::save() re-serialises the whole file through toml::to_string_pretty, so a
    //     projection silently rewrote the user's config and dropped its comments — including
    //     the "# default_target_income = 2.5" hint the setup wizard writes.
    //
    // A read-only analysis command should not mutate configuration as a side effect. If a
    // stored default is wanted later, it should be set by an explicit command and should
    // record when and against what data it was chosen.

    Ok(())
}

async fn run_tracking() -> Result<(), Box<dyn std::error::Error>> {
    // Your existing tracking logic
    println!("U Reflection Design & Build Inc. - ICP Neuron Tracker\n");
    println!("Loading configuration...");

    let config = load_config_or_exit();
    
    println!("🗄️  Initializing database...");
    let db = open_database_or_exit(&config.tracking.history_file);
    
    println!("🔑 Initializing IC client...");
    let ic_client = IcClient::new(
        &config.identity.pem_file,
        &config.ic.ic_url,
        &config.ic.governance_canister,
    )?;
    
    let portfolio_service = PortfolioService::new(ic_client);
    
    let neuron_ids: Vec<NeuronId> = config.neurons.ids
        .iter()
        .map(|&id| NeuronId::new(id))
        .collect();
    
    println!("\n🔍 Querying {} neurons...\n", neuron_ids.len());

    let fetch_result = portfolio_service.fetch_portfolio(&neuron_ids).await?;

    // Display any fetch errors
    for (idx, &neuron_id) in neuron_ids.iter().enumerate() {
        println!("[{}/{}] Fetching neuron {}...", idx + 1, neuron_ids.len(), neuron_id);
        if let Some((_, error)) = fetch_result.errors.iter().find(|(id, _)| *id == neuron_id) {
            eprintln!("  ✗ Error: {}", error);
            eprintln!("  → Make sure hot key is added to this neuron\n");
        }
    }

    let portfolio = fetch_result.portfolio;

    // Save snapshot if configured
    if config.tracking.snapshot_on_run {
        let tracking_service = TrackingService::new(&db, &db, &db);
        let snapshot_result = tracking_service.save_daily_snapshot(&portfolio)?;
        println!("💾 Saving daily snapshot for {}...", snapshot_result.date);
        println!("✓ Snapshot saved for {} neurons", snapshot_result.neurons_saved);

        // Show snapshot delta (change since last snapshot)
        use crate::domain::repositories::NeuronSnapshotRepository;
        if let Some(first_neuron) = portfolio.neurons().first() {
            if let Ok(Some((prev_neuron, prev_date))) = (&db as &dyn NeuronSnapshotRepository).get_previous_snapshot(first_neuron.id(), snapshot_result.date) {
                let days_elapsed = (snapshot_result.date - prev_date).num_days();
                let current_maturity = first_neuron.maturity().to_icp() + first_neuron.staked_maturity().to_icp();
                let prev_maturity = prev_neuron.maturity().to_icp() + prev_neuron.staked_maturity().to_icp();
                let maturity_delta = current_maturity - prev_maturity;

                println!("\nSnapshot Delta (change since last snapshot)");
                println!("─────────────────────────────────────────────────────────────");
                println!("  Previous: {}  (sample neuron maturity: {:.4} ICP)", prev_date, prev_maturity);
                println!("  Current:  {}  (sample neuron maturity: {:.4} ICP)", snapshot_result.date, current_maturity);
                if days_elapsed > 0 {
                    println!("  Change:   +{:.4} ICP over {} day(s)", maturity_delta, days_elapsed);
                    println!();
                    println!("  Note: This shows one neuron's delta. Other neurons may differ.");
                }
                println!();
            }
        }

        // Get daily income statistics
        match tracking_service.get_daily_income_stats(&portfolio) {
            Ok(stats) => {
                print!("{}", TerminalReportFormatter::format_income_analysis(&stats, snapshot_result.date));

                println!("Per-Neuron Breakdown");
                println!("─────────────────────────────────────────────────────────────");
                for (neuron_id, daily_icp) in &stats.neuron_contributions {
                    println!("  Neuron {}  {:.4} ICP/day", neuron_id, daily_icp);
                }
                println!();
            }
            Err(_) => {
                println!("\nDaily income stats not yet available (need 2+ days of data)");
                println!("Run this tool daily to build historical data.\n");
            }
        }
    }
    
    if config.display.show_individual_neurons {
        for neuron in portfolio.neurons() {
            print_neuron(neuron);
        }
    }
    
    if config.display.show_portfolio_summary {
        print_portfolio(&portfolio);
    }
    
    Ok(())
}

fn print_neuron(neuron: &domain::Neuron) {
    use domain::NeuronState;

    let state_str = match neuron.state() {
        NeuronState::Locked => "LOCKED",
        NeuronState::Dissolving => "DISSOLVING",
        NeuronState::Dissolved => "DISSOLVED",
    };

    println!("\nNeuron {}", neuron.id());
    println!("─────────────────────────────────────────────────────────────");

    println!("Financial");
    println!("  Staked              {:.4} ICP", neuron.stake().to_icp());
    println!("  Maturity            {:.4} ICP (ready to spawn)", neuron.maturity().to_icp());
    println!("  Staked Maturity     {:.4} ICP (auto-compound)", neuron.staked_maturity().to_icp());
    println!("  Total Value         {:.4} ICP", neuron.total_value().to_icp());
    println!();

    println!("Performance");
    println!("  Voting Power        {}", neuron.voting_power());
    println!("  Age                 {} days ({:.2}x bonus)", neuron.age_days(), neuron.age_bonus().value());
    println!("  Dissolve Delay      {} days ({:.2}x bonus)", neuron.dissolve_delay_days(), neuron.dissolve_bonus().value());
    println!("  Combined Mult       {:.2}x", neuron.combined_multiplier().value());
    println!();

    println!("Status");
    println!("  State               {}", state_str);
    println!("  Auto-stake          {}", if neuron.auto_stake_enabled() { "Enabled" } else { "Disabled" });
    println!("  Created             {}", neuron.created_date_formatted());
}

fn print_portfolio(portfolio: &Portfolio) {
    println!("\nPortfolio Summary ({} neurons)", portfolio.neuron_count());
    println!("─────────────────────────────────────────────────────────────");

    println!("Total Financial");
    println!("  Total Staked            {:.4} ICP", portfolio.total_stake().to_icp());
    println!("  Total Maturity          {:.4} ICP", portfolio.total_maturity().to_icp());
    println!("  Total Staked Maturity   {:.4} ICP", portfolio.total_staked_maturity().to_icp());
    println!("  TOTAL PORTFOLIO VALUE   {:.4} ICP", portfolio.total_value().to_icp());
    println!();

    println!("Total Performance");
    println!("  Total Voting Power      {}", portfolio.total_voting_power());
    println!("  Total Rewards Earned    {:.4} ICP", portfolio.total_rewards().to_icp());
    println!("  Overall Return          {:.2}%", portfolio.overall_return_percentage());
    println!();
}

fn handle_import_command(file: PathBuf, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("U Reflection Design & Build Inc. - ICP Neuron Tracker");
    println!("Historical Data Import\n");

    // Check if file exists
    if !file.exists() {
        eprintln!("Error: File not found: {}", file.display());
        eprintln!("\nMake sure the CSV file exists and the path is correct.");
        eprintln!("\nExample usage:");
        eprintln!("  icp-neuron-tracker import --file sample_import.csv");
        std::process::exit(1);
    }

    println!("Loading configuration...");
    let config = load_config_or_exit();

    println!("🗄️  Initializing database...");
    let db = open_database_or_exit(&config.tracking.history_file);

    let import_service = ImportService::new(db);

    let options = ImportOptions { dry_run };

    if dry_run {
        println!("\n🔍 DRY RUN MODE - No data will be imported\n");
    }

    match import_service.import_snapshots(&file, options) {
        Ok(result) => {
            result.print_summary();

            if dry_run {
                println!("\n💡 To actually import the data, run without --dry-run:");
                println!("   icp-neuron-tracker import --file {}", file.display());
            } else if result.new_snapshots > 0 {
                println!("\n✓ Import completed successfully!");
                println!("\nNext steps:");
                println!("  1. Verify data: icp-neuron-tracker report summary");
                println!("  2. View trends: icp-neuron-tracker report history --days 30");
                println!("  3. Calculate retirement: icp-neuron-tracker project --target 2.5");
            } else {
                println!("\n✓ No new data to import (all snapshots already exist)");
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("\n❌ Import failed: {}", e);
            eprintln!("\nPlease check:");
            eprintln!("  - CSV file format matches specification");
            eprintln!("  - All required columns are present");
            eprintln!("  - Data types are correct (numbers, dates in YYYY-MM-DD)");
            eprintln!("\nExpected CSV format:");
            eprintln!("  neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,");
            eprintln!("  available_maturity_e8s,voting_power,dissolve_delay_seconds,");
            eprintln!("  age_seconds,created_timestamp_seconds");
            std::process::exit(1);
        }
    }
}

fn handle_export_command(
    output: PathBuf,
    neurons: Option<Vec<u64>>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    use chrono::NaiveDate;

    println!("U Reflection Design & Build Inc. - ICP Neuron Tracker");
    println!("Historical Data Export\n");

    // Check if output file already exists
    if output.exists() {
        eprintln!("Error: Output file already exists: {}", output.display());
        eprintln!("\nPlease choose a different filename or remove the existing file.");
        eprintln!("\nExample usage:");
        eprintln!("  icp-neuron-tracker export --output neurons_backup.csv");
        std::process::exit(1);
    }

    println!("Loading configuration...");
    let config = load_config_or_exit();

    println!("🗄️  Initializing database...");
    let db = open_database_or_exit(&config.tracking.history_file);

    let import_service = ImportService::new(db);

    // Parse date filters
    let start_date_parsed = if let Some(ref date_str) = start_date {
        match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(date) => Some(date),
            Err(_) => {
                eprintln!("Error: Invalid start date format: {}", date_str);
                eprintln!("Expected format: YYYY-MM-DD (e.g., 2024-01-15)");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let end_date_parsed = if let Some(ref date_str) = end_date {
        match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(date) => Some(date),
            Err(_) => {
                eprintln!("Error: Invalid end date format: {}", date_str);
                eprintln!("Expected format: YYYY-MM-DD (e.g., 2024-12-31)");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    // Convert neuron IDs to NeuronId type
    let neuron_ids = neurons.map(|ids| ids.into_iter().map(NeuronId::new).collect());

    // Build export options
    let mut options = ExportOptions::new(output.clone());
    options.neuron_ids = neuron_ids;
    options.start_date = start_date_parsed;
    options.end_date = end_date_parsed;

    // Display filters if any
    if options.neuron_ids.is_some() || options.start_date.is_some() || options.end_date.is_some() {
        println!("\nApplying filters:");
        if let Some(ref ids) = options.neuron_ids {
            println!("  Neurons: {} selected", ids.len());
        }
        if let Some(start) = options.start_date {
            println!("  Start date: {}", start);
        }
        if let Some(end) = options.end_date {
            println!("  End date: {}", end);
        }
    }

    match import_service.export_snapshots(options) {
        Ok(result) => {
            result.print_summary();

            println!("\n✓ Export completed successfully!");
            println!("\nYour data has been exported to: {}", output.display());
            println!("\nNext steps:");
            println!("  1. Backup the file to a safe location");
            println!("  2. To re-import later: icp-neuron-tracker import --file {}", output.display());
            println!("  3. To export with filters:");
            println!("     icp-neuron-tracker export --output filtered.csv --neurons 123,456 --start-date 2024-01-01");

            Ok(())
        }
        Err(e) => {
            eprintln!("\n❌ Export failed: {}", e);
            eprintln!("\nPlease check:");
            eprintln!("  - Database file exists and is accessible");
            eprintln!("  - You have permission to write to the output directory");
            eprintln!("  - Filters match existing data (if applied)");
            eprintln!("\nExample usage:");
            eprintln!("  icp-neuron-tracker export --output neurons_backup.csv");
            eprintln!("  icp-neuron-tracker export --output recent.csv --start-date 2024-10-01");
            eprintln!("  icp-neuron-tracker export --output neuron123.csv --neurons 123456789");
            std::process::exit(1);
        }
    }
}

fn handle_report_command(report_type: ReportCommands) -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration to get database path
    let config = load_config_or_exit();
    let db_path = &config.tracking.history_file;

    // Initialize repository
    let repository = open_database_or_exit(db_path);

    // Create report service
    let report_service = ReportService::new(repository);

    match report_type {
        ReportCommands::Summary { format } => {
            // Generate report
            let report = report_service.generate_summary_report()?;

            // Format and display based on output format
            let output = match format {
                OutputFormat::Terminal => TerminalReportFormatter::format_summary(&report),
                OutputFormat::Json => TerminalReportFormatter::export_summary_json(&report)?,
                OutputFormat::Csv => TerminalReportFormatter::export_summary_csv(&report)?,
            };
            println!("{}", output);

            Ok(())
        }
        ReportCommands::History { days, format } => {
            // Validate days parameter
            if days == 0 {
                eprintln!("Error: Days must be at least 1");
                std::process::exit(1);
            }

            // Generate historical trend report
            let trend = report_service.generate_historical_report(days)?;

            // Format and display based on output format
            let output = match format {
                OutputFormat::Terminal => TerminalReportFormatter::format_historical(&trend, days),
                OutputFormat::Json => TerminalReportFormatter::export_history_json(&trend)?,
                OutputFormat::Csv => TerminalReportFormatter::export_history_csv(&trend)?,
            };
            println!("{}", output);

            Ok(())
        }
        ReportCommands::Rewards { days, format } => {
            if days == 0 {
                eprintln!("Error: Days must be at least 1");
                std::process::exit(1);
            }

            // Generate reward analysis report
            let analysis = match report_service.generate_reward_analysis(days) {
                Ok(a) => a,
                Err(e) => {
                    // Print with Display: these messages are multi-line and explanatory, and
                    // propagating via `?` would render them through Debug with escaped \n.
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            };

            // Format and display based on output format
            let output = match format {
                OutputFormat::Terminal => TerminalReportFormatter::format_rewards(&analysis, days),
                OutputFormat::Json => TerminalReportFormatter::export_rewards_json(&analysis)?,
                OutputFormat::Csv => TerminalReportFormatter::export_rewards_csv(&analysis)?,
            };
            println!("{}", output);

            Ok(())
        }
        ReportCommands::Neuron { neuron_id, format } => {
            // Generate neuron detail report
            let detail = report_service.generate_neuron_detail(NeuronId::new(neuron_id))?;

            // Format and display based on output format
            let output = match format {
                OutputFormat::Terminal => TerminalReportFormatter::format_neuron_detail(&detail),
                OutputFormat::Json => TerminalReportFormatter::export_neuron_json(&detail)?,
                OutputFormat::Csv => TerminalReportFormatter::export_neuron_csv(&detail)?,
            };
            println!("{}", output);

            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use application::StoredPortfolio;
    use chrono::{DateTime, NaiveDate, Utc};
    use domain::{IcpAmount, Neuron, NeuronId, NeuronState};

    fn stored_at(stake_e8s: u64, retrieved_ts: i64) -> StoredPortfolio {
        let neuron = Neuron::from_snapshot(
            NeuronId::new(111),
            IcpAmount::from_e8s(stake_e8s),
            IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(0),
            600_000_000_000,
            126_230_400,
            252_460_800,
            NeuronState::Locked,
            true,
            1_621_209_600,
            retrieved_ts as u64,
        );
        StoredPortfolio {
            portfolio: Portfolio::new(vec![neuron]),
            retrieved_at: DateTime::from_timestamp(retrieved_ts, 0),
            snapshot_date: NaiveDate::from_ymd_opt(2026, 1, 7),
        }
    }

    fn now() -> DateTime<Utc> {
        // 2026-08-05 00:00:00 UTC
        DateTime::from_timestamp(1_785_888_000, 0).unwrap()
    }

    /// A live fetch must look exactly as it did before this feature existed — no banner,
    /// no staleness note, nothing added to the output.
    #[test]
    fn live_fetch_produces_no_banner() {
        let stored = stored_at(100_000_000_000, 1_767_816_305);
        assert!(format_staleness_banner(&PortfolioSource::Live, &stored, now()).is_none());
    }

    /// Explicit --offline must always warn, even though the user asked for it.
    #[test]
    fn explicit_offline_always_produces_a_banner() {
        let stored = stored_at(100_000_000_000, 1_767_816_305);
        let banner = format_staleness_banner(&PortfolioSource::OfflineRequested, &stored, now())
            .expect("--offline must produce a banner");
        assert!(banner.contains("OFFLINE PROJECTION"));
    }

    /// A silent fallback is the failure mode this feature exists to prevent.
    #[test]
    fn network_fallback_always_produces_a_banner_naming_the_reason() {
        let stored = stored_at(100_000_000_000, 1_767_816_305);
        let banner = format_staleness_banner(
            &PortfolioSource::Fallback("query failed: connection refused".to_string()),
            &stored,
            now(),
        )
        .expect("fallback must produce a banner");
        assert!(banner.contains("NETWORK UNAVAILABLE"));
        assert!(banner.contains("connection refused"), "the reason must be shown");
    }

    /// The banner must carry all three required facts: which value was used, when it was
    /// observed with an age, and which way the resulting date is wrong.
    #[test]
    fn banner_carries_value_age_and_error_direction() {
        let stored = stored_at(594_437_351_997, 1_767_816_305); // 2026-01-07
        for source in [
            PortfolioSource::OfflineRequested,
            PortfolioSource::Fallback("simulated failure".to_string()),
        ] {
            let b = format_staleness_banner(&source, &stored, now()).expect("banner");

            // 1. which portfolio value was used
            assert!(b.contains("newest stored snapshot"), "missing source: {}", b);
            assert!(b.contains("5944.3735 ICP"), "missing the value itself: {}", b);

            // 2. retrieved_at with age in days
            assert!(b.contains("2026-01-07"), "missing retrieved_at: {}", b);
            assert!(b.contains("209 days old"), "missing age in days: {}", b);

            // 3. direction of the error, stated plainly
            assert!(b.contains("UNDERSTATES"), "missing understatement: {}", b);
            assert!(
                b.contains("LATER than reality"),
                "the banner must say the date errs LATER, not earlier: {}",
                b
            );
        }
    }

    /// Age is derived from the observation time, so a fresh snapshot reads as fresh.
    #[test]
    fn banner_age_tracks_the_observation_time() {
        let stored = stored_at(100_000_000_000, 1_785_801_600); // 1 day before `now`
        let b = format_staleness_banner(&PortfolioSource::OfflineRequested, &stored, now())
            .expect("banner");
        assert!(b.contains("1 days old"), "{}", b);
    }
}

#[cfg(test)]
mod config_side_effect_tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// A config file with comments and a specific key order, as the setup wizard writes it.
    const WIZARD_CONFIG: &str = r#"[identity]
pem_file = "/home/user/.config/icp-neuron-tracker/identity.pem"

[ic]
governance_canister = "rrkah-fqaaa-aaaaa-aaaaq-cai"
ic_url = "https://ic0.app"

[neurons]
ids = ["1000000000000000003"]

[tracking]
history_file = "neuron_history.db"
snapshot_on_run = true

[display]
show_individual_neurons = true
show_portfolio_summary = true

[retirement]
# default_target_income = 2.5
"#;

    /// Loading a config and dropping it must not disturb the file. `project` reads config to
    /// find the database and identity; it must not write it back.
    #[test]
    fn loading_a_config_does_not_modify_the_file() {
        let mut f = NamedTempFile::new().expect("temp file");
        f.write_all(WIZARD_CONFIG.as_bytes()).unwrap();
        f.flush().unwrap();
        let path = f.path().to_str().unwrap().to_string();

        let before = std::fs::read_to_string(&path).unwrap();
        let config = Config::load(&path).expect("load");
        assert_eq!(config.neurons.ids.len(), 1);
        let after = std::fs::read_to_string(&path).unwrap();

        assert_eq!(before, after, "reading config must not rewrite it");
    }

    /// Documents WHY the write-back was removed rather than gated: re-serialising the config
    /// destroys comments and cannot be made non-destructive without a different writer. This
    /// test pins that fact so nobody reinstates the old behaviour thinking it was harmless.
    #[test]
    fn re_serialising_a_config_would_destroy_its_comments() {
        let config: Config = toml::from_str(WIZARD_CONFIG).expect("parse");
        let round_tripped = toml::to_string_pretty(&config).expect("serialise");

        assert!(
            WIZARD_CONFIG.contains("# default_target_income = 2.5"),
            "fixture should carry a comment"
        );
        assert!(
            !round_tripped.contains("# default_target_income"),
            "toml::to_string_pretty drops comments — this is why `project` no longer saves config"
        );
    }

    /// The projection path must never persist its target. --target is a required argument
    /// with no default, so a stored value could not act as one; and a target chosen against
    /// stale offline data must not be recorded as though it came from a live fetch.
    #[test]
    fn project_does_not_persist_the_target_to_config() {
        let src = std::fs::read_to_string("src/main.rs").expect("read main.rs");
        let handler_start = src
            .find("async fn handle_project_command")
            .expect("handle_project_command must exist");
        let handler = &src[handler_start..];
        let handler_end = handler
            .find("\nasync fn ")
            .or_else(|| handler.find("\nfn "))
            .unwrap_or(handler.len());
        let body = &handler[..handler_end];

        // Strip comments so the explanatory note above the removal doesn't count as a match.
        let code: String = body
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !code.contains("update_retirement_target"),
            "project must not write default_target_income back to config"
        );
        assert!(
            !code.contains("config.save("),
            "project must not save config as a side effect of running a projection"
        );
    }
}
