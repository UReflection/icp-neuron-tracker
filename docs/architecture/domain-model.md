# Domain Model

**Document Type:** Architecture  
**Status:** Active  
**Last Updated:** 2025-10-25  
**Maintained By:** U Reflection Design & Build Inc.

---

> ## This document does not define NNS protocol constants
>
> It describes **aggregates, entities, value objects, invariants and layering** — the shape of
> the domain, which is current and is honoured in the code.
>
> It deliberately holds **no** maximum dissolve delay, bonus ceiling or curve shape. Those
> live in one place: the `protocol` module in
> [`src/domain/value_objects.rs`](../../src/domain/value_objects.rs), which carries the values,
> their source in the NNS governance implementation, a verified-as-of date, and the limits of
> what has been checked against observation.
>
> **Why there is no copy here.** Until 2026-08-06 this file taught the pre-Mission-70 protocol
> — an 8-year maximum dissolve delay and a linear 2.0x ceiling — including a reproduction of
> `from_dissolve_seconds` that readers could reasonably have taken for the specification. The
> protocol changed in 2026; the code was corrected; this document was not, and could not be
> without becoming a second copy to keep in step. The passages were removed rather than
> rewritten. A pointer that is always right beats a duplicate that is right on the day it is
> written.
>
> These constants are governance-mutable. Anything that restates them anywhere other than the
> `protocol` module is a copy waiting to go stale.

---

## Purpose

This document defines the domain model using Domain-Driven Design principles. Understand aggregates, entities, value objects, and their relationships. This is the heart of the business logic.

Target audience: Developers implementing features, architects reviewing design, contributors understanding the system's conceptual core.

---

## Domain-Driven Design Principles

### Ubiquitous Language

Code and conversation use identical terminology. When domain experts say "neuron," code says `Neuron`. When they say "maturity rewards," code says `maturity`. No translation layer.

**Domain Terms in Use:**
- Neuron - ICP staking unit with governance rights
- Portfolio - Collection of neurons under management
- Maturity - Earned rewards from staking
- Staked Maturity - Auto-compounded rewards
- Voting Power - Governance weight calculation
- Dissolve Delay - Lock period for stake
- Age Bonus - Time-based reward multiplier
- Hot Key - Read-only authentication principal

### Aggregate Boundaries

Aggregates define transactional consistency boundaries. Changes within aggregate are atomic. Changes across aggregates eventually consistent.

**Aggregate Roots:**
- `Neuron` - Single neuron lifecycle and state
- `Portfolio` - Collection of neurons (read model)

**Invariants:**
- Neuron stake must be positive
- Maturity cannot be negative
- Age bonus calculated from age seconds
- Portfolio aggregates existing neurons only

### Value Objects

Immutable objects defined by their values, not identity. Two `IcpAmount(100_000_000)` objects are identical regardless of instance.

**Properties:**
- Immutable after creation
- Validated at construction
- Value equality (not reference)
- No identity
- Behavior embedded in type

---

## Aggregates

### Neuron Aggregate

**Purpose:** Represents single ICP neuron with all associated state and behavior.

**Aggregate Root:** `Neuron`

**Structure:**
```rust
pub struct Neuron {
    // Identity
    id: NeuronId,
    
    // Financial State
    stake: IcpAmount,
    maturity: IcpAmount,
    staked_maturity: IcpAmount,
    
    // Governance
    voting_power: u64,
    age_days: u64,
    dissolve_delay_days: u64,
    age_bonus: BonusMultiplier,
    dissolve_bonus: BonusMultiplier,
    state: NeuronState,
    
    // Settings
    auto_stake_enabled: bool,
    
    // Timestamps
    created_timestamp: u64,
    retrieved_timestamp: u64,
}
```

**Responsibilities:**

Calculate Total Value:
```rust
pub fn total_value(&self) -> IcpAmount {
    self.stake + self.maturity + self.staked_maturity
}
```

