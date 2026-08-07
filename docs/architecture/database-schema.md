# Database Schema

**Document Type:** Architecture  
**Status:** Active  
**Last Updated:** 2025-10-25  
**Maintained By:** U Reflection Design & Build Inc.

---

## Purpose

Complete SQLite database schema documentation. Understand table structures, relationships, indexes, and design rationale.

Target audience: Developers querying the database, DBAs (if applicable), contributors adding features requiring new tables.

---

## Database Technology

**System:** SQLite 3.x  
**Migration Framework:** Refinery  
**Location:** `neuron_history.db` (configurable)

**Why SQLite:** See [DR-001: Use SQLite](../decisions/001-use-sqlite.md)

**Key Characteristics:**
- Embedded database (no server)
- Single file storage
- ACID transactions
- Full SQL support
- Zero configuration

---

## Schema Overview

Three core tables capturing neuron tracking data:
```
neuron_snapshots (daily state)
    ↓
daily_rewards (calculated deltas)

portfolio_snapshots (aggregates)
```

**Design Philosophy:**
- Event sourcing influence (immutable snapshots)
- Denormalization for query performance
- Temporal data (every row has date)
- Idempotent writes (UNIQUE constraints)

---

## Tables

### neuron_snapshots

**Purpose:** Daily immutable state snapshot for each neuron.

**Schema:**
```sql
CREATE TABLE neuron_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    neuron_id TEXT NOT NULL,
    snapshot_date DATE NOT NULL,
    stake_e8s INTEGER NOT NULL,
    maturity_e8s INTEGER NOT NULL,
    staked_maturity_e8s INTEGER NOT NULL,
    voting_power INTEGER NOT NULL,
    age_days INTEGER NOT NULL,
    dissolve_delay_days INTEGER NOT NULL,
    age_bonus_multiplier REAL NOT NULL,
    dissolve_bonus_multiplier REAL NOT NULL,
    state TEXT NOT NULL,
    auto_stake_enabled BOOLEAN NOT NULL,
    created_timestamp INTEGER NOT NULL,
    retrieved_timestamp INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(neuron_id, snapshot_date)
);

CREATE INDEX idx_neuron_date ON neuron_snapshots(neuron_id, snapshot_date);
CREATE INDEX idx_snapshot_date ON neuron_snapshots(snapshot_date);
```

**Columns:**

| Column | Type | Description | Constraints |
|--------|------|-------------|-------------|
| id | INTEGER | Primary key | AUTO INCREMENT |
| neuron_id | TEXT | IC neuron identifier | NOT NULL |
| snapshot_date | DATE | Date of capture (YYYY-MM-DD) | NOT NULL |
| stake_e8s | INTEGER | Locked ICP in e8s | NOT NULL, >= 0 |
| maturity_e8s | INTEGER | Unstaked rewards in e8s | NOT NULL, >= 0 |
| staked_maturity_e8s | INTEGER | Compounded rewards in e8s | NOT NULL, >= 0 |
| voting_power | INTEGER | Governance weight | NOT NULL, >= 0 |
| age_days | INTEGER | Neuron age in days | NOT NULL, >= 0 |
| dissolve_delay_days | INTEGER | Lock period in days | NOT NULL, >= 0 |
| age_bonus_multiplier | REAL | Age reward bonus (1.0-1.25) | NOT NULL |
| dissolve_bonus_multiplier | REAL | Dissolve reward bonus (1.0-2.0) | NOT NULL |
| state | TEXT | Locked, Dissolving, Dissolved | NOT NULL |
| auto_stake_enabled | BOOLEAN | Maturity auto-compounds | NOT NULL |
| created_timestamp | INTEGER | Unix timestamp neuron created | NOT NULL |
| retrieved_timestamp | INTEGER | Unix timestamp the data was observed | NOT NULL |
| created_at | TIMESTAMP | Row insert timestamp | DEFAULT NOW |

