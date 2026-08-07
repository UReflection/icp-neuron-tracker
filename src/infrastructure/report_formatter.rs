use crate::application::tracking_service::DailyIncomeStats;
use crate::domain::{PortfolioReport, HistoricalTrend, RewardAnalysis, NeuronDetail, DissolveStatus};
use chrono::{DateTime, NaiveDate, Utc};

/// Formatter for displaying portfolio reports in terminal
pub struct TerminalReportFormatter;

impl TerminalReportFormatter {
    /// Format the post-`track` income panel.
    ///
    /// Every line must describe the figure actually computed: total rewards over the last
    /// `records_used` reward rows per neuron, divided by the days those rows account for.
    /// That divisor is the sum of their `days_elapsed` — not the row count, and not a fixed
    /// 30. In the production database 30 rows spanned 247 days.
    ///
    /// The panel previously carried three mutually contradictory claims at once: a header of
    /// "30-day average", a trailing 30-day "Period:" the figure was not drawn from, and a
    /// "Data points: 1/30 days" counter measuring a third thing again. None described the
    /// arithmetic, and the APY built on the figure inherited the confusion.
    pub fn format_income_analysis(stats: &DailyIncomeStats, snapshot_date: NaiveDate) -> String {
        let mut output = String::new();

        output.push_str("\nIncome Analysis (mean daily rate)\n");
        match (stats.span_start, stats.span_end) {
            (Some(start), Some(end)) => {
                output.push_str(&format!(
                    "Last updated: {} | {} reward records per neuron\n",
                    snapshot_date, stats.records_used
                ));
                output.push_str(&format!(
                    "Covering: {} to {} ({} days of accrual)\n",
                    start, end, stats.days_covered
                ));
            }
            _ => {
                output.push_str(&format!(
                    "Last updated: {} | no reward records available\n",
                    snapshot_date
                ));
            }
        }
        output.push_str("─────────────────────────────────────────────────────────────\n");
        output.push_str(&format!("  Avg Daily Income       {:.4} ICP/day\n", stats.total_daily_icp));
        output.push_str(&format!("  Annual Projection      {:.2} ICP/year\n", stats.total_annual_icp));
        output.push_str(&format!("  Effective APY          {:.2}%\n", stats.effective_apy));
        output.push('\n');
        output.push_str(&format!(
            "Note: total rewards over those records divided by the {} days they\n",
            stats.days_covered
        ));
        output.push_str("      account for. ICP batches voting rewards, so a single record\n");
        output.push_str("      can carry several days' accrual; dividing by days rather than\n");
        output.push_str("      by records keeps the rate comparable across that batching.\n");
        if !stats.uniform_spans {
            output.push('\n');
            output.push_str("      Neurons differ in how much history they have, so this total\n");
            output.push_str("      sums rates measured over different spans. The dates above\n");
            output.push_str("      are the widest of them.\n");
        }
        output.push('\n');

        output
    }

    /// Format a portfolio summary report for terminal display
    ///
    /// Formats output to fit within 80 columns for standard terminal width.
    /// Uses clear section headers and aligned values for readability.
    pub fn format_summary(report: &PortfolioReport) -> String {
        let mut output = String::new();

        // Header
        output.push_str("============================================================\n");
        output.push_str("Portfolio Summary\n");
        output.push_str("============================================================\n\n");

        // Timestamp. "Report Generated" is when this report ran; "Data Retrieved" is when the
        // underlying snapshot was actually observed. These are not the same thing and the
        // difference matters — a report generated today off six-month-old snapshots previously
        // rendered as "Last Updated: <today>", which reads as fresh data.
        let timestamp = report.generated_at.format("%Y-%m-%d %H:%M:%S UTC");
        output.push_str(&format!("Report Generated: {}\n", timestamp));

        if let Some(retrieved) = Self::latest_retrieval(report) {
            let age_days = (report.generated_at - retrieved).num_days();
            let staleness = match age_days {
                d if d <= 1 => String::new(),
                d => format!(" ({} days ago)", d),
            };
            output.push_str(&format!(
                "Data Retrieved:   {}{}\n",
                retrieved.format("%Y-%m-%d %H:%M:%S UTC"),
                staleness
            ));
        }
        output.push('\n');

        // Basic metrics
        output.push_str(&format!("Total Neurons: {}\n", report.metrics.neuron_count));
        output.push_str(&format!("Total Stake: {} ICP\n", Self::format_icp(report.metrics.total_stake.to_icp())));
        output.push_str(&format!("Total Maturity: {} ICP\n", Self::format_icp(report.metrics.total_maturity.to_icp())));

        // Maturity breakdown
        let available_maturity = report.available_maturity().to_icp();
        output.push_str(&format!("  - Staked Maturity: {} ICP\n", Self::format_icp(report.metrics.total_staked_maturity.to_icp())));
        output.push_str(&format!("  - Available Maturity: {} ICP\n", Self::format_icp(available_maturity)));

        // Format voting power in human-readable form (billions)
        let voting_power_billions = report.metrics.total_voting_power as f64 / 1_000_000_000.0;
        output.push_str(&format!("Total Voting Power: {:.2}B\n\n", voting_power_billions));

        // Dissolve delay range
        if let Some(dissolve_range) = report.metrics.dissolve_delay_range {
            output.push_str(&format!("Dissolve Delay Range: {}\n", dissolve_range.format_readable()));
        }

        // Age statistics
        if let Some(age_range) = report.metrics.age_range {
            output.push_str(&format!("Average Age: {:.1} years\n", age_range.average_years()));
            output.push_str(&format!("Age Range: {:.1} to {:.1} years\n", age_range.min_years(), age_range.max_years()));
        }

        output.push_str("\n============================================================\n");

        output
    }