Calculate Total Rewards:
```rust
pub fn total_rewards(&self) -> IcpAmount {
    self.maturity + self.staked_maturity
}
```

Calculate Combined Multiplier:
```rust
pub fn combined_multiplier(&self) -> BonusMultiplier {
    self.age_bonus * self.dissolve_bonus
}
```

Format Created Date:
```rust
pub fn created_date_formatted(&self) -> String {
    // Convert timestamp to human-readable date
}
```

**Invariants:**
- Stake must be positive (validated in constructor)
- Maturity and staked maturity non-negative
- Age and dissolve bonuses lie within the ranges the protocol defines — see the `protocol`
  module in [`value_objects.rs`](../../src/domain/value_objects.rs)
- All ICP amounts in e8s internally

**Lifecycle:**
```
Created → Tracked → Updated (daily snapshots) → Historical
```

**Factory Method:**
```rust
impl Neuron {
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
        // Calculate derived values
        let age_bonus = BonusMultiplier::from_age_seconds(age_seconds);
        let dissolve_bonus = BonusMultiplier::from_dissolve_seconds(dissolve_delay_seconds);
        
        // Construct with validated state
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
            created_timestamp,
            retrieved_timestamp: current_timestamp(),
        }
    }
}
```

### Portfolio Aggregate

**Purpose:** Collection of neurons with aggregate calculations. Read model for portfolio-wide analytics.

**Aggregate Root:** `Portfolio`

**Structure:**
```rust
pub struct Portfolio {
    neurons: Vec<Neuron>,
}
```

**Responsibilities:**

Aggregate Financial Data:
```rust
pub fn total_stake(&self) -> IcpAmount;
pub fn total_maturity(&self) -> IcpAmount;
pub fn total_staked_maturity(&self) -> IcpAmount;
pub fn total_value(&self) -> IcpAmount;
pub fn total_rewards(&self) -> IcpAmount;
```

Aggregate Performance Metrics:
```rust
pub fn total_voting_power(&self) -> u64;
pub fn overall_return_percentage(&self) -> f64;
```

Access Neurons:
```rust
pub fn neurons(&self) -> &[Neuron];
pub fn neuron_count(&self) -> usize;
```

**Invariants:**
- Contains zero or more neurons
- All neurons must be valid Neuron aggregates
- Aggregations never modify underlying neurons

**Lifecycle:**
```
Constructed from neurons → Used for analytics → Discarded
```

**Factory Method:**
```rust
impl Portfolio {
    pub fn new(neurons: Vec<Neuron>) -> Self {
        Self { neurons }
    }
}
```

**Design Note:** Portfolio is immutable snapshot. Not persisted directly. Reconstructed from neuron snapshots when needed.

---

### RetirementProjection Aggregate

**Purpose:** Encapsulates retirement income projection with target income, current state, projected timeline, and risk scenarios.

**Aggregate Root:** `RetirementProjection`

**Structure:**
```rust
pub struct RetirementProjection {
    target_daily_income: TargetIncome,
    current_daily_income: f64,
    current_portfolio_value: IcpAmount,
    projected_timeline: ProjectionTimeline,
    scenarios: Vec<ProjectionScenario>,
    data_quality: DataQuality,
    assumptions: ProjectionAssumptions,
}
```

**Responsibilities:**

Calculate Portfolio Shortfall:
```rust
pub fn portfolio_shortfall(&self) -> f64;
```

Check Feasibility:
```rust
pub fn is_already_feasible(&self) -> bool;
pub fn is_reliable(&self) -> bool;
```

Access Projection Data:
```rust
pub fn target_daily_income(&self) -> &TargetIncome;
pub fn current_daily_income(&self) -> f64;
pub fn projected_timeline(&self) -> &ProjectionTimeline;
pub fn scenarios(&self) -> &[ProjectionScenario];
pub fn data_quality(&self) -> &DataQuality;
pub fn assumptions(&self) -> &ProjectionAssumptions;
```