**`retrieved_timestamp` is the provenance marker.** It records when a snapshot was actually
observed, which is not the same as `snapshot_date` (the day it describes) or `created_at` (when
the row reached the table). It is the only durable way to tell automated collection from a bulk
CSV import: a tracker run stamps each neuron as it is fetched, seconds apart, while an import
stamps every row of the file identically.

Prefer it over `created_at` for any provenance question. `created_at` is SQLite's
`DEFAULT CURRENT_TIMESTAMP` and is rewritten by any rebuild, restore or re-import; a same-key
replace also overwrites `retrieved_timestamp`, but a duplicate-skip preserves it.

**Indexes:**

`idx_neuron_date (neuron_id, snapshot_date)`:
- Purpose: Fast temporal queries for specific neuron
- Query: "Get neuron X snapshots between date A and B"
- Composite index for range scans

`idx_snapshot_date (snapshot_date)`:
- Purpose: Fast queries across all neurons for specific date
- Query: "Get all neuron snapshots for today"
- Portfolio reconstruction

**Constraints:**

`UNIQUE(neuron_id, snapshot_date)`:
- One snapshot per neuron per day
- Prevents more than one row per neuron per date

**⚠ Writes are a destructive upsert, not an append.** Every write to this table uses
`INSERT OR REPLACE`, so a second write for an existing `(neuron_id, snapshot_date)` **replaces
the earlier row entirely** — last write wins, and the previous observation is gone. This is
not idempotent when the values differ.

Two consequences worth knowing before relying on the history:

1. **`track` overwrites.** Running it twice in one day leaves only the later reading.
2. **A single CSV containing two rows for the same neuron and date stores only the last**, and
   the import summary counts both — so "Imported 2 snapshots" can leave one row.

The import path behaves differently *across* files: rows whose `(neuron_id, snapshot_date)`
already exists are detected beforehand and **skipped**, leaving the stored row untouched. So
re-importing an export is safe; a malformed single file is not.

If a value must be preserved, export it. An export is a genuine point-in-time copy.

**Design Rationale:**

Denormalization: Age bonus and dissolve bonus stored calculated. Alternative: Calculate on query. Trade-off: Storage for speed. Query performance critical for analytics.

TEXT for neuron_id: IC neuron IDs are u64 (up to 20 digits). SQLite INTEGER is signed 64-bit. TEXT avoids overflow risk and maintains exact representation.

e8s suffix: Makes unit explicit. stake_e8s vs stake_icp. Avoids confusion.

**Sample Data:**
```sql
INSERT INTO neuron_snapshots (
    neuron_id, snapshot_date, stake_e8s, maturity_e8s, staked_maturity_e8s,
    voting_power, age_days, dissolve_delay_days, age_bonus_multiplier,
    dissolve_bonus_multiplier, state, auto_stake_enabled,
    created_timestamp, retrieved_timestamp
) VALUES (
    '10000000000000000001',
    '2025-10-25',
    10000000000,  -- 100.00000000 ICP
    0,
    5000000000,   -- 50.00000000 ICP
    30000000000,
    785,
    2922,
    1.13,
    2.00,
    'Locked',
    1,
    1638576000,
    1729900800
);
```

---

### portfolio_snapshots

**Purpose:** Daily aggregate portfolio metrics for fast queries.

**Schema:**
```sql
CREATE TABLE portfolio_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_date DATE NOT NULL UNIQUE,
    total_neurons INTEGER NOT NULL,
    total_stake_e8s INTEGER NOT NULL,
    total_maturity_e8s INTEGER NOT NULL,
    total_staked_maturity_e8s INTEGER NOT NULL,
    total_voting_power INTEGER NOT NULL,
    overall_return_percentage REAL NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_portfolio_date ON portfolio_snapshots(snapshot_date);
```

**Columns:**

