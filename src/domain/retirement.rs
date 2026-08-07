use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use super::value_objects::IcpAmount;

/// Value object representing target daily income in ICP
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TargetIncome(f64);

impl TargetIncome {
    pub fn new(icp_per_day: f64) -> Result<Self, &'static str> {
        if icp_per_day <= 0.0 {
            return Err("Target income must be positive");
        }
        if icp_per_day > 10_000.0 {
            return Err("Target income must be reasonable (< 10,000 ICP/day)");
        }
        Ok(Self(icp_per_day))
    }

    pub fn icp_per_day(&self) -> f64 {
        self.0
    }
}

/// Represents a projection timeline showing when retirement is feasible
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionTimeline {
    pub years_until_retirement: f64,
    pub retirement_date: NaiveDate,
    pub required_portfolio_size: IcpAmount,
}

impl ProjectionTimeline {
    pub fn new(
        years_until_retirement: f64,
        retirement_date: NaiveDate,
        required_portfolio_size: IcpAmount,
    ) -> Self {
        Self {
            years_until_retirement,
            retirement_date,
            required_portfolio_size,
        }
    }
}

/// Enum representing different projection scenarios (optimistic, realistic, pessimistic)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProjectionScenario {
    Optimistic(ProjectionTimeline),
    Realistic(ProjectionTimeline),
    Pessimistic(ProjectionTimeline),
}


/// Percentile bands over non-overlapping reward windows, with the sample size behind them.
///
/// # Why non-overlapping
///
/// The earlier implementation took the most recent N reward dates and computed a trailing
/// 7-day rolling rate for each, so every day's reward appeared in up to seven windows. The
/// resulting samples were heavily autocorrelated and the distribution artificially tight:
/// on real data, non-overlapping windows spread 1.81x from p10 to p90 where overlapping
/// windows spread only 1.50x, and the overlapping p10 sat 19% high. That made the
/// pessimistic scenario less pessimistic than the evidence supports — the wrong direction
/// for a retirement estimate to err.
///
/// # Why a window at all
///
/// Rewards accrue only on days when proposals settle: 20.6% of dates in a real three-year
/// history carry a portfolio-wide reward of exactly zero. A raw daily distribution
/// therefore has a 10th percentile of 0.0000, which would project "never". Excluding
/// zero-days is worse — it discards every low observation and leaves a distribution of
/// payout sizes rather than yields, pushing p10 *above* the windowed estimate. A window
/// absorbs the cadence without modelling it.
///
/// Seven days is chosen because it spans the observed settlement rhythm, not because it is
/// derived from any published NNS reward frequency. It is a span, not a model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RewardPercentiles {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    /// Number of non-overlapping windows the bands were computed from.
    pub window_count: usize,
    pub window_days: i64,
    pub lookback_days: i64,
}

/// A single reward row whose `days_elapsed` exceeds one window: a collection gap.
///
/// The row is a true observation of its *total*, but it carries no within-span resolution.
/// 545 ICP booked to 2026-08-06 with `days_elapsed = 211` says what accrued between
/// 2026-01-08 and 2026-08-06 and nothing whatsoever about any 7-day slice of it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccrualGap {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub days: i64,
    pub total_icp: f64,
}

/// The outcome of slicing reward history into non-overlapping windows.
///
/// The distinction this type exists to carry is between *a window that observed no reward*
/// and *a window nothing was observed in*. Collapsing the two — which a bare `Vec<f64>` of
/// zero-filled rates does — turns a collection gap into evidence of zero income, and a
/// pessimistic scenario built on it projects "never" from an absence of data.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSample {
    /// Daily rates for populated windows only: every day in the window is covered by a
    /// reward row. A genuine zero-reward week is populated and appears here as `0.0`.
    pub rates: Vec<f64>,
    /// Windows inside the lookback with at least one day no reward row covers. Never
    /// zero-filled into `rates` — an unobserved week is not a zero week.
    pub unobservable: usize,
    /// How many of `rates` are genuine zero-reward windows. Reported so the caller can say
    /// "rewards were observed to be zero" rather than "no rewards were observed".
    pub zero_reward: usize,
    /// Longest accrual gap overlapping the lookback, if any. Explains the exclusions.
    pub longest_gap: Option<AccrualGap>,
}

impl WindowSample {
    /// Windows the bands may legitimately be computed from.
    pub fn populated(&self) -> usize {
        self.rates.len()
    }
}