**Invariants:**
- Target daily income must be positive (> 0)
- Current daily income must be non-negative (>= 0)
- At least one scenario must exist
- Data quality must reflect actual tracking history
- Projected timeline must be valid

**Lifecycle:**
```
Calculate from Portfolio + Target → Display results → Optionally compare alternatives
```

**Factory Method:**
```rust
impl RetirementProjection {
    pub fn new(
        target_daily_income: TargetIncome,
        current_daily_income: f64,
        current_portfolio_value: IcpAmount,
        projected_timeline: ProjectionTimeline,
        scenarios: Vec<ProjectionScenario>,
        data_quality: DataQuality,
        assumptions: ProjectionAssumptions,
    ) -> Result<Self, &'static str>
}
```

**Design Notes:**
- Used by RetirementService to encapsulate all projection logic
- Supports three risk scenarios (optimistic, realistic, pessimistic)
- Validates invariants at construction time
- Immutable after construction

---

## Value Objects

### IcpAmount

**Purpose:** Type-safe ICP quantity with automatic e8s conversion.

**Structure:**
```rust
pub struct IcpAmount(u64); // stored as e8s (10^-8 ICP)
```

**Responsibilities:**

Create from e8s:
```rust
pub fn from_e8s(e8s: u64) -> Self {
    Self(e8s)
}
```

Convert to ICP:
```rust
pub fn to_icp(&self) -> f64 {
    self.0 as f64 / 100_000_000.0
}
```

Arithmetic Operations:
```rust
impl std::ops::Add for IcpAmount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}
```

**Invariants:**
- Always non-negative (u64 enforces)
- Always in e8s internally
- Conversion to ICP for display only

**Why e8s?**  
ICP's smallest unit is 10^-8 ICP (like Bitcoin satoshis). Using u64 e8s avoids floating-point arithmetic errors.

### NeuronId

**Purpose:** Type-safe neuron identifier preventing ID mixups.

**Structure:**
```rust
pub struct NeuronId(u64);
```

**Responsibilities:**

Create ID:
```rust
pub fn new(id: u64) -> Self {
    Self(id)
}
```

Access Value:
```rust
pub fn value(&self) -> u64 {
    self.0
}
```

Display:
```rust
impl std::fmt::Display for NeuronId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

**Invariants:**
- Immutable after creation
- Unique per neuron (enforced by IC, not our system)

**Benefits:**
- Cannot accidentally use neuron ID as ICP amount
- Type system prevents ID confusion
- Self-documenting code

### BonusMultiplier

**Purpose:** Reward calculation multipliers with validation.

**Structure:**
```rust
pub struct BonusMultiplier(f64);
```

**Responsibilities:**

Two constructors derive a multiplier from a duration — `from_age_seconds` and
`from_dissolve_seconds` — and multiplication combines them.

**The curves and their ceilings are not reproduced here.** They are protocol parameters, they
are governance-mutable, and they have changed. See the `protocol` module in
[`value_objects.rs`](../../src/domain/value_objects.rs) for the current values, their source
and the date they were verified.

Multiply Bonuses:
```rust
impl std::ops::Mul for BonusMultiplier {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}
```

**Invariants:**
- Each bonus is at least 1.0x — a multiplier never reduces voting power
- Each saturates at its protocol ceiling rather than growing without bound
- The combined multiplier is the product of the two, not their sum

**Usage Example:**
```rust
let age_bonus = BonusMultiplier::from_age_seconds(age_seconds);
let dissolve_bonus = BonusMultiplier::from_dissolve_seconds(dissolve_delay_seconds);
let combined = age_bonus * dissolve_bonus;
```

---

### TargetIncome

**Purpose:** Type-safe target daily income with validation for retirement projections.

**Structure:**
```rust
pub struct TargetIncome(f64); // ICP per day
```

**Responsibilities:**

Create with Validation:
```rust
pub fn new(icp_per_day: f64) -> Result<Self, &'static str> {
    if icp_per_day <= 0.0 {
        return Err("Target income must be positive");
    }
    if icp_per_day > 10_000.0 {
        return Err("Target income must be reasonable (< 10,000 ICP/day)");
    }
    Ok(Self(icp_per_day))
}
```

Access Value:
```rust
pub fn icp_per_day(&self) -> f64 {
    self.0
}
```

**Invariants:**
- Must be positive (> 0.0)
- Must be reasonable (< 10,000.0 ICP/day)
- Prevents unrealistic retirement goals

**Usage Example:**
```rust
let target = TargetIncome::new(2.5)?; // 2.5 ICP/day
let daily_income = target.icp_per_day(); // 2.5
```

---

### WhatIfComparison

**Purpose:** Compares alternative retirement targets to base scenario, calculating timeline deltas.

**Structure:**
```rust
pub struct WhatIfComparison {
    pub target_income: TargetIncome,
    pub realistic_timeline: ProjectionTimeline,
    pub years_delta: f64,  // Positive = later, negative = earlier
}
```

**Responsibilities:**

Create Comparison:
```rust
pub fn new(
    target_income: TargetIncome,
    realistic_timeline: ProjectionTimeline,
    base_years: f64,
) -> Self {
    let years_delta = realistic_timeline.years_until_retirement - base_years;
    Self {
        target_income,
        realistic_timeline,
        years_delta,
    }
}
```

Check Timeline Impact:
```rust
pub fn is_earlier(&self) -> bool {
    self.years_delta < 0.0
}

