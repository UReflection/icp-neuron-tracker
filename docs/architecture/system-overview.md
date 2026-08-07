# System Architecture Overview

**Document Type:** Architecture  
**Status:** Active  
**Last Updated:** 2025-10-26  
**Maintained By:** U Reflection Design & Build Inc.

---

## Purpose

This document provides a comprehensive view of the ICP Neuron Portfolio Tracker architecture. Understand system structure, design principles, layer responsibilities, and data flow.

Target audience: Developers, architects, contributors seeking to understand how the system works at a high level before diving into specific components.

---

## Guiding Principles

### Reality-Mirroring Architecture

U Reflection Design & Build Inc. builds systems that reflect universal principles. Our architecture mirrors how reality organizes itself:

**Boundaries and Layers**  
Just as physics has clear boundaries (quantum, classical, relativistic), our system has clear architectural layers. Each layer has specific responsibilities. Dependencies flow in one direction.

**Immutable Events**  
The past cannot change. Our system stores immutable state snapshots, never mutating history. Time flows forward. Events accumulate. State reconstructible from event log.

**Actor Independence**  
In quantum mechanics, particles operate independently until interaction. Each neuron in our system is an independent actor with its own state timeline. Portfolio is emergent property of collective actors.

**Stable Foundations**  
Universe built on stable physical laws. Our system built on stable abstractions: domain logic isolated from infrastructure, business rules independent of technology choices.

### Domain-Driven Design

**Ubiquitous Language**  
Code speaks business language. Neuron, Portfolio, Maturity, Staking. Not Record, Entity, Table. Domain experts and developers share vocabulary.

**Aggregate Boundaries**  
Strong consistency within aggregates. Eventual consistency across aggregates. Neuron is atomic unit. Portfolio aggregates multiple neurons.

**Repository Abstraction**  
Domain defines data needs via interfaces. Infrastructure implements details. Swap SQLite for PostgreSQL without touching business logic.

**Layered Architecture**  
Clear separation of concerns. Domain layer pure business logic. Application layer orchestrates use cases. Infrastructure layer handles technical details.

---

## System Context

### External Actors

**User**  
Neuron portfolio holder. Tracks multiple neurons. Plans retirement. Analyzes performance.

**Internet Computer**  
Blockchain network hosting ICP governance. Source of neuron state data. Queried via public APIs.

**NNS Governance Canister**  
Smart contract managing neuron governance. Provides neuron state, maturity, voting power data.

**File System**  
Local storage. Database persistence. Configuration files. PEM key storage.

### System Boundary

What's inside the system:
- Neuron state tracking logic
- Historical data persistence
- Portfolio analytics
- Retirement projections
- CLI interface

What's outside the system:
- Internet Computer blockchain
- NNS governance logic
- Hot key management (user responsibility via NNS dapp)
- ICP price data (no external price feeds)
- Tax calculations

---

## High-Level Architecture

### Layered Structure
```
┌─────────────────────────────────────────────────────────────┐
│  CLI  (src/main.rs)                                         │
│  NOT a separate layer module - see note below               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  - Command parsing (clap)                             │  │
│  │  - Dispatch to application services                   │  │
│  │  - Output via infrastructure/report_formatter.rs      │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                │
│                            ↓                                │
├─────────────────────────────────────────────────────────────┤
│  Application Layer                                          │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  PortfolioService                                     │  │
│  │  - Fetch portfolio from IC                            │  │
│  │  - Coordinate neuron queries                          │  │
│  │                                                       │  │
│  │  TrackingService                                      │  │
│  │  - Save daily snapshots                               │  │
│  │  - Calculate reward deltas                            │  │
│  │  - Generate income statistics                         │  │
│  │                                                       │  │
│  │  RetirementService                                    │  │
│  │  - Calculate projections                              │  │
│  │  - Analyze scenarios                                  │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                │
│                            ↓                                │
├─────────────────────────────────────────────────────────────┤
│  Domain Layer                                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Aggregates                                           │  │
│  │  - Neuron (business entity)                           │  │
│  │  - Portfolio (collection aggregate)                   │  │
│  │                                                       │  │
│  │  Value Objects                                        │  │
│  │  - IcpAmount (immutable, validated)                   │  │
│  │  - NeuronId (type-safe identifier)                    │  │
│  │  - BonusMultiplier (reward calculation)               │  │
│  │  - NeuronState (lifecycle enum)                       │  │
│  │                                                       │  │
│  │  Repository Interfaces                                │  │
│  │  - NeuronSnapshotRepository (contract)                │  │
│  │  - PortfolioSnapshotRepository (contract)             │  │
│  │  - DailyRewardRepository (contract)                   │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                │
│                            ↓                                │
├─────────────────────────────────────────────────────────────┤
│  Infrastructure Layer                                       │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  SqliteRepository                                     │  │
│  │  - Implements repository interfaces                   │  │
│  │  - Database connection management                     │  │
│  │  - SQL query execution                                │  │
│  │                                                       │  │
│  │  IcClient                                             │  │
│  │  - Internet Computer communication                    │  │
│  │  - Authentication via hot key                         │  │
│  │  - Candid interface handling                          │  │
│  │                                                       │  │
│  │  Config                                               │  │
│  │  - TOML file parsing                                  │  │
│  │  - Application settings                               │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘

External Systems:
    ↓
┌─────────────────────────────────────────────────────────────┐
│  SQLite Database (neuron_history.db)                        │
│  - neuron_snapshots                                         │
│  - portfolio_snapshots                                      │
│  - daily_rewards                                            │
└─────────────────────────────────────────────────────────────┘

    ↓
┌─────────────────────────────────────────────────────────────┐
│  Internet Computer (IC Mainnet)                             │
│  - NNS Governance Canister                                  │
│  - Neuron state queries                                     │
└─────────────────────────────────────────────────────────────┘
```

