use rusqlite::{Connection, params, OptionalExtension};
use crate::domain::{Neuron, NeuronId, Portfolio, IcpAmount, NeuronState};
use crate::domain::repositories::*;
use crate::domain::retirement::{AccrualGap, WindowSample};
use chrono::NaiveDate;
use std::str::FromStr;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub struct SqliteRepository {
    conn: Connection,
}

impl SqliteRepository {
    pub fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut conn = Connection::open(db_path)?;
        
        // Run migrations
        println!("🔄 Running database migrations...");
        embedded::migrations::runner().run(&mut conn)?;
        println!("✓ Database migrations complete");
        
        Ok(Self { conn })
    }
}

// Neuron Snapshot Repository Implementation
impl NeuronSnapshotRepository for SqliteRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "INSERT OR REPLACE INTO neuron_snapshots 
             (neuron_id, snapshot_date, stake_e8s, maturity_e8s, staked_maturity_e8s, 
              voting_power, age_days, dissolve_delay_days, age_bonus_multiplier, 
              dissolve_bonus_multiplier, state, auto_stake_enabled, created_timestamp, retrieved_timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                neuron.id().value().to_string(),
                date.to_string(),
                neuron.stake().e8s() as i64,
                neuron.maturity().e8s() as i64,
                neuron.staked_maturity().e8s() as i64,
                neuron.voting_power() as i64,
                neuron.age_days() as i64,
                neuron.dissolve_delay_days() as i64,
                neuron.age_bonus().value(),
                neuron.dissolve_bonus().value(),
                format!("{:?}", neuron.state()),
                neuron.auto_stake_enabled(),
                neuron.created_at().timestamp(),
                neuron.retrieved_at().timestamp(),
            ],
        )?;
        Ok(())
    }

    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<Neuron>, Box<dyn std::error::Error>> {
        let neuron = self.conn.query_row(
            "SELECT neuron_id, stake_e8s, maturity_e8s, staked_maturity_e8s, voting_power,
                    age_days, dissolve_delay_days, state, auto_stake_enabled, created_timestamp,
                    retrieved_timestamp
             FROM neuron_snapshots
             WHERE neuron_id = ?1 AND snapshot_date = ?2",
            params![neuron_id.value().to_string(), date.to_string()],
            |row| {
                Ok(Self::row_to_neuron(row)?)
            }
        ).optional()?;
        
        Ok(neuron)
    }

    fn get_latest_snapshot(&self, neuron_id: NeuronId) -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        let result = self.conn.query_row(
            "SELECT neuron_id, stake_e8s, maturity_e8s, staked_maturity_e8s, voting_power,
                    age_days, dissolve_delay_days, state, auto_stake_enabled, created_timestamp, snapshot_date,
                    retrieved_timestamp
             FROM neuron_snapshots
             WHERE neuron_id = ?1
             ORDER BY snapshot_date DESC
             LIMIT 1",
            params![neuron_id.value().to_string()],
            |row| {
                let neuron = Self::row_to_neuron(row)?;
                let date_str: String = row.get(10)?;
                let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok((neuron, date))
            }
        ).optional()?;
        
        Ok(result)
    }

    fn get_previous_snapshot(&self, neuron_id: NeuronId, before_date: NaiveDate) -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        let result = self.conn.query_row(
            "SELECT neuron_id, stake_e8s, maturity_e8s, staked_maturity_e8s, voting_power,
                    age_days, dissolve_delay_days, state, auto_stake_enabled, created_timestamp, snapshot_date,
                    retrieved_timestamp
             FROM neuron_snapshots
             WHERE neuron_id = ?1 AND snapshot_date < ?2
             ORDER BY snapshot_date DESC
             LIMIT 1",
            params![neuron_id.value().to_string(), before_date.to_string()],
            |row| {
                let neuron = Self::row_to_neuron(row)?;
                let date_str: String = row.get(10)?;
                let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok((neuron, date))
            }
        ).optional()?;
        
        Ok(result)
    }

    fn get_snapshots_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) -> Result<Vec<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT neuron_id, stake_e8s, maturity_e8s, staked_maturity_e8s, voting_power,
                    age_days, dissolve_delay_days, state, auto_stake_enabled, created_timestamp, snapshot_date,
                    retrieved_timestamp
             FROM neuron_snapshots
             WHERE neuron_id = ?1 AND snapshot_date >= ?2 AND snapshot_date <= ?3
             ORDER BY snapshot_date ASC"
        )?;
        
        let rows = stmt.query_map(
            params![neuron_id.value().to_string(), start.to_string(), end.to_string()],
            |row| {
                let neuron = Self::row_to_neuron(row)?;
                let date_str: String = row.get(10)?;
                let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok((neuron, date))
            }
        )?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }

    fn get_all_snapshots_for_date(&self, date: NaiveDate) -> Result<Vec<Neuron>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT neuron_id, stake_e8s, maturity_e8s, staked_maturity_e8s, voting_power,
                    age_days, dissolve_delay_days, state, auto_stake_enabled, created_timestamp,
                    retrieved_timestamp
             FROM neuron_snapshots
             WHERE snapshot_date = ?1
             ORDER BY neuron_id"
        )?;
        
        let rows = stmt.query_map(params![date.to_string()], |row| {
            Ok(Self::row_to_neuron(row)?)
        })?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }
}

