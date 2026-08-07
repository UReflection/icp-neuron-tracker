use super::neuron::Neuron;
use super::value_objects::*;
use serde::{Serialize, Deserialize};

/// Portfolio aggregate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    neurons: Vec<Neuron>,
}

impl Portfolio {
    pub fn new(neurons: Vec<Neuron>) -> Self {
        Self { neurons }
    }

    pub fn neurons(&self) -> &[Neuron] {
        &self.neurons
    }

    pub fn neuron_count(&self) -> usize {
        self.neurons.len()
    }

    pub fn total_stake(&self) -> IcpAmount {
        self.neurons
            .iter()
            .map(|n| n.stake())
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt)
    }

    pub fn total_maturity(&self) -> IcpAmount {
        self.neurons
            .iter()
            .map(|n| n.maturity())
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt)
    }

    pub fn total_staked_maturity(&self) -> IcpAmount {
        self.neurons
            .iter()
            .map(|n| n.staked_maturity())
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt)
    }

    pub fn total_value(&self) -> IcpAmount {
        self.neurons
            .iter()
            .map(|n| n.total_value())
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt)
    }

    pub fn total_rewards(&self) -> IcpAmount {
        self.neurons
            .iter()
            .map(|n| n.total_rewards())
            .fold(IcpAmount::from_e8s(0), |acc, amt| acc + amt)
    }

    pub fn total_voting_power(&self) -> u64 {
        self.neurons.iter().map(|n| n.voting_power()).sum()
    }

    pub fn overall_return_percentage(&self) -> f64 {
        let total_stake = self.total_stake().to_icp();
        if total_stake == 0.0 {
            return 0.0;
        }
        (self.total_rewards().to_icp() / total_stake) * 100.0
    }
}