### Dependency Flow
```
CLI (main.rs) → Application → Domain ← Infrastructure
                                             ↓
                                      External Systems
```

**Key Rule:** Dependencies point inward. Infrastructure depends on domain. Domain depends on nothing.

**Why:** Domain logic remains stable regardless of technology changes. Swap SQLite for PostgreSQL. Domain unchanged. Swap CLI for Blazor UI. Domain unchanged.

---

## Layer Responsibilities

### CLI — not a separate layer

> **There are three layers in this codebase, not four.** `src/presentation/mod.rs` contains
> one comment — `// Presentation layer will go here` — and is **not declared as a module** in
> `main.rs`, so none of it compiles into the binary. It is a dead directory. All
> presentation lives in `src/main.rs` (command parsing and dispatch) and
> `src/infrastructure/report_formatter.rs` (all output construction). The
> application/domain/infrastructure separation described below is real and is honoured in
> the code — the fourth layer is not.

**Purpose:** User interaction and output formatting.

**Responsibilities:**
- Parse command-line arguments
- Format output (tables, charts, summaries)
- Handle user input validation
- Display error messages clearly

**Components:**
- `main.rs` - Entry point, command coordination
- CLI Commands:
  - `init` - Interactive first-run setup wizard
  - `track` - Track neurons and display portfolio (default)
  - `identity generate|verify|info` - Secp256k1 identity management
  - `project` - Retirement projection, with `--compare` and `--offline`
  - `import` / `export` - Historical CSV import and export
  - `report summary|history|rewards|neuron` - Analytics, each with
    `--format terminal|json|csv`
- Output construction lives in `infrastructure/report_formatter.rs`, never in the domain

**Technology:**
- Rust standard library, `clap` for argument parsing

**Dependencies:**
- Application layer services
- No direct domain or infrastructure access

**Testing:**
- Manual testing of output format
- Integration tests for command flows

### Application Layer

**Purpose:** Orchestrate business use cases.

**Responsibilities:**
- Coordinate domain objects
- Implement application workflows
- Manage transactions across repositories
- Transform data between layers

**Components:**
- `PortfolioService` - Fetch and manage portfolio
- `TrackingService` - Daily snapshot workflow
- `IdentityService` - Identity generation and verification
- `RetirementService` - Projection calculations

**Technology:**
- Pure Rust
- Async/await for IC communication

**Dependencies:**
- Domain layer (aggregates, value objects, repository interfaces)
- Infrastructure layer (via repository interfaces only)

**Testing:**
- Unit tests for service logic
- Integration tests with real repositories
- Mock repositories for isolated testing

### Domain Layer

**Purpose:** Express business logic and rules.

**Responsibilities:**
- Define business entities and value objects
- Enforce invariants and business rules
- Declare repository interfaces
- Contain zero infrastructure knowledge

**Components:**

Aggregates:
- `Neuron` - Single neuron entity with state, stake, maturity
- `Portfolio` - Collection of neurons with aggregations

