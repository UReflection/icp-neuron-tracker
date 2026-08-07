# ICP Neuron Portfolio Tracker

**U Reflection Design & Build Inc.**

Retirement income planning through automated ICP neuron staking analytics.

[![Rust](https://img.shields.io/badge/rust-2021_edition-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

---

## What this is

A single-binary Rust CLI that snapshots NNS neuron state to a local SQLite database and
derives portfolio, reward and retirement-projection reports from it.

It is a personal tool. All data stays on your machine.

Context on this repository's history and on the example data used throughout: [NOTICE.md](NOTICE.md).

---

## Quick Start

```bash
git clone https://github.com/ureflection/icp-neuron-tracker
cd icp-neuron-tracker
cargo install --path .

# Interactive setup: creates an identity, collects your neuron IDs,
# writes config.toml, and takes a first snapshot
icp-neuron-tracker init
```

`init` walks through four steps and tells you the principal to add as a hot key in the
[NNS dapp](https://nns.ic0.app). You need that hot key registered before the tracker can
read anything, so start here rather than with a bare `track`.

Two things worth knowing before you start:

- **`init` needs a terminal.** It is a wizard, so it cannot run from a script, a cron job or
  an SSH command without a TTY. To configure headlessly, copy `config.toml.example` into the
  config directory and edit it by hand — the tool prints the exact path if you try.
- **The first snapshot may fail, and that is fine.** A hot key added moments earlier takes a
  few minutes to become visible to the governance canister. Your configuration is saved
  regardless; run `icp-neuron-tracker track` shortly afterwards.

If you would rather not install the binary, every `icp-neuron-tracker <command>` below also
works as `cargo run -- <command>` from the repository root.

Once configured:

```bash
icp-neuron-tracker track                # capture today's snapshot
icp-neuron-tracker report summary       # current portfolio
icp-neuron-tracker report rewards       # recent reward analysis
```

`track` and `project` need network access and a registered hot key. Reports and
import/export work entirely offline, as does `project --offline`.

Run `icp-neuron-tracker --help`, or `<command> --help`, for all commands and flags.

---

## The cold start

**Retirement projection needs about eleven weeks of daily tracking before it will produce a
number.** Reports work from your first snapshot; `project` does not, and that is deliberate.

Risk scenarios come from percentile bands over eleven non-overlapping seven-day windows
within a ninety-day lookback. A database that has been collecting for three days contains no
such windows, and this tool will not manufacture them — an unobserved week is not a
zero-reward week, and a pessimistic scenario built on the difference projects "never" from an
absence of evidence rather than from your neurons.

So `project` refuses until the history exists, and says how far along you are each time you
ask. That is the cold start, not a fault, and it is the price of the tool declining to invent
a number it has not observed. Set up a daily `track` — cron, a systemd timer, whatever you
use — and it clears on its own.

If you already have history, `import` it and the wait shortens accordingly.

---

## Features

**Multi-neuron tracking** — monitor multiple neurons across different controller
identities. Hot key authentication keeps controller authority out of the tool.

**Daily snapshots** — each run records stake, maturity, staked maturity and voting power
per neuron, and computes day-over-day reward deltas.

**Portfolio reports** — `report summary`, `history`, `rewards` and `neuron`, each available
as human-readable text, JSON or CSV via `--format`.

**Retirement projection** — `project --target <ICP/day>` estimates when staking income
alone covers a target, across pessimistic, realistic and optimistic scenarios derived from
your own reward history. `--compare` explores alternative targets. `--offline` projects
from stored data when the network is unavailable. Needs roughly eleven weeks of daily
tracking first — see [The cold start](#the-cold-start).

**CSV import and export** — bring in years of existing records, export for backup or
analysis. See the [Usage Guide](docs/guides/usage.md) for the formats and a decimal-point
hazard worth knowing about before you import a spreadsheet.

**Local-first** — no cloud, no telemetry, no external calls beyond the IC governance
canister.

### Not built

SNS neuron support, a desktop GUI, and advanced trend reporting are ideas, not work in
progress. There is no ADR behind the GUI choice and nothing has been scaffolded.

---

## How storage actually works

**Snapshots are keyed `UNIQUE(neuron_id, snapshot_date)` — one row per neuron per day, and
writing the same key again replaces the existing row.** This is a destructive upsert, not
an append-only log:

- Running `track` twice in one day overwrites that day's row with the later reading.
- A CSV containing two rows for the same neuron and date stores only the last one. The
  import summary counts both, so the reported figure can exceed the rows actually kept.
- Re-importing a file whose dates are already present is different: those rows are detected
  as duplicates and skipped, leaving the originals intact.

Earlier versions of this README described the storage as "event-sourced" with "immutable
state tracking" and history "reconstructible from events". **That was never true of this
implementation** and the claim has been removed rather than softened. If you need a value
preserved, export it — an export is a point-in-time copy of the measured data.

`retrieved_timestamp` records when each snapshot was actually observed, which is what
distinguishes automated collection from bulk CSV import. It survives duplicate-skip but is
overwritten by a same-key replace.

**What an export does and does not carry.** As of export format 1.1 it includes
`retrieved_timestamp_seconds`, so an export/import round-trip preserves when each snapshot
was observed rather than restamping every row with the import time. Files written by format
1.0 lack the column and are still importable; those rows get the import time, because their
observation time was never recorded.

Two fields are still not exported and are **inferred** on import: `state` from the dissolve
delay, and `auto_stake_enabled` from whether any staked maturity is present. A round-trip
preserves them only where those inferences happen to be right — a dissolving neuron with a
non-zero delay comes back as `Locked`. Everything measured round-trips exactly; these two
are derived, and are the reason an export is a copy of the data rather than of the database.

---

## Architecture

Three layers, plus the CLI:

```
CLI (src/main.rs)      command parsing, dispatch, output
  ↓
Application            service orchestration
  ↓
Domain                 business logic and invariants — depends on nothing
  ↑
Infrastructure         SQLite, IC client, configuration, formatting
```

Dependencies point inward. The domain layer has no knowledge of SQLite, `ic-agent`, or the
filesystem, and data access sits behind traits defined in `src/domain/repositories.rs`.

`src/presentation/` exists as a directory but is not compiled in; all presentation lives in
`main.rs` and `infrastructure/report_formatter.rs`.

Detail: [System Overview](docs/architecture/system-overview.md) ·
[Domain Model](docs/architecture/domain-model.md) ·
[Database Schema](docs/architecture/database-schema.md)

---

## Documentation

| Document | Covers |
|---|---|
| [Usage Guide](docs/guides/usage.md) | CSV formats, import hazards, `--offline`, file locations, data-quality grades |
| [Identity Setup](docs/guides/identity-setup.md) | Generating and registering a hot key |
| [System Overview](docs/architecture/system-overview.md) | Layers, responsibilities, data flow |
| [Domain Model](docs/architecture/domain-model.md) | Aggregates and value objects |
| [Database Schema](docs/architecture/database-schema.md) | Tables and indexes |
| [Decisions](docs/README.md) | Architecture decision records |

Full index: [docs/README.md](docs/README.md). For flags and arguments, `--help` is
authoritative — there is no hand-maintained command reference.

---

## Technology

**Rust** (edition 2021) — one crate, no workspace.
**SQLite** via `rusqlite`, with `refinery` migrations. See [ADR-001](docs/decisions/001-use-sqlite.md).
**ic-agent** — DFINITY's official Rust client for the Internet Computer.
**Repository pattern** behind domain traits. See [ADR-002](docs/decisions/002-repository-pattern.md).

---

## Security Model

### Hot key pattern

The tool authenticates with a read-only principal registered as a hot key on your neurons.

**Can:** query neuron state, read maturity balances, access governance data.
**Cannot:** transfer stake, change the controller, dissolve or spawn neurons.

If the key is compromised, remove the hot key via the NNS dapp. Controller authority is
never held by this tool and is unaffected.

### Local-first

All data persists locally. No telemetry, no analytics, no external calls other than to the
IC governance canister. You own the database file.

### Credentials

`config.toml`, `*.pem`, `*.db` and `*.csv` are gitignored. Configuration stores paths only,
never keys. Generated identities are written with `600` permissions. Store the PEM outside
the repository and reference it by absolute path.

**Never commit real neuron IDs or keys.**

---

## Daily Usage

```bash
icp-neuron-tracker track          # capture the day's snapshot
icp-neuron-tracker report summary # see where things stand
```

Automate it with cron:

```
0 0 * * * icp-neuron-tracker track
```

The database is wherever `tracking.history_file` in your config points — see the
[Usage Guide](docs/guides/usage.md), since a bare filename resolves relative to the working
directory and can silently create a second database.

### Querying directly

```sql
-- Recent portfolio performance
SELECT
    snapshot_date,
    total_neurons,
    CAST(total_stake_e8s AS REAL) / 100000000 as stake_icp,
    CAST(total_maturity_e8s + total_staked_maturity_e8s AS REAL) / 100000000 as rewards_icp,
    overall_return_percentage
FROM portfolio_snapshots
ORDER BY snapshot_date DESC
LIMIT 7;

-- Average daily rewards per neuron (30-day)
SELECT
    neuron_id,
    COUNT(*) as days_tracked,
    AVG(CAST(total_reward_e8s AS REAL) / days_elapsed) / 100000000 as avg_daily_icp
FROM daily_rewards
WHERE reward_date >= date('now', '-30 days')
GROUP BY neuron_id
ORDER BY avg_daily_icp DESC;
```

---

## Development

```bash
cargo build
cargo test        # 67 tests, all inline #[cfg(test)] modules
icp-neuron-tracker <command>
```

**Toolchain.** `Cargo.toml` declares `rust-version = "1.78"`. That is a **lower bound derived
from the lockfile**, not a tested MSRV: `Cargo.lock` is lockfile version 4, which Cargo below
1.78 cannot parse — 1.75 fails outright. **Verified working on 1.90.0.** Anything between 1.78
and 1.89 is unverified. An earlier badge here claimed 1.75+, which does not build.

Conventions as practised in the tree are recorded in [CLAUDE.md](CLAUDE.md) — error
handling, money as e8s, value-object newtypes, and which files are load-bearing. Read it
before changing the data layer or the hand-declared Candid types.

Where documentation and code disagree, **code wins.** Known conflicts are listed in
CLAUDE.md; record new ones rather than resolving them silently.

---

## Troubleshooting

**"Principal X is not authorized to get full neuron information"**
The hot key is not registered on that neuron. Add it via https://nns.ic0.app/neurons/,
then `icp-neuron-tracker identity verify`.

**"Database is locked"**
Another process holds the database — a second tracker instance or an open SQLite browser.
Close it and retry.

**Migration failure**
Check `migrations/` for syntax errors and the stderr output for the failing statement.
Applied migrations are immutable; add a new `V2__` file rather than editing `V1__`.

**A projection looks wrong after an import**
Check the stake figures with `report summary`. If a value is out by a factor of
100,000,000, see the decimal-point hazard in the [Usage Guide](docs/guides/usage.md).

---

## License

MIT — see [LICENSE](LICENSE).

---

**U Reflection Design & Build Inc.**

Documentation: [docs/README.md](docs/README.md)

Last Updated: 2026-08-05
Version: 0.1.1
