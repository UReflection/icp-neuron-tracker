use super::portfolio::Portfolio;
use super::value_objects::*;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Aggregate containing portfolio report data and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioReport {
    #[allow(dead_code)]
    pub portfolio: Portfolio,
    pub generated_at: DateTime<Utc>,
    pub metrics: ReportMetrics,
}

/// Report metrics calculated from portfolio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetrics {
    pub total_stake: IcpAmount,
    pub total_maturity: IcpAmount,
    pub total_staked_maturity: IcpAmount,
    pub total_voting_power: u64,
    pub neuron_count: usize,
    pub dissolve_delay_range: Option<DissolveDelayRange>,
    pub age_range: Option<AgeRange>,
}

/// Value object for dissolve delay range
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DissolveDelayRange {
    pub min_days: u64,
    pub max_days: u64,
}

impl DissolveDelayRange {
    pub fn new(min_days: u64, max_days: u64) -> Self {
        Self { min_days, max_days }
    }

    pub fn format_readable(&self) -> String {
        let min_desc = Self::format_duration(self.min_days);
        let max_desc = Self::format_duration(self.max_days);
        format!("{} to {}", min_desc, max_desc)
    }

    fn format_duration(days: u64) -> String {
        if days == 0 {
            return "0 days".to_string();
        }

        let years = days / 365;
        let remaining_days = days % 365;
        let months = remaining_days / 30;

        if years > 0 && months > 0 {
            format!("{} years {} months", years, months)
        } else if years > 0 {
            format!("{} years", years)
        } else if months > 0 {
            format!("{} months", months)
        } else {
            format!("{} days", days)
        }
    }
}

/// Value object for age range
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AgeRange {
    pub min_days: u64,
    pub max_days: u64,
    pub average_days: f64,
}

impl AgeRange {
    pub fn new(min_days: u64, max_days: u64, average_days: f64) -> Self {
        Self { min_days, max_days, average_days }
    }

    pub fn average_years(&self) -> f64 {
        self.average_days / 365.25
    }

    pub fn min_years(&self) -> f64 {
        self.min_days as f64 / 365.25
    }

    pub fn max_years(&self) -> f64 {
        self.max_days as f64 / 365.25
    }
}

impl PortfolioReport {
    /// Create a new portfolio report with calculated metrics
    pub fn new(portfolio: Portfolio) -> Self {
        let neurons = portfolio.neurons();

        let dissolve_delay_range = if !neurons.is_empty() {
            let min = neurons.iter().map(|n| n.dissolve_delay_days()).min().unwrap();
            let max = neurons.iter().map(|n| n.dissolve_delay_days()).max().unwrap();
            Some(DissolveDelayRange::new(min, max))
        } else {
            None
        };

        let age_range = if !neurons.is_empty() {
            let min = neurons.iter().map(|n| n.age_days()).min().unwrap();
            let max = neurons.iter().map(|n| n.age_days()).max().unwrap();
            let sum: u64 = neurons.iter().map(|n| n.age_days()).sum();
            let average = sum as f64 / neurons.len() as f64;
            Some(AgeRange::new(min, max, average))
        } else {
            None
        };

        // Calculate total maturity as sum of staked + available
        let total_staked = portfolio.total_staked_maturity();
        let total_available = portfolio.total_maturity(); // This is available maturity
        let total_maturity = total_staked + total_available;

        let metrics = ReportMetrics {
            total_stake: portfolio.total_stake(),
            total_maturity,
            total_staked_maturity: total_staked,
            total_voting_power: portfolio.total_voting_power(),
            neuron_count: portfolio.neuron_count(),
            dissolve_delay_range,
            age_range,
        };

        Self {
            portfolio,
            generated_at: Utc::now(),
            metrics,
        }
    }

    pub fn available_maturity(&self) -> IcpAmount {
        // Available maturity = total maturity - staked maturity
        self.metrics.total_maturity - self.metrics.total_staked_maturity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Neuron, NeuronState};

    #[test]
    fn test_dissolve_delay_range_format() {
        let range = DissolveDelayRange::new(180, 2920); // 6 months to 8 years
        let formatted = range.format_readable();
        assert!(formatted.contains("months"));
        assert!(formatted.contains("years"));
    }

    #[test]
    fn test_portfolio_report_empty() {
        let portfolio = Portfolio::new(vec![]);
        let report = PortfolioReport::new(portfolio);

        assert_eq!(report.metrics.neuron_count, 0);
        assert!(report.metrics.dissolve_delay_range.is_none());
        assert!(report.metrics.age_range.is_none());
    }

    #[test]
    fn test_portfolio_report_with_neurons() {
        let neuron1 = Neuron::new(
            NeuronId::new(123),
            IcpAmount::from_e8s(100_000_000_000),
            IcpAmount::from_e8s(1_000_000_000),
            IcpAmount::from_e8s(500_000_000),
            1_000_000,
            365 * 86400, // 1 year
            730 * 86400, // 2 years dissolve
            NeuronState::Locked,
            true,
            1600000000,
        );

        let neuron2 = Neuron::new(
            NeuronId::new(456),
            IcpAmount::from_e8s(200_000_000_000),
            IcpAmount::from_e8s(2_000_000_000),
            IcpAmount::from_e8s(1_000_000_000),
            2_000_000,
            730 * 86400, // 2 years
            1460 * 86400, // 4 years dissolve
            NeuronState::Locked,
            true,
            1600000000,
        );

        let portfolio = Portfolio::new(vec![neuron1, neuron2]);
        let report = PortfolioReport::new(portfolio);

        assert_eq!(report.metrics.neuron_count, 2);
        assert!(report.metrics.dissolve_delay_range.is_some());
        assert!(report.metrics.age_range.is_some());

        let dissolve_range = report.metrics.dissolve_delay_range.unwrap();
        assert_eq!(dissolve_range.min_days, 730);
        assert_eq!(dissolve_range.max_days, 1460);

        let age_range = report.metrics.age_range.unwrap();
        assert_eq!(age_range.min_days, 365);
        assert_eq!(age_range.max_days, 730);
        assert_eq!(age_range.average_days, 547.5); // (365 + 730) / 2
    }

    #[test]
    fn test_available_maturity_calculation() {
        let neuron = Neuron::new(
            NeuronId::new(123),
            IcpAmount::from_e8s(100_000_000_000),
            IcpAmount::from_e8s(1_000_000_000), // 10 ICP available maturity
            IcpAmount::from_e8s(2_000_000_000), // 20 ICP staked maturity
            1_000_000,
            365 * 86400,
            730 * 86400,
            NeuronState::Locked,
            true,
            1600000000,
        );

        let portfolio = Portfolio::new(vec![neuron]);
        let report = PortfolioReport::new(portfolio);

        // Total maturity should be 30 ICP (10 available + 20 staked)
        assert_eq!(report.metrics.total_maturity.to_icp(), 30.0);
        // Available maturity should be 10 ICP
        assert_eq!(report.available_maturity().to_icp(), 10.0);
    }
}
