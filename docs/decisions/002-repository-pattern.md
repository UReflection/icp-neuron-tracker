# ADR-002: Repository Pattern for Data Access

**Status:** Accepted  
**Date:** 2025-10-26  
**Deciders:** U Reflection Design & Build Inc. Engineering Team  
**Related:** [ADR-001: Use SQLite](001-use-sqlite.md)

---

## Context

The application needs to persist neuron snapshots, portfolio aggregates, and daily reward calculations. The data access strategy must support:

1. **Domain-Driven Design principles** - Domain layer should not depend on infrastructure
2. **Testability** - Ability to test business logic without database
3. **Flexibility** - Ability to swap storage implementations (SQLite, IC Stable Memory, etc.)
4. **Clean Architecture** - Clear separation between business logic and data persistence
5. **Future multi-product support** - Same domain logic across CLI, Desktop, and SaaS products

The challenge is how to structure data access to achieve these goals while maintaining simplicity.

---

## Decision

We will use the **Repository Pattern** with the following structure:

### 1. Repository Traits in Domain Layer

Define repository interfaces as traits in the domain layer:
```rust
// In domain layer
pub trait NeuronSnapshotRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) 
        -> Result<(), Box<dyn std::error::Error>>;
    
    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) 
        -> Result<Option<Neuron>, Box<dyn std::error::Error>>;
    
    fn get_latest_snapshot(&self, neuron_id: NeuronId) 
        -> Result<Option<(Neuron, NaiveDate)>, Box<dyn std::error::Error>>;
}
```

### 2. Concrete Implementations in Infrastructure Layer

Implement storage-specific repositories in infrastructure:
```rust
// In infrastructure layer
pub struct SqliteNeuronSnapshotRepository {
    connection: Arc<Mutex<Connection>>,
}

impl NeuronSnapshotRepository for SqliteNeuronSnapshotRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) 
        -> Result<(), Box<dyn std::error::Error>> {
        // SQLite-specific implementation
    }
    // ... other methods
}
```

### 3. Dependency Injection in Application Layer

Application services depend on repository traits, not concrete implementations:
```rust
// In application layer
pub struct TrackingService<R: NeuronSnapshotRepository> {
    repository: R,
    ic_client: IcClient,
}

impl<R: NeuronSnapshotRepository> TrackingService<R> {
    pub fn new(repository: R, ic_client: IcClient) -> Self {
        Self { repository, ic_client }
    }
}
```

### 4. Aggregates are Repository Boundaries

Each aggregate root has its own repository:

- `NeuronSnapshotRepository` - For Neuron aggregate
- `PortfolioSnapshotRepository` - For Portfolio aggregate
- `DailyRewardRepository` - For DailyReward calculations

Repositories only expose operations that make sense for the aggregate.

---

## Alternatives Considered

### Alternative 1: Direct Database Access in Domain

**Approach:** Domain objects directly call database
```rust
impl Neuron {
    pub fn save_to_db(&self, conn: &Connection) -> Result<()> {
        // SQL here
    }
}
```

**Rejected because:**
- Violates single responsibility principle
- Domain layer depends on infrastructure (rusqlite)
- Cannot test domain logic without database
- Cannot swap storage implementations
- Tight coupling prevents multi-product architecture

### Alternative 2: Active Record Pattern

**Approach:** Domain objects inherit from base class with CRUD operations
```rust
pub struct Neuron {
    // Inherits save(), update(), delete() from ActiveRecord
}
```

**Rejected because:**
- Not idiomatic in Rust (no inheritance)
- Still couples domain to persistence
- Harder to test
- Less flexible for different storage backends

### Alternative 3: Data Mapper Pattern

**Approach:** Separate mapper classes convert between domain objects and database rows
```rust
pub struct NeuronMapper {
    pub fn to_domain(row: &Row) -> Neuron { ... }
    pub fn to_row(neuron: &Neuron) -> Row { ... }
}
```

**Rejected because:**
- More complex than needed
- Mappers still need coordination logic
- Repository pattern includes mapping naturally
- Additional abstraction layer without clear benefit

### Alternative 4: Generic DAO (Data Access Object)

