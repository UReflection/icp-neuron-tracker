use serde::{Deserialize, Serialize};

/// Value object representing ICP amount in e8s (10^-8 ICP)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcpAmount(u64);

impl IcpAmount {
    pub fn from_e8s(e8s: u64) -> Self {
        Self(e8s)
    }

    pub fn to_icp(&self) -> f64 {
        self.0 as f64 / 100_000_000.0
    }
    
    #[allow(dead_code)]
    pub fn e8s(&self) -> u64 {
        self.0
    }
}

impl std::ops::Add for IcpAmount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.checked_add(rhs.0).expect("IcpAmount overflow: addition would exceed maximum value"))
    }
}

impl std::ops::Sub for IcpAmount {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.checked_sub(rhs.0).expect("IcpAmount underflow: subtraction would result in negative value"))
    }
}

/// Value object for neuron ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NeuronId(u64);

impl NeuronId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NeuronId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Neuron state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeuronState {
    Locked,
    Dissolving,
    Dissolved,
}

impl From<i32> for NeuronState {
    fn from(state: i32) -> Self {
        match state {
            1 => NeuronState::Locked,
            2 => NeuronState::Dissolving,
            3 => NeuronState::Dissolved,
            _ => NeuronState::Locked,
        }
    }
}

/// Bonus multiplier
///
/// # Year length: 365.25 here, 365.0 elsewhere — both are deliberate
///
/// This codebase uses two different year lengths on purpose. Before "fixing" an
/// inconsistency found by grepping for `365`, establish which class the site belongs to.
///
/// **Class A — calendar conversion: 365.25 days.** Converting a duration in seconds or days
/// into years, or the reverse. 365.25 is the mean Gregorian year and is what the age and
/// dissolve bonus curves below are defined against. Sites: this file, `neuron_detail.rs`,
/// `reward_analysis.rs` (`*_years` helpers), `portfolio_report.rs`, `retirement_service.rs`,
/// and the two inversion constants in `csv_parser.rs`.
///
/// **Class B — annualisation: 365.0 days.** Multiplying a *daily rate* by days-per-year to
/// project annual income or an APY. This is financial convention, not a calendar
/// conversion: the quantity is "what a year at this daily rate yields". All six Class B
/// sites agree with each other and are individually marked.
///
/// Mixing the two is a real defect and has bitten this code twice — `csv_parser.rs`
/// inverted bonuses on a 365-day year while the functions below divide by 365.25, so an
/// imported 1.25 age bonus came back as 1.2498288843258043 and the dissolve ceiling was
/// unreachable. Both are fixed. The Class B sites were reviewed at the same time and are
/// correct as they stand.
///
/// The dissolve ceiling referred to above was 2.0x over eight years when that note was
/// written. Mission 70 made it 3.0x over two, and the curve quadratic — see
/// [`BonusMultiplier::from_dissolve_seconds`]. The year-length classification is unaffected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BonusMultiplier(f64);

impl BonusMultiplier {
    #[allow(dead_code)]
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> f64 {
        self.0
    }

    /// Age bonus: linear from 1.0x to 1.25x over four years.
    ///
    /// Unchanged by Mission 70, and independently confirmed against the chain — see
    /// [`Self::from_dissolve_seconds`] for the measurement. Two neurons of different ages in
    /// the production database reproduce this curve to within 0.002%.
    pub fn from_age_seconds(age_seconds: u64) -> Self {
        let age_years = age_seconds as f64 / (365.25 * 86400.0);
        let bonus = (age_years / MAX_AGE_BONUS_YEARS).min(1.0) * MAX_AGE_BONUS;
        Self(1.0 + bonus)
    }

    /// Dissolve-delay bonus: **quadratic** from 1.0x to 3.0x over **two years**.
    ///
    /// Mission 70 replaced the previous curve — linear from 1.0x to 2.0x over eight years —
    /// and this function carried the superseded version until 2026-08-06. Against live data
    /// the difference was not marginal: neurons at the new 730-day maximum were scored 1.25x
    /// (`1 + 730/2922`) where the protocol gives them the full 3.0x, understating the bonus
    /// 2.4-fold, on every row written since the change.
    ///
    /// Source: `dissolve_delay_bonus_multiplier` in `rs/nns/governance/src/neuron/types.rs`
    /// of `dfinity/ic`, whose documentation states "The maximum dissolve delay bonus is 3x,
    /// which occurs at 2 years. This bonus increases quadratically", and of the prior
    /// behaviour "Prior to Mission 70, this increased linearly up to 2x over 8 years".
    /// Corroborated by `docs.internetcomputer.org/concepts/governance/` ("dissolve delay
    /// bonus (up to 3x at 2 years)"). Note that `learn.internetcomputer.org` and
    /// `support.dfinity.org` still describe the 8-year/2x curve; they are stale.
    ///
    /// **The endpoints are verified; the interior of the curve is not.** Every neuron in the
    /// production database sits at the 730-day maximum, so the quadratic shape could not be
    /// checked against an observation between the endpoints. `1 + 2·(d/dmax)²` is the
    /// reading of "increases quadratically" that satisfies both endpoints; if the protocol
    /// uses a different quadratic, mid-range delays will be wrong while 0 and 730 stay right.
    ///
    /// These constants are governance-mutable and are hardcoded here only because the
    /// governance canister does not expose them on any interface this tool already calls.
    /// They are correct as of the date below and must be re-verified, not trusted.
    pub fn from_dissolve_seconds(dissolve_seconds: u64) -> Self {
        let dissolve_years = dissolve_seconds as f64 / (365.25 * 86400.0);
        let fraction_of_max = (dissolve_years / MAX_DISSOLVE_DELAY_YEARS).min(1.0);
        Self(1.0 + fraction_of_max.powi(2) * MAX_DISSOLVE_DELAY_BONUS)
    }
}