// Portfolio Snapshot Repository Implementation
impl PortfolioSnapshotRepository for SqliteRepository {
    fn save_snapshot(&self, portfolio: &Portfolio, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "INSERT OR REPLACE INTO portfolio_snapshots 
             (snapshot_date, total_neurons, total_stake_e8s, total_maturity_e8s, 
              total_staked_maturity_e8s, total_voting_power, overall_return_percentage)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                date.to_string(),
                portfolio.neuron_count() as i64,
                portfolio.total_stake().e8s() as i64,
                portfolio.total_maturity().e8s() as i64,
                portfolio.total_staked_maturity().e8s() as i64,
                portfolio.total_voting_power() as i64,
                portfolio.overall_return_percentage(),
            ],
        )?;
        Ok(())
    }

    fn get_snapshot(&self, date: NaiveDate) -> Result<Option<Portfolio>, Box<dyn std::error::Error>> {
        // Get all neurons for this date and reconstruct portfolio
        let neurons = self.get_all_snapshots_for_date(date)?;
        if neurons.is_empty() {
            return Ok(None);
        }
        Ok(Some(Portfolio::new(neurons)))
    }

    fn get_latest_snapshot(&self) -> Result<Option<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>> {
        let date_result: Option<String> = self.conn.query_row(
            "SELECT snapshot_date FROM portfolio_snapshots ORDER BY snapshot_date DESC LIMIT 1",
            [],
            |row| row.get(0)
        ).optional()?;

        if let Some(date_str) = date_result {
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
            match PortfolioSnapshotRepository::get_snapshot(self, date)? {
                Some(portfolio) => Ok(Some((portfolio, date))),
                None => Err(format!("Portfolio snapshot exists in portfolio_snapshots table but no neurons found for date {}", date).into())
            }
        } else {
            Ok(None)
        }
    }

    fn get_snapshots_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT snapshot_date FROM portfolio_snapshots 
             WHERE snapshot_date >= ?1 AND snapshot_date <= ?2
             ORDER BY snapshot_date ASC"
        )?;
        
        let dates: Vec<String> = stmt.query_map(
            params![start.to_string(), end.to_string()],
            |row| row.get(0)
        )?.collect::<Result<Vec<_>, _>>()?;
        
        let mut results = Vec::new();
        for date_str in dates {
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
            if let Some(portfolio) = PortfolioSnapshotRepository::get_snapshot(self, date)? {
                results.push((portfolio, date));
            }
        }
        
        Ok(results)
    }
}

// Daily Reward Repository Implementation
impl DailyRewardRepository for SqliteRepository {
    fn save_reward(&self, neuron_id: NeuronId, date: NaiveDate, maturity_delta: i64, staked_maturity_delta: i64, days_elapsed: i64) -> Result<(), Box<dyn std::error::Error>> {
        let total_reward = maturity_delta + staked_maturity_delta;
        
        self.conn.execute(
            "INSERT OR REPLACE INTO daily_rewards 
             (neuron_id, reward_date, maturity_delta_e8s, staked_maturity_delta_e8s, total_reward_e8s, days_elapsed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                neuron_id.value().to_string(),
                date.to_string(),
                maturity_delta,
                staked_maturity_delta,
                total_reward,
                days_elapsed,
            ],
        )?;
        Ok(())
    }

    fn get_reward(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<DailyReward>, Box<dyn std::error::Error>> {
        let reward = self.conn.query_row(
            "SELECT neuron_id, reward_date, maturity_delta_e8s, staked_maturity_delta_e8s, total_reward_e8s, days_elapsed
             FROM daily_rewards
             WHERE neuron_id = ?1 AND reward_date = ?2",
            params![neuron_id.value().to_string(), date.to_string()],
            |row| {
                let neuron_id_str: String = row.get(0)?;
                let date_str: String = row.get(1)?;
                let parsed_id = neuron_id_str.parse::<u64>()
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                let parsed_date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(DailyReward {
                    neuron_id: NeuronId::new(parsed_id),
                    date: parsed_date,
                    maturity_delta_e8s: row.get(2)?,
                    staked_maturity_delta_e8s: row.get(3)?,
                    total_reward_e8s: row.get(4)?,
                    days_elapsed: row.get(5)?,
                })
            }
        ).optional()?;

        Ok(reward)
    }

    fn get_rewards_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyReward>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT neuron_id, reward_date, maturity_delta_e8s, staked_maturity_delta_e8s, total_reward_e8s, days_elapsed
             FROM daily_rewards
             WHERE neuron_id = ?1 AND reward_date >= ?2 AND reward_date <= ?3
             ORDER BY reward_date ASC"
        )?;

        let rows = stmt.query_map(
            params![neuron_id.value().to_string(), start.to_string(), end.to_string()],
            |row| {
                let neuron_id_str: String = row.get(0)?;
                let date_str: String = row.get(1)?;
                let parsed_id = neuron_id_str.parse::<u64>()
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                let parsed_date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(DailyReward {
                    neuron_id: NeuronId::new(parsed_id),
                    date: parsed_date,
                    maturity_delta_e8s: row.get(2)?,
                    staked_maturity_delta_e8s: row.get(3)?,
                    total_reward_e8s: row.get(4)?,
                    days_elapsed: row.get(5)?,
                })
            }
        )?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    fn get_average_daily_reward_window(&self, neuron_id: NeuronId, records: i64) -> Result<Option<DailyAverageWindow>, Box<dyn std::error::Error>> {
        // Sum of rewards over sum of days_elapsed from the last N rows. The divisor is the
        // days those rows account for, which is what makes the rate correct across batched
        // and caught-up rewards; MIN/MAX carry the span so the caller can report it.
        // (sum rewards e8s, sum days_elapsed, row count, oldest date, newest date)
        type AverageWindowRow = (Option<f64>, Option<i64>, i64, Option<String>, Option<String>);
        let result: Option<AverageWindowRow> = self.conn.query_row(
            "SELECT SUM(CAST(total_reward_e8s AS REAL)), SUM(days_elapsed), COUNT(*),
                    MIN(reward_date), MAX(reward_date)
             FROM (
                 SELECT total_reward_e8s, days_elapsed, reward_date
                 FROM daily_rewards
                 WHERE neuron_id = ?1
                 ORDER BY reward_date DESC
                 LIMIT ?2
             )",
            params![neuron_id.value().to_string(), records],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        ).optional()?;

        let Some((sum_rewards, sum_days, row_count, first, last)) = result else {
            return Ok(None);
        };
        let (Some(rewards), Some(days), Some(first), Some(last)) = (sum_rewards, sum_days, first, last) else {
            return Ok(None);
        };
        if days <= 0 {
            return Ok(None);
        }

        Ok(Some(DailyAverageWindow {
            icp_per_day: rewards / days as f64 / 100_000_000.0,
            records: row_count,
            days_covered: days,
            first_date: NaiveDate::parse_from_str(&first, "%Y-%m-%d")?,
            last_date: NaiveDate::parse_from_str(&last, "%Y-%m-%d")?,
        }))
    }