**Approach:** Single generic repository for all entities
```rust
pub trait GenericRepository<T> {
    fn save(&self, entity: T) -> Result<()>;
    fn find_by_id(&self, id: u64) -> Result<Option<T>>;
    fn delete(&self, id: u64) -> Result<()>;
}
```

**Rejected because:**
- Too generic, doesn't capture domain-specific queries
- Forces all aggregates to have same operations
- Temporal queries (date ranges) don't fit generic pattern
- Loses domain-specific language

---

## Consequences

### Positive

**Separation of Concerns:**
- Domain layer has zero infrastructure dependencies
- Business logic can be tested without database
- Clear boundaries between layers

**Flexibility:**
- Can swap SQLite for IC Stable Memory without changing domain
- Can implement in-memory repository for tests
- Supports future multi-product architecture (CLI, Desktop, SaaS)

**Testability:**
- Mock repositories for unit tests
- Test domain logic in isolation
- Integration tests with real repository

**Domain-Driven Design:**
- Repository interface uses domain language
- Operations match aggregate boundaries
- Enforces single aggregate per transaction

**Future-Proof:**
- Same domain core compiles to native (CLI, Desktop) and WASM (IC Canister)
- Different products can have different repository implementations
- Stable API for domain layer

### Negative

**Additional Abstraction:**
- More files and interfaces to maintain
- Learning curve for contributors
- Boilerplate for simple CRUD operations

**Mitigation:** Keep repository interfaces focused. Only add methods as needed.

**Performance Overhead:**
- Trait dispatch has minimal runtime cost
- Abstraction prevents some optimizations

**Mitigation:** In practice, I/O dominates performance. Trait overhead is negligible.

**Dependency Injection Complexity:**
- Services must be constructed with repository dependencies
- More complex initialization code

**Mitigation:** Centralize construction in application setup. Worth it for testability.

---

## Implementation Details

### Repository Trait Location

Repository traits are defined in domain layer, even though they're only implemented in infrastructure:
```
src/
├── domain/
│   ├── mod.rs
│   ├── neuron.rs
│   ├── portfolio.rs
│   └── repositories.rs          # Trait definitions here
└── infrastructure/
    ├── mod.rs
    └── sqlite_repository.rs      # Trait implementations here
```

**Rationale:** Domain defines what it needs. Infrastructure provides it. Dependency points inward (Dependency Inversion Principle).

### Transaction Boundaries

Repositories handle single aggregate transactions:
```rust
// Good: Single aggregate
repository.save_snapshot(&neuron, date)?;

// Bad: Multiple aggregates (use application service to coordinate)
// repository.save_neuron_and_portfolio(&neuron, &portfolio)?;  // Don't do this
```

**Rationale:** Each aggregate is consistency boundary. Cross-aggregate operations go through application layer.

### Query Methods

Repositories include domain-specific query methods:
```rust
pub trait NeuronSnapshotRepository {
    // CRUD
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) -> Result<()>;
    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) -> Result<Option<Neuron>>;
    
    // Domain-specific queries
    fn get_latest_snapshot(&self, neuron_id: NeuronId) -> Result<Option<(Neuron, NaiveDate)>>;
    fn get_previous_snapshot(&self, neuron_id: NeuronId, before_date: NaiveDate) 
        -> Result<Option<(Neuron, NaiveDate)>>;
    fn get_snapshots_range(&self, neuron_id: NeuronId, start: NaiveDate, end: NaiveDate) 
        -> Result<Vec<(Neuron, NaiveDate)>>;
}
```

**Rationale:** Repository API should match domain needs, not generic CRUD.

### Error Handling

Repositories return domain-agnostic errors:
```rust
// Generic error type
Result<T, Box<dyn std::error::Error>>

// Not database-specific errors
// Result<T, rusqlite::Error>  // Don't expose this
```

**Rationale:** Domain shouldn't know about SQLite-specific errors. Infrastructure translates.

---

## Examples

### Testing with Mock Repository
```rust
struct MockNeuronRepository {
    snapshots: HashMap<(NeuronId, NaiveDate), Neuron>,
}

impl NeuronSnapshotRepository for MockNeuronRepository {
    fn save_snapshot(&self, neuron: &Neuron, date: NaiveDate) -> Result<()> {
        self.snapshots.insert((neuron.id, date), neuron.clone());
        Ok(())
    }
    
    fn get_snapshot(&self, neuron_id: NeuronId, date: NaiveDate) 
        -> Result<Option<Neuron>> {
        Ok(self.snapshots.get(&(neuron_id, date)).cloned())
    }
}

#[test]
fn test_tracking_service() {
    let mock_repo = MockNeuronRepository::new();
    let service = TrackingService::new(mock_repo, mock_ic_client);
    
    // Test without database
    service.track_neuron(123)?;
}
```