| Column | Type | Description | Constraints |
|--------|------|-------------|-------------|
| id | INTEGER | Primary key | AUTO INCREMENT |
| snapshot_date | DATE | Date of aggregate | NOT NULL, UNIQUE |
| total_neurons | INTEGER | Count of neurons | NOT NULL, >= 0 |
| total_stake_e8s | INTEGER | Sum of all stake | NOT NULL, >= 0 |
| total_maturity_e8s | INTEGER | Sum of all maturity | NOT NULL, >= 0 |
| total_staked_maturity_e8s | INTEGER | Sum of all staked maturity | NOT NULL, >= 0 |
| total_voting_power | INTEGER | Sum of voting power | NOT NULL, >= 0 |
| overall_return_percentage | REAL | Total rewards / total stake | NOT NULL |
| created_at | TIMESTAMP | Row insert timestamp | DEFAULT NOW |

**Indexes:**

`idx_portfolio_date (snapshot_date)`:
- Purpose: Fast temporal queries
- Query: "Portfolio growth over last 30 days"

**Constraints:**

`UNIQUE(snapshot_date)`:
- One portfolio snapshot per day
- Idempotent writes

**Design Rationale:**

Denormalized Aggregates: Could calculate from neuron_snapshots on every query. Trade-off: Duplicate data for query speed. Portfolio queries very common.

Why Store: Historical trend analysis requires fast aggregate access. Summing neurons on-demand slow for large datasets.

**Sample Data:**
```sql
INSERT INTO portfolio_snapshots (
    snapshot_date, total_neurons, total_stake_e8s, total_maturity_e8s,
    total_staked_maturity_e8s, total_voting_power, overall_return_percentage
) VALUES (
    '2025-10-25',
    4,
    25012340000,  -- 250.1234 ICP
    1256780000,   -- 12.5678 ICP
    8943210000,   -- 89.4321 ICP
    125847392847,
    40.79
);
```

---

### daily_rewards

**Purpose:** Calculated reward deltas between consecutive snapshots.

**Schema:**
```sql
CREATE TABLE daily_rewards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    neuron_id TEXT NOT NULL,
    reward_date DATE NOT NULL,
    maturity_delta_e8s INTEGER NOT NULL,
    staked_maturity_delta_e8s INTEGER NOT NULL,
    total_reward_e8s INTEGER NOT NULL,
    days_elapsed INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(neuron_id, reward_date)
);

CREATE INDEX idx_reward_neuron_date ON daily_rewards(neuron_id, reward_date);
```

**Columns:**

| Column | Type | Description | Constraints |
|--------|------|-------------|-------------|
| id | INTEGER | Primary key | AUTO INCREMENT |
| neuron_id | TEXT | IC neuron identifier | NOT NULL |
| reward_date | DATE | Date of reward calculation | NOT NULL |
| maturity_delta_e8s | INTEGER | Change in maturity | NOT NULL, can be negative |
| staked_maturity_delta_e8s | INTEGER | Change in staked maturity | NOT NULL, can be negative |
| total_reward_e8s | INTEGER | Sum of deltas | NOT NULL |
| days_elapsed | INTEGER | Days between snapshots | NOT NULL, DEFAULT 1 |
| created_at | TIMESTAMP | Row insert timestamp | DEFAULT NOW |

**Indexes:**

`idx_reward_neuron_date (neuron_id, reward_date)`:
- Purpose: Fast temporal queries for reward history
- Query: "Average daily reward last 30 days"

**Constraints:**

`UNIQUE(neuron_id, reward_date)`:
- One reward calculation per neuron per day
- Idempotent

**Design Rationale:**

Calculated Data: Could recalculate deltas from snapshots every time. Trade-off: Storage for speed and consistency.

Negative Deltas: Neuron fees can cause negative maturity change. Schema allows negative integers.

Days Elapsed: Tracks gaps in data collection. User skips days. Reward calculation normalizes by days elapsed for accurate daily rate.

**Calculation Logic:**
```sql
-- Pseudocode for reward calculation
today_snapshot = SELECT * FROM neuron_snapshots WHERE neuron_id = X AND snapshot_date = TODAY;
yesterday_snapshot = SELECT * FROM neuron_snapshots WHERE neuron_id = X AND snapshot_date < TODAY ORDER BY snapshot_date DESC LIMIT 1;

maturity_delta = today_snapshot.maturity_e8s - yesterday_snapshot.maturity_e8s;
staked_maturity_delta = today_snapshot.staked_maturity_e8s - yesterday_snapshot.staked_maturity_e8s;
total_reward = maturity_delta + staked_maturity_delta;
days_elapsed = DATEDIFF(today_snapshot.snapshot_date, yesterday_snapshot.snapshot_date);

INSERT INTO daily_rewards (...);
```