    fn get_reward_data_count(&self, neuron_id: NeuronId) -> Result<i64, Box<dyn std::error::Error>> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM daily_rewards WHERE neuron_id = ?1",
            params![neuron_id.value().to_string()],
            |row| row.get(0)
        )?;
        Ok(count)
    }

    fn get_portfolio_average_daily_reward(&self, days: i64) -> Result<Option<f64>, Box<dyn std::error::Error>> {
        // Calculate the average daily reward across the portfolio for the last N calendar days
        // Use total rewards over total calendar days to handle prorated ICP rewards correctly
        // When ICP batches rewards (e.g., 4 ICP on day 5 for days 2-5), we want to spread
        // that across the actual calendar days, not treat it as an inflated daily rate
        let result: Option<(f64, f64)> = self.conn.query_row(
            "WITH recent_dates AS (
                 SELECT reward_date
                 FROM daily_rewards
                 GROUP BY reward_date
                 ORDER BY reward_date DESC
                 LIMIT ?1
             )
             SELECT
                 SUM(CAST(total_reward_e8s AS REAL)) as total_rewards_e8s,
                 julianday(MAX(reward_date)) - julianday(MIN(reward_date)) + 1 as calendar_days
             FROM daily_rewards
             WHERE reward_date IN (SELECT reward_date FROM recent_dates)",
            params![days],
            |row| {
                let total_rewards: f64 = row.get(0)?;
                let calendar_days: f64 = row.get(1)?;
                Ok((total_rewards, calendar_days))
            }
        ).optional()?;

        Ok(result.map(|(total_rewards_e8s, calendar_days)| {
            if calendar_days > 0.0 {
                (total_rewards_e8s / calendar_days) / 100_000_000.0
            } else {
                0.0
            }
        }))
    }

    fn get_portfolio_reward_data_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        // Get the count of distinct dates across all neurons
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT reward_date) FROM daily_rewards",
            [],
            |row| row.get(0)
        )?;
        Ok(count)
    }

    fn get_newest_reward_date(&self) -> Result<Option<NaiveDate>, Box<dyn std::error::Error>> {
        let newest: Option<String> = self.conn.query_row(
            "SELECT MAX(reward_date) FROM daily_rewards",
            [],
            |row| row.get(0),
        ).optional()?.flatten();

        match newest {
            Some(s) => Ok(Some(NaiveDate::parse_from_str(&s, "%Y-%m-%d")?)),
            None => Ok(None),
        }
    }

    fn get_portfolio_window_sample(
        &self,
        lookback_days: i64,
        window_days: i64,
    ) -> Result<WindowSample, Box<dyn std::error::Error>> {
        let empty = WindowSample {
            rates: Vec::new(),
            unobservable: 0,
            zero_reward: 0,
            longest_gap: None,
        };

        // Anchor on the newest reward date rather than "today", so a stale database still
        // yields windows over the data it has instead of a run of empty ones.
        let newest: Option<String> = self.conn.query_row(
            "SELECT MAX(reward_date) FROM daily_rewards", [], |row| row.get(0),
        ).optional()?.flatten();
        let newest = match newest {
            Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")?,
            None => return Ok(empty),
        };

        let oldest: String = self.conn.query_row(
            "SELECT MIN(reward_date) FROM daily_rewards", [], |row| row.get(0),
        )?;
        let oldest = NaiveDate::parse_from_str(&oldest, "%Y-%m-%d")?;

        // One row per date, carrying the span it accrued over. `days_elapsed` is written per
        // neuron but is a property of the collection run, so MAX collapses the portfolio's
        // rows for a date without inventing a span any single neuron did not have.
        //
        // A day inside the lookback can only be covered by a row dated on or after it, so
        // restricting to the lookback loses no coverage.
        let lookback_start = newest - chrono::Duration::days(lookback_days - 1);
        let mut stmt = self.conn.prepare(
            "SELECT reward_date, MAX(days_elapsed), SUM(CAST(total_reward_e8s AS REAL))
             FROM daily_rewards
             WHERE reward_date <= ?1 AND reward_date >= ?2
             GROUP BY reward_date
             ORDER BY reward_date ASC"
        )?;
        let rows = stmt.query_map(
            params![newest.to_string(), lookback_start.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?)),
        )?;

        let mut covered: std::collections::HashSet<NaiveDate> = std::collections::HashSet::new();
        let mut booked: std::collections::HashMap<NaiveDate, f64> = std::collections::HashMap::new();
        let mut longest_gap: Option<AccrualGap> = None;

        for r in rows {
            let (date_str, days_elapsed, total) = r?;
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
            let span_days = days_elapsed.max(1);
            let span_start = date - chrono::Duration::days(span_days - 1);

            if span_days > window_days {
                // Accrual gap. The total is real but unresolved across the span, so it
                // neither covers any day nor funds any window. Recorded for the diagnostic.
                let gap = AccrualGap {
                    start: span_start,
                    end: date,
                    days: span_days,
                    total_icp: total / 100_000_000.0,
                };
                if longest_gap.map_or(true, |g: AccrualGap| gap.days > g.days) {
                    longest_gap = Some(gap);
                }
                continue;
            }

            for i in 0..span_days {
                covered.insert(span_start + chrono::Duration::days(i));
            }
            // Booked to the date it was recorded against. A short catch-up row that straddles
            // a window boundary lands wholly in the window holding its reward_date; over a
            // span no longer than the window itself the misattribution is bounded and, unlike
            // zero-filling, invents no observation.
            *booked.entry(date).or_insert(0.0) += total;
        }

        let max_buckets = lookback_days / window_days;
        let mut rates = Vec::new();
        let mut unobservable = 0usize;
        let mut zero_reward = 0usize;

        for b in 0..max_buckets {
            let window_end = newest - chrono::Duration::days(b * window_days);
            let window_start = window_end - chrono::Duration::days(window_days - 1);
            if window_start < oldest || window_start < lookback_start {
                break; // data runs out, or the window leaves the lookback: only partly observable
            }

            let fully_covered = (0..window_days)
                .all(|i| covered.contains(&(window_start + chrono::Duration::days(i))));
            if !fully_covered {
                unobservable += 1;
                continue;
            }

            let total: f64 = booked
                .iter()
                .filter(|(d, _)| **d >= window_start && **d <= window_end)
                .map(|(_, v)| *v)
                .sum();
            if total == 0.0 {
                zero_reward += 1;
            }
            rates.push(total / window_days as f64 / 100_000_000.0);
        }

        Ok(WindowSample { rates, unobservable, zero_reward, longest_gap })
    }

}

