use super::neuron::Neuron;
use super::portfolio::Portfolio;
use super::retirement::WindowSample;
use super::value_objects::NeuronId;
use chrono::NaiveDate;

/// Repository for neuron snapshots
#[allow(dead_code)]
pub trait NeuronSnapshotRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>>;
    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<Neuron>, Box<dyn std::error::Error>>;
    fn get_latest_snapshot(&self, neuron_id: NeuronId) -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
    fn get_previous_snapshot(&self, neuron_id: NeuronId, before_date: NaiveDate) -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
    fn get_snapshots_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) -> Result<Vec<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
    fn get_all_snapshots_for_date(&self, date: NaiveDate) -> Result<Vec<Neuron>, Box<dyn std::error::Error>>;
}

/// Repository for portfolio snapshots
#[allow(dead_code)]
pub trait PortfolioSnapshotRepository {
    fn save_snapshot(&self, portfolio: &Portfolio, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>>;
    fn get_snapshot(&self, date: NaiveDate) -> Result<Option<Portfolio>, Box<dyn std::error::Error>>;
    fn get_latest_snapshot(&self) -> Result<Option<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>>;
    fn get_snapshots_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>>;
}

/// Repository for daily reward calculations
#[allow(dead_code)]
pub trait DailyRewardRepository {
    fn save_reward(&self, neuron_id: NeuronId, date: NaiveDate, maturity_delta: i64, staked_maturity_delta: i64, days_elapsed: i64) -> Result<(), Box<dyn std::error::Error>>;
    fn get_reward(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<DailyReward>, Box<dyn std::error::Error>>;
    fn get_rewards_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyReward>, Box<dyn std::error::Error>>;
    /// Mean daily reward for a neuron over its most recent `records` reward rows.
    ///
    /// The rate is total rewards divided by the days those rows account for — the sum of
    /// their `days_elapsed`, not the number of rows and not a fixed 30. A row covering a
    /// multi-day accrual contributes all of the days it covers, so batched and caught-up
    /// rewards are spread over the period they actually accrued in.
    ///
    /// `records` therefore bounds how many rows are read, not how many days they span: 30
    /// rows spanned 247 days in the production database. The returned window reports the
    /// span and divisor so callers can state what the figure covers rather than assuming.
    fn get_average_daily_reward_window(&self, neuron_id: NeuronId, records: i64) -> Result<Option<DailyAverageWindow>, Box<dyn std::error::Error>>;

    /// Get the number of days of reward data available for a neuron
    fn get_reward_data_count(&self, neuron_id: NeuronId) -> Result<i64, Box<dyn std::error::Error>>;

    /// Get portfolio-wide average daily reward (sum across all neurons)
    fn get_portfolio_average_daily_reward(&self, days: i64) -> Result<Option<f64>, Box<dyn std::error::Error>>;

    /// Get the count of days with reward data across the entire portfolio
    fn get_portfolio_reward_data_count(&self) -> Result<i64, Box<dyn std::error::Error>>;

    /// Slice reward history into NON-OVERLAPPING windows, newest first.
    ///
    /// A window is *populated* only when every day in it is covered by a reward row, and it
    /// yields the portfolio's total across the window divided by `window_days`. A window with
    /// any uncovered day is *unobservable* and is excluded from the sample — it is not
    /// zero-filled. An absent day is a day nothing was recorded for, which is not the same
    /// claim as a day that paid nothing, and only the latter belongs in a yield distribution.
    ///
    /// A row whose `days_elapsed` exceeds `window_days` is an accrual gap: it fixes a total
    /// over a span it gives no within-span resolution for. Such a row contributes neither
    /// coverage nor reward to any window, so the windows across its span go unobservable.
    ///
    /// Only windows wholly inside both the lookback and the available data are considered, so
    /// a partial trailing window cannot drag the distribution down.
    fn get_portfolio_window_sample(
        &self,
        lookback_days: i64,
        window_days: i64,
    ) -> Result<WindowSample, Box<dyn std::error::Error>>;

    /// Most recent date for which any reward was recorded, across the portfolio.
    ///
    /// This is the freshness input. `get_portfolio_reward_data_count` counts how many days
    /// exist; this says when the newest of them was, which the count cannot express.
    fn get_newest_reward_date(&self) -> Result<Option<NaiveDate>, Box<dyn std::error::Error>>;
}

/// One row of a CSV export.
///
/// A struct rather than a tuple because it carries ten fields and the two timestamps are
/// easy to transpose: `created_timestamp_seconds` is when the neuron was created on chain,
/// `retrieved_timestamp_seconds` is when this observation was taken.
#[derive(Debug, Clone, Copy)]
pub struct ExportRow {
    pub neuron_id: NeuronId,
    pub date: NaiveDate,
    pub stake_e8s: u64,
    pub staked_maturity_e8s: u64,
    pub available_maturity_e8s: u64,
    pub voting_power: u64,
    pub dissolve_delay_seconds: u64,
    pub age_seconds: u64,
    pub created_timestamp_seconds: u64,
    /// When the snapshot was observed. The provenance marker that distinguishes automated
    /// collection from bulk import; an export that omits it cannot round-trip.
    pub retrieved_timestamp_seconds: u64,
}

/// A mean daily reward rate together with the evidence it was computed from.
///
/// Carried so output can state what the number covers. The rate alone invites a label like
/// "30-day average" for a figure whose divisor was 247 days.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DailyAverageWindow {
    /// Total rewards divided by `days_covered`, in ICP.
    pub icp_per_day: f64,
    /// Reward rows read.
    pub records: i64,
    /// Days those rows account for: the sum of their `days_elapsed`. This is the divisor.
    pub days_covered: i64,
    /// Reward date of the oldest row read.
    pub first_date: NaiveDate,
    /// Reward date of the newest row read.
    pub last_date: NaiveDate,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DailyReward {
    pub neuron_id: NeuronId,
    pub date: NaiveDate,
    pub maturity_delta_e8s: i64,
    pub staked_maturity_delta_e8s: i64,
    pub total_reward_e8s: i64,
    pub days_elapsed: i64,
}

#[allow(dead_code)]
impl DailyReward {
    pub fn daily_rate_icp(&self) -> f64 {
        if self.days_elapsed == 0 {
            return 0.0;
        }
        (self.total_reward_e8s as f64 / self.days_elapsed as f64) / 100_000_000.0
    }
}