**Sample Data:**
```sql
INSERT INTO daily_rewards (
    neuron_id, reward_date, maturity_delta_e8s, staked_maturity_delta_e8s,
    total_reward_e8s, days_elapsed
) VALUES (
    '10000000000000000001',
    '2025-10-25',
    0,           -- No unstaked maturity change
    12340000,    -- 0.1234 ICP staked maturity gained
    12340000,    -- 0.1234 ICP total reward
    1            -- 1 day since last snapshot
);
```

---

## Relationships

### Entity Relationship Diagram
```
┌─────────────────────────────────────┐
│      neuron_snapshots               │
│  ┌─────────────────────────────┐    │
│  │ id (PK)                     │    │
│  │ neuron_id                   │    │
│  │ snapshot_date               │    │
│  │ stake_e8s                   │    │
│  │ maturity_e8s                │    │
│  │ staked_maturity_e8s         │    │
│  │ ...                         │    │
│  └─────────────────────────────┘    │
└────────────┬────────────────────────┘
             │ 1:N
             │ (neuron_id, snapshot_date)
             │
             ↓
┌────────────┴────────────────────────┐
│      daily_rewards                  │
│  ┌─────────────────────────────┐    │
│  │ id (PK)                     │    │
│  │ neuron_id (FK conceptual)   │    │
│  │ reward_date                 │    │
│  │ maturity_delta_e8s          │    │
│  │ staked_maturity_delta_e8s   │    │
│  │ total_reward_e8s            │    │
│  │ days_elapsed                │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│      portfolio_snapshots            │
│  ┌─────────────────────────────┐    │
│  │ id (PK)                     │    │
│  │ snapshot_date               │    │
│  │ total_neurons               │    │
│  │ total_stake_e8s             │    │
│  │ total_maturity_e8s          │    │
│  │ ...                         │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘
        ↑
        │ Aggregates from
        │ neuron_snapshots
        │ (Same snapshot_date)
```

**Relationship Types:**

Neuron Snapshots → Daily Rewards:
- Type: One-to-Many (conceptual, not enforced FK)
- Logic: Each neuron has many reward calculations
- Join: `neuron_snapshots.neuron_id = daily_rewards.neuron_id`

Neuron Snapshots → Portfolio Snapshots:
- Type: Many-to-One (aggregation)
- Logic: Many neuron snapshots aggregate to one portfolio snapshot
- Join: `neuron_snapshots.snapshot_date = portfolio_snapshots.snapshot_date`

**Why No Foreign Keys:**

SQLite supports foreign keys but we don't use them here. Rationale:

1. Neuron IDs from external system (IC blockchain). No guarantee of referential integrity.
2. Snapshots can exist without rewards (first day has no delta)
3. Portfolio snapshots independent lifecycle
4. Application layer enforces consistency
5. Performance: No FK checking overhead

Trade-off: Flexibility and performance over database-enforced integrity.

---

## Indexes Strategy

### Index Design Principles

**Composite Index Order:**  
Most selective column first. For temporal queries: entity_id, then date.

**Covering Indexes:**  
Not implemented (would duplicate significant data). Query performance adequate without.

**Index Maintenance:**  
Automatic. SQLite updates indexes on INSERT/UPDATE.

### Performance Characteristics

**Query Performance (10,000 rows):**
- Single neuron temporal query: ~5ms
- Portfolio aggregate for date: ~20ms
- 30-day average reward: ~15ms

**Write Performance:**
- Single snapshot insert: ~2ms
- Daily batch (10 neurons + portfolio + rewards): ~50ms

**Index Size:**
- `idx_neuron_date`: ~500KB per 10,000 rows
- `idx_snapshot_date`: ~200KB per 10,000 rows
- `idx_reward_neuron_date`: ~500KB per 10,000 rows