// Helper methods
impl SqliteRepository {
    fn row_to_neuron(row: &rusqlite::Row) -> Result<Neuron, rusqlite::Error> {
        let neuron_id_str: String = row.get(0)?;
        let parsed_id = u64::from_str(&neuron_id_str)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
        let neuron_id = NeuronId::new(parsed_id);

        let stake_e8s: i64 = row.get(1)?;
        let maturity_e8s: i64 = row.get(2)?;
        let staked_maturity_e8s: i64 = row.get(3)?;
        let voting_power: i64 = row.get(4)?;
        let age_days: i64 = row.get(5)?;
        let dissolve_delay_days: i64 = row.get(6)?;
        let state_str: String = row.get(7)?;
        let auto_stake: bool = row.get(8)?;
        let created_timestamp: i64 = row.get(9)?;
        // Read by column name, not position: retrieved_timestamp is appended at the end of
        // each SELECT and so lands at a different index depending on whether that query also
        // selects snapshot_date. Reading by name keeps every existing index (0-10) untouched.
        let retrieved_timestamp: i64 = row.get("retrieved_timestamp")?;

        // Parse state
        let state = match state_str.as_str() {
            "Locked" => NeuronState::Locked,
            "Dissolving" => NeuronState::Dissolving,
            "Dissolved" => NeuronState::Dissolved,
            _ => NeuronState::Locked,
        };

        // Calculate age and dissolve in seconds for bonus calculation
        let age_seconds = age_days as u64 * 86400;
        let dissolve_seconds = dissolve_delay_days as u64 * 86400;

        Ok(Neuron::from_snapshot(
            neuron_id,
            IcpAmount::from_e8s(stake_e8s as u64),
            IcpAmount::from_e8s(maturity_e8s as u64),
            IcpAmount::from_e8s(staked_maturity_e8s as u64),
            voting_power as u64,
            age_seconds,
            dissolve_seconds,
            state,
            auto_stake,
            created_timestamp as u64,
            retrieved_timestamp as u64,
        ))
    }

    /// Batch insert neuron snapshots from CSV import
    /// Uses transactions for performance and atomicity
    pub fn batch_insert_snapshots(
        &self,
        records: &[crate::infrastructure::CsvRecord],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        const BATCH_SIZE: usize = 100;
        let mut inserted_count = 0;

        // Process records in batches
        for chunk in records.chunks(BATCH_SIZE) {
            let tx = self.conn.unchecked_transaction()?;

            {
                let mut stmt = tx.prepare(
                    "INSERT OR REPLACE INTO neuron_snapshots
                     (neuron_id, snapshot_date, stake_e8s, maturity_e8s, staked_maturity_e8s,
                      voting_power, age_days, dissolve_delay_days, age_bonus_multiplier,
                      dissolve_bonus_multiplier, state, auto_stake_enabled, created_timestamp, retrieved_timestamp)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
                )?;

                for record in chunk {
                    stmt.execute(params![
                        record.neuron_id.value().to_string(),
                        record.date.to_string(),
                        record.neuron.stake().e8s() as i64,
                        record.neuron.maturity().e8s() as i64,
                        record.neuron.staked_maturity().e8s() as i64,
                        record.neuron.voting_power() as i64,
                        record.neuron.age_days() as i64,
                        record.neuron.dissolve_delay_days() as i64,
                        record.neuron.age_bonus().value(),
                        record.neuron.dissolve_bonus().value(),
                        format!("{:?}", record.neuron.state()),
                        record.neuron.auto_stake_enabled(),
                        record.neuron.created_at().timestamp(),
                        record.neuron.retrieved_at().timestamp(),
                    ])?;
                    inserted_count += 1;
                }
            }

            tx.commit()?;
        }

        Ok(inserted_count)
    }