pub fn is_later(&self) -> bool {
    self.years_delta > 0.0
}
```

**Invariants:**
- Target income must be valid TargetIncome
- Timeline must be valid ProjectionTimeline
- Delta accurately reflects difference from base
- Negative delta = earlier retirement
- Positive delta = later retirement

**Usage Example:**
```rust
let base_target = TargetIncome::new(2.5)?;
let alt_target = TargetIncome::new(1.5)?;

let base_projection = service.calculate_projection(&portfolio, base_target)?;
let alt_projection = service.calculate_projection(&portfolio, alt_target)?;

let comparison = WhatIfComparison::new(
    alt_target,
    alt_projection.projected_timeline().clone(),
    base_projection.projected_timeline().years_until_retirement,
);

if comparison.is_earlier() {
    println!("Retire {:.1} years EARLIER", comparison.years_delta.abs());
}
```

---

### ProjectionTimeline

**Purpose:** Encapsulates retirement timeline with required portfolio size.

**Structure:**
```rust
pub struct ProjectionTimeline {
    pub years_until_retirement: f64,
    pub retirement_date: NaiveDate,
    pub required_portfolio_size: IcpAmount,
}
```

**Invariants:**
- `years_until_retirement` is non-negative; zero means the target is already met
- `retirement_date` is derived from that figure, not stored independently
- `required_portfolio_size` is the portfolio value at which the target income is reached

### NeuronState

**Purpose:** Neuron lifecycle state.

**Structure:**
```rust
pub enum NeuronState {
    Locked,      // Cannot withdraw, earning full rewards
    Dissolving,  // Timer counting down, earning rewards
    Dissolved,   // Can withdraw, not earning rewards
}
```

**Conversion from IC State:**
```rust
impl From<i32> for NeuronState {
    fn from(state: i32) -> Self {
        match state {
            1 => NeuronState::Locked,
            2 => NeuronState::Dissolving,
            3 => NeuronState::Dissolved,
            _ => NeuronState::Locked, // default to locked
        }
    }
}
```

**State Transitions:**
```
Locked → Dissolving (user initiates)
       → Locked (stays if not dissolving)

Dissolving → Dissolved (timer reaches zero)
           → Locked (user stops dissolving)

