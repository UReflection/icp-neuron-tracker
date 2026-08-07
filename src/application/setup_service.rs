use std::path::PathBuf;
use console::style;
use dialoguer::{Confirm, Select, Input};
use indicatif::ProgressBar;
use std::time::Duration;
use directories::ProjectDirs;

use crate::domain::NeuronId;
use crate::application::{IdentityService, TrackingService, PortfolioService};
use crate::infrastructure::{IcClient, SqliteRepository};

/// Result of the setup wizard
#[derive(Debug)]
#[allow(dead_code)]
pub struct SetupResult {
    pub success: bool,
    pub identity_path: PathBuf,
    pub principal_id: String,
    pub neurons_configured: usize,
    pub snapshots_imported: usize,
}

/// What the first snapshot found, when it succeeded.
struct SnapshotSummary {
    total_stake_icp: f64,
    total_maturity_icp: f64,
    neuron_count: usize,
}

/// Service orchestrating the interactive setup wizard
pub struct SetupService {
    config_path: PathBuf,
}

impl SetupService {
    pub fn new() -> Self {
        let config_path = Self::get_default_config_path();
        Self { config_path }
    }

    /// Where `init` will write config.toml. Public so the CLI can name the path when it
    /// declines to run the wizard and points at the manual fallback instead.
    pub fn default_config_path() -> PathBuf {
        Self::get_default_config_path()
    }

