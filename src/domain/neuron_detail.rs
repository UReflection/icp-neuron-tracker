use super::value_objects::*;
use super::neuron::Neuron;
use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};

/// Reward history for different time periods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardHistory {
    pub days_7: IcpAmount,
    pub days_30: IcpAmount,
    pub days_7_daily_avg: f64,
    pub days_30_daily_avg: f64,
}

impl RewardHistory {
    pub fn new(
        days_7_total: IcpAmount,
        days_30_total: IcpAmount,
        days_7_actual: i64,
        days_30_actual: i64,
    ) -> Self {
        let days_7_daily_avg = if days_7_actual > 0 {
            days_7_total.to_icp() / days_7_actual as f64
        } else {
            0.0
        };

        let days_30_daily_avg = if days_30_actual > 0 {
            days_30_total.to_icp() / days_30_actual as f64
        } else {
            0.0
        };

        Self {
            days_7: days_7_total,
            days_30: days_30_total,
            days_7_daily_avg,
            days_30_daily_avg,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.days_7.e8s() == 0 && self.days_30.e8s() == 0
    }
}

/// Indicates if neuron is nearing dissolve date
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DissolveStatus {
    /// Not dissolving (locked)
    Locked,
    /// Dissolving, with days remaining
    Dissolving { days_remaining: u64 },
    /// Dissolved
    Dissolved,
}

impl DissolveStatus {
    /// Returns true if dissolving and within warning threshold (30 days)
    pub fn is_near_dissolve(&self) -> bool {
        matches!(self, DissolveStatus::Dissolving { days_remaining } if *days_remaining <= 30)
    }

    #[allow(dead_code)]
    pub fn days_until_dissolve(&self) -> Option<u64> {
        match self {
            DissolveStatus::Dissolving { days_remaining } => Some(*days_remaining),
            _ => None,
        }
    }
}

/// Comprehensive neuron detail aggregate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuronDetail {
    pub neuron_id: NeuronId,
    pub stake: IcpAmount,
    pub maturity: IcpAmount,
    pub staked_maturity: IcpAmount,
    pub total_value: IcpAmount,
    pub voting_power: u64,
    pub dissolve_delay_days: u64,
    pub age_days: u64,
    pub age_bonus: BonusMultiplier,
    pub dissolve_bonus: BonusMultiplier,
    pub combined_bonus: BonusMultiplier,
    pub state: NeuronState,
    pub auto_stake_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub dissolve_status: DissolveStatus,
    pub reward_history: Option<RewardHistory>,
}

impl NeuronDetail {
    pub fn new(
        neuron: Neuron,
        dissolve_status: DissolveStatus,
        reward_history: Option<RewardHistory>,
    ) -> Self {
        let total_value = neuron.stake() + neuron.maturity() + neuron.staked_maturity();
        let combined_bonus = neuron.age_bonus() * neuron.dissolve_bonus();

        Self {
            neuron_id: neuron.id(),
            stake: neuron.stake(),
            maturity: neuron.maturity(),
            staked_maturity: neuron.staked_maturity(),
            total_value,
            voting_power: neuron.voting_power(),
            dissolve_delay_days: neuron.dissolve_delay_days(),
            age_days: neuron.age_days(),
            age_bonus: neuron.age_bonus(),
            dissolve_bonus: neuron.dissolve_bonus(),
            combined_bonus,
            state: neuron.state(),
            auto_stake_enabled: neuron.auto_stake_enabled(),
            created_at: neuron.created_at(),
            dissolve_status,
            reward_history,
        }
    }

    /// Get age in years (decimal)
    pub fn age_years(&self) -> f64 {
        self.age_days as f64 / 365.25
    }

    /// Get dissolve delay in years (decimal)
    pub fn dissolve_delay_years(&self) -> f64 {
        self.dissolve_delay_days as f64 / 365.25
    }

    /// Check if neuron has earned any rewards
    #[allow(dead_code)]
    pub fn has_reward_data(&self) -> bool {
        self.reward_history.is_some()
    }

    /// Check if neuron has zero rewards in both periods
    pub fn has_zero_rewards(&self) -> bool {
        self.reward_history
            .as_ref()
            .map(|h| h.is_empty())
            .unwrap_or(true)
    }