Dissolved → Locked (user restakes)
```

**Invariants:**
- Cannot skip states
- Timer-based transitions for Dissolving
- Only Locked and Dissolving earn rewards

---

## Repository Interfaces

Repositories defined in domain, implemented in infrastructure. Domain declares what it needs, not how it's done.

### NeuronSnapshotRepository

**Purpose:** Persist and retrieve neuron state snapshots.

**Interface:**
```rust
pub trait NeuronSnapshotRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) 
        -> Result<(), Box<dyn std::error::Error>>;
    
    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) 
        -> Result<Option<Neuron>, Box<dyn std::error::Error>>;
    
    fn get_latest_snapshot(&self, neuron_id: NeuronId) 
        -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
    
    fn get_previous_snapshot(&self, neuron_id: NeuronId, before_date: NaiveDate) 
        -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
    
    fn get_snapshots_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) 
        -> Result<Vec<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
    
    fn get_all_snapshots_for_date(&self, date: NaiveDate) 
        -> Result<Vec<Neuron>, Box<dyn std::error::Error>>;
}
```

**Responsibilities:**
- Save immutable neuron snapshots
- Retrieve historical state
- Support temporal queries

**Design Note:** One snapshot per neuron per day. Uniqueness constraint: (neuron_id, date).

### PortfolioSnapshotRepository

**Purpose:** Persist and retrieve portfolio aggregate snapshots.

**Interface:**
```rust
pub trait PortfolioSnapshotRepository {
    fn save_snapshot(&self, portfolio: &Portfolio, date: NaiveDate) 
        -> Result<(), Box<dyn std::error::Error>>;
    
    fn get_snapshot(&self, date: NaiveDate) 
        -> Result<Option<Portfolio>, Box<dyn std::error::Error>>;
    
    fn get_latest_snapshot(&self) 
        -> Result<Option<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>>;
    
    fn get_snapshots_range(&self, start: NaiveDate, end: NaiveDate) 
        -> Result<Vec<(Portfolio, NaiveDate)>, Box<dyn std::error::Error>>;
}
```

**Responsibilities:**
- Save aggregate portfolio state
- Retrieve historical portfolios
- Support trend analysis

**Design Note:** Portfolio reconstructed from neuron snapshots. Aggregate data duplicated for query performance.

### DailyRewardRepository

**Purpose:** Persist and retrieve daily reward calculations.

**Interface:**
```rust
pub trait DailyRewardRepository {
    fn save_reward(
        &self, 
        neuron_id: NeuronId, 
        date: NaiveDate, 
        maturity_delta: i64, 
        staked_maturity_delta: i64, 
        days_elapsed: i64
    ) -> Result<(), Box<dyn std::error::Error>>;
    
    fn get_reward(&self, neuron_id: NeuronId, date: NaiveDate) 
        -> Result<Option<DailyReward>, Box<dyn std::error::Error>>;
    
    fn get_rewards_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) 
        -> Result<Vec<DailyReward>, Box<dyn std::error::Error>>;
    
    fn get_average_daily_reward(&self, neuron_id: NeuronId, days: i64) 
        -> Result<Option<f64>, Box<dyn std::error::Error>>;
}
```

**Supporting Type:**
```rust
pub struct DailyReward {
    pub neuron_id: NeuronId,
    pub date: NaiveDate,
    pub maturity_delta_e8s: i64,
    pub staked_maturity_delta_e8s: i64,
    pub total_reward_e8s: i64,
    pub days_elapsed: i64,
}