    /// Format a historical trend report for terminal display
    ///
    /// Shows maturity growth over the specified period with warnings for missing data.
    pub fn format_historical(trend: &HistoricalTrend, requested_days: u32) -> String {
        let mut output = String::new();

        // Header
        output.push_str("============================================================\n");
        output.push_str(&format!("Maturity Growth - Last {} Days\n", requested_days));
        output.push_str("============================================================\n\n");

        // Date range
        output.push_str(&format!("Period: {} to {}\n\n",
            trend.start_date.format("%Y-%m-%d"),
            trend.end_date.format("%Y-%m-%d")));

        // Data quality warning
        let quality = trend.data_quality();
        if quality.needs_warning() {
            output.push_str(&format!("⚠ Data Quality: {} ({}/{} days)\n",
                quality.as_str(),
                trend.days_with_data,
                trend.days_in_period));
            output.push_str("  Consider tracking more consistently for reliable analysis.\n\n");
        }

        // Core metrics
        output.push_str(&format!("Starting Maturity: {} ICP\n",
            Self::format_icp(trend.start_maturity.to_icp())));
        output.push_str(&format!("Ending Maturity: {} ICP\n",
            Self::format_icp(trend.end_maturity.to_icp())));

        let delta = trend.maturity_delta().to_icp();
        let sign = if delta >= 0.0 { "+" } else { "" };
        output.push_str(&format!("Growth: {}{} ICP ({}{:.2}%)\n\n",
            sign,
            Self::format_icp(delta.abs()),
            sign,
            trend.growth_rate_percentage()));

        // Daily average
        output.push_str(&format!("Average Daily Growth: {} ICP\n",
            Self::format_icp(trend.average_daily_growth())));

        // Projections
        let daily_avg = trend.average_daily_growth();
        if daily_avg > 0.0 {
            let weekly = daily_avg * 7.0;
            let monthly = daily_avg * 30.0;
            // Class B annualisation: 365.0 is deliberate financial convention, NOT a calendar
            // conversion. Do not change to 365.25 — see BonusMultiplier in domain/value_objects.rs.
            let yearly = daily_avg * 365.0;

            output.push_str(&format!("Projected Weekly: {} ICP\n", Self::format_icp(weekly)));
            output.push_str(&format!("Projected Monthly: {} ICP\n", Self::format_icp(monthly)));
            output.push_str(&format!("Projected Yearly: {} ICP\n", Self::format_icp(yearly)));
        }

        // Missing data note
        if trend.missing_days > 0 {
            output.push_str(&format!("\nNote: {} day(s) with missing snapshots\n",
                trend.missing_days));
        }

        // Data coverage
        let coverage = (trend.days_with_data as f64 / trend.days_in_period as f64) * 100.0;
        output.push_str(&format!("Data Coverage: {:.1}% ({} days)\n",
            coverage,
            trend.days_with_data));

        output.push_str("\n============================================================\n");

        output
    }