    /// Check if a snapshot already exists for a given neuron and date
    pub fn snapshot_exists(
        &self,
        neuron_id: NeuronId,
        date: NaiveDate,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM neuron_snapshots WHERE neuron_id = ?1 AND snapshot_date = ?2",
            params![neuron_id.value().to_string(), date.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get count of snapshots for a specific neuron
    #[allow(dead_code)]
    pub fn get_snapshot_count(&self, neuron_id: NeuronId) -> Result<usize, Box<dyn std::error::Error>> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM neuron_snapshots WHERE neuron_id = ?1",
            params![neuron_id.value().to_string()],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Get all unique neuron IDs from neuron_snapshots table
    pub fn get_all_neuron_ids(&self) -> Result<Vec<NeuronId>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT neuron_id FROM neuron_snapshots ORDER BY neuron_id"
        )?;

        let neuron_ids: Vec<NeuronId> = stmt.query_map([], |row| {
            let neuron_id_str: String = row.get(0)?;
            let parsed_id = u64::from_str(&neuron_id_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(NeuronId::new(parsed_id))
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(neuron_ids)
    }

    /// Get all snapshots for a specific neuron, sorted by date
    pub fn get_all_snapshots_for_neuron(&self, neuron_id: NeuronId) -> Result<Vec<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT neuron_id, stake_e8s, maturity_e8s, staked_maturity_e8s, voting_power,
                    age_days, dissolve_delay_days, state, auto_stake_enabled, created_timestamp, snapshot_date,
                    retrieved_timestamp
             FROM neuron_snapshots
             WHERE neuron_id = ?1
             ORDER BY snapshot_date ASC"
        )?;

        let rows = stmt.query_map(params![neuron_id.value().to_string()], |row| {
            let neuron = Self::row_to_neuron(row)?;
            let date_str: String = row.get(10)?;
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok((neuron, date))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Get all unique dates from neuron_snapshots table
    pub fn get_all_unique_dates(&self) -> Result<Vec<NaiveDate>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT snapshot_date FROM neuron_snapshots ORDER BY snapshot_date ASC"
        )?;

        let rows = stmt.query_map([], |row| {
            let date_str: String = row.get(0)?;
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(date)
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Get the latest snapshot for all tracked neurons
    /// Returns a vector of (Neuron, NaiveDate) tuples
    pub fn get_all_latest_snapshots(&self) -> Result<Vec<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT n1.neuron_id, n1.stake_e8s, n1.maturity_e8s, n1.staked_maturity_e8s, n1.voting_power,
                    n1.age_days, n1.dissolve_delay_days, n1.state, n1.auto_stake_enabled,
                    n1.created_timestamp, n1.snapshot_date,
                    n1.retrieved_timestamp AS retrieved_timestamp
             FROM neuron_snapshots n1
             INNER JOIN (
                 SELECT neuron_id, MAX(snapshot_date) as max_date
                 FROM neuron_snapshots
                 GROUP BY neuron_id
             ) n2 ON n1.neuron_id = n2.neuron_id AND n1.snapshot_date = n2.max_date
             ORDER BY n1.neuron_id"
        )?;

        let rows = stmt.query_map([], |row| {
            let neuron = Self::row_to_neuron(row)?;
            let date_str: String = row.get(10)?;
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok((neuron, date))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Check if a portfolio snapshot exists for a given date
    pub fn portfolio_snapshot_exists(&self, date: NaiveDate) -> Result<bool, Box<dyn std::error::Error>> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM portfolio_snapshots WHERE snapshot_date = ?",
            params![date.to_string()],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    /// Query snapshots with optional filters for export
    /// Returns tuples with all fields needed for CSV export:
    /// (neuron_id, snapshot_date, stake_e8s, staked_maturity_e8s, available_maturity_e8s,
    ///  voting_power, dissolve_delay_seconds, age_seconds, created_timestamp_seconds)
    pub fn query_snapshots_for_export(
        &self,
        neuron_ids: Option<&[NeuronId]>,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<Vec<ExportRow>, Box<dyn std::error::Error>> {
        // Build dynamic query based on filters
        // Note: age_days and dissolve_delay_days are stored as days in DB but need to be exported as seconds
        let mut query = String::from(
            "SELECT neuron_id, snapshot_date, stake_e8s, staked_maturity_e8s, maturity_e8s, \
                    voting_power, dissolve_delay_days, age_days, created_timestamp, \
                    retrieved_timestamp \
             FROM neuron_snapshots WHERE 1=1"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Add neuron_id filter
        if let Some(ids) = neuron_ids {
            if !ids.is_empty() {
                let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND neuron_id IN ({})", placeholders));
                for neuron_id in ids {
                    params_vec.push(Box::new(neuron_id.value().to_string()));
                }
            }
        }

        // Add date range filters
        if let Some(start) = start_date {
            query.push_str(" AND snapshot_date >= ?");
            params_vec.push(Box::new(start.to_string()));
        }

        if let Some(end) = end_date {
            query.push_str(" AND snapshot_date <= ?");
            params_vec.push(Box::new(end.to_string()));
        }

        // Order by neuron_id and date for consistent output
        query.push_str(" ORDER BY neuron_id ASC, snapshot_date ASC");

        // Execute query
        let mut stmt = self.conn.prepare(&query)?;

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let neuron_id_str: String = row.get(0)?;
            let neuron_id = u64::from_str(&neuron_id_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            let date_str: String = row.get(1)?;
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;

            // Get values from database
            let stake_e8s = row.get::<_, i64>(2)? as u64;
            let staked_maturity_e8s = row.get::<_, i64>(3)? as u64;
            let maturity_e8s = row.get::<_, i64>(4)? as u64; // available_maturity
            let voting_power = row.get::<_, i64>(5)? as u64;
            let dissolve_delay_days = row.get::<_, i64>(6)? as u64;
            let age_days = row.get::<_, i64>(7)? as u64;
            let created_timestamp = row.get::<_, i64>(8)? as u64;
            let retrieved_timestamp = row.get::<_, i64>(9)? as u64;

            // Convert days to seconds for CSV export (86400 seconds per day)
            let dissolve_delay_seconds = dissolve_delay_days * 86400;
            let age_seconds = age_days * 86400;

            Ok(ExportRow {
                neuron_id: NeuronId::new(neuron_id),
                date,
                stake_e8s,
                staked_maturity_e8s,
                available_maturity_e8s: maturity_e8s,
                voting_power,
                dissolve_delay_seconds,
                age_seconds,
                created_timestamp_seconds: created_timestamp,
                retrieved_timestamp_seconds: retrieved_timestamp,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }
}

// Implement traits for &SqliteRepository to allow borrowing
impl NeuronSnapshotRepository for &SqliteRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
        <SqliteRepository as NeuronSnapshotRepository>::save_snapshot(*self, neuron, date)
    }

    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<Neuron>, Box<dyn std::error::Error>> {
        <SqliteRepository as NeuronSnapshotRepository>::get_snapshot(*self, neuron_id, date)
    }

    fn get_latest_snapshot(&self, neuron_id: NeuronId) -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        <SqliteRepository as NeuronSnapshotRepository>::get_latest_snapshot(*self, neuron_id)
    }

    fn get_previous_snapshot(&self, neuron_id: NeuronId, before_date: NaiveDate) -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        <SqliteRepository as NeuronSnapshotRepository>::get_previous_snapshot(*self, neuron_id, before_date)
    }

    fn get_snapshots_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) -> Result<Vec<(Neuron, NaiveDate)>, Box<dyn std::error::Error>> {
        <SqliteRepository as NeuronSnapshotRepository>::get_snapshots_range(*self, neuron_id, start, end)
    }

    fn get_all_snapshots_for_date(&self, date: NaiveDate) -> Result<Vec<Neuron>, Box<dyn std::error::Error>> {
        <SqliteRepository as NeuronSnapshotRepository>::get_all_snapshots_for_date(*self, date)
    }
}

impl PortfolioSnapshotRepository for &SqliteRepository {
    fn save_snapshot(&self, portfolio: &Portfolio, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
        <SqliteRepository as PortfolioSnapshotRepository>::save_snapshot(*self, portfolio, date)
    }

    fn get_snapshot(&self, date: NaiveDate) -> Result<Option<Portfolio>, Box<dyn std::error::Error>> {
        <SqliteRepository as PortfolioSnapshotRepository>::get_snapshot(*self, date)
    }

    fn get_latest_snapshot(&self) -> Result<Option<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>> {
        <SqliteRepository as PortfolioSnapshotRepository>::get_latest_snapshot(*self)
    }

    fn get_snapshots_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>> {
        <SqliteRepository as PortfolioSnapshotRepository>::get_snapshots_range(*self, start, end)
    }
}

impl DailyRewardRepository for &SqliteRepository {
    fn save_reward(&self, neuron_id: NeuronId, date: NaiveDate, maturity_delta: i64, staked_maturity_delta: i64, days_elapsed: i64) -> Result<(), Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::save_reward(*self, neuron_id, date, maturity_delta, staked_maturity_delta, days_elapsed)
    }

    fn get_reward(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<DailyReward>, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_reward(*self, neuron_id, date)
    }

    fn get_rewards_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyReward>, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_rewards_range(*self, neuron_id, start, end)
    }

    fn get_average_daily_reward_window(&self, neuron_id: NeuronId, records: i64) -> Result<Option<DailyAverageWindow>, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_average_daily_reward_window(*self, neuron_id, records)
    }

    fn get_reward_data_count(&self, neuron_id: NeuronId) -> Result<i64, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_reward_data_count(*self, neuron_id)
    }

    fn get_portfolio_average_daily_reward(&self, days: i64) -> Result<Option<f64>, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_portfolio_average_daily_reward(*self, days)
    }

    fn get_portfolio_reward_data_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_portfolio_reward_data_count(*self)
    }