impl DailyReward {
    pub fn daily_rate_icp(&self) -> f64 {
        if self.days_elapsed == 0 {
            return 0.0;
        }
        (self.total_reward_e8s as f64 / self.days_elapsed as f64) / 100_000_000.0
    }
}
```

**Responsibilities:**
- Calculate reward deltas between snapshots
- Persist reward history
- Support income analytics

**Design Note:** Deltas can be negative (neuron fees). Days elapsed tracks gaps in data collection.

---

## Domain Services

Services coordinate domain objects but contain no state themselves.

### Planned: RetirementService

**Purpose:** Calculate retirement projections using domain logic.

**Responsibilities:**
- Calculate required portfolio size for target income
- Project future value with compounding
- Generate risk scenarios
- Provide confidence intervals

**Dependencies:**
- DailyRewardRepository (for historical rates)
- Domain calculations (no infrastructure knowledge)

**Design Note:** Service in application layer, but calculations pure domain logic. May extract calculation to domain service if complexity grows.

---

## Domain Events

Currently not implemented, but architecture supports event sourcing pattern.

### Potential Events

**NeuronSnapshotCaptured:**
- Neuron ID
- Snapshot date
- Financial state at capture time

**RewardCalculated:**
- Neuron ID
- Calculation date
- Reward delta

**PortfolioAnalyzed:**
- Analysis date
- Aggregate metrics

**Future Consideration:** Full event sourcing with event store. Current implementation uses snapshot model (simpler, sufficient for requirements).

---

## Relationships

### Neuron → Portfolio
```
Portfolio "1" contains "0..*" Neuron
```

Portfolio is aggregate of neurons. Relationship: composition. Portfolio lifecycle independent of neuron lifecycle.

### Neuron → DailyReward
```
Neuron "1" generates "0..*" DailyReward
```

Calculated relationship. Reward derived from consecutive neuron snapshots.

### Repository → Aggregate
```
Repository manages persistence of Aggregate
```

Repositories handle CRUD for aggregates. Aggregates never know about repositories.

---

## Ubiquitous Language Dictionary

**Aggregate:** Cluster of domain objects treated as single unit for data changes.

**Age Bonus:** Reward multiplier based on how long a neuron has been locked without
dissolving. Ceiling and curve: see the `protocol` module in
[`value_objects.rs`](../../src/domain/value_objects.rs).

**Auto-Stake:** Setting where maturity automatically compounds into staked maturity.

**Dissolve Bonus:** Reward multiplier based on the lock period. Ceiling and curve: see the
`protocol` module in [`value_objects.rs`](../../src/domain/value_objects.rs).

**Dissolve Delay:** Time remaining before stake can be withdrawn.

**e8s:** Smallest unit of ICP. 100,000,000 e8s = 1 ICP.

**Hot Key:** Read-only principal for querying neuron without controller authority.

**Maturity:** Unstaked rewards. Can spawn new neurons or merge into stake.

**Neuron:** ICP staking unit. Holds stake, accumulates maturity, participates in governance.

**Portfolio:** Collection of neurons under management.

**Snapshot:** Immutable state capture at specific point in time.

**Staked Maturity:** Auto-compounded rewards. Increases voting power.

**Stake:** Locked ICP in neuron. Cannot withdraw while locked.

**Value Object:** Immutable object defined by value, not identity.

**Voting Power:** Governance weight. Function of stake, age, dissolve delay.

---

## Design Evolution

### Current Model (v0.1.1)

Snapshot-based persistence. Aggregates reconstructed from database. Simple, effective.

### Future Considerations (v0.3.0+)

**Event Sourcing:**
- Store domain events instead of snapshots
- Reconstruct state by replaying events
- Complete audit trail
- Enables temporal queries

**Command/Query Separation (CQRS):**
- Separate write model (commands) from read model (queries)
- Optimize each independently
- Better scalability

**Richer Domain Model:**
- Add RetirementProjection aggregate
- Add SnsNeuron subtype
- Add GovernanceProposal value object

**When to Evolve:**  
Complexity threshold. Current model handles requirements well. Evolve when pain exceeds benefit of simplicity.

---

## Related Documentation

**Architecture:**
- [System Overview](system-overview.md) - Overall architecture, including data flow
- [Database Schema](database-schema.md) - Persistence model

**Decisions:**
- [ADR-002: Repository Pattern](../decisions/002-repository-pattern.md) - Data access

**Code:** `src/domain/` is the implementation of everything described here;
`src/domain/repositories.rs` holds the repository contracts. **For current NNS protocol
constants, see EXTERNAL 1 in [CLAUDE.md](../../CLAUDE.md)** — not this document.

---

**U Reflection Design & Build Inc.**

Model reality. Code clarity.  
Domain drives design.

Last Updated: 2025-10-25  
Version: 0.1.1