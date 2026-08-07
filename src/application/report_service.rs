use crate::domain::{PortfolioReport, HistoricalTrend, IcpAmount, RewardAnalysis, NeuronReward, NeuronDetail, RewardHistory, DissolveStatus, NeuronId, NeuronState};
use crate::domain::repositories::{NeuronSnapshotRepository, DailyRewardRepository};
use crate::infrastructure::SqliteRepository;
use chrono::Utc;

/// Service for generating portfolio reports
pub struct ReportService {
    repository: SqliteRepository,
}

impl ReportService {
    pub fn new(repository: SqliteRepository) -> Self {
        Self { repository }
    }

    /// Generate portfolio summary report from latest snapshot data
    ///
    /// This method fetches the most recent snapshot for each tracked neuron
    /// and builds a comprehensive portfolio report with aggregated metrics.
    ///
    /// # Returns
    /// - `Ok(PortfolioReport)` - Report with current portfolio state and metrics
    /// - `Err` - If database query fails or no neurons are tracked
    ///
    /// # Example
    /// ```no_run
    /// use icp_neuron_tracker::application::ReportService;
    /// use icp_neuron_tracker::infrastructure::SqliteRepository;
    ///
    /// let repo = SqliteRepository::new("data/tracker.db")?;
    /// let service = ReportService::new(repo);
    /// let report = service.generate_summary_report()?;
    /// println!("Total neurons: {}", report.metrics.neuron_count);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn generate_summary_report(&self) -> Result<PortfolioReport, Box<dyn std::error::Error>> {
        // Shared with the offline path of the `project` command so the two cannot drift.
        let stored = crate::application::load_stored_portfolio(&self.repository)?;