Value Objects:
- `IcpAmount` - Type-safe ICP quantity (e8s internally)
- `NeuronId` - Type-safe neuron identifier
- `BonusMultiplier` - Age and dissolve bonus calculations
- `NeuronState` - Locked, Dissolving, Dissolved enum
- `MaturityModulation` - Reward rate tracking

Repository Interfaces:
- `NeuronSnapshotRepository` - Neuron persistence contract
- `PortfolioSnapshotRepository` - Portfolio persistence contract
- `DailyRewardRepository` - Reward calculation contract

**Technology:**
- Pure Rust
- No external dependencies except serde (for serialization)
- No async (pure synchronous logic)

**Dependencies:**
- None (fully self-contained)

**Testing:**
- Comprehensive unit tests
- Property-based testing for invariants
- 100% coverage goal

### Infrastructure Layer

**Purpose:** Implement technical concerns.

**Responsibilities:**
- Database access (SQLite)
- External API calls (Internet Computer)
- File system operations (config loading)
- Infrastructure-specific error handling

**Components:**

`SqliteRepository`:
- Implements all repository interfaces
- Manages database connections
- Executes SQL queries
- Handles migrations

`IcClient`:
- Wraps ic-agent library
- Manages authentication
- Converts Candid types to domain types

`IdentityClient`:
- Manages Secp256k1 identity generation
- Handles PEM file operations (SEC1 format)
- Validates identity files
- Derives principals from keys

`Config`:
- Loads TOML configuration
- Validates settings
- Provides application settings

**Technology:**
- rusqlite (SQLite bindings)
- refinery (migrations)
- ic-agent (Internet Computer client)
- toml (configuration parsing)

**Dependencies:**
- Domain layer (implements repository interfaces, uses domain types)
- External libraries (rusqlite, ic-agent)

**Testing:**
- Integration tests with real SQLite
- Mock IC responses for testing
- Migration tests

---

## Data Flow

### Query Flow: Fetch Neuron
```
User
  │
  ├─ cargo run
  │
  ↓
Presentation (main.rs)
  │
  ├─ Parse config
  ├─ Initialize services
  │
  ↓
Application (PortfolioService)
  │
  ├─ fetch_portfolio(neuron_ids)
  │
  ↓
Infrastructure (IcClient)
  │
  ├─ HTTP request to IC mainnet
  ├─ Call governance canister
  ├─ Decode Candid response
  │
  ↓
Internet Computer
  │
  ├─ Query neuron state
  ├─ Return maturity, stake, voting power
  │
  ↓
Infrastructure (IcClient)
  │
  ├─ Convert to domain types
  │
  ↓
Domain (Neuron::new)
  │
  ├─ Validate invariants
  ├─ Calculate bonuses
  ├─ Create Neuron aggregate
  │
  ↓
Application (PortfolioService)
  │
  ├─ Collect all neurons
  ├─ Create Portfolio aggregate
  │
  ↓
Presentation (main.rs)
  │
  ├─ Format output
  ├─ Display to user
  │
  ↓
User (sees results)
```

### Command Flow: Save Snapshot
```
User
  │
  ├─ cargo run (with snapshot_on_run: true)
  │
  ↓
Presentation (main.rs)
  │
  ├─ Check config
  ├─ Portfolio already fetched
  │
  ↓
Application (TrackingService)
  │
  ├─ save_daily_snapshot(portfolio)
  │
  ↓
Infrastructure (SqliteRepository)
  │
  ├─ Begin transaction
  ├─ For each neuron:
  │   ├─ INSERT INTO neuron_snapshots
  │   ├─ Get previous snapshot
  │   ├─ Calculate delta
  │   ├─ INSERT INTO daily_rewards
  ├─ INSERT INTO portfolio_snapshots
  ├─ Commit transaction
  │
  ↓
SQLite Database
  │
  ├─ Persist data
  ├─ Return success
  │
  ↓
Application (TrackingService)
  │
  ├─ Calculate income statistics
  ├─ Query average daily rewards
  │
  ↓
Infrastructure (SqliteRepository)
  │
  ├─ SELECT AVG(...) FROM daily_rewards
  │
  ↓
SQLite Database
  │
  ├─ Return aggregated data
  │
  ↓
Application (TrackingService)
  │
  ├─ Return DailyIncomeStats
  │
  ↓
Presentation (main.rs)
  │
  ├─ Format statistics
  ├─ Display daily income analysis
  │
  ↓
User (sees snapshot confirmation and income stats)
```

---

## Key Design Patterns

