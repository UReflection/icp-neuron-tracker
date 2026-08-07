use crate::domain::{Portfolio, NeuronId, repositories::*};
use chrono::{Utc, NaiveDate};

pub struct TrackingService<N, P, R>
where
    N: NeuronSnapshotRepository,
    P: PortfolioSnapshotRepository,
    R: DailyRewardRepository,
{
    neuron_repo: N,
    portfolio_repo: P,
    reward_repo: R,
}

impl<N, P, R> TrackingService<N, P, R>
where
    N: NeuronSnapshotRepository,
    P: PortfolioSnapshotRepository,
    R: DailyRewardRepository,
{
    pub fn new(neuron_repo: N, portfolio_repo: P, reward_repo: R) -> Self {
        Self {
            neuron_repo,
            portfolio_repo,
            reward_repo,
        }
    }

    pub fn save_daily_snapshot(&self, portfolio: &Portfolio) -> Result<SnapshotResult, Box<dyn std::error::Error>> {
        let today = Utc::now().date_naive();

        // Save individual neuron snapshots
        for neuron in portfolio.neurons() {
            self.neuron_repo.save_snapshot(neuron, today)?;
        }

        // Save portfolio summary
        self.portfolio_repo.save_snapshot(portfolio, today)?;

        // Calculate and save daily rewards (if previous snapshot exists)
        self.calculate_and_save_rewards(portfolio, today)?;

        Ok(SnapshotResult {
            date: today,
            neurons_saved: portfolio.neuron_count(),
        })
    }

    fn calculate_and_save_rewards(&self, portfolio: &Portfolio, today: chrono::NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
        for neuron in portfolio.neurons() {
            let neuron_id = neuron.id();
            
            // Try to get previous snapshot
            if let Some((previous_neuron, previous_date)) = self.neuron_repo.get_previous_snapshot(neuron_id, today)? {
                let days_elapsed = (today - previous_date).num_days();
                
                if days_elapsed > 0 {
                    // Calculate deltas
                    let maturity_delta = neuron.maturity().e8s() as i64 - previous_neuron.maturity().e8s() as i64;
                    let staked_maturity_delta = neuron.staked_maturity().e8s() as i64 - previous_neuron.staked_maturity().e8s() as i64;
                    
                    // Save reward calculation
                    self.reward_repo.save_reward(
                        neuron_id,
                        today,
                        maturity_delta,
                        staked_maturity_delta,
                        days_elapsed,
                    )?;
                }
            }
        }
        
        Ok(())
    }

    /// Reward rows read per neuron. A row bound, not a day bound — see
    /// [`DailyRewardRepository::get_average_daily_reward_window`]. The span these rows cover
    /// is whatever the collection history gives and is reported alongside the rate.
    const AVERAGE_RECORDS: i64 = 30;

    pub fn get_daily_income_stats(&self, portfolio: &Portfolio) -> Result<DailyIncomeStats, Box<dyn std::error::Error>> {
        let mut total_daily_icp = 0.0;
        let mut neuron_stats = Vec::new();

        // Widest span any contributing neuron drew on, and the largest divisor behind it.
        // Reported rather than assumed: neurons are snapshotted together so these normally
        // agree, but the total is a sum of per-neuron rates and the label must not claim a
        // tighter span than one of its terms actually used.
        let mut span_start: Option<NaiveDate> = None;
        let mut span_end: Option<NaiveDate> = None;
        let mut days_covered: i64 = 0;
        let mut records_used: i64 = 0;
        let mut uniform_spans = true;

        for neuron in portfolio.neurons() {
            if let Some(window) = self.reward_repo.get_average_daily_reward_window(neuron.id(), Self::AVERAGE_RECORDS)? {
                total_daily_icp += window.icp_per_day;
                neuron_stats.push((neuron.id(), window.icp_per_day));

                if days_covered != 0 && (days_covered != window.days_covered || records_used != window.records) {
                    uniform_spans = false;
                }
                span_start = Some(span_start.map_or(window.first_date, |d: NaiveDate| d.min(window.first_date)));
                span_end = Some(span_end.map_or(window.last_date, |d: NaiveDate| d.max(window.last_date)));
                days_covered = days_covered.max(window.days_covered);
                records_used = records_used.max(window.records);
            }
        }

        // Class B annualisation: 365.0 is deliberate financial convention, NOT a calendar
        // conversion. Do not change to 365.25 — see BonusMultiplier in domain/value_objects.rs.
        Ok(DailyIncomeStats {
            total_daily_icp,
            total_annual_icp: total_daily_icp * 365.0,
            effective_apy: if portfolio.total_stake().to_icp() > 0.0 {
                (total_daily_icp * 365.0 / portfolio.total_stake().to_icp()) * 100.0
            } else {
                0.0
            },
            neuron_contributions: neuron_stats,
            span_start,
            span_end,
            days_covered,
            records_used,
            uniform_spans,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotResult {
    pub date: NaiveDate,
    pub neurons_saved: usize,
}

#[derive(Debug, Clone)]
pub struct DailyIncomeStats {
    pub total_daily_icp: f64,
    pub total_annual_icp: f64,
    pub effective_apy: f64,
    pub neuron_contributions: Vec<(NeuronId, f64)>,
    /// Oldest reward date any contributing neuron drew on. `None` when no neuron had data.
    pub span_start: Option<NaiveDate>,
    /// Newest reward date any contributing neuron drew on.
    pub span_end: Option<NaiveDate>,
    /// Days the rate is divided by: the sum of `days_elapsed` across the rows read. This is
    /// the figure's real denominator and is not the number of rows or a fixed 30.
    pub days_covered: i64,
    /// Reward rows read per neuron. Fewer than requested when history is short.
    pub records_used: i64,
    /// Whether every contributing neuron used the same record count and divisor. False means
    /// the total sums rates measured over different spans, which output must not hide.
    pub uniform_spans: bool,
}