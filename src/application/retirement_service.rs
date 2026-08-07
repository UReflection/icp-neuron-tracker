use crate::domain::{
    Portfolio,
    IcpAmount,
    repositories::DailyRewardRepository,
    retirement::{self, *},
};
use chrono::{Utc, Duration};

/// Percentile bands are computed over NON-OVERLAPPING windows of this length, across the
/// lookback below. Module-level so the shortfall messages quote the same numbers the logic
/// enforces — the "needs 7 days" message that preceded them named a threshold nothing used.
const LOOKBACK_DAYS: i64 = 90;
const WINDOW_DAYS: i64 = 7;

/// Service for retirement projection calculations
pub struct RetirementService<R>
where
    R: DailyRewardRepository,
{
    reward_repo: R,
}

impl<R> RetirementService<R>
where
    R: DailyRewardRepository,
{
    pub fn new(reward_repo: R) -> Self {
        Self { reward_repo }
    }

    /// Calculate a basic retirement projection based on historical reward rates
    ///
    /// User Story 1 & 2: Basic Retirement Projection with Risk Scenario Analysis
    /// This implements the core functionality: calculate when the user can retire
    /// based on their target daily income and current portfolio trajectory.
    /// Now includes optimistic, realistic, and pessimistic scenarios.
    pub fn calculate_basic_projection(
        &self,
        portfolio: &Portfolio,
        target_income: TargetIncome,
    ) -> Result<RetirementProjection, Box<dyn std::error::Error>> {
        // Delegate to risk scenarios method which provides all three scenarios
        self.calculate_risk_scenarios(portfolio, target_income)
    }

    /// Legacy method kept for explicit single-scenario calculations if needed
    #[allow(dead_code)]
    fn calculate_single_scenario_projection(
        &self,
        portfolio: &Portfolio,
        target_income: TargetIncome,
    ) -> Result<RetirementProjection, Box<dyn std::error::Error>> {
        // Get historical data quality
        let data_days = self.reward_repo.get_portfolio_reward_data_count()?;
        let data_quality = DataQuality::from_days(data_days);

        // Validate we have enough data
        if data_days < 7 {
            return Err(format!(
                "Not enough tracking history for a projection yet: {data_days} day(s) recorded.\n\n\
                 Risk scenarios need {min} populated {w}-day windows within a {lb}-day lookback, \
                 which is about {weeks} weeks of daily tracking from a standing start. This \
                 message will keep reporting where you are; it is a cold start, not a fault. \
                 The tool will not estimate a band from data it has not observed.\n\n\
                 Reports work from the first snapshot: try `icp-neuron-tracker report summary` \
                 or `report rewards --days 30` in the meantime.",
                data_days = data_days,
                min = RewardPercentiles::MIN_WINDOWS,
                w = WINDOW_DAYS,
                lb = LOOKBACK_DAYS,
                weeks = RewardPercentiles::MIN_WINDOWS,
            ).into());
        }

        // Get current daily income (30-day average)
        let current_daily_income = self.reward_repo
            .get_portfolio_average_daily_reward(30)?
            .ok_or("No reward data available")?;

        // Get current portfolio value
        let current_portfolio_value = portfolio.total_value();

        // Check if already feasible
        if current_daily_income >= target_income.icp_per_day() {
            // Already retired!
            let timeline = ProjectionTimeline::new(
                0.0,
                Utc::now().naive_utc().date(),
                current_portfolio_value,
            );

            let scenario = ProjectionScenario::Realistic(timeline.clone());
            let assumptions = ProjectionAssumptions::default();

            return RetirementProjection::new(
                target_income,
                current_daily_income,
                current_portfolio_value,
                timeline,
                vec![scenario],
                data_quality,
                assumptions,
            ).map_err(|e| e.into());
        }

        // Calculate projection using compound growth
        let assumptions = ProjectionAssumptions::default();

        // Calculate daily growth rate from current income and portfolio size
        let daily_growth_rate = if current_portfolio_value.to_icp() > 0.0 {
            current_daily_income / current_portfolio_value.to_icp()
        } else {
            return Err("Portfolio value is zero, cannot calculate projection".into());
        };

        // Calculate years to retirement using compound growth formula
        // We need: target_income <= daily_growth_rate * future_portfolio_value
        // With compounding: future_value = current_value * (1 + rate)^days
        // So: target_income <= daily_growth_rate * current_value * (1 + rate)^days
        // Solving for days: days = ln(target_income / (daily_growth_rate * current_value)) / ln(1 + rate)

        // Apply safety margin to target
        let safe_target_income = target_income.icp_per_day() * (1.0 + assumptions.safety_margin);

        // Calculate required portfolio size to generate target income
        let required_portfolio = safe_target_income / daily_growth_rate;
        let required_portfolio_icp_amount = IcpAmount::from_e8s((required_portfolio * 100_000_000.0) as u64);

        // Calculate time to reach required portfolio with compound growth
        let growth_multiplier = required_portfolio / current_portfolio_value.to_icp();

        let years_until_retirement = if daily_growth_rate > 0.0 && growth_multiplier > 1.0 {
            // Using compound growth: years = ln(multiplier) / ln(1 + daily_rate) / 365.25
            growth_multiplier.ln() / (1.0 + daily_growth_rate).ln() / 365.25
        } else if growth_multiplier <= 1.0 {
            // Already have enough or very close
            0.0
        } else {
            // No growth or negative growth
            return Err("Retirement not feasible with current reward rates (zero or negative growth)".into());
        };

        // Calculate retirement date
        let days_until_retirement = (years_until_retirement * 365.25) as i64;
        let retirement_date = Utc::now().naive_utc().date() + Duration::days(days_until_retirement);

        // Build the projection timeline
        let timeline = ProjectionTimeline::new(
            years_until_retirement,
            retirement_date,
            required_portfolio_icp_amount,
        );

        // For User Story 1, we just create one realistic scenario
        let scenario = ProjectionScenario::Realistic(timeline.clone());

        // Create the projection
        RetirementProjection::new(
            target_income,
            current_daily_income,
            current_portfolio_value,
            timeline,
            vec![scenario],
            data_quality,
            assumptions,
        ).map_err(|e| e.into())
    }

    /// Explain a MIN_WINDOWS shortfall in terms of what was and was not observed.
    ///
    /// The three counts are kept apart deliberately. A window excluded because no reward row
    /// covers it is missing data; a window that is present and paid nothing is an observation
    /// of zero income. Reporting the shortfall without that split is what let a 211-day
    /// collection gap surface as "zero or negative growth" over 545 ICP of real rewards.
    fn insufficient_windows_message(
        sample: &WindowSample,
        window_days: i64,
        lookback_days: i64,
    ) -> String {
        let populated = sample.populated();
        let min = RewardPercentiles::MIN_WINDOWS;

        let mut msg = format!(
            "Not enough observed reward history for scenario analysis.\n\n\
             Risk scenarios need at least {min} populated {w}-day windows within the last \
             {lb} days. Found {populated}:\n  \
             - {populated} populated (every day covered by a reward row)\n  \
             - {zero} of those were genuine zero-reward windows, which do count\n  \
             - {unobs} excluded as unobservable (no reward row covers part of the span)\n\n",
            min = min,
            w = window_days,
            lb = lookback_days,
            populated = populated,
            zero = sample.zero_reward,
            unobs = sample.unobservable,
        );

        if sample.unobservable > 0 {
            msg.push_str(
                "The excluded windows are missing data, not zero income. They are left out \
                 rather than counted as zero, because a week nothing was recorded for is not \
                 a week that paid nothing, and treating it as one drives the pessimistic \
                 scenario to \"never\" on an absence of evidence.\n\n",
            );
        }

        if let Some(gap) = sample.longest_gap {
            msg.push_str(&format!(
                "Cause: a {days}-day collection gap from {start} to {end}. The {icp:.2} ICP that \
                 accrued over it is real and is recorded against {end}, but a single total over \
                 {days} days says nothing about any {w}-day window inside it, so those windows \
                 cannot be scored.\n\n",
                days = gap.days,
                start = gap.start,
                end = gap.end,
                icp = gap.total_icp,
                w = window_days,
            ));
        }

        msg.push_str(&format!(
            "Below {min} windows the 10th percentile is simply the single worst window, so a \
             pessimistic scenario computed from it would be one bad week rather than a band.\n\n\
             Run `icp-neuron-tracker track` daily for about {need} more days, or check \
             `report rewards --days {lb}` to see what history exists.",
            min = min,
            need = (min - populated) as i64 * window_days,
            lb = lookback_days,
        ));

        msg
    }

    /// Calculate a single projection timeline using a specific daily income rate
    ///
    /// This is a helper method used by scenario analysis to compute projections
    /// for different reward rate assumptions (optimistic, realistic, pessimistic)
    fn calculate_timeline_with_rate(
        &self,
        current_portfolio_value: &IcpAmount,
        current_daily_income: f64,
        target_income: &TargetIncome,
        assumptions: &ProjectionAssumptions,
    ) -> Result<ProjectionTimeline, Box<dyn std::error::Error>> {
        // Check if already feasible
        if current_daily_income >= target_income.icp_per_day() {
            return Ok(ProjectionTimeline::new(
                0.0,
                Utc::now().naive_utc().date(),
                *current_portfolio_value,
            ));
        }

        // Calculate daily growth rate from current income and portfolio size
        let daily_growth_rate = if current_portfolio_value.to_icp() > 0.0 {
            current_daily_income / current_portfolio_value.to_icp()
        } else {
            return Err("Portfolio value is zero, cannot calculate projection".into());
        };

        // Apply safety margin to target
        let safe_target_income = target_income.icp_per_day() * (1.0 + assumptions.safety_margin);

        // Calculate required portfolio size to generate target income
        let required_portfolio = safe_target_income / daily_growth_rate;
        let required_portfolio_icp_amount = IcpAmount::from_e8s((required_portfolio * 100_000_000.0) as u64);

        // Calculate time to reach required portfolio with compound growth
        let growth_multiplier = required_portfolio / current_portfolio_value.to_icp();

        let years_until_retirement = if daily_growth_rate > 0.0 && growth_multiplier > 1.0 {
            // Using compound growth: years = ln(multiplier) / ln(1 + daily_rate) / 365.25
            growth_multiplier.ln() / (1.0 + daily_growth_rate).ln() / 365.25
        } else if growth_multiplier <= 1.0 {
            // Already have enough or very close
            0.0
        } else {
            // No growth or negative growth
            return Err("Retirement not feasible with current reward rates (zero or negative growth)".into());
        };

        // Calculate retirement date
        let days_until_retirement = (years_until_retirement * 365.25) as i64;
        let retirement_date = Utc::now().naive_utc().date() + Duration::days(days_until_retirement);

        Ok(ProjectionTimeline::new(
            years_until_retirement,
            retirement_date,
            required_portfolio_icp_amount,
        ))
    }

    /// Calculate what-if comparisons for multiple target incomes
    ///
    /// User Story 3: What-If Analysis
    /// Compares alternative retirement targets to show impact on retirement timeline
    pub fn calculate_what_if_analysis(
        &self,
        portfolio: &Portfolio,
        base_target: TargetIncome,
        alternative_targets: Vec<f64>,
    ) -> Result<(RetirementProjection, Vec<retirement::WhatIfComparison>), Box<dyn std::error::Error>> {
        // Calculate base projection first
        let base_projection = self.calculate_risk_scenarios(portfolio, base_target)?;
        let base_years = base_projection.projected_timeline().years_until_retirement;

        // Calculate comparisons for alternative targets
        let mut comparisons = Vec::new();

        for alt_target_value in alternative_targets {
            let alt_target = TargetIncome::new(alt_target_value)?;

            // Calculate projection for alternative target
            let alt_projection = self.calculate_risk_scenarios(portfolio, alt_target)?;

            // Extract the realistic timeline for comparison
            let realistic_timeline = alt_projection.projected_timeline().clone();

            // Create comparison
            let comparison = retirement::WhatIfComparison::new(
                alt_target,
                realistic_timeline,
                base_years,
            );

            comparisons.push(comparison);
        }

        Ok((base_projection, comparisons))
    }

    /// Calculate all three risk scenarios (optimistic, realistic, pessimistic)
    ///
    /// User Story 2: Risk Scenario Analysis
    /// Uses historical percentiles to model best-case, expected, and worst-case outcomes
    pub fn calculate_risk_scenarios(
        &self,
        portfolio: &Portfolio,
        target_income: TargetIncome,
    ) -> Result<RetirementProjection, Box<dyn std::error::Error>> {
        // Data quality has two axes. Depth counts how many days of reward history exist;
        // freshness is how long ago the newest of them was. They are independent — three
        // years of history that stopped updating in January is deep and not fresh — and the
        // projection is only as good as the weaker one.
        let data_days = self.reward_repo.get_portfolio_reward_data_count()?;
        let data_quality = DataQuality::from_days(data_days);

        let days_since_newest = match self.reward_repo.get_newest_reward_date()? {
            Some(newest) => (Utc::now().naive_utc().date() - newest).num_days().max(0),
            // No rewards at all; the depth check below rejects this anyway.
            None => i64::MAX,
        };
        let assessment = DataQualityAssessment::new(data_days, days_since_newest);

        // Validate we have enough data
        if data_days < 7 {
            return Err(format!(
                "Not enough tracking history for a projection yet: {data_days} day(s) recorded.\n\n\
                 Risk scenarios need {min} populated {w}-day windows within a {lb}-day lookback, \
                 which is about {weeks} weeks of daily tracking from a standing start. This \
                 message will keep reporting where you are; it is a cold start, not a fault. \
                 The tool will not estimate a band from data it has not observed.\n\n\
                 Reports work from the first snapshot: try `icp-neuron-tracker report summary` \
                 or `report rewards --days 30` in the meantime.",
                data_days = data_days,
                min = RewardPercentiles::MIN_WINDOWS,
                w = WINDOW_DAYS,
                lb = LOOKBACK_DAYS,
                weeks = RewardPercentiles::MIN_WINDOWS,
            ).into());
        }

        // Percentile bands over NON-OVERLAPPING 7-day windows across a 90-day lookback.
        //
        // 90 days yields 12-13 complete windows in normal operation, which is the point at
        // which p10 stops being the minimum observation (see RewardPercentiles::MIN_WINDOWS).
        // The previous 30-day lookback with overlapping windows produced samples that shared
        // days with each other, narrowing the spread and flattering the pessimistic case.
        let sample = self
            .reward_repo
            .get_portfolio_window_sample(LOOKBACK_DAYS, WINDOW_DAYS)?;

        let percentiles =
            RewardPercentiles::from_window_rates(sample.rates.clone(), WINDOW_DAYS, LOOKBACK_DAYS)
                .ok_or_else(|| Self::insufficient_windows_message(&sample, WINDOW_DAYS, LOOKBACK_DAYS))?;

        let (p10_income, p50_income, p90_income) =
            (percentiles.p10, percentiles.p50, percentiles.p90);

        // Get current daily income (30-day average) for display
        let current_daily_income = self.reward_repo
            .get_portfolio_average_daily_reward(30)?
            .ok_or("No reward data available")?;

        // Get current portfolio value
        let current_portfolio_value = portfolio.total_value();
        let assumptions = ProjectionAssumptions::default();

        // Calculate three scenarios
        let pessimistic_timeline = self.calculate_timeline_with_rate(
            &current_portfolio_value,
            p10_income,
            &target_income,
            &assumptions,
        )?;

        let realistic_timeline = self.calculate_timeline_with_rate(
            &current_portfolio_value,
            p50_income,
            &target_income,
            &assumptions,
        )?;

        let optimistic_timeline = self.calculate_timeline_with_rate(
            &current_portfolio_value,
            p90_income,
            &target_income,
            &assumptions,
        )?;

        // Build scenarios
        let scenarios = vec![
            ProjectionScenario::Pessimistic(pessimistic_timeline),
            ProjectionScenario::Realistic(realistic_timeline.clone()),
            ProjectionScenario::Optimistic(optimistic_timeline),
        ];

        // Use 30-day average as the current daily income for display
        // Note: Scenarios use percentiles (p10, p50, p90) for projections
        RetirementProjection::new(
            target_income,
            current_daily_income,
            current_portfolio_value,
            realistic_timeline,
            scenarios,
            data_quality,
            assumptions,
        )
        .map(|p| p.with_quality_assessment(assessment).with_percentiles(percentiles))
        .map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Neuron, NeuronId, NeuronState};
    use crate::domain::repositories::DailyRewardRepository;
    use chrono::{Utc, NaiveDate};

    // Mock repository for testing
    struct MockRewardRepo {
        data_count: i64,
        daily_income: Option<f64>,
        /// Newest reward date. `None` means "as of today", so existing tests exercise the
        /// fresh-data path without being rewritten.
        newest_reward_date: Option<NaiveDate>,
    }

    impl DailyRewardRepository for MockRewardRepo {
        fn save_reward(&self, _: NeuronId, _: NaiveDate, _: i64, _: i64, _: i64) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn get_reward(&self, _: NeuronId, _: NaiveDate) -> Result<Option<crate::domain::repositories::DailyReward>, Box<dyn std::error::Error>> {
            Ok(None)
        }

        fn get_rewards_range(&self, _: NeuronId, _: NaiveDate, _: NaiveDate) -> Result<Vec<crate::domain::repositories::DailyReward>, Box<dyn std::error::Error>> {
            Ok(vec![])
        }

        fn get_newest_reward_date(&self) -> Result<Option<NaiveDate>, Box<dyn std::error::Error>> {
            Ok(Some(self.newest_reward_date.unwrap_or_else(|| Utc::now().naive_utc().date())))
        }

        fn get_average_daily_reward_window(&self, _: NeuronId, _: i64) -> Result<Option<crate::domain::repositories::DailyAverageWindow>, Box<dyn std::error::Error>> {
            Ok(None)
        }

        fn get_reward_data_count(&self, _: NeuronId) -> Result<i64, Box<dyn std::error::Error>> {
            Ok(self.data_count)
        }

        fn get_portfolio_average_daily_reward(&self, _: i64) -> Result<Option<f64>, Box<dyn std::error::Error>> {
            Ok(self.daily_income)
        }

        fn get_portfolio_reward_data_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
            Ok(self.data_count)
        }

        fn get_portfolio_window_sample(&self, _: i64, _: i64) -> Result<WindowSample, Box<dyn std::error::Error>> {
            // Enough POPULATED windows to clear MIN_WINDOWS, spread around the mock daily
            // income so the resulting median matches what the old mock returned.
            let rates = match self.daily_income {
                Some(median) => (0..12).map(|i| median * (0.8 + 0.04 * i as f64)).collect(),
                None => Vec::new(),
            };
            Ok(WindowSample {
                rates,
                unobservable: 0,
                zero_reward: 0,
                longest_gap: None,
            })
        }
    }

    fn create_test_portfolio() -> Portfolio {
        let neuron = Neuron::new(
            NeuronId::new(123),
            IcpAmount::from_e8s(100_000_000_000), // 1000 ICP stake
            IcpAmount::from_e8s(10_000_000_000),  // 100 ICP maturity
            IcpAmount::from_e8s(5_000_000_000),   // 50 ICP staked maturity
            150_000,
            365 * 86400,  // age_seconds (1 year)
            2 * 365 * 86400,  // dissolve_delay_seconds (2 years)
            NeuronState::Locked,
            true,
            Utc::now().timestamp() as u64,  // created_timestamp as u64
        );

        Portfolio::new(vec![neuron])
    }

    /// The assessment must reach the projection, carrying BOTH axes — this is what the
    /// `project` command reads to decide which axis to warn about.
    #[test]
    fn projection_carries_both_quality_axes() {
        let stale = Utc::now().naive_utc().date() - chrono::Duration::days(210);
        let mock_repo = MockRewardRepo {
            data_count: 1164,               // deep history
            daily_income: Some(0.5),
            newest_reward_date: Some(stale), // that stopped 210 days ago
        };

        let service = RetirementService::new(mock_repo);
        let projection = service
            .calculate_basic_projection(&create_test_portfolio(), TargetIncome::new(2.0).unwrap())
            .expect("projection");

        let a = projection.quality_assessment().expect("assessment must be attached");
        assert_eq!(a.depth, DataQuality::Excellent);
        assert_eq!(a.freshness, DataFreshness::Obsolete);
        assert_eq!(a.days_since_newest, 210);
        assert!(!a.overall_is_reliable());
        assert_eq!(a.limiting_axis(), LimitingAxis::Freshness);
    }

    #[test]
    fn test_insufficient_data_error() {
        let mock_repo = MockRewardRepo {
            data_count: 5,  // Less than 7 days
            daily_income: Some(0.5),
            newest_reward_date: None,
        };

        let service = RetirementService::new(mock_repo);
        let portfolio = create_test_portfolio();
        let target = TargetIncome::new(2.0).unwrap();

        let result = service.calculate_basic_projection(&portfolio, target);
        assert!(result.is_err());

        // The message must report where the user actually is, and must not name a
        // seven-day threshold that nothing enforces: clearing seven days only moves them
        // on to the eleven-window gate, which is another ten weeks away.
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("5 day(s) recorded"), "states the current position:\n{msg}");
        assert!(msg.contains("11 populated 7-day windows"), "names the real gate:\n{msg}");
        assert!(msg.contains("cold start, not a fault"), "frames the wait:\n{msg}");
        assert!(
            !msg.contains("Need at least 7 days"),
            "must not promise a threshold that unblocks nothing:\n{msg}"
        );
    }

    #[test]
    fn test_already_retired() {
        let mock_repo = MockRewardRepo {
            data_count: 30,
            daily_income: Some(3.0),  // Already earning more than target
            newest_reward_date: None,
        };

        let service = RetirementService::new(mock_repo);
        let portfolio = create_test_portfolio();
        let target = TargetIncome::new(2.0).unwrap();

        let result = service.calculate_basic_projection(&portfolio, target);
        assert!(result.is_ok());

        let projection = result.unwrap();
        assert!(projection.is_already_feasible());
        assert_eq!(projection.projected_timeline().years_until_retirement, 0.0);
    }

    /// The shortfall message must attribute the gap to missing data, and must not recycle
    /// the old "zero or negative growth" line — which was reported over 545 ICP of real
    /// rewards and told the user their neurons had stopped earning.
    #[test]
    fn shortfall_message_blames_missing_data_not_zero_income() {
        let sample = WindowSample {
            rates: Vec::new(),
            unobservable: 12,
            zero_reward: 0,
            longest_gap: Some(AccrualGap {
                start: NaiveDate::from_ymd_opt(2026, 1, 8).unwrap(),
                end: NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
                days: 211,
                total_icp: 545.60098591,
            }),
        };

        let msg = RetirementService::<MockRewardRepo>::insufficient_windows_message(&sample, 7, 90);

        assert!(msg.contains("missing data, not zero income"), "message was:\n{msg}");
        assert!(msg.contains("211-day collection gap"), "message was:\n{msg}");
        assert!(msg.contains("2026-01-08"), "names the gap start:\n{msg}");
        assert!(msg.contains("2026-08-06"), "names the gap end:\n{msg}");
        assert!(msg.contains("545.60"), "names the rewards that did accrue:\n{msg}");
        assert!(msg.contains("12 excluded as unobservable"), "counts exclusions:\n{msg}");
        assert!(
            !msg.contains("zero or negative growth"),
            "the defect being fixed:\n{msg}"
        );
    }

    /// The three counts must be reported separately: genuine zero-reward windows are
    /// observations and are counted as populated, not lumped in with the exclusions.
    #[test]
    fn shortfall_message_separates_genuine_zero_windows_from_exclusions() {
        let sample = WindowSample {
            rates: vec![0.0, 0.0, 1.5],
            unobservable: 9,
            zero_reward: 2,
            longest_gap: None,
        };

        let msg = RetirementService::<MockRewardRepo>::insufficient_windows_message(&sample, 7, 90);

        assert!(msg.contains("Found 3:"), "populated count leads:\n{msg}");
        assert!(msg.contains("2 of those were genuine zero-reward windows"), "{msg}");
        assert!(msg.contains("9 excluded as unobservable"), "{msg}");
        // 3 populated, so 8 windows short of the floor of 11 -> about 56 more days.
        assert!(msg.contains("56 more days"), "states the shortfall in days:\n{msg}");
    }

}