### Swapping Implementations
```rust
// CLI and Desktop: Use SQLite
let repo = SqliteNeuronSnapshotRepository::new(connection);
let service = TrackingService::new(repo, ic_client);

// SaaS Canister: Use Stable Memory
let repo = StableMemoryNeuronRepository::new();
let service = TrackingService::new(repo, ic_client);

// Same service, different storage!
```

---

## Related Patterns

**Domain-Driven Design:**
- Aggregates define consistency boundaries
- Repositories per aggregate root
- Ubiquitous language in method names

**Hexagonal Architecture (Ports and Adapters):**
- Repository traits are "ports"
- Concrete implementations are "adapters"
- Domain at center, infrastructure at edges

**Dependency Inversion Principle:**
- High-level (domain) doesn't depend on low-level (infrastructure)
- Both depend on abstractions (repository traits)
- Dependencies point inward

---

## Future Considerations

### Query Objects

If queries become complex, consider query objects:
```rust
pub struct NeuronSnapshotQuery {
    pub neuron_id: Option<NeuronId>,
    pub date_range: Option<(NaiveDate, NaiveDate)>,
    pub order_by: OrderBy,
}

impl NeuronSnapshotRepository {
    fn query(&self, query: NeuronSnapshotQuery) -> Result<Vec<Neuron>>;
}
```

**When:** More than 5 query methods in repository.

### CQRS (Command Query Responsibility Segregation)

Separate read and write models if needed:
```rust
pub trait NeuronCommandRepository {
    fn save_snapshot(&self, neuron: &Neuron) -> Result<()>;
}

pub trait NeuronQueryRepository {
    fn get_snapshot(&self, neuron_id: NeuronId) -> Result<Option<Neuron>>;
    fn search(&self, criteria: SearchCriteria) -> Result<Vec<Neuron>>;
}
```

**When:** Read and write performance requirements diverge.

### Event Sourcing

Store events instead of state:
```rust
pub trait EventStore {
    fn append(&self, events: Vec<DomainEvent>) -> Result<()>;
    fn get_events(&self, aggregate_id: AggregateId) -> Result<Vec<DomainEvent>>;
}
```

**When:** Need complete audit trail or temporal queries become complex.

---

## Validation Criteria

Repository pattern implementation is successful when:

1. ✅ Domain layer has zero dependencies on rusqlite or any infrastructure
2. ✅ Unit tests run without database (using mock repositories)
3. ✅ Integration tests use real SQLite repository
4. ✅ Can swap SQLite for different storage without changing domain
5. ✅ Application services depend on repository traits, not concrete types
6. ✅ Repository methods use domain language (not SQL terminology)

---

## Related Documentation

**Architecture:**
- [System Overview](../architecture/system-overview.md)
- [Domain Model](../architecture/domain-model.md)

**Decisions:**
- [ADR-001: Use SQLite](001-use-sqlite.md) - Storage technology choice
- [ADR-003: Multi-Product Architecture](003-multi-product-architecture.md) - Why this pattern enables multiple products

**Code:**
- Domain repository traits: `src/domain/repositories.rs`
- SQLite implementation: `src/infrastructure/sqlite_repository.rs`
- Application services: `src/application/`

---

## References

**Books:**
- "Domain-Driven Design" by Eric Evans - Repository pattern chapter
- "Clean Architecture" by Robert C. Martin - Dependency rule
- "Implementing Domain-Driven Design" by Vaughn Vernon - Repository implementation

**Articles:**
- Martin Fowler: "Repository Pattern" - https://martinfowler.com/eaaCatalog/repository.html
- Microsoft: "Repository and Unit of Work Patterns"

---

**U Reflection Design & Build Inc.**

Abstract infrastructure.  
Preserve domain purity.  
Enable flexibility.

Date: 2025-10-26  
Status: Accepted  
Version: 0.1.1