**When to Add More Indexes:**
- Query time exceeds 100ms consistently
- New query patterns emerge requiring different access paths
- Full table scans detected in EXPLAIN QUERY PLAN

---

## Common Queries

### Get Latest Snapshot for Neuron
```sql
SELECT *
FROM neuron_snapshots
WHERE neuron_id = '10000000000000000001'
ORDER BY snapshot_date DESC
LIMIT 1;
```

**Index Used:** `idx_neuron_date`  
**Performance:** ~2ms

### Get Portfolio Growth (Last 30 Days)
```sql
SELECT 
    snapshot_date,
    total_neurons,
    CAST(total_stake_e8s AS REAL) / 100000000 as total_stake_icp,
    CAST(total_maturity_e8s + total_staked_maturity_e8s AS REAL) / 100000000 as total_rewards_icp,
    overall_return_percentage
FROM portfolio_snapshots
WHERE snapshot_date >= date('now', '-30 days')
ORDER BY snapshot_date ASC;
```

**Index Used:** `idx_portfolio_date`  
**Performance:** ~10ms for 30 rows

### Calculate Average Daily Reward (30 Days)
```sql
SELECT 
    neuron_id,
    COUNT(*) as days_tracked,
    AVG(CAST(total_reward_e8s AS REAL) / days_elapsed) / 100000000 as avg_daily_icp,
    SUM(CAST(total_reward_e8s AS REAL)) / 100000000 as total_30d_icp
FROM daily_rewards
WHERE reward_date >= date('now', '-30 days')
GROUP BY neuron_id
ORDER BY avg_daily_icp DESC;
```

**Index Used:** `idx_reward_neuron_date`  
**Performance:** ~20ms for 300 rows (30 days × 10 neurons)

### Find Missing Snapshots (Data Quality)
```sql
WITH RECURSIVE dates(date) AS (
    SELECT date('2025-01-01')
    UNION ALL
    SELECT date(date, '+1 day')
    FROM dates
    WHERE date < date('2025-12-31')
)
SELECT 
    d.date,
    COUNT(ns.id) as snapshot_count
FROM dates d
LEFT JOIN neuron_snapshots ns ON d.date = ns.snapshot_date
GROUP BY d.date
HAVING snapshot_count = 0
ORDER BY d.date;
```

**Index Used:** `idx_snapshot_date`  
**Performance:** ~100ms for 365 days

### Neuron Performance Comparison
```sql
SELECT 
    ns.neuron_id,
    CAST(ns.stake_e8s AS REAL) / 100000000 as stake_icp,
    CAST(ns.staked_maturity_e8s AS REAL) / 100000000 as rewards_icp,
    CAST(ns.staked_maturity_e8s AS REAL) / ns.stake_e8s * 100 as return_pct,
    ns.age_days,
    ns.dissolve_delay_days,
    ns.age_bonus_multiplier * ns.dissolve_bonus_multiplier as combined_bonus
FROM neuron_snapshots ns
WHERE ns.snapshot_date = (SELECT MAX(snapshot_date) FROM neuron_snapshots WHERE neuron_id = ns.neuron_id)
ORDER BY return_pct DESC;
```

**Performance:** ~30ms for 10 neurons

---

## Data Types and Precision

### INTEGER for ICP Amounts

**Why not REAL for currency?**

Floating-point arithmetic has precision issues:
```sql
-- Floating point error example
SELECT 0.1 + 0.2;  -- Returns 0.30000000000000004
```

Using INTEGER e8s avoids this:
```sql
-- Exact arithmetic
SELECT 10000000 + 20000000;  -- Returns 30000000 (0.3 ICP)
```

**Conversion:**
- Storage: Always e8s (multiply ICP by 100,000,000)
- Display: Convert to ICP (divide e8s by 100,000,000)
- Queries: Cast to REAL only for display, never for calculation

### DATE Format

**Format:** ISO 8601 (YYYY-MM-DD)  
**Storage:** TEXT in SQLite (no native DATE type)  
**Rationale:** Sortable, unambiguous, standard