impl RewardPercentiles {
    /// Fewest windows for which a 10th percentile is a real order statistic.
    ///
    /// The percentile index is `ceil(n * p) - 1`, so for p=0.10 the index is 0 — the
    /// minimum observation — for every n up to and including 10. Only at n = 11 does p10
    /// gain an observation beneath it. Below that the "10th percentile" label is false:
    /// it is an extreme-value estimate in which one anomalous week, a collection gap or a
    /// quiet governance period, sets the entire pessimistic scenario with nothing to temper
    /// it. A 90-day lookback yields 12-13 full 7-day windows in normal operation, so this
    /// floor bites only when data is genuinely sparse.
    ///
    /// The floor counts **populated** windows — windows every day of which is covered by a
    /// reward row. It must never be satisfied by windows manufactured from absence. Counting
    /// windows *produced* rather than observed is what let a 211-day collection gap present
    /// itself as eleven consecutive zero-income weeks and drive p10 to 0.0.
    pub const MIN_WINDOWS: usize = 11;

    /// Compute bands from per-window daily rates.
    ///
    /// `rates` must contain populated windows only; see [`WindowSample::rates`]. Passing
    /// zero-filled placeholders for unobserved windows defeats [`Self::MIN_WINDOWS`].
    /// `None` if there are too few of them.
    pub fn from_window_rates(
        mut rates: Vec<f64>,
        window_days: i64,
        lookback_days: i64,
    ) -> Option<Self> {
        if rates.len() < Self::MIN_WINDOWS {
            return None;
        }
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = |p: f64| -> usize {
            let n = rates.len();
            (((n as f64) * p).ceil() as usize).saturating_sub(1).min(n - 1)
        };
        Some(Self {
            p10: rates[idx(0.10)],
            p50: rates[idx(0.50)],
            p90: rates[idx(0.90)],
            window_count: rates.len(),
            window_days,
            lookback_days,
        })
    }
}

/// Data quality indicator for projections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataQuality {
    Insufficient,    // < 7 days
    Low,            // 7-29 days
    Moderate,       // 30-89 days
    Good,           // 90-179 days
    Excellent,      // 180+ days
}

impl DataQuality {
    pub fn from_days(days: i64) -> Self {
        match days {
            d if d < 7 => Self::Insufficient,
            d if d < 30 => Self::Low,
            d if d < 90 => Self::Moderate,
            d if d < 180 => Self::Good,
            _ => Self::Excellent,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Insufficient => "Insufficient (< 7 days)",
            Self::Low => "Low (7-29 days)",
            Self::Moderate => "Moderate (30-89 days)",
            Self::Good => "Good (90-179 days)",
            Self::Excellent => "Excellent (180+ days)",
        }
    }

    pub fn is_reliable(&self) -> bool {
        matches!(self, Self::Moderate | Self::Good | Self::Excellent)
    }

    /// Rank for comparison. Higher is better.
    fn rank(&self) -> u8 {
        match self {
            Self::Insufficient => 0,
            Self::Low => 1,
            Self::Moderate => 2,
            Self::Good => 3,
            Self::Excellent => 4,
        }
    }
}

/// How recently the data was collected.
///
/// Distinct from `DataQuality`, which counts how many days of history exist. The two are
/// independent: a database holding three years of rewards that stopped updating in January
/// has excellent depth and no freshness at all. Grading on depth alone reported
/// "Excellent (180+ days)" over data that was seven months old.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataFreshness {
    Current,    // <= 1 day since the newest reward
    Recent,     // 2-7 days
    Stale,      // 8-30 days
    VeryStale,  // 31-90 days
    Obsolete,   // > 90 days
}

impl DataFreshness {
    pub fn from_days_since(days_since: i64) -> Self {
        match days_since {
            d if d <= 1 => Self::Current,
            d if d <= 7 => Self::Recent,
            d if d <= 30 => Self::Stale,
            d if d <= 90 => Self::VeryStale,
            _ => Self::Obsolete,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Current => "Current (<= 1 day old)",
            Self::Recent => "Recent (2-7 days old)",
            Self::Stale => "Stale (8-30 days old)",
            Self::VeryStale => "Very stale (31-90 days old)",
            Self::Obsolete => "Obsolete (> 90 days old)",
        }
    }

