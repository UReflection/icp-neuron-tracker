use super::value_objects::*;
use serde::{Serialize, Deserialize};

/// Value object representing a neuron's reward performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuronReward {
    pub neuron_id: NeuronId,
    pub total_reward: IcpAmount,
    pub days_tracked: i64,
    pub stake: IcpAmount,
    pub dissolve_delay_days: u64,
    pub age_days: u64,
    pub age_bonus: BonusMultiplier,
    pub dissolve_bonus: BonusMultiplier,
}

impl NeuronReward {
    pub fn new(
        neuron_id: NeuronId,
        total_reward: IcpAmount,
        days_tracked: i64,
        stake: IcpAmount,
        dissolve_delay_days: u64,
        age_days: u64,
        age_bonus: BonusMultiplier,
        dissolve_bonus: BonusMultiplier,
    ) -> Self {
        Self {
            neuron_id,
            total_reward,
            days_tracked,
            stake,
            dissolve_delay_days,
            age_days,
            age_bonus,
            dissolve_bonus,
        }
    }

    /// Calculate average daily reward
    pub fn daily_average(&self) -> f64 {
        if self.days_tracked == 0 {
            return 0.0;
        }
        self.total_reward.to_icp() / self.days_tracked as f64
    }

    /// Calculate weekly extrapolated reward
    pub fn weekly_projected(&self) -> f64 {
        self.daily_average() * 7.0
    }

    /// Calculate monthly extrapolated reward (30 days)
    pub fn monthly_projected(&self) -> f64 {
        self.daily_average() * 30.0
    }

    /// Calculate reward rate as percentage of stake
    pub fn reward_rate_percentage(&self) -> f64 {
        let stake_icp = self.stake.to_icp();
        if stake_icp == 0.0 {
            return 0.0;
        }
        (self.total_reward.to_icp() / stake_icp) * 100.0
    }

    /// Calculate annualized return percentage
    pub fn annualized_return(&self) -> f64 {
        if self.days_tracked == 0 {
            return 0.0;
        }
        let daily_rate = self.reward_rate_percentage() / self.days_tracked as f64;
        // Class B annualisation: 365.0 is deliberate financial convention, NOT a calendar
        // conversion. Do not change to 365.25 — see BonusMultiplier in domain/value_objects.rs.
        daily_rate * 365.0
    }

    /// Check if neuron earned zero rewards (potential issue)
    pub fn has_zero_rewards(&self) -> bool {
        self.total_reward.e8s() == 0
    }

    /// Get dissolve delay in years
    pub fn dissolve_delay_years(&self) -> f64 {
        self.dissolve_delay_days as f64 / 365.25
    }

    /// Get age in years
    pub fn age_years(&self) -> f64 {
        self.age_days as f64 / 365.25
    }
}

/// Aggregate containing reward analysis for all neurons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardAnalysis {
    pub neuron_rewards: Vec<NeuronReward>,
    pub total_rewards: IcpAmount,
    pub period_days: i64,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub snapshot_count: usize,
}

impl RewardAnalysis {
    pub fn new(
        mut neuron_rewards: Vec<NeuronReward>,
        period_days: i64,
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
        snapshot_count: usize,
    ) -> Self {
        // Sort by total reward (highest first)
        neuron_rewards.sort_by(|a, b| {
            b.total_reward.e8s().cmp(&a.total_reward.e8s())
        });

        let total_rewards = neuron_rewards.iter()
            .map(|nr| nr.total_reward)
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt);