**Date Functions:**
```sql
-- Current date
SELECT date('now');  -- '2025-10-25'

-- Date arithmetic
SELECT date('2025-10-25', '+30 days');  -- '2025-11-24'
SELECT date('2025-10-25', '-1 year');   -- '2024-10-25'

-- Date comparison
WHERE snapshot_date >= date('now', '-30 days')
```

### BOOLEAN Representation

SQLite has no native BOOLEAN. We use INTEGER:
- 0 = FALSE
- 1 = TRUE

**Example:**
```sql
auto_stake_enabled INTEGER NOT NULL  -- 0 or 1
```

**Query:**
```sql
WHERE auto_stake_enabled = 1  -- TRUE
```

---

## Migration Strategy

### Refinery Migrations

**Location:** `migrations/` directory  
**Naming:** `V{N}__{description}.sql`  
**Example:** `V1__initial_schema.sql`

**Migration Process:**

1. Application startup
2. Refinery checks migration table
3. Applies unapplied migrations in order
4. Updates migration tracking

**Current Migrations:**

V1__initial_schema.sql:
- Create neuron_snapshots table
- Create portfolio_snapshots table
- Create daily_rewards table
- Create all indexes
- Create unique constraints

**Future Migration Example:**

`V2__add_sns_support.sql`:
```sql
-- Add SNS project tracking
CREATE TABLE sns_projects (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_name TEXT NOT NULL UNIQUE,
    governance_canister TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Add SNS neuron snapshots
CREATE TABLE sns_neuron_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    neuron_id TEXT NOT NULL,
    snapshot_date DATE NOT NULL,
    stake_e8s INTEGER NOT NULL,
    -- ... other fields
    UNIQUE(project_id, neuron_id, snapshot_date),
    FOREIGN KEY (project_id) REFERENCES sns_projects(id)
);
```

### Rollback Strategy

SQLite doesn't support schema rollback natively. Strategy:

**Option 1: Backup Before Migration**
```bash
cp neuron_history.db neuron_history.db.backup
cargo run  # Apply migrations
# If failure, restore backup
```

**Option 2: Down Migrations (Future)**
Create paired down migrations for complex changes.

**Option 3: Test First**
Test migrations on copy of production database before applying to real data.

---

## Database Maintenance

### Vacuum

SQLite files can fragment. Periodic vacuum reclaims space:
```sql
VACUUM;
```

**When:** After large deletions (rare in our system, append-only)  
**Frequency:** Annually or manually as needed

### Analyze

Update query optimizer statistics:
```sql
ANALYZE;
```

**When:** After significant data growth  
**Frequency:** Quarterly or after adding 10,000+ rows

### Integrity Check

Verify database integrity:
```sql
PRAGMA integrity_check;
```

**Expected Result:** `ok`  
**Frequency:** If corruption suspected or after crash

### File Size Management

Monitor database growth:
```sql
-- Check page count and size
PRAGMA page_count;
PRAGMA page_size;

-- Unused pages
PRAGMA freelist_count;
```

**Expected Growth:** ~1MB per neuron per year

---

## Backup and Recovery

### Backup Strategy

**Full Backup:**
```bash
cp neuron_history.db neuron_history_$(date +%Y%m%d).db
```

**Incremental Backup:**
Not needed. Single file makes full backup trivial.

**Backup Frequency:**
- Daily: Automated script
- Before upgrades: Manual
- Before migrations: Automatic in deployment script

### Recovery

**From Backup:**
```bash
cp neuron_history_20251025.db neuron_history.db
```

**Export to SQL:**
```bash
sqlite3 neuron_history.db .dump > backup.sql
```

**Import from SQL:**
```bash
sqlite3 new_database.db < backup.sql
```

### Data Export

**CSV Export:**
```sql
.mode csv
.output neuron_snapshots.csv
SELECT * FROM neuron_snapshots;
.output stdout
```

