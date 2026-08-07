use super::value_objects::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Neuron aggregate root
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neuron {
    id: NeuronId,
    stake: IcpAmount,
    maturity: IcpAmount,
    staked_maturity: IcpAmount,
    voting_power: u64,
    age_days: u64,
    dissolve_delay_days: u64,
    age_bonus: BonusMultiplier,
    dissolve_bonus: BonusMultiplier,
    state: NeuronState,
    auto_stake_enabled: bool,
    created_at: DateTime<Utc>,
    retrieved_at: DateTime<Utc>,
}

impl Neuron {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: NeuronId,
        stake: IcpAmount,
        maturity: IcpAmount,
        staked_maturity: IcpAmount,
        voting_power: u64,
        age_seconds: u64,
        dissolve_delay_seconds: u64,
        state: NeuronState,
        auto_stake_enabled: bool,
        created_timestamp: u64,
    ) -> Self {
        let age_bonus = BonusMultiplier::from_age_seconds(age_seconds);
        let dissolve_bonus = BonusMultiplier::from_dissolve_seconds(dissolve_delay_seconds);

        Self {
            id,
            stake,
            maturity,
            staked_maturity,
            voting_power,
            age_days: age_seconds / 86400,
            dissolve_delay_days: dissolve_delay_seconds / 86400,
            age_bonus,
            dissolve_bonus,
            state,
            auto_stake_enabled,
            created_at: DateTime::from_timestamp(created_timestamp as i64, 0)
                .unwrap_or_else(Utc::now),
            retrieved_at: Utc::now(),
        }
    }

    /// Reconstruct a neuron from a stored snapshot, preserving when it was observed.
    ///
    /// `Neuron::new` stamps `retrieved_at` with the current time, which is correct for a
    /// neuron just fetched from the chain but wrong for one loaded out of the database —
    /// it discards the observation time. `retrieved_timestamp` is the only durable record
    /// of when a snapshot was actually taken, so the read path must carry it through.
    #[allow(clippy::too_many_arguments)]
    pub fn from_snapshot(
        id: NeuronId,
        stake: IcpAmount,
        maturity: IcpAmount,
        staked_maturity: IcpAmount,
        voting_power: u64,
        age_seconds: u64,
        dissolve_delay_seconds: u64,
        state: NeuronState,
        auto_stake_enabled: bool,
        created_timestamp: u64,
        retrieved_timestamp: u64,
    ) -> Self {
        let mut neuron = Self::new(
            id,
            stake,
            maturity,
            staked_maturity,
            voting_power,
            age_seconds,
            dissolve_delay_seconds,
            state,
            auto_stake_enabled,
            created_timestamp,
        );
        neuron.retrieved_at = DateTime::from_timestamp(retrieved_timestamp as i64, 0)
            .unwrap_or_else(Utc::now);
        neuron
    }

    // Getters
    pub fn id(&self) -> NeuronId {
        self.id
    }

    pub fn total_value(&self) -> IcpAmount {
        self.stake + self.maturity + self.staked_maturity
    }

    pub fn total_rewards(&self) -> IcpAmount {
        self.maturity + self.staked_maturity
    }

    pub fn combined_multiplier(&self) -> BonusMultiplier {
        self.age_bonus * self.dissolve_bonus
    }

    pub fn stake(&self) -> IcpAmount {
        self.stake
    }

    pub fn maturity(&self) -> IcpAmount {
        self.maturity
    }

    pub fn staked_maturity(&self) -> IcpAmount {
        self.staked_maturity
    }

    pub fn voting_power(&self) -> u64 {
        self.voting_power
    }

    pub fn age_days(&self) -> u64 {
        self.age_days
    }

    pub fn dissolve_delay_days(&self) -> u64 {
        self.dissolve_delay_days
    }

    pub fn age_bonus(&self) -> BonusMultiplier {
        self.age_bonus
    }

    pub fn dissolve_bonus(&self) -> BonusMultiplier {
        self.dissolve_bonus
    }

    pub fn state(&self) -> NeuronState {
        self.state
    }

    pub fn auto_stake_enabled(&self) -> bool {
        self.auto_stake_enabled
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    
    pub fn retrieved_at(&self) -> DateTime<Utc> {
        self.retrieved_at
    }

    pub fn created_date_formatted(&self) -> String {
        self.created_at.format("%Y-%m-%d").to_string()
    }
}