    /// Format a reward analysis report for terminal display
    ///
    /// Shows ranked neurons by reward performance with projections.
    pub fn format_rewards(analysis: &RewardAnalysis, requested_days: u32) -> String {
        let mut output = String::new();

        // Header
        output.push_str("============================================================\n");
        output.push_str(&format!("Reward Analysis - Last {} Days\n", requested_days));
        output.push_str("============================================================\n");
        output.push_str(&format!("Period: {} to {}\n", analysis.start_date, analysis.end_date));
        output.push_str(&format!("Data points: {} snapshots\n", analysis.snapshot_count));
        output.push_str("\nNote: ICP batches voting rewards periodically. Rewards shown\n");
        output.push_str("      may include accumulated maturity from multiple voting sessions.\n");
        output.push_str("============================================================\n\n");

        // Summary
        output.push_str(&format!("Total Rewards: {} ICP\n", Self::format_icp(analysis.total_rewards.to_icp())));
        // Name the divisor. It is the days observed, not the days requested — and when those
        // differ, saying "Average Daily" alone invites the reader to assume the window.
        output.push_str(&format!(
            "Average Daily: {} ICP  (over {} day(s) with data)\n",
            Self::format_icp(analysis.average_daily_reward()),
            analysis.observed_days()
        ));
        output.push_str(&format!("Projected Monthly: {} ICP\n", Self::format_icp(analysis.total_monthly_projected())));
        output.push_str(&format!("Projected Yearly: {} ICP\n\n", Self::format_icp(analysis.total_yearly_projected())));

        // Check for zero reward neurons
        let zero_neurons = analysis.zero_reward_neurons();
        if !zero_neurons.is_empty() {
            let with_data = zero_neurons.iter().filter(|nr| nr.days_tracked > 0).count();
            let without_data = zero_neurons.len() - with_data;
            if with_data > 0 {
                output.push_str(&format!(
                    "⚠  {} neuron(s) earned nothing despite having data in this period\n",
                    with_data));
            }
            if without_data > 0 {
                output.push_str(&format!(
                    "⚠  {} neuron(s) have no data in this period — widen --days or run `track`\n",
                    without_data));
            }
            output.push('\n');
        }

        // Neuron rankings
        output.push_str("Neuron Rankings (by total reward):\n");
        output.push_str("------------------------------------------------------------\n");

        for (rank, neuron_reward) in analysis.neuron_rewards.iter().enumerate() {
            let rank_num = rank + 1;

            // Rank and neuron ID
            output.push_str(&format!("\n#{} Neuron {}\n", rank_num, neuron_reward.neuron_id.value()));

            // Reward info
            output.push_str(&format!("  Total Reward: {} ICP\n", Self::format_icp(neuron_reward.total_reward.to_icp())));
            output.push_str(&format!("  Daily Average: {} ICP\n", Self::format_icp(neuron_reward.daily_average())));
            output.push_str(&format!("  Weekly: {} ICP | Monthly: {} ICP\n",
                Self::format_icp(neuron_reward.weekly_projected()),
                Self::format_icp(neuron_reward.monthly_projected())));

            // Reward rate
            output.push_str(&format!("  Reward Rate: {:.2}% (Annualized: {:.2}%)\n",
                neuron_reward.reward_rate_percentage(),
                neuron_reward.annualized_return()));

            // Configuration
            output.push_str(&format!("  Stake: {} ICP\n", Self::format_icp(neuron_reward.stake.to_icp())));
            output.push_str(&format!("  Bonuses: Age {:.2}x | Dissolve {:.2}x\n",
                neuron_reward.age_bonus.value(),
                neuron_reward.dissolve_bonus.value()));
            output.push_str(&format!("  Age: {:.1}y | Dissolve Delay: {:.1}y\n",
                neuron_reward.age_years(),
                neuron_reward.dissolve_delay_years()));

            // Zero-reward warning. Only point at configuration when this neuron actually had
            // observed days in the window — with no days, zero is an absence of data, not a
            // finding about the neuron, and blaming configuration is a misdiagnosis.
            if neuron_reward.has_zero_rewards() {
                if neuron_reward.days_tracked > 0 {
                    output.push_str("  ⚠  WARNING: No rewards earned over ");
                    output.push_str(&format!("{} day(s) of data (check configuration)\n",
                        neuron_reward.days_tracked));
                } else {
                    output.push_str("  ⚠  No data for this neuron in the period (not a ");
                    output.push_str("configuration problem)\n");
                }
            }
        }

        output.push_str("\n============================================================\n");

        output
    }

    /// Format ICP amount with appropriate decimal precision and thousand separators
    fn format_icp(amount: f64) -> String {
        let formatted = if amount >= 1.0 {
            format!("{:.2}", amount)
        } else if amount >= 0.0001 {
            format!("{:.4}", amount)
        } else {
            format!("{:.8}", amount)
        };

        // Add thousand separators for amounts >= 1000
        if amount >= 1000.0 {
            Self::add_thousand_separators(&formatted)
        } else {
            formatted
        }
    }

    /// Add thousand separators to a formatted number string
    fn add_thousand_separators(s: &str) -> String {
        if let Some(dot_pos) = s.find('.') {
            let integer_part = &s[..dot_pos];
            let decimal_part = &s[dot_pos..];
            let formatted_integer = Self::format_number_str(integer_part);
            format!("{}{}", formatted_integer, decimal_part)
        } else {
            Self::format_number_str(s)
        }
    }