**JSON Export (via tool):**
```bash
sqlite3 neuron_history.db \
  "SELECT json_object('id', id, 'neuron_id', neuron_id, ...) FROM neuron_snapshots" \
  > export.json
```

---

## Performance Tuning

### Pragmas

**Journal Mode (WAL recommended for concurrency):**
```sql
PRAGMA journal_mode = WAL;
```

Benefit: Better concurrency, faster writes.  
Trade-off: Three files (-wal, -shm) instead of one.

**Synchronous Mode:**
```sql
PRAGMA synchronous = NORMAL;
```

Balance between safety and speed. NORMAL sufficient for most cases.

**Cache Size:**
```sql
PRAGMA cache_size = -64000;  -- 64MB
```

More cache = faster queries. Adjust based on available memory.

### Query Optimization

**Use EXPLAIN QUERY PLAN:**
```sql
EXPLAIN QUERY PLAN
SELECT * FROM neuron_snapshots WHERE neuron_id = 'X' AND snapshot_date > '2025-01-01';
```

Check for:
- Index usage (SEARCH vs SCAN)
- Unnecessary full table scans
- Join strategy

**Optimization Techniques:**

Prefer indexed columns in WHERE:
```sql
-- Good (uses index)
WHERE neuron_id = 'X' AND snapshot_date >= '2025-01-01'

-- Bad (full scan)
WHERE SUBSTR(neuron_id, 1, 4) = '5476'
```

Limit result sets:
```sql
-- Fetch only needed columns
SELECT neuron_id, snapshot_date, stake_e8s
-- Not SELECT *

-- Use LIMIT for pagination
ORDER BY snapshot_date DESC LIMIT 100 OFFSET 0
```

---

## Security Considerations

### File Permissions

**Recommended:**
```bash
chmod 600 neuron_history.db
```

Owner read/write only. No group or world access.

### Encryption at Rest

SQLite doesn't encrypt by default. Options:

**File System Encryption:**
- Linux: LUKS, ecryptfs
- macOS: FileVault
- Windows: BitLocker

**SQLite Encryption Extensions:**
- SQLCipher (third-party)
- Not implemented in current system

Trade-off: Complexity vs threat model. File system encryption sufficient for desktop use.

### SQL Injection

**Not applicable.** All queries parameterized via rusqlite:
```rust
// Safe - parameterized
conn.execute(
    "INSERT INTO neuron_snapshots (neuron_id, ...) VALUES (?1, ...)",
    params![neuron_id, ...]
)?;

// Never do this - string concatenation
// conn.execute(&format!("INSERT ... VALUES ({})", neuron_id))?;  // UNSAFE
```

---

## Troubleshooting

### Database Locked

**Error:** `database is locked`

**Cause:** Another process has write lock.

**Resolution:**
```bash
# Find processes using database
lsof neuron_history.db

# Close other connections
# Ensure only one tracker instance running
```

### Disk Full

**Error:** `disk I/O error` or `database or disk is full`

**Resolution:**
```bash
# Check disk space
df -h

# Free space or move database to larger volume
```

### Corruption

**Error:** `database disk image is malformed`

**Resolution:**
```bash
# Try recovery
sqlite3 neuron_history.db ".recover" | sqlite3 recovered.db

# Restore from backup if recovery fails
cp neuron_history_backup.db neuron_history.db
```

---

## Related Documentation

**Architecture:**
- [System Overview](system-overview.md) - Overall system design, including data flow
- [Domain Model](domain-model.md) - Business entities

**Decisions:**
- [ADR-001: Use SQLite](../decisions/001-use-sqlite.md) - Why SQLite
- [ADR-002: Repository Pattern](../decisions/002-repository-pattern.md) - Data access

**Guides:**
- [Usage Guide](../guides/usage.md) - CSV formats, on-disk locations, import hazards

**Code:** the repository traits in `src/domain/repositories.rs` are the data-access contract
of record; `src/infrastructure/sqlite_repository.rs` is the only implementation.

---

**U Reflection Design & Build Inc.**

Schema mirrors domain.  
Structure enables queries.

Last Updated: 2026-08-05  
Version: 0.1.1