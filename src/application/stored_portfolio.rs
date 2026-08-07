use crate::domain::Portfolio;
use crate::infrastructure::SqliteRepository;
use chrono::{DateTime, NaiveDate, Utc};

/// A portfolio reconstructed from the newest stored snapshot of each tracked neuron,
/// together with when those snapshots were actually observed.
///
/// The observation time is carried alongside the data rather than left implicit: a portfolio
/// built from stored snapshots is only as current as the last time the tracker ran, and any
/// figure derived from it inherits that staleness.
pub struct StoredPortfolio {
    pub portfolio: Portfolio,
    /// Most recent `retrieved_at` across the neurons, i.e. when this data was observed.
    /// `None` only if the portfolio is empty.
    pub retrieved_at: Option<DateTime<Utc>>,
    /// Most recent snapshot date across the neurons.
    pub snapshot_date: Option<NaiveDate>,
}

impl StoredPortfolio {
    /// Whole days between the observation and `now`. `None` if the portfolio is empty.
    pub fn age_days(&self, now: DateTime<Utc>) -> Option<i64> {
        self.retrieved_at.map(|r| (now - r).num_days())
    }
}

/// Build a portfolio from the latest stored snapshot for every tracked neuron.
///
/// Shared by `ReportService::generate_summary_report` and the offline path of the `project`
/// command so the two cannot drift apart.
pub fn load_stored_portfolio(
    repository: &SqliteRepository,
) -> Result<StoredPortfolio, Box<dyn std::error::Error>> {
    let snapshots = repository.get_all_latest_snapshots()?;

    if snapshots.is_empty() {
        return Err(
            "No neuron snapshots found. Run `icp-neuron-tracker track` to collect one.".into(),
        );
    }

    let retrieved_at = snapshots.iter().map(|(n, _)| n.retrieved_at()).max();
    let snapshot_date = snapshots.iter().map(|(_, d)| *d).max();
    let neurons: Vec<_> = snapshots.into_iter().map(|(neuron, _date)| neuron).collect();

    Ok(StoredPortfolio {
        portfolio: Portfolio::new(neurons),
        retrieved_at,
        snapshot_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::repositories::NeuronSnapshotRepository;
    use crate::domain::{IcpAmount, Neuron, NeuronId, NeuronState};
    use tempfile::TempDir;

    fn temp_repo() -> (TempDir, SqliteRepository) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("test.db");
        let repo = SqliteRepository::new(path.to_str().unwrap()).expect("open repo");
        (dir, repo)
    }

    fn neuron_observed_at(id: u64, stake_e8s: u64, retrieved_ts: u64) -> Neuron {
        Neuron::from_snapshot(
            NeuronId::new(id),
            IcpAmount::from_e8s(stake_e8s),
            IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(0),
            600_000_000_000,
            126_230_400,
            252_460_800,
            NeuronState::Locked,
            true,
            1_621_209_600,
            retrieved_ts,
        )
    }

    /// The offline path must produce a usable portfolio from stored snapshots alone —
    /// this is what makes a projection possible without the network.
    #[test]
    fn loads_a_portfolio_from_stored_snapshots_without_a_network() {
        let (_dir, repo) = temp_repo();
        let date = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
        let ts = 1_767_816_305; // 2026-01-07 UTC

        repo.save_snapshot(&neuron_observed_at(111, 100_000_000_000, ts), date).unwrap();
        repo.save_snapshot(&neuron_observed_at(222, 50_000_000_000, ts), date).unwrap();

        let stored = load_stored_portfolio(&repo).expect("load");

        assert_eq!(stored.portfolio.neuron_count(), 2);
        assert_eq!(stored.portfolio.total_value().e8s(), 150_000_000_000);
        assert_eq!(stored.snapshot_date, Some(date));
        assert_eq!(stored.retrieved_at.unwrap().timestamp(), ts as i64);
    }

    /// The banner's age figure comes from the real observation time, so it has to be the
    /// NEWEST across neurons, not an arbitrary one.
    #[test]
    fn reports_the_newest_observation_time_across_neurons() {
        let (_dir, repo) = temp_repo();
        let date = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
        let older = 1_762_487_131; // 2025-11-07
        let newer = 1_767_816_305; // 2026-01-07

        repo.save_snapshot(&neuron_observed_at(111, 1, older), date).unwrap();
        repo.save_snapshot(&neuron_observed_at(222, 1, newer), date).unwrap();

        let stored = load_stored_portfolio(&repo).expect("load");
        assert_eq!(stored.retrieved_at.unwrap().timestamp(), newer as i64);
    }

    /// Staleness in days is what the banner prints; it must be derived, not assumed.
    #[test]
    fn computes_staleness_in_days_from_the_observation_time() {
        let (_dir, repo) = temp_repo();
        let date = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
        let ts = 1_767_816_305; // 2026-01-07 20:05:05 UTC
        repo.save_snapshot(&neuron_observed_at(111, 1, ts), date).unwrap();

        let stored = load_stored_portfolio(&repo).expect("load");
        // 2026-08-05 00:00:00 UTC
        let now = DateTime::from_timestamp(1_785_888_000, 0).unwrap();
        assert_eq!(stored.age_days(now), Some(209));
    }

    /// An empty database must fail with a message that says what to do, not panic — this is
    /// the one case where the offline path genuinely cannot answer.
    #[test]
    fn errors_clearly_when_there_are_no_stored_snapshots() {
        let (_dir, repo) = temp_repo();
        let err = match load_stored_portfolio(&repo) {
            Ok(_) => panic!("expected an error for an empty database"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("No neuron snapshots found"), "got: {}", err);
    }
}