    /// Format integer string with thousand separators
    fn format_number_str(s: &str) -> String {
        let mut result = String::new();

        for (count, c) in s.chars().rev().enumerate() {
            if count > 0 && count % 3 == 0 {
                result.insert(0, ',');
            }
            result.insert(0, c);
        }

        result
    }

    /// Format large numbers with thousand separators
    #[allow(dead_code)]
    fn format_number(num: u64) -> String {
        let s = num.to_string();
        let mut result = String::new();

        for (count, c) in s.chars().rev().enumerate() {
            if count > 0 && count % 3 == 0 {
                result.insert(0, ',');
            }
            result.insert(0, c);
        }

        result
    }

    /// Format detailed neuron information for terminal display
    ///
    /// Displays comprehensive stats for a single neuron including:
    /// - Basic info (stake, maturity, voting power)
    /// - Bonus multipliers
    /// - Reward history (7-day and 30-day)
    /// - Dissolve status with warnings if near dissolve date
    pub fn format_neuron_detail(detail: &NeuronDetail) -> String {
        let mut output = String::new();

        // Header
        output.push_str("============================================================\n");
        output.push_str(&format!("Neuron Detail - {}\n", detail.neuron_id.value()));
        output.push_str("============================================================\n\n");

        // Core Stats Section
        output.push_str("Core Statistics:\n");
        output.push_str("------------------------------------------------------------\n");
        output.push_str(&format!("Stake:            {} ICP\n", Self::format_icp(detail.stake.to_icp())));
        output.push_str(&format!("Maturity:         {} ICP (Available)\n", Self::format_icp(detail.maturity.to_icp())));
        output.push_str(&format!("Staked Maturity:  {} ICP\n", Self::format_icp(detail.staked_maturity.to_icp())));
        output.push_str(&format!("Total Value:      {} ICP\n", Self::format_icp(detail.total_value.to_icp())));
        // Voting power: convert from e8s-like format to human-readable (dimensionless)
        let voting_power_display = detail.voting_power as f64 / 100_000_000.0;
        output.push_str(&format!("Voting Power:     {}\n", Self::format_icp(voting_power_display)));
        output.push_str(&format!("Auto-Stake:       {}\n", if detail.auto_stake_enabled { "Enabled" } else { "Disabled" }));
        output.push_str(&format!("State:            {:?}\n", detail.state));

        // Age & Dissolve Section
        output.push_str("\nAge & Dissolve Delay:\n");
        output.push_str("------------------------------------------------------------\n");
        output.push_str(&format!("Age:              {:.1} years ({} days)\n",
            detail.age_years(), detail.age_days));
        output.push_str(&format!("Dissolve Delay:   {:.1} years ({} days)\n",
            detail.dissolve_delay_years(), detail.dissolve_delay_days));
        output.push_str(&format!("Created:          {}\n",
            detail.created_at.format("%Y-%m-%d")));

        // Bonus Multipliers Section
        output.push_str("\nBonus Multipliers:\n");
        output.push_str("------------------------------------------------------------\n");
        output.push_str(&format!("Age Bonus:        {:.2}x\n", detail.age_bonus.value()));
        output.push_str(&format!("Dissolve Bonus:   {:.2}x\n", detail.dissolve_bonus.value()));
        output.push_str(&format!("Combined Bonus:   {:.2}x\n", detail.combined_bonus.value()));

        // Dissolve Status Section
        output.push_str("\nDissolve Status:\n");
        output.push_str("------------------------------------------------------------\n");
        match detail.dissolve_status {
            DissolveStatus::Locked => {
                output.push_str("Status:           Locked\n");
                output.push_str("Note:             Neuron is locked and not dissolving\n");
            }
            DissolveStatus::Dissolving { days_remaining } => {
                output.push_str("Status:           Dissolving\n");
                output.push_str(&format!("Days Remaining:   {} days\n", days_remaining));

                if let Some(dissolve_date) = detail.estimated_dissolve_date() {
                    output.push_str(&format!("Dissolve Date:    {}\n",
                        dissolve_date.format("%Y-%m-%d")));
                }

                // Warning if near dissolve
                if detail.dissolve_status.is_near_dissolve() {
                    output.push_str("\n⚠  WARNING: Neuron will dissolve in less than 30 days!\n");
                    output.push_str("   Voting rewards will decrease as dissolve delay reduces.\n");
                }
            }
            DissolveStatus::Dissolved => {
                output.push_str("Status:           Dissolved\n");
                output.push_str("⚠  WARNING: Dissolved neurons earn NO voting rewards!\n");
            }
        }

        // Reward History Section
        output.push_str("\nReward History:\n");
        output.push_str("------------------------------------------------------------\n");

        if let Some(history) = &detail.reward_history {
            // 7-day stats
            output.push_str(&format!("7-Day Total:      {} ICP\n",
                Self::format_icp(history.days_7.to_icp())));
            output.push_str(&format!("7-Day Daily Avg:  {} ICP/day\n",
                Self::format_icp(history.days_7_daily_avg)));

            // 30-day stats
            output.push_str(&format!("30-Day Total:     {} ICP\n",
                Self::format_icp(history.days_30.to_icp())));
            output.push_str(&format!("30-Day Daily Avg: {} ICP/day\n",
                Self::format_icp(history.days_30_daily_avg)));

            // Reward rates
            if let Some(rate_7) = detail.reward_rate_7day() {
                output.push_str(&format!("\n7-Day Rate:       {:.4}%\n", rate_7));
            }
            if let Some(annual) = detail.annualized_return() {
                output.push_str(&format!("Annualized:       {:.2}%\n", annual));
            }

            // Zero reward warning
            if detail.has_zero_rewards() {
                output.push_str("\n⚠  WARNING: No rewards earned in tracking period!\n");
                output.push_str("   Possible issues:\n");
                output.push_str("   - Neuron not configured for voting\n");
                output.push_str("   - Not following any neurons or topics\n");
                output.push_str("   - Dissolve delay too short (< 6 months)\n");
                output.push_str("   - Neuron is dissolved\n");
            }
        } else {
            output.push_str("No reward data available\n");
            output.push_str("\nNote: Run tracker daily to build reward history\n");
        }

        output.push_str("\n============================================================\n");

        output
    }