    /// Rank for comparison. Higher is better. Deliberately aligned with
    /// `DataQuality::rank` so the two axes can be compared directly.
    fn rank(&self) -> u8 {
        match self {
            Self::Obsolete => 0,
            Self::VeryStale => 1,
            Self::Stale => 2,
            Self::Recent => 3,
            Self::Current => 4,
        }
    }

    pub fn is_reliable(&self) -> bool {
        matches!(self, Self::Current | Self::Recent | Self::Stale)
    }
}

/// Both axes of data quality, reported together.
///
/// The overall grade is the WORSE of the two, because a projection is only as trustworthy
/// as its weakest input — but both are surfaced rather than collapsed into one label, so a
/// reader can see which axis is failing and what to do about it. Thin history is fixed by
/// waiting; stale history is fixed by running the tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataQualityAssessment {
    pub depth: DataQuality,
    pub freshness: DataFreshness,
    pub days_of_history: i64,
    pub days_since_newest: i64,
}

impl DataQualityAssessment {
    pub fn new(days_of_history: i64, days_since_newest: i64) -> Self {
        Self {
            depth: DataQuality::from_days(days_of_history),
            freshness: DataFreshness::from_days_since(days_since_newest),
            days_of_history,
            days_since_newest,
        }
    }

    /// The binding constraint: whichever axis is worse.
    pub fn overall_is_reliable(&self) -> bool {
        self.depth.is_reliable() && self.freshness.is_reliable()
    }

    /// Which axis is limiting, for a caller that wants to say what to fix.
    pub fn limiting_axis(&self) -> LimitingAxis {
        if self.freshness.rank() < self.depth.rank() {
            LimitingAxis::Freshness
        } else if self.depth.rank() < self.freshness.rank() {
            LimitingAxis::Depth
        } else {
            LimitingAxis::Neither
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitingAxis {
    Depth,
    Freshness,
    Neither,
}

/// Assumptions used in the projection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionAssumptions {
    pub reward_rate_basis: String,  // e.g., "30-day median"
    pub compounding_enabled: bool,
    pub safety_margin: f64,         // percentage (e.g., 0.20 for 20%)
    pub no_withdrawals: bool,
}

impl Default for ProjectionAssumptions {
    fn default() -> Self {
        Self {
            reward_rate_basis: "30-day median".to_string(),
            compounding_enabled: true,
            safety_margin: 0.20,
            no_withdrawals: true,
        }
    }
}

/// What-if scenario for comparing alternative retirement targets
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhatIfComparison {
    pub target_income: TargetIncome,
    pub realistic_timeline: ProjectionTimeline,
    pub years_delta: f64,  // Positive = later, negative = earlier
}

impl WhatIfComparison {
    pub fn new(
        target_income: TargetIncome,
        realistic_timeline: ProjectionTimeline,
        base_years: f64,
    ) -> Self {
        let years_delta = realistic_timeline.years_until_retirement - base_years;
        Self {
            target_income,
            realistic_timeline,
            years_delta,
        }
    }

    pub fn is_earlier(&self) -> bool {
        self.years_delta < 0.0
    }

    pub fn is_later(&self) -> bool {
        self.years_delta > 0.0
    }
}

/// Main aggregate: RetirementProjection
///
/// This aggregate encapsulates all the information needed for a retirement projection,
/// including the target income, current state, projected timeline, and assumptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetirementProjection {
    target_daily_income: TargetIncome,
    current_daily_income: f64,
    current_portfolio_value: IcpAmount,
    projected_timeline: ProjectionTimeline,
    scenarios: Vec<ProjectionScenario>,
    data_quality: DataQuality,
    /// Both axes. Optional so existing constructions remain valid; `project` always sets it.
    quality_assessment: Option<DataQualityAssessment>,
    /// The bands behind the scenarios, including how many windows produced them.
    percentiles: Option<RewardPercentiles>,
    assumptions: ProjectionAssumptions,
}

impl RetirementProjection {
    pub fn new(
        target_daily_income: TargetIncome,
        current_daily_income: f64,
        current_portfolio_value: IcpAmount,
        projected_timeline: ProjectionTimeline,
        scenarios: Vec<ProjectionScenario>,
        data_quality: DataQuality,
        assumptions: ProjectionAssumptions,
    ) -> Result<Self, &'static str> {
        // Validate invariants
        if current_daily_income < 0.0 {
            return Err("Current daily income must be non-negative");
        }

        if scenarios.is_empty() {
            return Err("At least one scenario must exist");
        }

