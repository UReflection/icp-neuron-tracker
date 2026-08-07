use crate::infrastructure::{CsvParser, CsvRecord, SqliteRepository};
use std::path::Path;
use std::time::Instant;
use std::collections::{HashMap, HashSet};
use crate::domain::NeuronId;
use crate::domain::repositories::ExportRow;
use chrono::NaiveDate;
use std::fs::File;
use std::io::Write;

/// Options for importing historical data
pub struct ImportOptions {
    pub dry_run: bool,
}

/// Options for exporting historical data
pub struct ExportOptions {
    pub neuron_ids: Option<Vec<NeuronId>>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub output_path: std::path::PathBuf,
}

impl ExportOptions {
    pub fn new(output_path: std::path::PathBuf) -> Self {
        Self {
            neuron_ids: None,
            start_date: None,
            end_date: None,
            output_path,
        }
    }
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { dry_run: false }
    }
}

/// Result of an import operation
pub struct ImportResult {
    pub total_rows: usize,
    pub new_snapshots: usize,
    pub duplicates_skipped: usize,
    pub rewards_calculated: usize,
    pub rewards_skipped: usize,
    pub portfolio_snapshots_calculated: usize,
    pub portfolio_snapshots_skipped: usize,
    pub duration_seconds: f64,
    pub neuron_summary: HashMap<NeuronId, NeuronImportStats>,
    pub date_range: Option<(String, String)>,
    pub errors: Vec<String>,
}

/// Statistics for a single neuron's import
pub struct NeuronImportStats {
    pub snapshots: usize,
    pub rewards: usize,
}

/// Result of an export operation
pub struct ExportResult {
    pub total_snapshots: usize,
    pub neurons_exported: usize,
    pub date_range: Option<(String, String)>,
    pub file_path: std::path::PathBuf,
    pub file_size_bytes: u64,
    pub duration_seconds: f64,
}

impl ExportResult {
    pub fn print_summary(&self) {
        println!("\n{}", "=".repeat(60));
        println!("Export Summary");
        println!("{}", "=".repeat(60));

        println!("\nRecords Exported:");
        println!("  Total snapshots:     {}", self.total_snapshots);
        println!("  Neurons:             {}", self.neurons_exported);

        if let Some((start, end)) = &self.date_range {
            println!("\nDate Range:");
            println!("  From: {}", start);
            println!("  To:   {}", end);
        }

        println!("\nOutput:");
        println!("  File:    {}", self.file_path.display());
        println!("  Size:    {} bytes ({:.2} KB)", self.file_size_bytes, self.file_size_bytes as f64 / 1024.0);

        println!("\nPerformance:");
        println!("  Duration: {:.2} seconds", self.duration_seconds);
        if self.duration_seconds > 0.0 {
            let rate = self.total_snapshots as f64 / self.duration_seconds;
            println!("  Rate:     {:.0} snapshots/second", rate);
        }

        println!("{}", "=".repeat(60));
    }
}

impl ImportResult {
    pub fn print_summary(&self) {
        println!("\n{}", "=".repeat(60));
        println!("Import Summary");
        println!("{}", "=".repeat(60));

        println!("\nRecords Processed:");
        println!("  Total rows:          {}", self.total_rows);
        println!("  New snapshots:       {}", self.new_snapshots);
        println!("  Duplicates skipped:  {}", self.duplicates_skipped);

        if self.rewards_calculated > 0 || self.rewards_skipped > 0 {
            println!("\nRewards Calculated:");
            println!("  New rewards:         {}", self.rewards_calculated);
            println!("  Duplicates skipped:  {}", self.rewards_skipped);
        }

        if self.portfolio_snapshots_calculated > 0 || self.portfolio_snapshots_skipped > 0 {
            println!("\nPortfolio Snapshots Calculated:");
            println!("  New snapshots:       {}", self.portfolio_snapshots_calculated);
            println!("  Duplicates skipped:  {}", self.portfolio_snapshots_skipped);
        }

        if let Some((start, end)) = &self.date_range {
            println!("\nDate Range:");
            println!("  From: {}", start);
            println!("  To:   {}", end);
        }

        println!("\nNeurons Imported:");
        for (neuron_id, stats) in &self.neuron_summary {
            if stats.rewards > 0 {
                println!("  Neuron {}: {} snapshots, {} rewards", neuron_id, stats.snapshots, stats.rewards);
            } else {
                println!("  Neuron {}: {} snapshots", neuron_id, stats.snapshots);
            }
        }

        println!("\nPerformance:");
        println!("  Duration: {:.2} seconds", self.duration_seconds);
        if self.duration_seconds > 0.0 {
            let rate = self.new_snapshots as f64 / self.duration_seconds;
            println!("  Rate:     {:.0} snapshots/second", rate);
        }

        if !self.errors.is_empty() {
            println!("\nErrors:");
            for error in &self.errors {
                println!("  - {}", error);
            }
        }

        println!("{}", "=".repeat(60));
    }
}