    // ============================================================
    // JSON Export Methods
    // ============================================================

    /// Export portfolio summary as JSON
    pub fn export_summary_json(report: &PortfolioReport) -> Result<String, Box<dyn std::error::Error>> {
        serde_json::to_string_pretty(report)
            .map_err(|e| format!("Failed to serialize report to JSON: {}", e).into())
    }

    /// Export historical trend as JSON
    pub fn export_history_json(trend: &HistoricalTrend) -> Result<String, Box<dyn std::error::Error>> {
        serde_json::to_string_pretty(trend)
            .map_err(|e| format!("Failed to serialize trend to JSON: {}", e).into())
    }

    /// Export reward analysis as JSON
    pub fn export_rewards_json(analysis: &RewardAnalysis) -> Result<String, Box<dyn std::error::Error>> {
        serde_json::to_string_pretty(analysis)
            .map_err(|e| format!("Failed to serialize analysis to JSON: {}", e).into())
    }

    /// Export neuron detail as JSON
    pub fn export_neuron_json(detail: &NeuronDetail) -> Result<String, Box<dyn std::error::Error>> {
        serde_json::to_string_pretty(detail)
            .map_err(|e| format!("Failed to serialize detail to JSON: {}", e).into())
    }

    // ============================================================
    // CSV Export Methods
    // ============================================================

    /// Export portfolio summary as CSV
    pub fn export_summary_csv(report: &PortfolioReport) -> Result<String, Box<dyn std::error::Error>> {
        let mut output = String::new();

        // Header comment with metadata
        output.push_str(&format!("# Portfolio Summary Report\n"));
        output.push_str(&format!("# Generated: {}\n", report.generated_at.format("%Y-%m-%d %H:%M:%S UTC")));
        if let Some(retrieved) = Self::latest_retrieval(report) {
            output.push_str(&format!("# Data Retrieved: {}\n", retrieved.format("%Y-%m-%d %H:%M:%S UTC")));
        }
        output.push_str("\n");

        // Summary metrics
        output.push_str("Metric,Value\n");
        output.push_str(&format!("Total Neurons,{}\n", report.metrics.neuron_count));
        output.push_str(&format!("Total Stake (ICP),{}\n", report.metrics.total_stake.to_icp()));
        output.push_str(&format!("Total Maturity (ICP),{}\n", report.metrics.total_maturity.to_icp()));
        output.push_str(&format!("Staked Maturity (ICP),{}\n", report.metrics.total_staked_maturity.to_icp()));
        output.push_str(&format!("Available Maturity (ICP),{}\n", report.available_maturity().to_icp()));
        output.push_str(&format!("Total Voting Power,{}\n", report.metrics.total_voting_power));

        if let Some(dissolve_range) = report.metrics.dissolve_delay_range {
            output.push_str(&format!("Min Dissolve Delay (days),{}\n", dissolve_range.min_days));
            output.push_str(&format!("Max Dissolve Delay (days),{}\n", dissolve_range.max_days));
        }

        if let Some(age_range) = report.metrics.age_range {
            output.push_str(&format!("Min Age (days),{}\n", age_range.min_days));
            output.push_str(&format!("Max Age (days),{}\n", age_range.max_days));
            output.push_str(&format!("Average Age (days),{}\n", age_range.average_days));
        }

        if let Some(retrieved) = Self::latest_retrieval(report) {
            output.push_str(&format!("Data Retrieved (UTC),{}\n", retrieved.format("%Y-%m-%d %H:%M:%S")));
        }

        Ok(output)
    }