        Ok(PortfolioReport::new(stored.portfolio))
    }

    /// Generate historical trend report for maturity growth over time
    ///
    /// # Arguments
    /// * `days` - Number of days to look back (e.g., 7, 30, 90)
    ///
    /// # Returns
    /// - `Ok(HistoricalTrend)` - Trend analysis with growth metrics
    /// - `Err` - If insufficient data or database error
    ///
    /// # Example
    /// ```no_run
    /// use icp_neuron_tracker::application::ReportService;
    /// use icp_neuron_tracker::infrastructure::SqliteRepository;
    ///
    /// let repo = SqliteRepository::new("data/tracker.db")?;
    /// let service = ReportService::new(repo);
    /// let trend = service.generate_historical_report(30)?;
    /// println!("Growth: {} ICP", trend.maturity_delta().to_icp());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn generate_historical_report(&self, days: u32) -> Result<HistoricalTrend, Box<dyn std::error::Error>> {
        if days == 0 {
            return Err("Days must be at least 1".into());
        }

        // Calculate date range
        let end_date = Utc::now().date_naive();
        let start_date = end_date - chrono::Duration::days(days as i64);

        // Get all snapshots for each neuron in the range
        let all_neuron_ids = self.repository.get_all_neuron_ids()?;

        if all_neuron_ids.is_empty() {
            return Err("No tracked neurons found. Run `icp-neuron-tracker track` to collect a snapshot.".into());
        }

        // Collect maturity totals for each date that has data
        let mut date_maturity_map = std::collections::HashMap::new();

        for neuron_id in &all_neuron_ids {
            let snapshots = (&self.repository).get_snapshots_range(*neuron_id, start_date, end_date)?;

            for (neuron, date) in snapshots {
                let total_maturity = neuron.maturity() + neuron.staked_maturity();
                *date_maturity_map.entry(date).or_insert(IcpAmount::from_e8s(0)) =
                    date_maturity_map.get(&date).copied().unwrap_or(IcpAmount::from_e8s(0)) + total_maturity;
            }
        }

        if date_maturity_map.is_empty() {
            return Err(format!(
                "No snapshot data found in the last {} days. Run `icp-neuron-tracker track` to collect data.",
                days
            ).into());
        }

        // Find actual start and end dates with data
        let mut dates: Vec<_> = date_maturity_map.keys().copied().collect();
        dates.sort();

        let actual_start_date = *dates.first().unwrap();
        let actual_end_date = *dates.last().unwrap();

        // Get start and end maturity
        let start_maturity = *date_maturity_map.get(&actual_start_date).unwrap();
        let end_maturity = *date_maturity_map.get(&actual_end_date).unwrap();

        // Count days with data
        let days_with_data = date_maturity_map.len() as i64;

        Ok(HistoricalTrend::new(
            actual_start_date,
            actual_end_date,
            start_maturity,
            end_maturity,
            days_with_data,
        ))
    }

    /// Generate reward analysis report showing neuron performance rankings
    ///
    /// # Arguments
    /// * `days` - Number of days to analyze (e.g., 7, 30, 90)
    ///
    /// # Returns
    /// - `Ok(RewardAnalysis)` - Ranked reward analysis with projections
    /// - `Err` - If insufficient data or database error
    ///
    /// # Example
    /// ```no_run
    /// use icp_neuron_tracker::application::ReportService;
    /// use icp_neuron_tracker::infrastructure::SqliteRepository;
    ///
    /// let repo = SqliteRepository::new("data/tracker.db")?;
    /// let service = ReportService::new(repo);
    /// let analysis = service.generate_reward_analysis(30)?;
    /// println!("Top earner: {:?}", analysis.neuron_rewards.first());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn generate_reward_analysis(&self, days: u32) -> Result<RewardAnalysis, Box<dyn std::error::Error>> {
        if days == 0 {
            return Err("Days must be at least 1".into());
        }

        // Calculate date range
        let end_date = Utc::now().date_naive();
        let start_date = end_date - chrono::Duration::days(days as i64);

        // Get all neuron IDs
        let all_neuron_ids = self.repository.get_all_neuron_ids()?;

        if all_neuron_ids.is_empty() {
            return Err("No tracked neurons found. Run `icp-neuron-tracker track` to collect a snapshot.".into());
        }

        let mut neuron_rewards = Vec::new();
        let mut total_snapshot_count = 0;

        for neuron_id in &all_neuron_ids {
            // Get rewards for this neuron in the period
            let rewards = (&self.repository).get_rewards_range(*neuron_id, start_date, end_date)?;

            // Track max snapshot count across all neurons
            if rewards.len() > total_snapshot_count {
                total_snapshot_count = rewards.len();
            }

            // Sum up total rewards
            let total_reward = rewards.iter()
                .map(|r| IcpAmount::from_e8s(r.total_reward_e8s as u64))
                .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt);

            // Get latest snapshot for neuron configuration
            let latest_snapshot = (&self.repository).get_latest_snapshot(*neuron_id)?;

            if let Some((neuron, _date)) = latest_snapshot {
                let neuron_reward = NeuronReward::new(
                    *neuron_id,
                    total_reward,
                    rewards.len() as i64,
                    neuron.stake(),
                    neuron.dissolve_delay_days(),
                    neuron.age_days(),
                    neuron.age_bonus(),
                    neuron.dissolve_bonus(),
                );

                neuron_rewards.push(neuron_reward);
            }
        }

        if neuron_rewards.is_empty() {
            return Err(format!(
                "No reward data found in the last {} days. Run `icp-neuron-tracker track` to collect data.",
                days
            ).into());
        }

        // Distinguish "no data in this window" from "data exists and no rewards were earned".
        //
        // These have opposite remedies and the report previously conflated them: with an
        // empty window it still built a full ranking of every neuron at 0.00 ICP and printed
        // "WARNING: No rewards earned (check configuration)" against each — telling the user
        // to fix a configuration that was correct. The real cause is that no snapshot exists
        // in the requested period.
        if total_snapshot_count == 0 {
            let newest = <&SqliteRepository as DailyRewardRepository>::get_newest_reward_date(
                &&self.repository,
            )?;
            return Err(match newest {
                Some(d) => {
                    let age = (chrono::Utc::now().naive_utc().date() - d).num_days().max(0);
                    format!(
                        "No snapshot data in the last {} days, so no rewards can be calculated \
                         for that window.\n\n\
                         This is NOT a neuron configuration problem. The newest reward data is \
                         from {} ({} days ago).\n\n\
                         Either run `icp-neuron-tracker track` to collect current data, or widen \
                         the window, e.g. `report rewards --days {}`.",
                        days, d, age, (age + 7).max(days as i64 + 1)
                    )
                }
                None => format!(
                    "No reward data exists at all, so nothing can be reported for the last {} \
                     days. Run `icp-neuron-tracker track` on at least two separate days — \
                     rewards are calculated as the change between consecutive snapshots.",
                    days
                ),
            }
            .into());
        }

        Ok(RewardAnalysis::new(
            neuron_rewards,
            days as i64,
            start_date,
            end_date,
            total_snapshot_count,
        ))
    }

    /// Generate detailed report for a single neuron
    ///
    /// This method fetches comprehensive statistics for a specific neuron including:
    /// - Current state (stake, maturity, voting power, bonuses)
    /// - Reward history (7-day and 30-day)
    /// - Dissolve status
    ///
    /// # Arguments
    /// * `neuron_id` - ID of the neuron to analyze
    ///
    /// # Returns
    /// - `Ok(NeuronDetail)` - Detailed neuron information
    /// - `Err` - If neuron not found or database query fails
    ///
    /// # Example
    /// ```no_run
    /// use icp_neuron_tracker::application::ReportService;
    /// use icp_neuron_tracker::infrastructure::SqliteRepository;
    /// use icp_neuron_tracker::domain::NeuronId;
    ///
    /// let repo = SqliteRepository::new("data/tracker.db")?;
    /// let service = ReportService::new(repo);
    /// let detail = service.generate_neuron_detail(NeuronId::new(12345))?;
    /// println!("Neuron stake: {} ICP", detail.stake.as_icp());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn generate_neuron_detail(&self, neuron_id: NeuronId) -> Result<NeuronDetail, Box<dyn std::error::Error>> {
        // Get latest snapshot for this neuron
        let snapshot = self.repository.get_latest_snapshot(neuron_id)?;

        let (neuron, _snapshot_date) = snapshot
            .ok_or_else(|| format!("Neuron {} not found. Has it been tracked?", neuron_id))?;

        // Determine dissolve status
        let dissolve_status = match neuron.state() {
            NeuronState::Dissolved => DissolveStatus::Dissolved,
            NeuronState::Dissolving => DissolveStatus::Dissolving {
                days_remaining: neuron.dissolve_delay_days(),
            },
            NeuronState::Locked => DissolveStatus::Locked,
        };

        // Get reward history for 7 and 30 days
        let reward_history = self.get_reward_history(neuron_id)?;

        Ok(NeuronDetail::new(neuron, dissolve_status, reward_history))
    }

    /// Helper to get reward history for 7 and 30 day periods
    fn get_reward_history(&self, neuron_id: NeuronId) -> Result<Option<RewardHistory>, Box<dyn std::error::Error>> {
        let now = Utc::now().date_naive();

        // Get 7-day rewards
        let start_7 = now - chrono::Duration::days(7);
        let rewards_7 = (&self.repository).get_rewards_range(neuron_id, start_7, now)?;

        // Get 30-day rewards
        let start_30 = now - chrono::Duration::days(30);
        let rewards_30 = (&self.repository).get_rewards_range(neuron_id, start_30, now)?;

        // If no reward data at all, return None
        if rewards_7.is_empty() && rewards_30.is_empty() {
            return Ok(None);
        }

        // Calculate totals
        let total_7 = rewards_7.iter()
            .map(|r| IcpAmount::from_e8s(r.total_reward_e8s as u64))
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt);

        let total_30 = rewards_30.iter()
            .map(|r| IcpAmount::from_e8s(r.total_reward_e8s as u64))
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt);

        // Count actual days with data (not just calendar days)
        let days_7_actual = rewards_7.len() as i64;
        let days_30_actual = rewards_30.len() as i64;

        Ok(Some(RewardHistory::new(total_7, total_30, days_7_actual, days_30_actual)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Neuron, NeuronId, IcpAmount, NeuronState};
    use crate::domain::repositories::NeuronSnapshotRepository;
    use chrono::Utc;
    use tempfile::tempdir;

    fn create_test_neuron(id: u64, stake_icp: u64, maturity_icp: u64) -> Neuron {
        Neuron::new(
            NeuronId::new(id),
            IcpAmount::from_e8s(stake_icp * 100_000_000),
            IcpAmount::from_e8s(maturity_icp * 100_000_000),
            IcpAmount::from_e8s(0),
            1_000_000,
            365 * 86400, // 1 year age
            730 * 86400, // 2 years dissolve delay
            NeuronState::Locked,
            true,
            Utc::now().timestamp() as u64,
        )
    }

    #[test]
    fn test_generate_summary_report_with_data() {
        // Create temporary database
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let repo = SqliteRepository::new(db_path.to_str().unwrap()).unwrap();

        // Insert test data
        let neuron1 = create_test_neuron(123, 1000, 10);
        let neuron2 = create_test_neuron(456, 2000, 20);

        let today = chrono::Utc::now().date_naive();
        repo.save_snapshot(&neuron1, today).unwrap();
        repo.save_snapshot(&neuron2, today).unwrap();

        // Generate report
        let service = ReportService::new(repo);
        let report = service.generate_summary_report().unwrap();

        // Verify metrics
        assert_eq!(report.metrics.neuron_count, 2);
        assert_eq!(report.metrics.total_stake.to_icp(), 3000.0);
        assert_eq!(report.metrics.total_maturity.to_icp(), 30.0);
        assert!(report.metrics.dissolve_delay_range.is_some());
        assert!(report.metrics.age_range.is_some());
    }

    #[test]
    fn test_generate_summary_report_empty_database() {
        // Create temporary database with no data
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("empty.db");
        let repo = SqliteRepository::new(db_path.to_str().unwrap()).unwrap();

        let service = ReportService::new(repo);
        let result = service.generate_summary_report();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No neuron snapshots found"));
    }

    #[test]
    fn test_generate_summary_report_uses_latest_data() {
        // Create temporary database
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("latest.db");
        let repo = SqliteRepository::new(db_path.to_str().unwrap()).unwrap();

        // Insert historical data
        let neuron_old = create_test_neuron(123, 1000, 5);
        let neuron_new = create_test_neuron(123, 1000, 15); // Same ID, more maturity

        let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);
        let today = chrono::Utc::now().date_naive();

        repo.save_snapshot(&neuron_old, yesterday).unwrap();
        repo.save_snapshot(&neuron_new, today).unwrap();

        // Generate report
        let service = ReportService::new(repo);
        let report = service.generate_summary_report().unwrap();

        // Should use today's data (15 ICP maturity, not 5)
        assert_eq!(report.metrics.neuron_count, 1);
        assert_eq!(report.metrics.total_maturity.to_icp(), 15.0);
    }
}