/// Service for importing historical neuron data
pub struct ImportService {
    repository: SqliteRepository,
    csv_parser: CsvParser,
}

impl ImportService {
    pub fn new(repository: SqliteRepository) -> Self {
        Self {
            repository,
            csv_parser: CsvParser::new(),
        }
    }

    /// Import neuron snapshots from a CSV file
    ///
    /// # Arguments
    /// * `file_path` - Path to the CSV file
    /// * `options` - Import options (dry run, etc.)
    ///
    /// # Returns
    /// ImportResult with statistics about the import operation
    pub fn import_snapshots(
        &self,
        file_path: &Path,
        options: ImportOptions,
    ) -> Result<ImportResult, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        let errors = Vec::new();

        // Phase 1: Parse CSV file
        println!("\nParsing CSV file '{}'...", file_path.display());
        let records = self.csv_parser.parse_file(file_path)?;
        println!("✓ Parsed {} records", records.len());

        if records.is_empty() {
            return Err("CSV file contains no data rows".into());
        }

        // Phase 2: Analyze records
        println!("\nAnalyzing records...");
        let analysis = self.analyze_records(&records);

        println!("  Unique neurons: {}", analysis.unique_neurons.len());
        println!("  Date range: {} to {}", analysis.min_date, analysis.max_date);

        // Phase 3: Check for duplicates
        println!("\nChecking for existing snapshots...");
        let (new_records, duplicate_count) = if options.dry_run {
            println!("  (Dry run: skipping duplicate check)");
            (records.clone(), 0)
        } else {
            self.filter_duplicates(&records)?
        };

        println!("  New snapshots: {}", new_records.len());
        println!("  Duplicates:    {}", duplicate_count);

        // Phase 4: Import (or simulate for dry run)
        let inserted_count = if options.dry_run {
            println!("\n[DRY RUN] Would import {} snapshots", new_records.len());
            println!("Run without --dry-run to actually import");
            new_records.len()
        } else {
            println!("\nImporting {} snapshots...", new_records.len());
            self.import_to_database(&new_records)?
        };

        // Phase 5: Calculate daily rewards (if not dry run and snapshots were imported)
        let (rewards_calculated, rewards_skipped) = if options.dry_run || inserted_count == 0 {
            (0, 0)
        } else {
            println!("\nCalculating daily rewards from imported snapshots...");
            self.calculate_daily_rewards()?
        };

        // Phase 6: Calculate portfolio snapshots (if not dry run and snapshots were imported)
        let (portfolio_snapshots_calculated, portfolio_snapshots_skipped) = if options.dry_run || inserted_count == 0 {
            (0, 0)
        } else {
            println!("\nCalculating portfolio snapshots from imported data...");
            self.calculate_portfolio_snapshots()?
        };

        let duration = start_time.elapsed();

        // Build result
        let result = ImportResult {
            total_rows: records.len(),
            new_snapshots: inserted_count,
            duplicates_skipped: duplicate_count,
            rewards_calculated,
            rewards_skipped,
            portfolio_snapshots_calculated,
            portfolio_snapshots_skipped,
            duration_seconds: duration.as_secs_f64(),
            neuron_summary: self.build_neuron_summary(&new_records, rewards_calculated),
            date_range: Some((analysis.min_date, analysis.max_date)),
            errors,
        };