    fn get_portfolio_window_sample(&self, lookback_days: i64, window_days: i64) -> Result<WindowSample, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_portfolio_window_sample(*self, lookback_days, window_days)
    }

    fn get_newest_reward_date(&self) -> Result<Option<NaiveDate>, Box<dyn std::error::Error>> {
        <SqliteRepository as DailyRewardRepository>::get_newest_reward_date(*self)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::Utc;

    /// Build a repository backed by a real on-disk SQLite file with migrations applied.
    ///
    /// Every other test in this crate runs against pure functions or a mock repository. These
    /// exercise the actual persistence layer, which is one of the two files the 2026-08-04
    /// audit flagged as crossing a trust boundary with zero coverage.
    fn temp_repo() -> (TempDir, SqliteRepository) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("test.db");
        let repo = SqliteRepository::new(path.to_str().unwrap()).expect("open repo");
        (dir, repo)
    }

    fn sample_neuron(id: u64) -> Neuron {
        Neuron::new(
            NeuronId::new(id),
            IcpAmount::from_e8s(194_098_425_353),
            IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(73_568_491_574),
            602_802_786_833,
            126_230_400,  // 4 years
            252_460_800,  // 8 years
            NeuronState::Locked,
            true,
            1_621_209_600,
        )
    }

    // ---- Window sampling across collection gaps -----------------------------------------
    //
    // The 2026-01-07 -> 2026-08-06 gap in the real database was never covered by a test. The
    // old implementation zero-filled every window it produced no rows for, so a 211-day
    // outage arrived at the projection as eleven consecutive zero-income weeks and drove p10
    // to 0.0 — reported to the user as "zero or negative growth" over 545 ICP of real rewards.

    const NID: u64 = 18_114_914_950_691_531_093;

    /// Write one reward row per date at `icp_per_day`, `days_elapsed = 1`.
    fn seed_daily_rewards(repo: &SqliteRepository, start: NaiveDate, days: i64, icp_per_day: f64) {
        let e8s = (icp_per_day * 100_000_000.0) as i64;
        for i in 0..days {
            DailyRewardRepository::save_reward(
                repo,
                NeuronId::new(NID),
                start + chrono::Duration::days(i),
                0,
                e8s,
                1,
            )
            .expect("save reward");
        }
    }

    /// A 211-day gap must yield NO populated windows and must not fabricate zero ones.
    #[test]
    fn a_collection_gap_produces_no_windows_rather_than_zero_filled_ones() {
        let (_dir, repo) = temp_repo();

        // Healthy daily history, then a single row carrying 211 days of accrual — exactly the
        // shape `track` wrote on 2026-08-06 after the outage.
        let start = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
        seed_daily_rewards(&repo, start, 68, 2.6); // through 2026-01-07
        DailyRewardRepository::save_reward(
            &repo,
            NeuronId::new(NID),
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            0,
            54_560_098_591, // 545.60 ICP
            211,
        )
        .expect("save gap reward");

        let sample = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7)
            .expect("window sample");

        assert_eq!(
            sample.populated(),
            0,
            "no 7-day window inside the gap is observable; got rates {:?}",
            sample.rates
        );
        assert_eq!(sample.unobservable, 12, "all 12 windows in the lookback are unobserved");
        assert_eq!(sample.zero_reward, 0, "an unobserved window is not a zero-reward window");

        let gap = sample.longest_gap.expect("the 211-day row is reported as a gap");
        assert_eq!(gap.days, 211);
        assert_eq!(gap.end, NaiveDate::from_ymd_opt(2026, 8, 6).unwrap());
        assert!((gap.total_icp - 545.60098591).abs() < 1e-6, "gap total: {}", gap.total_icp);
    }

    /// The gap must not reach `RewardPercentiles` as a satisfied floor.
    #[test]
    fn a_collection_gap_does_not_satisfy_the_min_windows_floor() {
        let (_dir, repo) = temp_repo();
        let start = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
        seed_daily_rewards(&repo, start, 68, 2.6);
        DailyRewardRepository::save_reward(
            &repo,
            NeuronId::new(NID),
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            0,
            54_560_098_591,
            211,
        )
        .expect("save gap reward");

        let sample = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7)
            .expect("window sample");
        let bands = crate::domain::retirement::RewardPercentiles::from_window_rates(
            sample.rates.clone(),
            7,
            90,
        );
        assert!(
            bands.is_none(),
            "bands must not be computable from a gap; got {:?}",
            bands
        );
    }

    /// Uninterrupted daily tracking must still clear the floor — the fix must not starve
    /// healthy data. Strict containment did: it dropped real weeks to 8 populated windows.
    #[test]
    fn uninterrupted_history_fills_every_window() {
        let (_dir, repo) = temp_repo();
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        seed_daily_rewards(&repo, start, 120, 3.0);

        let sample = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7)
            .expect("window sample");

        assert_eq!(sample.unobservable, 0, "nothing is missing");
        assert_eq!(sample.populated(), 12, "90 days holds 12 whole 7-day windows");
        assert!(
            sample.rates.iter().all(|r| (r - 3.0).abs() < 1e-9),
            "every window is 3.0 ICP/day; got {:?}",
            sample.rates
        );
    }

    /// A short catch-up row (`days_elapsed` <= window) still covers its span, so the window
    /// stays observable. This is the case strict containment wrongly excluded.
    #[test]
    fn a_short_catch_up_row_keeps_its_window_observable() {
        let (_dir, repo) = temp_repo();
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        seed_daily_rewards(&repo, start, 120, 3.0);

        // Replace one date with a 2-day catch-up covering the day before it, and delete that
        // day's own row — a skipped run, which is not a gap.
        repo.conn
            .execute("DELETE FROM daily_rewards WHERE reward_date = '2026-03-10'", [])
            .expect("delete");
        DailyRewardRepository::save_reward(
            &repo,
            NeuronId::new(NID),
            NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(),
            0,
            600_000_000, // 6 ICP over 2 days
            2,
        )
        .expect("save catch-up");

        let sample = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7)
            .expect("window sample");
        assert_eq!(
            sample.unobservable, 0,
            "a 2-day catch-up covers its span; no window is unobserved"
        );
        assert_eq!(sample.populated(), 12);
    }

    /// Genuine zero-reward weeks are observations and must survive as populated 0.0 rates.
    /// They are the reason absence cannot simply be treated as zero: both look like 0.
    #[test]
    fn genuine_zero_reward_windows_count_as_populated() {
        let (_dir, repo) = temp_repo();
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        seed_daily_rewards(&repo, start, 120, 3.0);

        // One whole week where rows exist and every reward really was zero. Windows are
        // anchored on the newest reward date (2026-04-30), so this is window b=3 exactly;
        // a range straddling two windows would zero neither of them.
        repo.conn
            .execute(
                "UPDATE daily_rewards SET total_reward_e8s = 0, staked_maturity_delta_e8s = 0
                 WHERE reward_date BETWEEN '2026-04-03' AND '2026-04-09'",
                [],
            )
            .expect("zero a week");

        let sample = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7)
            .expect("window sample");

        assert_eq!(sample.unobservable, 0, "the rows exist; nothing is missing");
        assert_eq!(sample.populated(), 12);
        assert_eq!(
            sample.zero_reward, 1,
            "the zeroed week is an observation of zero income, not an absence"
        );
        assert!(
            sample.rates.iter().any(|r| *r == 0.0),
            "the genuine zero week appears as a 0.0 rate: {:?}",
            sample.rates
        );
    }

    // ---- What the daily average actually covers -----------------------------------------

    /// Uninterrupted daily rows: records and days coincide, which is the only case in which
    /// the old "30-day average" label happened to be true.
    #[test]
    fn average_window_reports_records_days_and_span() {
        let (_dir, repo) = temp_repo();
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        seed_daily_rewards(&repo, start, 40, 3.0);

        let w = DailyRewardRepository::get_average_daily_reward_window(&repo, NeuronId::new(NID), 30)
            .expect("query")
            .expect("window");

        assert_eq!(w.records, 30, "30 rows read");
        assert_eq!(w.days_covered, 30, "days_elapsed = 1 each, so 30 rows span 30 days");
        assert_eq!(w.last_date, NaiveDate::from_ymd_opt(2026, 2, 9).unwrap());
        assert_eq!(w.first_date, NaiveDate::from_ymd_opt(2026, 1, 11).unwrap());
        assert!((w.icp_per_day - 3.0).abs() < 1e-9, "rate {}", w.icp_per_day);
    }

    /// The case the labels got wrong: a gap row makes the divisor far exceed the record
    /// count, so "30 records" and "30 days" are different claims and the span is neither.
    #[test]
    fn average_window_divisor_counts_gap_days_not_records() {
        let (_dir, repo) = temp_repo();
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        seed_daily_rewards(&repo, start, 29, 3.0); // 2026-01-01 .. 2026-01-29, 87 ICP

        // One row carrying 211 days of accrual, as `track` wrote on 2026-08-06.
        DailyRewardRepository::save_reward(
            &repo,
            NeuronId::new(NID),
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            0,
            54_560_000_000, // 545.60 ICP
            211,
        )
        .expect("save gap reward");

        let w = DailyRewardRepository::get_average_daily_reward_window(&repo, NeuronId::new(NID), 30)
            .expect("query")
            .expect("window");

        assert_eq!(w.records, 30, "30 rows read");
        assert_eq!(w.days_covered, 240, "29 single days + one 211-day accrual");
        assert_eq!(w.first_date, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(w.last_date, NaiveDate::from_ymd_opt(2026, 8, 6).unwrap());

        // (87 + 545.60) / 240 — divided by days accrued, not by rows read.
        let expected = (87.0 + 545.60) / 240.0;
        assert!(
            (w.icp_per_day - expected).abs() < 1e-9,
            "rate {} != {}",
            w.icp_per_day,
            expected
        );
        assert_ne!(
            w.days_covered, w.records,
            "the divisor and the record count are different quantities; labels must not conflate them"
        );
    }

    /// The regression this whole change exists to prevent.
    ///
    /// `retrieved_timestamp` was written on save but never read back, so a neuron loaded from
    /// the database was stamped with `Utc::now()` — silently replacing the observation time
    /// with the report time and destroying the only durable provenance marker in the schema.
    #[test]
    fn retrieved_at_survives_a_save_load_round_trip() {
        let (_dir, repo) = temp_repo();
        let date = NaiveDate::from_ymd_opt(2025, 11, 7).unwrap();
        let neuron = sample_neuron(18_114_914_950_691_531_093);
        let saved_at = neuron.retrieved_at();

        NeuronSnapshotRepository::save_snapshot(&repo, &neuron, date).expect("save");
        let loaded = NeuronSnapshotRepository::get_snapshot(&repo, NeuronId::new(18_114_914_950_691_531_093), date)
            .expect("read")
            .expect("row present");

        // Equal to the second — the column stores whole seconds.
        assert_eq!(
            loaded.retrieved_at().timestamp(),
            saved_at.timestamp(),
            "retrieved_at must come from the stored column, not from Utc::now()"
        );
    }

    /// A snapshot observed in the past must still read as the past, not as now.
    #[test]
    fn retrieved_at_preserves_a_historical_observation_time() {
        let (_dir, repo) = temp_repo();
        let date = NaiveDate::from_ymd_opt(2025, 11, 7).unwrap();

        // 2025-11-07 03:45:31 UTC — the timestamp the real bulk import carries.
        let bulk_import_ts: i64 = 1_762_487_131;
        let neuron = Neuron::from_snapshot(
            NeuronId::new(337_988_880_188_974_440),
            IcpAmount::from_e8s(193_999_000_493),
            IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(73_568_491_574),
            602_802_786_833,
            126_230_400,
            252_460_800,
            NeuronState::Locked,
            true,
            1_621_209_600,
            bulk_import_ts as u64,
        );

        NeuronSnapshotRepository::save_snapshot(&repo, &neuron, date).expect("save");
        let loaded = NeuronSnapshotRepository::get_snapshot(&repo, NeuronId::new(337_988_880_188_974_440), date)
            .expect("read")
            .expect("row present");

        assert_eq!(loaded.retrieved_at().timestamp(), bulk_import_ts);
        assert!(
            loaded.retrieved_at() < Utc::now(),
            "a historical snapshot must not read as retrieved now"
        );
    }

    /// `retrieved_timestamp` lands at a different column index depending on whether the query
    /// also selects `snapshot_date`, so it is read by name. This covers both shapes.
    #[test]
    fn retrieved_at_is_populated_across_every_read_path() {
        let (_dir, repo) = temp_repo();
        let d1 = NaiveDate::from_ymd_opt(2025, 11, 6).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2025, 11, 7).unwrap();
        let id = NeuronId::new(15_085_838_586_007_515_412);
        let ts: u64 = 1_762_487_131;

        for date in [d1, d2] {
            let n = Neuron::from_snapshot(
                id,
                IcpAmount::from_e8s(206_363_930_207),
                IcpAmount::from_e8s(0),
                IcpAmount::from_e8s(0),
                257_954_912_759,
                126_230_400,
                252_460_800,
                NeuronState::Locked,
                false,
                1_724_371_200,
                ts,
            );
            NeuronSnapshotRepository::save_snapshot(&repo, &n, date).expect("save");
        }

        // 10-column shape (no snapshot_date)
        let by_date = NeuronSnapshotRepository::get_all_snapshots_for_date(&repo, d2).expect("by date");
        assert_eq!(by_date.len(), 1);
        assert_eq!(by_date[0].retrieved_at().timestamp(), ts as i64);

        // 11-column shape (snapshot_date at index 10, retrieved_timestamp appended after)
        let (latest, latest_date) = NeuronSnapshotRepository::get_latest_snapshot(&repo, id).expect("latest").expect("present");
        assert_eq!(latest_date, d2);
        assert_eq!(latest.retrieved_at().timestamp(), ts as i64);

        let range = NeuronSnapshotRepository::get_snapshots_range(&repo, id, d1, d2).expect("range");
        assert_eq!(range.len(), 2);
        for (n, _) in &range {
            assert_eq!(n.retrieved_at().timestamp(), ts as i64);
        }

        let all = repo.get_all_snapshots_for_neuron(id).expect("all for neuron");
        assert_eq!(all.len(), 2);
        for (n, _) in &all {
            assert_eq!(n.retrieved_at().timestamp(), ts as i64);
        }

        let prev = NeuronSnapshotRepository::get_previous_snapshot(&repo, id, d2).expect("previous").expect("present");
        assert_eq!(prev.1, d1);
        assert_eq!(prev.0.retrieved_at().timestamp(), ts as i64);
    }

    /// The window rates must be NON-OVERLAPPING: each reward day contributes to exactly one
    /// window. Seeded with one distinct value per week so double-counting would be visible.
    #[test]
    fn window_rates_are_non_overlapping_and_complete_only() {
        let (_dir, repo) = temp_repo();
        let anchor = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let id = NeuronId::new(111);

        // Every day for 90 days back, so all 12 windows are fully covered by data. Each day
        // in window b pays (b+1) ICP, making window b's daily rate exactly (b+1).
        for d in 0..90i64 {
            let day = anchor - chrono::Duration::days(d);
            let week = d / 7;
            DailyRewardRepository::save_reward(
                &repo, id, day, 0, (week + 1) * 100_000_000, 1,
            ).unwrap();
        }

        let rates = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7).unwrap().rates;

        // 90/7 = 12 whole windows, and data covers all of them.
        assert_eq!(rates.len(), 12, "expected 12 complete windows, got {:?}", rates);

        // Each week's single payment lands in exactly one window: window b holds (b+1) ICP
        // spread over 7 days. If windows overlapped, neighbouring values would repeat.
        for (b, r) in rates.iter().enumerate() {
            let expected = b as f64 + 1.0;
            assert!(
                (r - expected).abs() < 1e-9,
                "window {} = {}, expected {} — overlap would blend neighbouring weeks",
                b, r, expected
            );
        }
        // Each day counted exactly once: the sum of rates equals the sum of weekly values.
        let sum: f64 = rates.iter().sum();
        let expected_sum: f64 = (1..=12).map(|i| i as f64).sum();
        assert!((sum - expected_sum).abs() < 1e-9, "sum {} != {}", sum, expected_sum);
    }

    /// A window only partly covered by data is excluded rather than counted low.
    #[test]
    fn incomplete_trailing_window_is_excluded() {
        let (_dir, repo) = temp_repo();
        let anchor = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        // Only 20 days of history: 2 complete 7-day windows, and a third only partly covered.
        for d in 0..20i64 {
            DailyRewardRepository::save_reward(
                &repo, NeuronId::new(111), anchor - chrono::Duration::days(d), 0, 100_000_000, 1,
            ).unwrap();
        }
        let rates = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7).unwrap().rates;
        assert_eq!(rates.len(), 2, "only whole windows count, got {:?}", rates);
        for r in &rates {
            assert!((r - 1.0/7.0*1.0).abs() < 1e-9 || (r - 1.0).abs() < 1e-9 || *r > 0.0);
        }
    }

    /// An empty rewards table yields no windows rather than an error or a phantom zero.
    #[test]
    fn no_rewards_yields_no_windows() {
        let (_dir, repo) = temp_repo();
        let sample = DailyRewardRepository::get_portfolio_window_sample(&repo, 90, 7).unwrap();
        assert!(sample.rates.is_empty());
        assert_eq!(sample.unobservable, 0, "no data at all is not 12 unobserved windows");
        assert_eq!(sample.zero_reward, 0);
        assert!(sample.longest_gap.is_none());
    }

    /// Two rows written by different ingest paths must remain distinguishable on read —
    /// this is the property the 4,408/252 provenance split depends on.
    #[test]
    fn distinct_retrieval_times_remain_distinguishable_on_read() {
        let (_dir, repo) = temp_repo();
        let date = NaiveDate::from_ymd_opt(2025, 11, 7).unwrap();
        let bulk: u64 = 1_762_487_131;   // 03:45:31 — CSV import
        let automated: u64 = 1_762_548_269; // 20:44:29 — tracker run, same calendar day

        let a = Neuron::from_snapshot(
            NeuronId::new(111), IcpAmount::from_e8s(1), IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(0), 1, 0, 252_460_800, NeuronState::Locked, false, 1, bulk,
        );
        let b = Neuron::from_snapshot(
            NeuronId::new(222), IcpAmount::from_e8s(1), IcpAmount::from_e8s(0),
            IcpAmount::from_e8s(0), 1, 0, 252_460_800, NeuronState::Locked, false, 1, automated,
        );
        NeuronSnapshotRepository::save_snapshot(&repo, &a, date).expect("save a");
        NeuronSnapshotRepository::save_snapshot(&repo, &b, date).expect("save b");

        let rows = NeuronSnapshotRepository::get_all_snapshots_for_date(&repo, date).expect("read");
        assert_eq!(rows.len(), 2);
        let mut stamps: Vec<i64> = rows.iter().map(|n| n.retrieved_at().timestamp()).collect();
        stamps.sort();
        assert_eq!(stamps, vec![bulk as i64, automated as i64]);
        assert_ne!(
            stamps[0], stamps[1],
            "same-day rows from different ingest paths must not collapse to one timestamp"
        );
    }
}