#[cfg(test)]
mod reward_message_tests {
    use super::*;
    use crate::domain::repositories::NeuronSnapshotRepository;
    use crate::domain::{IcpAmount, Neuron, NeuronId, NeuronState};
    use crate::infrastructure::TerminalReportFormatter;
    use chrono::{Duration, NaiveDate, Utc};
    use tempfile::TempDir;

    fn temp_service() -> (TempDir, SqliteRepository) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("t.db");
        let repo = SqliteRepository::new(path.to_str().unwrap()).expect("repo");
        (dir, repo)
    }

    fn neuron(id: u64, stake_e8s: u64, staked_maturity_e8s: u64) -> Neuron {
        Neuron::new(
            NeuronId::new(id),
            IcpAmount::from_e8s(stake_e8s),
            IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(staked_maturity_e8s),
            600_000_000_000,
            126_230_400,
            252_460_800,
            NeuronState::Locked,
            true,
            1_621_209_600,
        )
    }

    /// PATH 1 — data exists, but all of it is outside the requested window.
    ///
    /// This is the misdiagnosis this change exists to fix. The report previously built a
    /// full ranking at 0.00 ICP and printed "check configuration" against every neuron,
    /// blaming a configuration that was correct. The real cause is an empty window.
    #[test]
    fn no_data_in_window_says_so_and_does_not_blame_configuration() {
        let (_d, repo) = temp_service();
        let old = Utc::now().naive_utc().date() - Duration::days(200);
        repo.save_snapshot(&neuron(111, 100_000_000_000, 1_000_000_000), old).unwrap();
        repo.save_snapshot(
            &neuron(111, 100_000_000_000, 2_000_000_000),
            old + Duration::days(1),
        )
        .unwrap();
        // Rewards exist, but only outside the 30-day window.
        DailyRewardRepository::save_reward(
            &repo, NeuronId::new(111), old + Duration::days(1), 0, 1_000_000_000, 1,
        ).unwrap();

        let service = ReportService::new(repo);
        let err = service
            .generate_reward_analysis(30)
            .expect_err("an empty window must be an error, not a table of zeros")
            .to_string();

        assert!(err.contains("No snapshot data in the last 30 days"), "got: {}", err);
        assert!(
            err.contains("NOT a neuron configuration problem"),
            "the message must rule out the wrong cause explicitly: {}",
            err
        );
        assert!(err.contains("track"), "must say how to fix it: {}", err);
    }

    /// The empty-window message must name the newest data it does have, so the user can
    /// judge how far back to widen.
    #[test]
    fn no_data_in_window_reports_the_newest_available_date() {
        let (_d, repo) = temp_service();
        let old = NaiveDate::from_ymd_opt(2026, 1, 6).unwrap();
        let newest = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
        repo.save_snapshot(&neuron(111, 100_000_000_000, 1_000_000_000), old).unwrap();
        repo.save_snapshot(&neuron(111, 100_000_000_000, 2_000_000_000), newest).unwrap();
        DailyRewardRepository::save_reward(
            &repo, NeuronId::new(111), newest, 0, 1_000_000_000, 1,
        ).unwrap();

        let service = ReportService::new(repo);
        let err = service.generate_reward_analysis(30).unwrap_err().to_string();

        assert!(err.contains("2026-01-07"), "must name the newest reward date: {}", err);
        assert!(err.contains("days ago"), "must quantify the staleness: {}", err);
    }

    /// PATH 2 — data exists IN the window and rewards really are zero. Here "check
    /// configuration" is the correct diagnosis and must still appear.
    #[test]
    fn data_in_window_with_zero_rewards_still_points_at_configuration() {
        let (_d, repo) = temp_service();
        let today = Utc::now().naive_utc().date();
        // Two consecutive days with IDENTICAL maturity: real data, genuinely no reward.
        for back in [2_i64, 1] {
            repo.save_snapshot(
                &neuron(111, 100_000_000_000, 5_000_000_000),
                today - Duration::days(back),
            )
            .unwrap();
        }
        // A reward row inside the window whose deltas are zero — observed, and zero.
        DailyRewardRepository::save_reward(
            &repo, NeuronId::new(111), today - Duration::days(1), 0, 0, 1,
        ).unwrap();

        let service = ReportService::new(repo);
        let analysis = service
            .generate_reward_analysis(30)
            .expect("data exists in the window, so this must succeed");

        assert_eq!(analysis.neuron_rewards.len(), 1);
        let nr = &analysis.neuron_rewards[0];
        assert!(nr.has_zero_rewards(), "maturity did not change, so reward is zero");
        assert!(nr.days_tracked > 0, "but the neuron DID have observed days");

        let out = TerminalReportFormatter::format_rewards(&analysis, 30);
        assert!(
            out.contains("check configuration"),
            "with data present, configuration IS the right thing to question: {}",
            out
        );
        assert!(
            out.contains("earned nothing despite having data"),
            "portfolio line must distinguish this case: {}",
            out
        );
    }

    /// No rewards anywhere at all — a distinct third case, and the advice differs again:
    /// rewards need two snapshots to exist.
    #[test]
    fn no_reward_data_at_all_explains_that_two_snapshots_are_needed() {
        let (_d, repo) = temp_service();
        let today = Utc::now().naive_utc().date();
        repo.save_snapshot(&neuron(111, 100_000_000_000, 1_000_000_000), today).unwrap();

        let service = ReportService::new(repo);
        let err = service.generate_reward_analysis(30).unwrap_err().to_string();

        assert!(
            err.contains("two separate days") || err.contains("No reward data"),
            "a single snapshot yields no rewards; say why: {}",
            err
        );
    }
}
