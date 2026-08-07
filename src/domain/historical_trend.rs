use super::value_objects::IcpAmount;
use chrono::NaiveDate;
use serde::{Serialize, Deserialize};

/// Value object representing a historical trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalTrend {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub start_maturity: IcpAmount,
    pub end_maturity: IcpAmount,
    pub days_in_period: i64,
    pub days_with_data: i64,
    pub missing_days: i64,
}

impl HistoricalTrend {
    pub fn new(
        start_date: NaiveDate,
        end_date: NaiveDate,
        start_maturity: IcpAmount,
        end_maturity: IcpAmount,
        days_with_data: i64,
    ) -> Self {
        let days_in_period = (end_date - start_date).num_days() + 1; // +1 to include both endpoints
        let missing_days = days_in_period - days_with_data;

        Self {
            start_date,
            end_date,
            start_maturity,
            end_maturity,
            days_in_period,
            days_with_data,
            missing_days,
        }
    }

    /// Calculate the delta (change) in maturity
    pub fn maturity_delta(&self) -> IcpAmount {
        self.end_maturity - self.start_maturity
    }

    /// Calculate average daily maturity growth
    pub fn average_daily_growth(&self) -> f64 {
        if self.days_in_period == 0 {
            return 0.0;
        }
        self.maturity_delta().to_icp() / self.days_in_period as f64
    }

    /// Calculate percentage growth rate
    pub fn growth_rate_percentage(&self) -> f64 {
        let start_icp = self.start_maturity.to_icp();
        if start_icp == 0.0 {
            return 0.0;
        }
        (self.maturity_delta().to_icp() / start_icp) * 100.0
    }

    /// Check if there is sufficient data for reliable analysis
    #[allow(dead_code)]
    pub fn has_sufficient_data(&self) -> bool {
        // Consider data sufficient if we have at least 50% coverage
        let coverage = self.days_with_data as f64 / self.days_in_period as f64;
        coverage >= 0.5
    }

    /// Get data quality level
    pub fn data_quality(&self) -> DataQuality {
        let coverage = self.days_with_data as f64 / self.days_in_period as f64;

        if coverage >= 0.95 {
            DataQuality::Excellent
        } else if coverage >= 0.80 {
            DataQuality::Good
        } else if coverage >= 0.60 {
            DataQuality::Moderate
        } else if coverage >= 0.40 {
            DataQuality::Low
        } else {
            DataQuality::Insufficient
        }
    }
}

/// Data quality indicator for historical trends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataQuality {
    Insufficient,
    Low,
    Moderate,
    Good,
    Excellent,
}

impl DataQuality {
    pub fn as_str(&self) -> &str {
        match self {
            DataQuality::Insufficient => "Insufficient",
            DataQuality::Low => "Low",
            DataQuality::Moderate => "Moderate",
            DataQuality::Good => "Good",
            DataQuality::Excellent => "Excellent",
        }
    }

    pub fn needs_warning(&self) -> bool {
        matches!(self, DataQuality::Insufficient | DataQuality::Low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_historical_trend_calculations() {
        let start_date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2025, 1, 30).unwrap();
        let start_maturity = IcpAmount::from_e8s(100_000_000_000); // 1000 ICP
        let end_maturity = IcpAmount::from_e8s(115_000_000_000); // 1150 ICP
        let days_with_data = 28; // Missing 2 days

        let trend = HistoricalTrend::new(
            start_date,
            end_date,
            start_maturity,
            end_maturity,
            days_with_data,
        );

        // Test delta calculation
        assert_eq!(trend.maturity_delta().to_icp(), 150.0);

        // Test average daily growth (150 ICP over 30 days)
        assert!((trend.average_daily_growth() - 5.0).abs() < 0.001);

        // Test growth rate (150/1000 = 15%)
        assert!((trend.growth_rate_percentage() - 15.0).abs() < 0.001);

        // Test missing days
        assert_eq!(trend.missing_days, 2);
        assert_eq!(trend.days_with_data, 28);
    }

    #[test]
    fn test_data_quality_levels() {
        let start_date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2025, 1, 30).unwrap();
        let maturity = IcpAmount::from_e8s(100_000_000_000);

        // Excellent: 95%+ (29/30 days)
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 29);
        assert_eq!(trend.data_quality(), DataQuality::Excellent);

        // Good: 80%+ (25/30 days)
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 25);
        assert_eq!(trend.data_quality(), DataQuality::Good);

        // Moderate: 60%+ (20/30 days)
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 20);
        assert_eq!(trend.data_quality(), DataQuality::Moderate);

        // Low: 40%+ (15/30 days)
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 15);
        assert_eq!(trend.data_quality(), DataQuality::Low);

        // Insufficient: <40% (10/30 days)
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 10);
        assert_eq!(trend.data_quality(), DataQuality::Insufficient);
    }

    #[test]
    fn test_has_sufficient_data() {
        let start_date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2025, 1, 30).unwrap();
        let maturity = IcpAmount::from_e8s(100_000_000_000);

        // 20/30 = 66.7% coverage - sufficient
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 20);
        assert!(trend.has_sufficient_data());

        // 10/30 = 33.3% coverage - insufficient
        let trend = HistoricalTrend::new(start_date, end_date, maturity, maturity, 10);
        assert!(!trend.has_sufficient_data());
    }

    #[test]
    fn test_zero_start_maturity() {
        let start_date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2025, 1, 30).unwrap();
        let start_maturity = IcpAmount::from_e8s(0);
        let end_maturity = IcpAmount::from_e8s(100_000_000_000);

        let trend = HistoricalTrend::new(
            start_date,
            end_date,
            start_maturity,
            end_maturity,
            30,
        );

        // Growth rate should be 0.0 when starting from zero to avoid division by zero
        assert_eq!(trend.growth_rate_percentage(), 0.0);
        assert_eq!(trend.average_daily_growth(), 1000.0 / 30.0);
    }
}