    /// Most recent observation time across the portfolio's neurons.
    ///
    /// Sourced from each neuron's `retrieved_at`, which the repository now populates from the
    /// stored `retrieved_timestamp` column rather than stamping with the current time.
    fn latest_retrieval(report: &PortfolioReport) -> Option<DateTime<Utc>> {
        report.portfolio.neurons().iter().map(|n| n.retrieved_at()).max()
    }

    /// Export historical trend as CSV
    pub fn export_history_csv(trend: &HistoricalTrend) -> Result<String, Box<dyn std::error::Error>> {
        let mut output = String::new();

        // Header comment
        output.push_str("# Historical Trend Analysis\n");
        output.push_str(&format!("# Period: {} to {}\n", trend.start_date, trend.end_date));
        output.push_str("\n");

        // Data
        output.push_str("Metric,Value\n");
        output.push_str(&format!("Start Date,{}\n", trend.start_date));
        output.push_str(&format!("End Date,{}\n", trend.end_date));
        output.push_str(&format!("Start Maturity (ICP),{}\n", trend.start_maturity.to_icp()));
        output.push_str(&format!("End Maturity (ICP),{}\n", trend.end_maturity.to_icp()));
        output.push_str(&format!("Maturity Delta (ICP),{}\n", trend.maturity_delta().to_icp()));
        output.push_str(&format!("Days in Period,{}\n", trend.days_in_period));
        output.push_str(&format!("Days with Data,{}\n", trend.days_with_data));
        output.push_str(&format!("Missing Days,{}\n", trend.missing_days));
        output.push_str(&format!("Average Daily Growth (ICP),{}\n", trend.average_daily_growth()));
        output.push_str(&format!("Growth Rate (%),{}\n", trend.growth_rate_percentage()));
        output.push_str(&format!("Data Quality,{}\n", trend.data_quality().as_str()));

        Ok(output)
    }