        Ok(result)
    }

    /// Analyze CSV records to gather statistics
    fn analyze_records(&self, records: &[CsvRecord]) -> RecordAnalysis {
        let mut unique_neurons = HashSet::new();
        let mut min_date = records[0].date;
        let mut max_date = records[0].date;

        for record in records {
            unique_neurons.insert(record.neuron_id);
            if record.date < min_date {
                min_date = record.date;
            }
            if record.date > max_date {
                max_date = record.date;
            }
        }

        RecordAnalysis {
            unique_neurons,
            min_date: min_date.to_string(),
            max_date: max_date.to_string(),
        }
    }

    /// Filter out records that already exist in the database
    fn filter_duplicates(
        &self,
        records: &[CsvRecord],
    ) -> Result<(Vec<CsvRecord>, usize), Box<dyn std::error::Error>> {
        let mut new_records = Vec::new();
        let mut duplicate_count = 0;

        for record in records {
            if self.repository.snapshot_exists(record.neuron_id, record.date)? {
                duplicate_count += 1;
            } else {
                new_records.push(record.clone());
            }
        }

        Ok((new_records, duplicate_count))
    }

    /// Import records to the database
    fn import_to_database(
        &self,
        records: &[CsvRecord],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        if records.is_empty() {
            return Ok(0);
        }

        // Show progress for large imports
        if records.len() > 100 {
            println!("  [{}{}] 0%", " ".repeat(50), " ".repeat(0));
        }

        let inserted = self.repository.batch_insert_snapshots(records)?;

        if records.len() > 100 {
            println!("  [{}] 100%", "█".repeat(50));
        }

        println!("✓ Imported {} snapshots successfully", inserted);
        Ok(inserted)
    }

    /// Build a summary of snapshots per neuron
    fn build_neuron_summary(&self, records: &[CsvRecord], total_rewards: usize) -> HashMap<NeuronId, NeuronImportStats> {
        let mut summary: HashMap<NeuronId, NeuronImportStats> = HashMap::new();
        for record in records {
            let stats = summary.entry(record.neuron_id).or_insert(NeuronImportStats {
                snapshots: 0,
                rewards: 0,
            });
            stats.snapshots += 1;
        }

        // Distribute rewards evenly across neurons (rough approximation)
        // In reality, rewards are calculated per neuron separately
        if total_rewards > 0 && !summary.is_empty() {
            let rewards_per_neuron = total_rewards / summary.len();
            for stats in summary.values_mut() {
                stats.rewards = if stats.snapshots > 1 {
                    stats.snapshots.saturating_sub(1).min(rewards_per_neuron)
                } else {
                    0
                };
            }
        }

        summary
    }

    /// Calculate daily rewards from all snapshots in database
    fn calculate_daily_rewards(&self) -> Result<(usize, usize), Box<dyn std::error::Error>> {

        // Get all unique neuron IDs from neuron_snapshots table
        let neuron_ids = self.get_all_neuron_ids()?;

        let mut total_calculated = 0;
        let mut total_skipped = 0;

        for neuron_id in neuron_ids {
            // Get all snapshots for this neuron, sorted by date
            let snapshots = self.get_snapshots_for_neuron(neuron_id)?;

            if snapshots.len() < 2 {
                continue; // Need at least 2 snapshots to calculate rewards
            }

            // Process consecutive pairs
            for i in 0..snapshots.len() - 1 {
                let (prev_neuron, prev_date) = &snapshots[i];
                let (curr_neuron, curr_date) = &snapshots[i + 1];

                // Check if reward already exists
                if self.reward_exists(neuron_id, *curr_date)? {
                    total_skipped += 1;
                    continue;
                }

                // Calculate days elapsed
                let days_elapsed = (*curr_date - *prev_date).num_days();

                // Calculate maturity deltas
                let maturity_delta = curr_neuron.maturity().e8s() as i64 - prev_neuron.maturity().e8s() as i64;
                let staked_maturity_delta = curr_neuron.staked_maturity().e8s() as i64 - prev_neuron.staked_maturity().e8s() as i64;

                // Save reward
                use crate::domain::repositories::DailyRewardRepository;
                self.repository.save_reward(
                    neuron_id,
                    *curr_date,
                    maturity_delta,
                    staked_maturity_delta,
                    days_elapsed,
                )?;

                total_calculated += 1;
            }
        }

        println!("✓ Calculated {} daily rewards ({} duplicates skipped)", total_calculated, total_skipped);
        Ok((total_calculated, total_skipped))
    }

    /// Get all unique neuron IDs from the database
    fn get_all_neuron_ids(&self) -> Result<Vec<NeuronId>, Box<dyn std::error::Error>> {
        self.repository.get_all_neuron_ids()
    }

    /// Get all snapshots for a neuron, sorted by date
    fn get_snapshots_for_neuron(&self, neuron_id: NeuronId) -> Result<Vec<(crate::domain::Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        self.repository.get_all_snapshots_for_neuron(neuron_id)
    }

    /// Check if a reward already exists
    fn reward_exists(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<bool, Box<dyn std::error::Error>> {
        use crate::domain::repositories::DailyRewardRepository;
        Ok(self.repository.get_reward(neuron_id, date)?.is_some())
    }

    /// Calculate portfolio snapshots from all snapshots in database
    fn calculate_portfolio_snapshots(&self) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        use crate::domain::repositories::{NeuronSnapshotRepository, PortfolioSnapshotRepository};

        // Get all unique dates from neuron_snapshots table
        let dates = self.repository.get_all_unique_dates()?;

        let mut total_calculated = 0;
        let mut total_skipped = 0;

        for date in dates {
            // Check if portfolio snapshot already exists
            if self.repository.portfolio_snapshot_exists(date)? {
                total_skipped += 1;
                continue;
            }

            // Get all neurons for this date
            let neurons = self.repository.get_all_snapshots_for_date(date)?;

            if neurons.is_empty() {
                continue; // Skip dates with no neuron data
            }

            // Create portfolio from neurons
            let portfolio = crate::domain::Portfolio::new(neurons);

            // Save portfolio snapshot
            PortfolioSnapshotRepository::save_snapshot(&self.repository, &portfolio, date)?;

            total_calculated += 1;
        }

        println!("✓ Calculated {} portfolio snapshots ({} duplicates skipped)", total_calculated, total_skipped);
        Ok((total_calculated, total_skipped))
    }

    /// Export neuron snapshots to a CSV file
    ///
    /// # Arguments
    /// * `options` - Export options (filters, output path)
    ///
    /// # Returns
    /// ExportResult with statistics about the export operation
    pub fn export_snapshots(
        &self,
        options: ExportOptions,
    ) -> Result<ExportResult, Box<dyn std::error::Error>> {
        let start_time = Instant::now();

        println!("\nPreparing export...");

        // Query database with filters
        let snapshots = self.query_snapshots_for_export(&options)?;

        if snapshots.is_empty() {
            return Err("No snapshots found matching the specified criteria".into());
        }

        println!("  Found {} snapshots to export", snapshots.len());

        // Analyze data
        let mut unique_neurons = HashSet::new();
        let mut min_date: Option<NaiveDate> = None;
        let mut max_date: Option<NaiveDate> = None;

        for row in &snapshots {
            let (neuron_id, date) = (&row.neuron_id, &row.date);
            unique_neurons.insert(*neuron_id);
            match min_date {
                None => min_date = Some(*date),
                Some(d) if *date < d => min_date = Some(*date),
                _ => {}
            }
            match max_date {
                None => max_date = Some(*date),
                Some(d) if *date > d => max_date = Some(*date),
                _ => {}
            }
        }

        println!("  Neurons: {}", unique_neurons.len());
        if let (Some(min), Some(max)) = (min_date, max_date) {
            println!("  Date range: {} to {}", min, max);
        }

        // Write CSV file
        println!("\nWriting CSV file...");
        let file_size = self.write_csv_file(&options.output_path, &snapshots)?;

        println!("✓ Exported to '{}'", options.output_path.display());

        let duration = start_time.elapsed();

        let date_range = match (min_date, max_date) {
            (Some(min), Some(max)) => Some((min.to_string(), max.to_string())),
            _ => None,
        };

        Ok(ExportResult {
            total_snapshots: snapshots.len(),
            neurons_exported: unique_neurons.len(),
            date_range,
            file_path: options.output_path,
            file_size_bytes: file_size,
            duration_seconds: duration.as_secs_f64(),
        })
    }

    /// Query snapshots from database based on export options
    fn query_snapshots_for_export(
        &self,
        options: &ExportOptions,
    ) -> Result<Vec<ExportRow>, Box<dyn std::error::Error>> {
        // Use the repository method to query with filters
        let neuron_ids_slice = options.neuron_ids.as_ref().map(|v| v.as_slice());
        self.repository.query_snapshots_for_export(
            neuron_ids_slice,
            options.start_date,
            options.end_date,
        )
    }

    /// Write snapshots to CSV file with metadata header
    fn write_csv_file(
        &self,
        path: &std::path::Path,
        snapshots: &[ExportRow],
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let mut file = File::create(path)?;

        // Write metadata header
        let now = chrono::Local::now();
        writeln!(file, "# ICP Neuron Tracker - Historical Data Export")?;
        writeln!(file, "# Export Date: {}", now.format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(file, "# Total Records: {}", snapshots.len())?;
        writeln!(file, "# Format Version: 1.1")?;
        writeln!(file, "#")?;
        writeln!(file, "# retrieved_timestamp_seconds is when each snapshot was observed. It is")?;
        writeln!(file, "# what distinguishes automated collection from bulk import, and it is")?;
        writeln!(file, "# preserved on re-import. Added in format 1.1; files written by 1.0 lack")?;
        writeln!(file, "# it and will be re-imported with the import time instead.")?;
        writeln!(file, "#")?;
        writeln!(file, "# state and auto_stake_enabled are NOT exported. On import they are")?;
        writeln!(file, "# inferred: state from the dissolve delay, auto-stake from whether any")?;
        writeln!(file, "# staked maturity is present. A round-trip preserves them only when those")?;
        writeln!(file, "# inferences happen to be right.")?;
        writeln!(file, "#")?;

        // Write CSV header (FS-003 compliant format, extended with the provenance column)
        writeln!(file, "neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds,retrieved_timestamp_seconds")?;

        // Write data rows
        for row in snapshots {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{}",
                row.neuron_id,
                row.date.format("%Y-%m-%d"),
                row.stake_e8s,
                row.staked_maturity_e8s,
                row.available_maturity_e8s,
                row.voting_power,
                row.dissolve_delay_seconds,
                row.age_seconds,
                row.created_timestamp_seconds,
                row.retrieved_timestamp_seconds
            )?;
        }

        // Get file size
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }
}

struct RecordAnalysis {
    unique_neurons: HashSet<NeuronId>,
    min_date: String,
    max_date: String,
}