        Ok(Self {
            target_daily_income,
            current_daily_income,
            current_portfolio_value,
            projected_timeline,
            scenarios,
            data_quality,
            quality_assessment: None,
            percentiles: None,
            assumptions,
        })
    }

    // Getters
    pub fn target_daily_income(&self) -> &TargetIncome {
        &self.target_daily_income
    }

    pub fn current_daily_income(&self) -> f64 {
        self.current_daily_income
    }

    pub fn current_portfolio_value(&self) -> &IcpAmount {
        &self.current_portfolio_value
    }

    pub fn projected_timeline(&self) -> &ProjectionTimeline {
        &self.projected_timeline
    }

    pub fn data_quality(&self) -> &DataQuality {
        &self.data_quality
    }

    pub fn quality_assessment(&self) -> Option<&DataQualityAssessment> {
        self.quality_assessment.as_ref()
    }

    pub fn with_quality_assessment(mut self, assessment: DataQualityAssessment) -> Self {
        self.quality_assessment = Some(assessment);
        self
    }

    pub fn percentiles(&self) -> Option<&RewardPercentiles> {
        self.percentiles.as_ref()
    }

    pub fn with_percentiles(mut self, percentiles: RewardPercentiles) -> Self {
        self.percentiles = Some(percentiles);
        self
    }

    pub fn assumptions(&self) -> &ProjectionAssumptions {
        &self.assumptions
    }

    pub fn scenarios(&self) -> &[ProjectionScenario] {
        &self.scenarios
    }

    /// Calculate the portfolio shortfall (or surplus if negative)
    pub fn portfolio_shortfall(&self) -> f64 {
        self.projected_timeline.required_portfolio_size.to_icp() - self.current_portfolio_value.to_icp()
    }

    /// Check if retirement is currently feasible
    pub fn is_already_feasible(&self) -> bool {
        self.current_daily_income >= self.target_daily_income.icp_per_day()
    }