        Self {
            neuron_rewards,
            total_rewards,
            period_days,
            start_date,
            end_date,
            snapshot_count,
        }
    }

    /// Get neurons with zero rewards
    pub fn zero_reward_neurons(&self) -> Vec<&NeuronReward> {
        self.neuron_rewards.iter()
            .filter(|nr| nr.has_zero_rewards())
            .collect()
    }

    /// Get top N performing neurons
    #[allow(dead_code)]
    pub fn top_performers(&self, n: usize) -> Vec<&NeuronReward> {
        self.neuron_rewards.iter().take(n).collect()
    }

    /// Mean daily reward across the portfolio, over the days actually observed.
    ///
    /// This is the sum of the per-neuron daily averages, which makes the portfolio figure
    /// and its parts the same quantity by construction. It previously divided the portfolio
    /// total by `period_days` — the window the user *asked* for — while each neuron divided
    /// by the days it was *observed* over. A single-neuron portfolio therefore printed two
    /// different "daily" figures that were required to be equal: 0.0043 against 0.0100 for
    /// 13 days of history queried with `--days 30`.
    ///
    /// Dividing by the requested window also charges the rate for days that were never
    /// observed, so asking for `--days 90` instead of `--days 30` lowered the reported daily
    /// income without any change in the data. That is the same "absence counted as zero"
    /// error corrected in the projection windowing, and the resolution is the same: divide
    /// by what was observed. The divergence closes as history exceeds the window, so the old
    /// behaviour was worst for new users, whose history is shortest.
    pub fn average_daily_reward(&self) -> f64 {
        self.neuron_rewards.iter().map(|nr| nr.daily_average()).sum()
    }

    /// Days of reward history the rate above is averaged over — the widest any contributing
    /// neuron observed. Reported so output can name the divisor instead of implying
    /// `period_days`.
    pub fn observed_days(&self) -> i64 {
        self.neuron_rewards
            .iter()
            .map(|nr| nr.days_tracked)
            .max()
            .unwrap_or(0)
    }

    /// Get total projected monthly rewards
    pub fn total_monthly_projected(&self) -> f64 {
        self.average_daily_reward() * 30.0
    }

    /// Get total projected yearly rewards
    pub fn total_yearly_projected(&self) -> f64 {
        // Class B annualisation: 365.0 is deliberate financial convention, NOT a calendar
        // conversion. Do not change to 365.25 — see BonusMultiplier in domain/value_objects.rs.
        self.average_daily_reward() * 365.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_reward(
        id: u64,
        reward_icp: u64,
        days: i64,
        stake_icp: u64,
        dissolve_days: u64,
        age_days: u64,
    ) -> NeuronReward {
        NeuronReward::new(
            NeuronId::new(id),
            IcpAmount::from_e8s(reward_icp * 100_000_000),
            days,
            IcpAmount::from_e8s(stake_icp * 100_000_000),
            dissolve_days,
            age_days,
            BonusMultiplier::new(1.25), // 25% age bonus
            BonusMultiplier::new(2.0),  // 100% dissolve bonus
        )
    }

    #[test]
    fn test_neuron_reward_calculations() {
        let reward = create_test_reward(
            123,
            100, // 100 ICP reward
            30,  // over 30 days
            1000, // 1000 ICP stake
            2920, // 8 years dissolve
            730,  // 2 years age
        );

        // Daily average: 100 / 30 = 3.33 ICP
        assert!((reward.daily_average() - 3.333).abs() < 0.01);

        // Weekly: 3.33 * 7 = 23.33 ICP
        assert!((reward.weekly_projected() - 23.333).abs() < 0.01);

        // Monthly: 3.33 * 30 = 100 ICP
        assert!((reward.monthly_projected() - 100.0).abs() < 0.01);

        // Reward rate: 100/1000 = 10%
        assert!((reward.reward_rate_percentage() - 10.0).abs() < 0.01);

        // Annualized: (10% / 30 days) * 365 = 121.67%
        assert!((reward.annualized_return() - 121.67).abs() < 0.1);

        assert!(!reward.has_zero_rewards());
    }

    #[test]
    fn test_zero_rewards() {
        let reward = create_test_reward(123, 0, 30, 1000, 2920, 730);
        assert!(reward.has_zero_rewards());
        assert_eq!(reward.daily_average(), 0.0);
    }

    #[test]
    fn test_reward_analysis_sorting() {
        use chrono::NaiveDate;
        let reward1 = create_test_reward(1, 50, 30, 1000, 2920, 730);
        let reward2 = create_test_reward(2, 100, 30, 1000, 2920, 730);
        let reward3 = create_test_reward(3, 75, 30, 1000, 2920, 730);

        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let analysis = RewardAnalysis::new(vec![reward1, reward2, reward3], 30, start_date, end_date, 30);

        // Should be sorted by total reward (highest first)
        assert_eq!(analysis.neuron_rewards[0].neuron_id.value(), 2); // 100 ICP
        assert_eq!(analysis.neuron_rewards[1].neuron_id.value(), 3); // 75 ICP
        assert_eq!(analysis.neuron_rewards[2].neuron_id.value(), 1); // 50 ICP

        assert_eq!(analysis.total_rewards.to_icp(), 225.0);
    }

    #[test]
    fn test_zero_reward_neurons_filter() {
        use chrono::NaiveDate;
        let reward1 = create_test_reward(1, 50, 30, 1000, 2920, 730);
        let reward2 = create_test_reward(2, 0, 30, 1000, 2920, 730);
        let reward3 = create_test_reward(3, 0, 30, 1000, 2920, 730);

        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let analysis = RewardAnalysis::new(vec![reward1, reward2, reward3], 30, start_date, end_date, 30);

        let zero_rewards = analysis.zero_reward_neurons();
        assert_eq!(zero_rewards.len(), 2);
    }

    #[test]
    fn test_top_performers() {
        use chrono::NaiveDate;
        let reward1 = create_test_reward(1, 50, 30, 1000, 2920, 730);
        let reward2 = create_test_reward(2, 100, 30, 1000, 2920, 730);
        let reward3 = create_test_reward(3, 75, 30, 1000, 2920, 730);

        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let analysis = RewardAnalysis::new(vec![reward1, reward2, reward3], 30, start_date, end_date, 30);

        let top2 = analysis.top_performers(2);
        assert_eq!(top2.len(), 2);
        assert_eq!(top2[0].neuron_id.value(), 2); // 100 ICP
        assert_eq!(top2[1].neuron_id.value(), 3); // 75 ICP
    }

    #[test]
    fn test_analysis_projections() {
        use chrono::NaiveDate;
        let reward1 = create_test_reward(1, 60, 30, 1000, 2920, 730);
        let reward2 = create_test_reward(2, 90, 30, 1000, 2920, 730);

        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let analysis = RewardAnalysis::new(vec![reward1, reward2], 30, start_date, end_date, 30);

        // Total 150 ICP over 30 days = 5 ICP/day
        assert!((analysis.average_daily_reward() - 5.0).abs() < 0.01);

        // Monthly: 5 * 30 = 150 ICP
        assert!((analysis.total_monthly_projected() - 150.0).abs() < 0.01);

        // Yearly: 5 * 365 = 1825 ICP
        assert!((analysis.total_yearly_projected() - 1825.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod divisor_tests {
    use super::*;

    fn reward(id: u64, total_icp: u64, days_tracked: i64) -> NeuronReward {
        NeuronReward::new(
            NeuronId::new(id),
            IcpAmount::from_e8s(total_icp * 100_000_000),
            days_tracked,
            IcpAmount::from_e8s(100 * 100_000_000),
            730,
            400,
            BonusMultiplier::new(1.07),
            BonusMultiplier::new(3.0),
        )
    }

    /// The defect: one neuron, two "daily" figures that must be equal.
    ///
    /// 13 days of history queried with `--days 30` printed a portfolio average of 0.0043
    /// against a per-neuron average of 0.0100 for the same single neuron.
    #[test]
    fn single_neuron_portfolio_average_equals_its_own_average() {
        let nr = reward(1, 13, 13);
        let expected = nr.daily_average();
        let analysis = RewardAnalysis::new(
            vec![nr],
            30, // requested window, wider than the history
            chrono::NaiveDate::from_ymd_opt(2026, 7, 7).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            13,
        );

        assert!(
            (analysis.average_daily_reward() - expected).abs() < 1e-12,
            "portfolio {} != neuron {}",
            analysis.average_daily_reward(),
            expected
        );
    }

    /// The rate must not change because the user asked for a wider window. Dividing by the
    /// requested period charged the average for days that were never observed.
    #[test]
    fn requested_window_does_not_change_the_rate() {
        let dates = (
            chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
        );
        let rate_for = |period_days: i64| {
            RewardAnalysis::new(vec![reward(1, 14, 14)], period_days, dates.0, dates.1, 14)
                .average_daily_reward()
        };

        let thirty = rate_for(30);
        let ninety = rate_for(90);
        assert!(
            (thirty - ninety).abs() < 1e-12,
            "same data, different --days: {thirty} vs {ninety}"
        );
        assert!((thirty - 1.0).abs() < 1e-12, "14 ICP over 14 observed days is 1.0/day");
    }

    /// Portfolio total is the sum of its parts, for more than one neuron.
    #[test]
    fn portfolio_average_sums_the_neuron_averages() {
        let a = reward(1, 20, 10); // 2.0/day
        let b = reward(2, 15, 15); // 1.0/day
        let analysis = RewardAnalysis::new(
            vec![a, b],
            90,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            15,
        );
        assert!((analysis.average_daily_reward() - 3.0).abs() < 1e-12);
        assert_eq!(analysis.observed_days(), 15, "widest observed history");
    }
}