    /// Export reward analysis as CSV
    pub fn export_rewards_csv(analysis: &RewardAnalysis) -> Result<String, Box<dyn std::error::Error>> {
        let mut output = String::new();

        // Header comment
        output.push_str(&format!("# Reward Analysis - {} Days\n", analysis.period_days));
        output.push_str(&format!("# Total Rewards: {} ICP\n", analysis.total_rewards.to_icp()));
        output.push_str("\n");

        // CSV header
        output.push_str("Neuron ID,Total Reward (ICP),Days Tracked,Stake (ICP),Daily Avg (ICP),Weekly Projected (ICP),Monthly Projected (ICP),Reward Rate (%),Annualized Return (%),Dissolve Delay (days),Age (days),Age Bonus,Dissolve Bonus\n");

        // Neuron rows
        for nr in &analysis.neuron_rewards {
            output.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                nr.neuron_id.value(),
                nr.total_reward.to_icp(),
                nr.days_tracked,
                nr.stake.to_icp(),
                nr.daily_average(),
                nr.weekly_projected(),
                nr.monthly_projected(),
                nr.reward_rate_percentage(),
                nr.annualized_return(),
                nr.dissolve_delay_days,
                nr.age_days,
                nr.age_bonus.value(),
                nr.dissolve_bonus.value()
            ));
        }

        Ok(output)
    }

    /// Export neuron detail as CSV
    pub fn export_neuron_csv(detail: &NeuronDetail) -> Result<String, Box<dyn std::error::Error>> {
        let mut output = String::new();

        // Header comment
        output.push_str(&format!("# Neuron Detail Report - {}\n", detail.neuron_id.value()));
        output.push_str("\n");

        // CSV data
        output.push_str("Metric,Value\n");
        output.push_str(&format!("Neuron ID,{}\n", detail.neuron_id.value()));
        output.push_str(&format!("Stake (ICP),{}\n", detail.stake.to_icp()));
        output.push_str(&format!("Maturity (ICP),{}\n", detail.maturity.to_icp()));
        output.push_str(&format!("Staked Maturity (ICP),{}\n", detail.staked_maturity.to_icp()));
        output.push_str(&format!("Total Value (ICP),{}\n", detail.total_value.to_icp()));
        output.push_str(&format!("Voting Power,{}\n", detail.voting_power));
        output.push_str(&format!("Dissolve Delay (days),{}\n", detail.dissolve_delay_days));
        output.push_str(&format!("Age (days),{}\n", detail.age_days));
        output.push_str(&format!("Age Bonus,{}\n", detail.age_bonus.value()));
        output.push_str(&format!("Dissolve Bonus,{}\n", detail.dissolve_bonus.value()));
        output.push_str(&format!("Combined Bonus,{}\n", detail.combined_bonus.value()));
        output.push_str(&format!("State,{:?}\n", detail.state));
        output.push_str(&format!("Auto Stake Enabled,{}\n", detail.auto_stake_enabled));
        output.push_str(&format!("Created At,{}\n", detail.created_at.format("%Y-%m-%d")));

        match detail.dissolve_status {
            DissolveStatus::Locked => output.push_str("Dissolve Status,Locked\n"),
            DissolveStatus::Dissolving { days_remaining } => {
                output.push_str(&format!("Dissolve Status,Dissolving\n"));
                output.push_str(&format!("Days Until Dissolved,{}\n", days_remaining));
            }
            DissolveStatus::Dissolved => output.push_str("Dissolve Status,Dissolved\n"),
        }

        if let Some(history) = &detail.reward_history {
            output.push_str(&format!("7-Day Total Reward (ICP),{}\n", history.days_7.to_icp()));
            output.push_str(&format!("7-Day Daily Avg (ICP),{}\n", history.days_7_daily_avg));
            output.push_str(&format!("30-Day Total Reward (ICP),{}\n", history.days_30.to_icp()));
            output.push_str(&format!("30-Day Daily Avg (ICP),{}\n", history.days_30_daily_avg));

            if let Some(rate) = detail.reward_rate_7day() {
                output.push_str(&format!("7-Day Reward Rate (%),{}\n", rate));
            }

            if let Some(annualized) = detail.annualized_return() {
                output.push_str(&format!("Annualized Return (%),{}\n", annualized));
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Portfolio, Neuron, NeuronId, IcpAmount, NeuronState};

    fn create_test_neuron(id: u64, stake_icp: u64, maturity_icp: u64, staked_maturity_icp: u64, age_days: u64, dissolve_days: u64) -> Neuron {
        Neuron::new(
            NeuronId::new(id),
            IcpAmount::from_e8s(stake_icp * 100_000_000),
            IcpAmount::from_e8s(maturity_icp * 100_000_000),
            IcpAmount::from_e8s(staked_maturity_icp * 100_000_000),
            1_000_000,
            age_days * 86400,
            dissolve_days * 86400,
            NeuronState::Locked,
            true,
            1600000000,
        )
    }

    // ---- The income panel's labels ------------------------------------------------------
    //
    // The panel is a claim about where its number came from. It previously made three
    // incompatible ones simultaneously — a "30-day average" header, a trailing 30-day
    // "Period:" the figure was not drawn from, and a "Data points: 1/30 days" counter — over
    // a rate whose real divisor was 247 days. These assert the labels track the arithmetic.

    fn income_stats(days_covered: i64, records: i64, span: Option<(NaiveDate, NaiveDate)>, uniform: bool) -> DailyIncomeStats {
        DailyIncomeStats {
            total_daily_icp: 2.6682,
            total_annual_icp: 973.88,
            effective_apy: 16.38,
            neuron_contributions: vec![],
            span_start: span.map(|s| s.0),
            span_end: span.map(|s| s.1),
            days_covered,
            records_used: records,
            uniform_spans: uniform,
        }
    }

    /// The production case: 30 records, 247 days. The panel must state both and confuse
    /// neither for the other.
    #[test]
    fn income_panel_states_records_and_the_span_they_cover() {
        let stats = income_stats(
            247,
            30,
            Some((
                NaiveDate::from_ymd_opt(2025, 12, 3).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            )),
            true,
        );
        let out = TerminalReportFormatter::format_income_analysis(
            &stats,
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
        );

        assert!(out.contains("30 reward records per neuron"), "{out}");
        assert!(out.contains("Covering: 2025-12-03 to 2026-08-06 (247 days of accrual)"), "{out}");
        assert!(out.contains("divided by the 247 days they"), "the note names the divisor:\n{out}");
        assert!(out.contains("2.6682 ICP/day"), "{out}");

        // The three retired claims, none of which described the arithmetic.
        assert!(!out.contains("30-day average"), "stale header:\n{out}");
        assert!(!out.contains("Data points"), "stale counter:\n{out}");
        assert!(!out.contains("Period:"), "stale trailing period:\n{out}");
    }

    /// The header must not imply a day count at all — records and days are only equal when
    /// every row carries one day, which is exactly the case that hid the defect.
    #[test]
    fn income_panel_does_not_call_a_record_count_a_day_count() {
        let stats = income_stats(
            240,
            30,
            Some((
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            )),
            true,
        );
        let out = TerminalReportFormatter::format_income_analysis(
            &stats,
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
        );

        assert!(out.contains("Income Analysis (mean daily rate)"), "{out}");
        assert!(out.contains("30 reward records"), "{out}");
        assert!(out.contains("240 days of accrual"), "{out}");
        assert!(!out.contains("30 days"), "30 records is not 30 days:\n{out}");
    }

    /// With no reward history the panel must say so rather than print a span it does not have.
    #[test]
    fn income_panel_without_records_claims_no_span() {
        let stats = income_stats(0, 0, None, true);
        let out = TerminalReportFormatter::format_income_analysis(
            &stats,
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
        );

        assert!(out.contains("no reward records available"), "{out}");
        assert!(!out.contains("Covering:"), "no span to report:\n{out}");
    }

    /// A total summing rates measured over different spans must disclose that.
    #[test]
    fn income_panel_discloses_non_uniform_neuron_spans() {
        let stats = income_stats(
            247,
            30,
            Some((
                NaiveDate::from_ymd_opt(2025, 12, 3).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            )),
            false,
        );
        let out = TerminalReportFormatter::format_income_analysis(
            &stats,
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
        );

        assert!(out.contains("sums rates measured over different spans"), "{out}");
    }

    #[test]
    fn test_format_summary_with_multiple_neurons() {
        let neuron1 = create_test_neuron(123, 1000, 10, 5, 365, 730);
        let neuron2 = create_test_neuron(456, 2000, 20, 15, 730, 1460);

        let portfolio = Portfolio::new(vec![neuron1, neuron2]);
        let report = PortfolioReport::new(portfolio);

        let output = TerminalReportFormatter::format_summary(&report);

        // Verify key sections present
        assert!(output.contains("Portfolio Summary"));
        assert!(output.contains("Total Neurons: 2"));
        assert!(output.contains("Total Stake: 3,000.00 ICP"));
        assert!(output.contains("Total Maturity: 50.00 ICP")); // (10+20) available + (5+15) staked
        assert!(output.contains("Staked Maturity: 20.00 ICP"));
        assert!(output.contains("Available Maturity: 30.00 ICP"));
        assert!(output.contains("Total Voting Power:")); // Check voting power is present
        assert!(output.contains("B")); // Check it's in billions format
        assert!(output.contains("Dissolve Delay Range:"));
        assert!(output.contains("Average Age:"));
    }

    #[test]
    fn test_format_icp_various_amounts() {
        assert_eq!(TerminalReportFormatter::format_icp(1234567.89), "1,234,567.89");
        assert_eq!(TerminalReportFormatter::format_icp(1000.50), "1,000.50");
        assert_eq!(TerminalReportFormatter::format_icp(123.456), "123.46");
        assert_eq!(TerminalReportFormatter::format_icp(0.1234), "0.1234");
        assert_eq!(TerminalReportFormatter::format_icp(0.00005678), "0.00005678");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(TerminalReportFormatter::format_number(1234567), "1,234,567");
        assert_eq!(TerminalReportFormatter::format_number(1000), "1,000");
        assert_eq!(TerminalReportFormatter::format_number(999), "999");
        assert_eq!(TerminalReportFormatter::format_number(0), "0");
    }

    #[test]
    fn test_format_summary_fits_80_columns() {
        let neuron = create_test_neuron(123, 100, 1, 0, 365, 730);
        let portfolio = Portfolio::new(vec![neuron]);
        let report = PortfolioReport::new(portfolio);

        let output = TerminalReportFormatter::format_summary(&report);

        // Verify no line exceeds 80 characters
        for line in output.lines() {
            assert!(
                line.len() <= 80,
                "Line exceeds 80 characters: '{}' (length: {})",
                line,
                line.len()
            );
        }
    }

    #[test]
    fn test_format_summary_single_neuron() {
        let neuron = create_test_neuron(123, 500, 5, 3, 365, 730);
        let portfolio = Portfolio::new(vec![neuron]);
        let report = PortfolioReport::new(portfolio);

        let output = TerminalReportFormatter::format_summary(&report);

        assert!(output.contains("Total Neurons: 1"));
        assert!(output.contains("Total Stake: 500.00 ICP"));
        assert!(output.contains("Total Maturity: 8.00 ICP")); // 5 available + 3 staked
        assert!(output.contains("Staked Maturity: 3.00 ICP"));
        assert!(output.contains("Available Maturity: 5.00 ICP"));
    }
}