/// NNS protocol parameters, verified 2026-08-06. **Governance-mutable — re-verify.**
///
/// The previous values here were the pre-Mission-70 ones and went wrong silently: nothing in
/// the tool compares them against the chain, so a superseded constant produces a confident
/// wrong number rather than an error. `voting_power` *is* read from the chain, which is the
/// only reason the drift was visible at all — the tool printed its own 1.46x combined
/// multiplier beside a chain-sourced voting power implying 3.85x.
mod protocol {
    /// Age at which the age bonus saturates.
    pub const MAX_AGE_BONUS_YEARS: f64 = 4.0;
    /// Bonus added at saturation: 1.25x total.
    pub const MAX_AGE_BONUS: f64 = 0.25;
    /// Maximum dissolve delay, and the point at which the dissolve bonus saturates.
    /// Was 8.0 before Mission 70.
    pub const MAX_DISSOLVE_DELAY_YEARS: f64 = 2.0;
    /// Bonus added at saturation: 3.0x total. Was 1.0 (2.0x total) before Mission 70.
    pub const MAX_DISSOLVE_DELAY_BONUS: f64 = 2.0;
}

use protocol::{MAX_AGE_BONUS, MAX_AGE_BONUS_YEARS, MAX_DISSOLVE_DELAY_BONUS, MAX_DISSOLVE_DELAY_YEARS};

impl std::ops::Mul for BonusMultiplier {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}
#[cfg(test)]
mod bonus_tests {
    use super::*;

    const YEAR: u64 = 31_557_600; // 365.25 days, matching the conversion in csv_parser

    /// The endpoints, which are the part of the curve that is actually verified.
    #[test]
    fn dissolve_bonus_runs_from_1x_to_3x_over_two_years() {
        assert_eq!(BonusMultiplier::from_dissolve_seconds(0).value(), 1.0);
        assert_eq!(BonusMultiplier::from_dissolve_seconds(2 * YEAR).value(), 3.0);
    }

    /// Mission 70 moved the maximum from eight years to two. A delay at or beyond the old
    /// maximum must saturate, not extrapolate.
    #[test]
    fn dissolve_bonus_saturates_beyond_two_years() {
        for years in [2_u64, 3, 8, 100] {
            assert_eq!(
                BonusMultiplier::from_dissolve_seconds(years * YEAR).value(),
                3.0,
                "{years} years must be capped at the 3.0x maximum"
            );
        }
    }

    /// Quadratic, not linear: at half the maximum delay the bonus is a quarter of the way
    /// up, not half. This is the property that distinguishes the Mission 70 curve from the
    /// one it replaced, and from a naive linear rewrite to the new endpoints.
    #[test]
    fn dissolve_bonus_is_quadratic_not_linear() {
        let half = BonusMultiplier::from_dissolve_seconds(YEAR).value();
        assert!((half - 1.5).abs() < 1e-9, "1 year should give 1 + 2*(0.5)^2 = 1.5, got {half}");

        let linear_equivalent = 1.0 + 2.0 * 0.5;
        assert!(
            (half - linear_equivalent).abs() > 0.4,
            "a linear curve would give {linear_equivalent}; the quadratic must differ"
        );
    }

    /// The regression this replaced: the superseded curve scored a neuron at the current
    /// maximum delay as 1.25x, understating the real 3.0x bonus 2.4-fold. 730 days is what
    /// the chain reported for every neuron in the production database on 2026-08-06.
    #[test]
    fn a_neuron_at_the_current_maximum_delay_is_not_scored_as_a_quarter_bonus() {
        let observed_delay_seconds = 730 * 86400;
        let bonus = BonusMultiplier::from_dissolve_seconds(observed_delay_seconds).value();

        assert!(bonus > 2.99, "730 days is the maximum delay and earns ~3.0x, got {bonus}");

        let superseded = 1.0 + (730.0 / 2922.0);
        assert!(
            (bonus - superseded).abs() > 1.7,
            "must not reproduce the old 8-year linear result of {superseded:.4}"
        );
    }

    /// Age bonus is unchanged by Mission 70, and this curve was confirmed against the chain:
    /// two production neurons aged 991 and 1076 days have chain voting powers whose ratio
    /// matches this formula's ratio to within 0.002%.
    #[test]
    fn age_bonus_is_linear_to_1_25x_over_four_years() {
        assert_eq!(BonusMultiplier::from_age_seconds(0).value(), 1.0);
        assert_eq!(BonusMultiplier::from_age_seconds(4 * YEAR).value(), 1.25);

        let two_years = BonusMultiplier::from_age_seconds(2 * YEAR).value();
        assert!((two_years - 1.125).abs() < 1e-9, "linear: half the age, half the bonus");

        // Saturates rather than growing without bound.
        assert_eq!(BonusMultiplier::from_age_seconds(20 * YEAR).value(), 1.25);
    }

    /// The combined ceiling the protocol documents: 3.0 x 1.25 = 3.75.
    #[test]
    fn combined_maximum_multiplier_is_3_75x() {
        let combined = BonusMultiplier::from_dissolve_seconds(2 * YEAR)
            * BonusMultiplier::from_age_seconds(4 * YEAR);
        assert_eq!(combined.value(), 3.75);
    }
}