    /// Get default config.toml path (config directory)
    fn get_default_config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "u-reflection", "icp-neuron-tracker") {
            let config_dir = proj_dirs.config_dir();
            std::fs::create_dir_all(config_dir).ok();
            config_dir.join("config.toml")
        } else {
            // Fallback to current directory
            PathBuf::from("config.toml")
        }
    }

    /// Get default database path (data directory)
    fn get_default_db_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "u-reflection", "icp-neuron-tracker") {
            let data_dir = proj_dirs.data_dir();
            std::fs::create_dir_all(data_dir).ok();
            data_dir.join("neuron_history.db")
        } else {
            // Fallback to current directory
            PathBuf::from("neuron_history.db")
        }
    }

    /// Run the complete interactive setup wizard
    pub async fn run_interactive_setup(&self) -> Result<SetupResult, Box<dyn std::error::Error>> {
        self.print_welcome();

        // Step 1: Identity
        let (identity_path, principal_id) = self.setup_identity_interactive().await?;

        // Step 2: Neurons
        let neuron_ids = self.setup_neurons_interactive(&principal_id).await?;

        // Step 3: Historical Import (Optional - skip for now, just prompt)
        let snapshots_imported = self.setup_import_interactive().await?;

        // Config is written BEFORE the snapshot, and the snapshot cannot abort setup.
        //
        // A hot key added moments ago in the NNS dapp has not necessarily propagated, so a
        // failed first snapshot is the *expected* day-one outcome, not an exceptional one.
        // Previously the snapshot ran first and propagated its error with `?`, so the wizard
        // died before `finalize_setup` ever ran: the user lost every neuron ID they had just
        // typed, and re-running asked for all of it again. Nothing about that failure implies
        // the configuration is wrong — it is the one part of setup we already know is good.
        self.finalize_setup(&identity_path, &neuron_ids)?;

        // Step 4: Initial Snapshot
        let snapshot_taken = self.take_initial_snapshot(&neuron_ids, &identity_path).await;

        // Print completion message
        self.print_completion(snapshots_imported > 0, &snapshot_taken)?;

        Ok(SetupResult {
            success: true,
            identity_path,
            principal_id,
            neurons_configured: neuron_ids.len(),
            snapshots_imported,
        })
    }

    fn print_welcome(&self) {
        println!();
        println!("{}", style("╔═════════════════════════════════════════════════════════╗").cyan());
        println!("{}", style("║          ICP Neuron Tracker - Setup Wizard              ║").cyan());
        println!("{}", style("╚═════════════════════════════════════════════════════════╝").cyan());
        println!();
        println!("Let's get you set up in 4 quick steps!");
        println!();
    }

    async fn setup_identity_interactive(&self) -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
        println!("{}", style("─".repeat(60)));
        println!("{}", style("Step 1 of 4: Create Your Identity").bold());
        println!("{}", style("─".repeat(60)));
        println!();
        println!("Your identity allows read-only access to neuron data.");
        println!("This stays on your machine - we never see your keys.");
        println!();

        let default_path = Self::get_default_identity_path();

        // Check if identity already exists
        if default_path.exists() {
            let use_existing = Confirm::new()
                .with_prompt("Identity already exists. Use it?")
                .default(true)
                .interact()?;

            if use_existing {
                // Load existing identity
                let identity_service = IdentityService::new();
                let identity_info = identity_service.get_identity_info(&default_path)?;

                println!();
                println!("{} Identity loaded", style("✓").green());
                println!("{} Principal ID: {}", style("✓").green(), identity_info.principal);
                println!();

                return Ok((default_path, identity_info.principal));
            }
        }

        println!("Creating identity at: {}", default_path.display());
        println!();

        let spinner = ProgressBar::new_spinner();
        spinner.set_message("Generating keypair...");
        spinner.enable_steady_tick(Duration::from_millis(100));

        // Create identity
        let identity_service = IdentityService::new();
        let identity_info = identity_service.generate_identity(&default_path)?;

        spinner.finish_with_message(format!("{} Identity created", style("✓").green()));

        println!("{} Principal ID: {}", style("✓").green(), identity_info.principal);
        println!();

        Ok((default_path, identity_info.principal))
    }

    async fn setup_neurons_interactive(&self, principal_id: &str) -> Result<Vec<NeuronId>, Box<dyn std::error::Error>> {
        println!();
        println!("{}", style("─".repeat(60)));
        println!("{}", style("Step 2 of 4: Configure Neuron Access").bold());
        println!("{}", style("─".repeat(60)));
        println!();
        println!("To track neurons, add this hot key to each neuron in NNS:");
        println!();
        println!("  {}", style("─".repeat(70)));
        println!();
        println!("  {}", style("YOUR HOT KEY:").cyan().bold());
        println!();
        println!("    {}", style(principal_id).green().bold());
        println!();
        println!("  {}", style("─".repeat(70)));
        println!();

        // Try to copy to clipboard
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if clipboard.set_text(principal_id).is_ok() {
                println!("  {}", style("(Copied to clipboard!)").dim());
                println!();
            }
        }

        println!("How to add hot key:");
        println!("  1. Open NNS Dapp: {}", style("https://nns.ic0.app").underlined());
        println!("  2. Select each neuron you want to track");
        println!("  3. Click \"Add Hotkey\" and paste: {}", principal_id);
        println!("  4. Submit transaction");
        println!("  5. Note down the neuron IDs");
        println!();

        // A Confirm renders "[y/n]", so a prompt reading "Press Enter" described a keystroke
        // that does not answer it. Wording matches the widget, and a default lets Enter work.
        Confirm::new()
            .with_prompt("Have you added the hot key and got your neuron IDs to hand?")
            .default(true)
            .interact()?;

        println!();
        println!("Enter your neuron IDs (comma-separated):");
        println!("Example: 1000000000000000003,10000000000000000002");
        println!();

        let neuron_ids_input: String = Input::new()
            .with_prompt("Neuron IDs")
            .interact_text()?;

        // Parse neuron IDs
        let neuron_ids: Vec<NeuronId> = neuron_ids_input
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u64>())
            .collect::<Result<Vec<u64>, _>>()
            .map_err(|e| format!("Invalid neuron ID format: {}", e))?
            .into_iter()
            .map(NeuronId::new)
            .collect();

        if neuron_ids.is_empty() {
            return Err("No neuron IDs provided".into());
        }

        println!();
        println!("{} Will track {} neuron{}",
            style("✓").green(),
            neuron_ids.len(),
            if neuron_ids.len() == 1 { "" } else { "s" }
        );

        Ok(neuron_ids)
    }

    async fn setup_import_interactive(&self) -> Result<usize, Box<dyn std::error::Error>> {
        println!();
        println!("{}", style("─".repeat(60)));
        println!("{}", style("Step 3 of 4: Import Historical Data (Optional)").bold());
        println!("{}", style("─".repeat(60)));
        println!();
        println!("Do you have historical tracking data to import?");
        println!();

        let choice = Select::new()
            .with_prompt("Choose an option")
            .items(&[
                "Yes, I have a CSV file",
                "No, start tracking from today",
                "I'll import later"
            ])
            .default(1)
            .interact()?;

        match choice {
            0 => {
                // User has CSV - for now, just say it's coming soon
                println!();
                println!("{} CSV import during setup coming soon!", style("ℹ").cyan());
                println!("You can import historical data anytime with:");
                println!("  {}", style("icp-neuron-tracker import --file <path>").cyan());
                println!();
                Ok(0)
            }
            1 => {
                // No historical data
                println!();
                println!("{} Will start tracking from today", style("ℹ").cyan());
                println!();
                Ok(0)
            }
            2 => {
                // Import later
                println!();
                println!("{} You can import historical data anytime with:", style("ℹ").cyan());
                println!("  {}", style("icp-neuron-tracker import --file <path>").cyan());
                println!();
                Ok(0)
            }
            _ => unreachable!()
        }
    }

    /// Take the first snapshot. Failure is reported, never fatal — see `run_interactive_setup`.
    async fn take_initial_snapshot(&self, neuron_ids: &[NeuronId], identity_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        println!();
        println!("{}", style("─".repeat(60)));
        println!("{}", style("Step 4 of 4: Initial Snapshot").bold());
        println!("{}", style("─".repeat(60)));
        println!();
        println!("Taking first snapshot of current neuron state...");
        println!();

        let spinner = ProgressBar::new_spinner();
        spinner.set_message("Collecting snapshot...");
        spinner.enable_steady_tick(Duration::from_millis(100));

        let result = self.collect_and_save_snapshot(neuron_ids, identity_path).await;

        match result {
            Ok(summary) => {
                spinner.finish_with_message(format!("{} Snapshot collected successfully", style("✓").green()));
                println!();
                println!("Current portfolio:");
                println!("  Total stake: {:.2} ICP", summary.total_stake_icp);
                println!("  Total maturity: {:.2} ICP", summary.total_maturity_icp);
                println!("  Neurons: {}", summary.neuron_count);
                println!();
                Ok(())
            }
            Err(e) => {
                spinner.finish_and_clear();
                println!("{} Could not take the first snapshot yet.", style("!").yellow().bold());
                println!();
                println!("  Reason: {}", e);
                println!();
                println!("{}", style("This is normal on a first run.").bold());
                println!("A hot key added in the NNS dapp takes a few minutes to become visible");
                println!("to the governance canister. Until it does, the tracker cannot read your");
                println!("neurons — and it cannot tell that case apart from a hot key that was");
                println!("never added, so it does not guess.");
                println!();
                println!("Your configuration has been saved either way. Nothing needs re-entering.");
                println!();
                println!("Wait a few minutes, then run:");
                println!("    {}", style("icp-neuron-tracker track").cyan());
                println!();
                println!("If it still fails after ten minutes or so, check that the hot key");
                println!("below is listed on each neuron in the NNS dapp:");
                println!("    {}", style(Self::principal_hint(identity_path)).green());
                println!();
                Err(e)
            }
        }
    }

    /// The network-and-disk half of step 4, separated so the caller can report failure
    /// without the spinner still running.
    async fn collect_and_save_snapshot(
        &self,
        neuron_ids: &[NeuronId],
        identity_path: &PathBuf,
    ) -> Result<SnapshotSummary, Box<dyn std::error::Error>> {
        let ic_client = IcClient::new(
            identity_path.to_str().ok_or("Invalid identity path")?,
            "https://ic0.app",
            "rrkah-fqaaa-aaaaa-aaaaq-cai"
        )?;

        let portfolio_service = PortfolioService::new(ic_client);
        let fetch_result = portfolio_service.fetch_portfolio(neuron_ids).await?;

        let db_path = Self::get_default_db_path();
        let db_path_str = db_path.to_str().ok_or("Invalid database path")?;
        let neuron_repo = SqliteRepository::new(db_path_str)?;
        let portfolio_repo = SqliteRepository::new(db_path_str)?;
        let reward_repo = SqliteRepository::new(db_path_str)?;
        let tracking_service = TrackingService::new(neuron_repo, portfolio_repo, reward_repo);
        tracking_service.save_daily_snapshot(&fetch_result.portfolio)?;

        Ok(SnapshotSummary {
            total_stake_icp: fetch_result.portfolio.total_stake().to_icp(),
            total_maturity_icp: fetch_result.portfolio.total_maturity().to_icp(),
            neuron_count: fetch_result.portfolio.neuron_count(),
        })
    }

    /// Best-effort principal for the failure hint. The identity is already on disk at this
    /// point, so a read failure here is not worth another error path.
    fn principal_hint(identity_path: &PathBuf) -> String {
        IdentityService::new()
            .get_identity_info(identity_path)
            .map(|info| info.principal)
            .unwrap_or_else(|_| "(run `icp-neuron-tracker identity info` to see it)".to_string())
    }

    fn finalize_setup(&self, identity_path: &PathBuf, neuron_ids: &[NeuronId]) -> Result<(), Box<dyn std::error::Error>> {
        // Get default database path
        let db_path = Self::get_default_db_path();

        // Generate config content
        let config_content = format!(r#"[identity]
pem_file = "{}"

[ic]
governance_canister = "rrkah-fqaaa-aaaaa-aaaaq-cai"
ic_url = "https://ic0.app"

[neurons]
ids = [{}]

[tracking]
history_file = "{}"
snapshot_on_run = true

[display]
show_individual_neurons = true
show_portfolio_summary = true

[retirement]
# default_target_income = 2.5
"#,
            identity_path.display(),
            neuron_ids.iter()
                .map(|id| format!("\"{}\"", id.value()))
                .collect::<Vec<_>>()
                .join(", "),
            db_path.display()
        );

        std::fs::write(&self.config_path, config_content)?;

        Ok(())
    }

    fn print_completion(
        &self,
        has_historical_data: bool,
        snapshot_taken: &Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!();
        if snapshot_taken.is_ok() {
            println!("{}", style("╔═════════════════════════════════════════════════════════╗").green());
            println!("{}", style("║                    Setup Complete! 🎉                   ║").green().bold());
            println!("{}", style("╚═════════════════════════════════════════════════════════╝").green());
            println!();
            println!("You're all set to track your ICP neurons!");
        } else {
            println!("{}", style("╔═════════════════════════════════════════════════════════╗").yellow());
            println!("{}", style("║           Setup Saved - First Snapshot Pending          ║").yellow().bold());
            println!("{}", style("╚═════════════════════════════════════════════════════════╝").yellow());
            println!();
            println!("Your configuration is written and complete. Only the first snapshot");
            println!("is outstanding, and `track` will take it once the hot key is visible.");
        }
        println!();
        println!("Files created:");
        println!("  Config:   {}", style(self.config_path.display()).cyan());
        println!("  Database: {}", style(Self::get_default_db_path().display()).cyan());
        println!();
        println!("What you can do now:");
        println!();
        println!("  View your portfolio:");
        println!("    {}", style("icp-neuron-tracker report summary").cyan());
        println!();

        if has_historical_data {
            println!("  See historical trends:");
            println!("    {}", style("icp-neuron-tracker report history --days 30").cyan());
            println!();
        }

        println!("  Analyze recent rewards:");
        println!("    {}", style("icp-neuron-tracker report rewards --days 30").cyan());
        println!();
        // `project` is deliberately NOT recommended here. Risk scenarios need eleven
        // populated 7-day windows within a 90-day lookback, so on a fresh database the
        // command cannot succeed for about eleven weeks. Offering it on the completion
        // screen sent every new user straight into a refusal.
        println!("  Retirement projection needs about 11 weeks of daily tracking before it");
        println!("  can produce risk bands — see below. It will tell you where you are.");
        println!();
        println!("Schedule daily tracking:");
        println!();
        println!("  Linux/Mac (cron):");
        println!("    {}", style("0 0 * * * icp-neuron-tracker track").dim());
        println!();
        println!("  Or run manually:");
        println!("    {}", style("icp-neuron-tracker track").cyan());
        println!();
        println!("Daily tracking is what makes the rest work. The reward history it builds is");
        println!("the only input to the projection, and the tool will not estimate from data");
        println!("it has not observed — so the first eleven weeks are the cold start, not a");
        println!("fault. Reports work from the first snapshot.");
        println!();
        println!("Need help? Run: {}", style("icp-neuron-tracker --help").cyan());
        println!();
        println!("{}", style("─".repeat(60)));
        println!();
        println!("Thanks for using ICP Neuron Tracker!");
        println!();

        Ok(())
    }

    fn get_default_identity_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "u-reflection", "icp-neuron-tracker") {
            let config_dir = proj_dirs.config_dir();
            std::fs::create_dir_all(config_dir).ok();
            config_dir.join("identity.pem")
        } else {
            // Fallback to current directory
            PathBuf::from("identity.pem")
        }
    }
}