    /// Check if the projection is reliable based on data quality
    pub fn is_reliable(&self) -> bool {
        self.data_quality.is_reliable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_target_income_validation() {
        // Valid target income
        assert!(TargetIncome::new(2.5).is_ok());

        // Zero or negative should fail
        assert!(TargetIncome::new(0.0).is_err());
        assert!(TargetIncome::new(-1.0).is_err());

        // Unreasonably high should fail
        assert!(TargetIncome::new(10_001.0).is_err());
    }

    #[test]
    fn test_data_quality_from_days() {
        assert_eq!(DataQuality::from_days(5), DataQuality::Insufficient);
        assert_eq!(DataQuality::from_days(15), DataQuality::Low);
        assert_eq!(DataQuality::from_days(45), DataQuality::Moderate);
        assert_eq!(DataQuality::from_days(120), DataQuality::Good);
        assert_eq!(DataQuality::from_days(200), DataQuality::Excellent);
    }

    // ---- Freshness axis -------------------------------------------------------------


    // ---- RewardPercentiles against known distributions -------------------------------

    /// 1..=20 sorted. Index formula is ceil(n*p)-1, so for n=20: p10 -> idx 1 (value 2),
    /// p50 -> idx 9 (value 10), p90 -> idx 17 (value 18).
    #[test]
    fn percentiles_on_a_known_uniform_distribution() {
        let rates: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let p = RewardPercentiles::from_window_rates(rates, 7, 140).expect("enough windows");
        assert_eq!(p.p10, 2.0);
        assert_eq!(p.p50, 10.0);
        assert_eq!(p.p90, 18.0);
        assert_eq!(p.window_count, 20);
        assert_eq!(p.window_days, 7);
        assert_eq!(p.lookback_days, 140);
    }

    /// Input order must not matter — the function sorts.
    #[test]
    fn percentiles_are_order_independent() {
        let ascending: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let mut descending = ascending.clone();
        descending.reverse();
        let a = RewardPercentiles::from_window_rates(ascending, 7, 140).unwrap();
        let b = RewardPercentiles::from_window_rates(descending, 7, 140).unwrap();
        assert_eq!(a.p10, b.p10);
        assert_eq!(a.p50, b.p50);
        assert_eq!(a.p90, b.p90);
    }

    /// A constant series has no spread; all three bands must coincide rather than drift.
    #[test]
    fn percentiles_on_a_constant_distribution_collapse_to_one_value() {
        let p = RewardPercentiles::from_window_rates(vec![3.5; 12], 7, 90).unwrap();
        assert_eq!(p.p10, 3.5);
        assert_eq!(p.p50, 3.5);
        assert_eq!(p.p90, 3.5);
    }

    /// Zero-reward windows are legitimate observations and must pull the lower band down,
    /// not be discarded.
    #[test]
    fn zero_windows_are_kept_and_lower_the_bottom_band() {
        let mut rates = vec![0.0, 0.0];
        rates.extend(std::iter::repeat(4.0).take(10));
        let p = RewardPercentiles::from_window_rates(rates, 7, 90).unwrap();
        assert_eq!(p.window_count, 12);
        assert_eq!(p.p10, 0.0, "a genuinely dead fortnight must show in the pessimistic band");
        assert_eq!(p.p90, 4.0);
    }

    // ---- The MIN_WINDOWS floor -------------------------------------------------------

    /// Below the floor the bands are refused outright rather than emitted from too few
    /// samples. n=10 is the last count at which p10 is simply the minimum observation.
    #[test]
    fn too_few_windows_yields_none() {
        for n in 0..RewardPercentiles::MIN_WINDOWS {
            let rates: Vec<f64> = (0..n).map(|i| i as f64).collect();
            assert!(
                RewardPercentiles::from_window_rates(rates, 7, 90).is_none(),
                "n={} is below the floor and must not produce bands",
                n
            );
        }
    }

    #[test]
    fn the_floor_is_where_p10_stops_being_the_minimum() {
        // At exactly MIN_WINDOWS, p10 must have at least one observation beneath it —
        // this is the property the constant is chosen for.
        let n = RewardPercentiles::MIN_WINDOWS;
        let rates: Vec<f64> = (1..=n).map(|i| i as f64).collect();
        let p = RewardPercentiles::from_window_rates(rates.clone(), 7, 90).expect("at the floor");
        let min = rates.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(
            p.p10 > min,
            "at n={} p10 ({}) must be above the minimum ({})",
            n, p.p10, min
        );

        // And one below the floor it would have been the minimum, which is why it is refused.
        let fewer: Vec<f64> = (1..n).map(|i| i as f64).collect();
        let idx = (((fewer.len() as f64) * 0.10).ceil() as usize).saturating_sub(1);
        assert_eq!(idx, 0, "one below the floor, p10 would be the minimum");
    }

    /// Autocorrelation is the defect this replaced. Overlapping windows let one exceptional
    /// week appear in several samples, inflating the upper tail; non-overlapping samples
    /// count it once. Same underlying days, different upper band.
    #[test]
    fn overlapping_samples_inflate_the_upper_band_relative_to_non_overlapping() {
        // Eleven quiet weeks and one exceptional one.
        let non_overlapping: Vec<f64> = {
            let mut v = vec![2.0; 11];
            v.push(8.0);
            v
        };
        // The same exceptional week, counted seven times as overlap would.
        let overlapping: Vec<f64> = {
            let mut v = vec![2.0; 11];
            v.extend(std::iter::repeat(8.0).take(7));
            v
        };
        let n = RewardPercentiles::from_window_rates(non_overlapping, 7, 90).unwrap();
        let o = RewardPercentiles::from_window_rates(overlapping, 7, 126).unwrap();
        assert_eq!(n.p90, 2.0, "counted once, the outlier does not reach the 90th percentile");
        assert_eq!(o.p90, 8.0, "counted seven times, it dominates the upper band");
        assert!(o.p90 > n.p90);
    }

    #[test]
    fn freshness_boundaries() {
        // Current: <= 1
        assert_eq!(DataFreshness::from_days_since(0), DataFreshness::Current);
        assert_eq!(DataFreshness::from_days_since(1), DataFreshness::Current);
        // Recent: 2-7
        assert_eq!(DataFreshness::from_days_since(2), DataFreshness::Recent);
        assert_eq!(DataFreshness::from_days_since(7), DataFreshness::Recent);
        // Stale: 8-30
        assert_eq!(DataFreshness::from_days_since(8), DataFreshness::Stale);
        assert_eq!(DataFreshness::from_days_since(30), DataFreshness::Stale);
        // VeryStale: 31-90
        assert_eq!(DataFreshness::from_days_since(31), DataFreshness::VeryStale);
        assert_eq!(DataFreshness::from_days_since(90), DataFreshness::VeryStale);
        // Obsolete: > 90
        assert_eq!(DataFreshness::from_days_since(91), DataFreshness::Obsolete);
        assert_eq!(DataFreshness::from_days_since(10_000), DataFreshness::Obsolete);
    }

    #[test]
    fn freshness_reliability_cuts_at_very_stale() {
        assert!(DataFreshness::Current.is_reliable());
        assert!(DataFreshness::Recent.is_reliable());
        assert!(DataFreshness::Stale.is_reliable());
        assert!(!DataFreshness::VeryStale.is_reliable());
        assert!(!DataFreshness::Obsolete.is_reliable());
    }

    // ---- The two axes together ------------------------------------------------------

    /// The case that prompted this work: a real database held 1,164 days
    /// of history whose newest entry was 210 days old. Depth alone graded it "Excellent",
    /// which read as a green light over data more than half a year stale.
    #[test]
    fn deep_but_stale_is_not_reliable() {
        let a = DataQualityAssessment::new(1164, 210);
        assert_eq!(a.depth, DataQuality::Excellent);
        assert_eq!(a.freshness, DataFreshness::Obsolete);
        assert!(!a.overall_is_reliable(), "excellent depth must not mask obsolete data");
        assert_eq!(a.limiting_axis(), LimitingAxis::Freshness);
    }

    /// The mirror case: collected today, but only a few days of it.
    #[test]
    fn fresh_but_thin_is_not_reliable() {
        let a = DataQualityAssessment::new(10, 0);
        assert_eq!(a.depth, DataQuality::Low);
        assert_eq!(a.freshness, DataFreshness::Current);
        assert!(!a.overall_is_reliable());
        assert_eq!(a.limiting_axis(), LimitingAxis::Depth);
    }

    #[test]
    fn deep_and_fresh_is_reliable_and_has_no_limiting_axis() {
        let a = DataQualityAssessment::new(400, 1);
        assert_eq!(a.depth, DataQuality::Excellent);
        assert_eq!(a.freshness, DataFreshness::Current);
        assert!(a.overall_is_reliable());
        assert_eq!(a.limiting_axis(), LimitingAxis::Neither);
    }

    #[test]
    fn overall_is_the_worse_of_the_two_axes() {
        // Reliable only when BOTH are reliable.
        assert!(DataQualityAssessment::new(90, 5).overall_is_reliable());
        assert!(!DataQualityAssessment::new(90, 200).overall_is_reliable()); // freshness fails
        assert!(!DataQualityAssessment::new(5, 0).overall_is_reliable());    // depth fails
        assert!(!DataQualityAssessment::new(5, 200).overall_is_reliable());  // both fail
    }

    #[test]
    fn assessment_retains_the_raw_numbers_for_display() {
        let a = DataQualityAssessment::new(1164, 210);
        assert_eq!(a.days_of_history, 1164);
        assert_eq!(a.days_since_newest, 210);
    }


    #[test]
    fn test_projection_timeline_already_feasible() {
        let timeline = ProjectionTimeline::new(
            0.0,
            Utc::now().naive_utc().date(),
            IcpAmount::from_e8s(1000_00000000),
        );
        assert!(timeline.years_until_retirement <= 0.0);

        let timeline_future = ProjectionTimeline::new(
            5.5,
            Utc::now().naive_utc().date(),
            IcpAmount::from_e8s(1000_00000000),
        );
        assert!(timeline_future.years_until_retirement > 0.0);
    }

    #[test]
    fn test_retirement_projection_validation() {
        let target = TargetIncome::new(2.5).unwrap();
        let timeline = ProjectionTimeline::new(
            5.0,
            Utc::now().naive_utc().date(),
            IcpAmount::from_e8s(1000_00000000),
        );
        let scenario = ProjectionScenario::Realistic(timeline.clone());

        // Valid projection
        let projection = RetirementProjection::new(
            target,
            0.5,
            IcpAmount::from_e8s(100_00000000),
            timeline,
            vec![scenario],
            DataQuality::Good,
            ProjectionAssumptions::default(),
        );
        assert!(projection.is_ok());

        // Invalid: negative current income
        let projection_invalid = RetirementProjection::new(
            target,
            -0.5,
            IcpAmount::from_e8s(100_00000000),
            ProjectionTimeline::new(5.0, Utc::now().naive_utc().date(), IcpAmount::from_e8s(1000_00000000)),
            vec![],
            DataQuality::Good,
            ProjectionAssumptions::default(),
        );
        assert!(projection_invalid.is_err());
    }
}