    /// Get estimated dissolve date (if dissolving)
    pub fn estimated_dissolve_date(&self) -> Option<DateTime<Utc>> {
        match self.dissolve_status {
            DissolveStatus::Dissolving { days_remaining } => {
                Some(Utc::now() + Duration::days(days_remaining as i64))
            }
            _ => None,
        }
    }

    /// Calculate voting rewards per ICP stake (if reward data available)
    pub fn reward_rate_7day(&self) -> Option<f64> {
        self.reward_history.as_ref().map(|h| {
            if self.stake.to_icp() > 0.0 {
                (h.days_7.to_icp() / self.stake.to_icp()) * 100.0 // percentage
            } else {
                0.0
            }
        })
    }

    /// Calculate annualized reward rate based on 30-day history
    pub fn annualized_return(&self) -> Option<f64> {
        self.reward_history.as_ref().map(|h| {
            if self.stake.to_icp() > 0.0 {
                let daily_rate = h.days_30_daily_avg / self.stake.to_icp();
                // Class B annualisation: 365.0 is deliberate financial convention, NOT a calendar
                // conversion. Do not change to 365.25 — see BonusMultiplier in domain/value_objects.rs.
                daily_rate * 365.0 * 100.0 // percentage
            } else {
                0.0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dissolve_status_near_dissolve() {
        let locked = DissolveStatus::Locked;
        assert!(!locked.is_near_dissolve());

        let dissolving_far = DissolveStatus::Dissolving { days_remaining: 365 };
        assert!(!dissolving_far.is_near_dissolve());

        let dissolving_near = DissolveStatus::Dissolving { days_remaining: 15 };
        assert!(dissolving_near.is_near_dissolve());

        let dissolved = DissolveStatus::Dissolved;
        assert!(!dissolved.is_near_dissolve());
    }

    #[test]
    fn test_reward_history() {
        let history = RewardHistory::new(
            IcpAmount::from_e8s(700_000_000), // 7 ICP
            IcpAmount::from_e8s(3_000_000_000), // 30 ICP
            7,
            30,
        );

        assert_eq!(history.days_7_daily_avg, 1.0); // 7 ICP / 7 days
        assert_eq!(history.days_30_daily_avg, 1.0); // 30 ICP / 30 days
        assert!(!history.is_empty());
    }

    #[test]
    fn test_reward_history_empty() {
        let history = RewardHistory::new(
            IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(0),
            7,
            30,
        );

        assert!(history.is_empty());
    }

    #[test]
    fn test_neuron_detail_calculations() {
        let neuron = Neuron::new(
            NeuronId::new(12345),
            IcpAmount::from_e8s(100_000_000_000), // 1000 ICP stake
            IcpAmount::from_e8s(10_000_000_000),  // 100 ICP maturity
            IcpAmount::from_e8s(5_000_000_000),   // 50 ICP staked maturity
            1250 * 100_000_000, // voting power
            365 * 86400 * 2,    // 2 years age
            365 * 86400 * 8,    // 8 years dissolve delay
            NeuronState::Locked,
            true,
            1640000000, // created timestamp
        );

        let reward_history = RewardHistory::new(
            IcpAmount::from_e8s(700_000_000), // 7 ICP in 7 days
            IcpAmount::from_e8s(3_000_000_000), // 30 ICP in 30 days
            7,
            30,
        );

        let detail = NeuronDetail::new(
            neuron,
            DissolveStatus::Locked,
            Some(reward_history),
        );

        assert_eq!(detail.total_value.to_icp(), 1150.0);
        assert!((detail.age_years() - 2.0).abs() < 0.01);
        assert!((detail.dissolve_delay_years() - 8.0).abs() < 0.01);
        assert!(detail.has_reward_data());
        assert!(!detail.has_zero_rewards());

        // Reward rate: 7 ICP / 1000 ICP = 0.7% for 7 days
        assert!((detail.reward_rate_7day().unwrap() - 0.7).abs() < 0.01);

        // Annualized: (30 ICP / 30 days) / 1000 ICP * 365 days * 100 = 36.5%
        assert!((detail.annualized_return().unwrap() - 36.5).abs() < 0.01);
    }
}