### Repository Pattern

**Purpose:** Abstract data access behind domain-defined interface.

**Implementation:**
- Domain defines trait: `NeuronSnapshotRepository`
- Infrastructure implements: `SqliteRepository`
- Application uses trait, not concrete type

**Benefits:**
- Domain independent of database choice
- Easy to swap persistence technology
- Mockable for testing

**Example:**
```rust
// Domain layer defines contract
pub trait NeuronSnapshotRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) 
        -> Result<(), Error>;
}

// Infrastructure implements
impl NeuronSnapshotRepository for SqliteRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) 
        -> Result<(), Error> {
        // SQLite-specific implementation
    }
}

// Application uses trait
pub struct TrackingService<R: NeuronSnapshotRepository> {
    repository: R,
}
```

### Value Object Pattern

**Purpose:** Encapsulate primitive values with validation and behavior.

**Implementation:**
- `IcpAmount` wraps `u64` (e8s)
- Validation in constructor
- Immutable after creation
- Type-safe operations

**Benefits:**
- Impossible to create invalid state
- Type safety prevents errors (can't add NeuronId to IcpAmount)
- Business logic embedded in type

**Example:**
```rust
#[derive(Debug, Clone, Copy)]
pub struct IcpAmount(u64); // e8s

impl IcpAmount {
    pub fn from_e8s(e8s: u64) -> Self {
        Self(e8s)
    }
    
    pub fn to_icp(&self) -> f64 {
        self.0 as f64 / 100_000_000.0
    }
}

impl std::ops::Add for IcpAmount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}
```

### Aggregate Pattern

**Purpose:** Define consistency boundaries and lifecycle management.

**Implementation:**
- `Neuron` is aggregate root
- All neuron data accessed through Neuron
- Invariants enforced in aggregate

**Benefits:**
- Clear consistency boundary
- Transactional integrity
- Encapsulation of business rules

**Example:**
```rust
pub struct Neuron {
    id: NeuronId,
    stake: IcpAmount,
    maturity: IcpAmount,
    // ... other fields
}

impl Neuron {
    // Factory ensures invariants
    pub fn new(
        id: NeuronId,
        stake: IcpAmount,
        // ... parameters
    ) -> Self {
        // Validate invariants
        assert!(stake.e8s() > 0, "Stake must be positive");
        
        Self { id, stake, /* ... */ }
    }
    
    // Behavior encapsulated
    pub fn total_value(&self) -> IcpAmount {
        self.stake + self.maturity + self.staked_maturity
    }
}
```

### Service Layer Pattern

**Purpose:** Coordinate domain objects for use cases.

**Implementation:**
- Application services orchestrate workflows
- Services stateless (repository passed as dependency)
- Each service represents bounded context

**Benefits:**
- Clear separation: domain logic vs application workflow
- Reusable across presentation layers
- Testable in isolation

**Example:**
```rust
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

impl<N, P, R> TrackingService<N, P, R> {
    pub fn save_daily_snapshot(&self, portfolio: &Portfolio) 
        -> Result<(), Error> {
        // Orchestrate workflow
        // 1. Save neurons
        // 2. Calculate deltas
        // 3. Save rewards
        // 4. Save portfolio summary
    }
}
```

---

## Technology Stack

### Core Technologies

**Language:** Rust 1.75+  
Rationale: Memory safety, performance, type safety, excellent ecosystem for blockchain and systems programming.

**Database:** SQLite 3.x with Refinery migrations  
Rationale: Embedded, zero-config, full SQL, reliable migrations. See [DR-001](../decisions/001-use-sqlite.md).

**IC Client:** ic-agent 0.37  
Rationale: Official DFINITY Rust library for Internet Computer interaction.

**Configuration:** TOML via toml crate  
Rationale: Human-readable, Rust ecosystem standard, type-safe parsing.

### Future Technologies

**Frontend:** Blazor WebAssembly  
Rationale: Leverage C# expertise, true client-side, cross-platform via Tauri.

⚠ **Aspirational, and undecided.** There is no ADR for this choice — the previously linked
`005-blazor-for-frontend.md` was never written — and nothing has been built. ADR-003, which
would house the multi-product structure such a frontend implies, is itself marked Accepted
but is not implemented. Treat this section as an idea of record, not a decision of record.

**Desktop Packaging:** Tauri  
Rationale: Lightweight, Rust-based, native performance, single codebase for all platforms.

---

## Non-Functional Characteristics

### Performance

**Query Performance:**
- Single neuron query: Under 2 seconds (network-bound)
- Database queries: Under 50ms for 10,000 records
- Snapshot save: Under 1 second for 10 neurons

**Scalability:**
- Supports 100+ neurons per portfolio
- 10+ years of daily snapshots
- Database size: ~1MB per neuron per year

### Reliability

**Data Integrity:**
- Atomic snapshot transactions
- Foreign key constraints enforced
- Immutable historical data (append-only)

**Error Handling:**
- Graceful degradation on network failures
- Clear error messages
- Safe failure (no data corruption)

**Recovery:**
- Database backup: Copy single file
- No data loss on crash (committed data persists)
- Idempotent operations (can retry safely)

### Security

**Data Protection:**
- Local-only storage (no cloud)
- PEM files excluded from version control
- Hot key pattern (read-only access)

**Authentication:**
- Hot key stored in PEM file
- User manages key via NNS dapp
- No passwords in application

**Privacy:**
- No telemetry or tracking
- No external API calls except IC
- User controls all data

### Maintainability

**Code Quality:**
- Strict separation of concerns
- Comprehensive testing
- Clear abstractions
- Self-documenting code (ubiquitous language)

**Evolution:**
- Schema migrations automated
- Repository pattern enables database swap
- Domain logic stable across changes

**Documentation:**
- Architecture decision records
- Feature specifications
- API reference documentation
- Code comments for complex logic

---

## Deployment Model

### Current (v0.1.1)

**Distribution:**
- Source code via GitHub
- Users build locally: `cargo build --release`
- Binary runs on user's machine

**Installation:**
1. Clone repository
2. Install Rust toolchain
3. Build project
4. Configure config.toml
5. Run: `cargo run`

**Dependencies:**
- Rust 1.75+
- Internet connection (for IC queries)
- File system access (for database)

### Future (v0.4.0)

**Distribution:**
- Pre-built binaries for Windows, macOS, Linux
- Tauri desktop application
- Downloadable installers

**Installation:**
1. Download installer for platform
2. Run installer
3. Launch application
4. Configure via UI

---

## Evolution Strategy

### Phase 1: Core Foundation (Complete)

Establish solid base for all future features.

- Domain-Driven Design architecture
- SQLite persistence with migrations
- Multi-neuron tracking
- Daily snapshots
- Portfolio analytics

### Phase 2: Analytics (Current)

Transform data into insights.

- Retirement income calculator
- Historical trend analysis
- Risk scenario modeling
- Export capabilities

### Phase 3: Expansion (Planned)

Broaden capabilities.

- SNS neuron support
- Extended CLI commands (report, export)
- Advanced query interface
- Notification system

### Phase 4: User Interface

Accessibility and visualization.

- Blazor WebAssembly UI
- Tauri desktop packaging
- Interactive charts
- Visual configuration

### Architectural Stability

**What Won't Change:**
- DDD layer structure
- Domain model independence
- Repository abstraction
- Value object pattern

**What May Change:**
- User interface (CLI → GUI)
- Database technology (SQLite → PostgreSQL if SaaS)
- Deployment model (local → cloud if needed)

**Change Management:**
- Architecture decisions documented
- Migration paths planned
- Backward compatibility maintained when feasible

---

## Related Documentation

**Architecture Details:**
- [Domain Model](domain-model.md) - Aggregates and value objects (⚠ its NNS protocol
  constants are superseded; see that file's header)
- [Database Schema](database-schema.md) - SQLite table structure
- [Layer Assignment](../diagrams/layer-assignment.md) - Identity command path

**Architecture Decisions:**
- [ADR-001: SQLite](../decisions/001-use-sqlite.md) - implemented
- [ADR-002: Repository Pattern](../decisions/002-repository-pattern.md) - implemented
- [ADR-003: Multi-Product Architecture](../decisions/003-multi-product-architecture.md) -
  marked Accepted but **not implemented**; the tree is a single crate

**There is no API reference.** `docs/api/` was listed by the previous index but never
written. The repository traits in `src/domain/repositories.rs` are the contract of record.

**Data flow** is documented in this file, under "Data Flow" above; there is no separate
`data-flow.md`. **Technology choices** are under "Technology Stack" above; there is no
separate `technology-stack.md`.

---

**U Reflection Design & Build Inc.**

Architecture mirrors reality.  
Clear layers. Clear boundaries. Clear purpose.

Last Updated: 2025-10-26
Version: 0.1.1