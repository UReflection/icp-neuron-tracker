# CLAUDE.md

Working notes for agents in this repo. Conventions here are **as practiced in the tree**, not aspirational.
Where docs and code disagree, code wins — see "Known doc/code conflicts".

## What this is

Single-binary Rust CLI (`icp-neuron-tracker`, edition 2021) that snapshots NNS neuron state to local
SQLite and derives portfolio, reward and retirement-projection reports. One crate, no workspace.

## Module layout

```
src/main.rs              clap CLI: init | track | identity | project | import | export | report
src/domain/              pure: no rusqlite, no ic-agent, no I/O. Aggregates + value objects + repo traits
src/application/         *Service types, generic over repository traits. Orchestration only
src/infrastructure/      SqliteRepository, IcClient, CsvParser, TerminalReportFormatter, Config, IdentityClient
migrations/              refinery, embedded via embed_migrations!("migrations")
```

Dependency direction points inward: `infrastructure -> application -> domain`. Repository traits live in
[src/domain/repositories.rs](src/domain/repositories.rs); the only implementation is
[src/infrastructure/sqlite_repository.rs](src/infrastructure/sqlite_repository.rs). This is ADR-002 —
read [docs/decisions/002-repository-pattern.md](docs/decisions/002-repository-pattern.md) before changing it.

`src/presentation/mod.rs` is empty and is **not** declared as a module in main.rs. Dead directory.

## Build and test

**Verified by execution 2026-08-06**, not transcribed. A Rust toolchain is installed:
`rustc 1.90.0 (1159e78c4 2025-09-14)`, `cargo 1.90.0`, `stable-x86_64-unknown-linux-gnu` via rustup.
Both commands below were run in this working copy and pass.

```bash
cargo build
cargo test          # 102 #[test] fns across 13 files, all inline #[cfg(test)] mod tests. No tests/ dir
cargo run -- <cmd>
```

Config: copy `config.toml.example` to `config.toml`. At runtime main.rs looks in the `directories`
config dir first, then falls back to `./config.toml`.

**On Linux the real paths are `~/.config/icp-neuron-tracker/` and `~/.local/share/icp-neuron-tracker/`.**
`ProjectDirs::from("com", "u-reflection", "icp-neuron-tracker")` keeps the qualifier and organisation
only on macOS and Windows; on Linux they are dropped and the XDG dirs are named from the application
alone. A `com/u-reflection/icp-neuron-tracker` path does not exist on Linux — do not go looking
for one, and do not "fix" the call to produce it. **The three `ProjectDirs::from` arguments are
load-bearing**: they resolve to the installed `config.toml`, `identity.pem` and `neuron_history.db`.
Changing any of them silently repoints the CLI at an empty directory, and `track` would start a new
history rather than fail. Call sites: [src/main.rs:17](src/main.rs#L17) and three in
[src/application/setup_service.rs](src/application/setup_service.rs).

`config.toml`, `*.pem`, `*.db` and `.csv` are gitignored — never commit real neuron IDs or keys.

## Conventions as practiced

- **CLI error output — two known cosmetic issues, deliberately left.** `main()` returns
  `Result<_, Box<dyn Error>>`, so anything reaching it via `?` is printed with **Debug**, not
  Display. Two consequences were surveyed on 2026-08-05 and consciously not fixed:
  1. **Quoted messages.** `Err("...".into())` renders as `Error: "No tracked neurons found."`
     — quotes included. Affects `report summary`, `report history`, `report neuron` and
     ~9 `return Err("...")` sites in `report_service.rs` / `import_service.rs`.
  2. **Three error conventions coexist** — Debug via `?`, explicit `eprintln!` + `exit(1)`
     (e.g. main.rs "Days must be at least 1", which also duplicates a check `report_service`
     already makes), and a `❌ <op> failed:` prefix in import/export.
  Both are cosmetic and neither changes what the user is told to do. They were left because
  fixing them means touching ~12 propagation sites and picking one convention across the
  whole CLI — tidiness, not correctness, and not worth the expansion at 0.1.0. **The one
  that was NOT cosmetic — raw `Os { code: 2, … }` from config and database loads, which
  named neither the file nor the remedy — is fixed:** see `load_config_or_exit` and
  `open_database_or_exit` in main.rs. Route new config/database opens through those.

- **Errors**: `Result<T, Box<dyn std::error::Error>>` everywhere (~142 sites). `anyhow` is in Cargo.toml
  but imported nowhere. Do not introduce a second error convention without a decision record.
- **Money**: always e8s as `u64` inside `IcpAmount`; convert to f64 ICP only at display edges.
- **Value objects**: newtype-wrap primitives (`NeuronId`, `IcpAmount`, `TargetIncome`, `BonusMultiplier`).
- **Services**: generic over repo traits (static dispatch), constructed in main.rs. Mock the trait in tests —
  see `MockRewardRepo` in [src/application/retirement_service.rs](src/application/retirement_service.rs).
- **Formatting**: all output construction lives in `infrastructure/report_formatter.rs`, never in domain.
- **Commits**: conventional prefixes, `feat:` dominant. Work lands via PR from `<issue>-cj-<slug>` branches.

## Load-bearing — change with care

- `migrations/V1__initial_schema.sql` — applied migrations are immutable. Add `V2__`, never edit V1.
- Hand-declared Candid types in [src/infrastructure/ic_client.rs](src/infrastructure/ic_client.rs).
  There is no `.did` file to diff against, and **Candid silently drops response fields absent from these
  structs** — a decode succeeds while data vanishes. Verify against the live interface when touching them.
- `BonusMultiplier` in [src/domain/value_objects.rs](src/domain/value_objects.rs) — see EXTERNAL 1 below.
- Repository trait signatures — changing one touches every service and the mocks.

## Safe to touch

`report_formatter.rs` output strings, CLI help text, new `report` subcommands, new `V2__` migrations,
docs. Adding a test is always safe.

## Known doc/code conflicts (do not silently resolve)

**Re-checked 2026-08-06. Most of what this section used to list has since been fixed; the
entries below were verified as still true on that date.** Do not re-add resolved entries
without re-checking the tree — a stale warning costs as much time as a stale doc.

- **ADR-003 is marked Accepted but is not built.** It specifies a `crates/neuron-tracker-*` workspace;
  the tree is one crate. Trust the tree.

That is the only one left.

Resolved since this section was written, listed so they are not re-reported as conflicts:

- **`docs/architecture/domain-model.md` no longer teaches any NNS protocol constant.** It
  described the pre-Mission-70 curve — 8-year maximum, linear 2.0x ceiling — including a
  reproduction of `from_dissolve_seconds`. On 2026-08-06 those passages were **deleted rather
  than rewritten**, each replaced by a pointer to the `protocol` module in
  `value_objects.rs`. The file now carries aggregates, value objects, invariants and layering
  only, and its header says so. **Do not re-document the protocol there**: a second copy is
  what made it wrong the first time, and it is the same failure mode that left `usage.md`
  describing a percentile method that had already been replaced.
- `LICENSE` now exists (MIT), so the README badge and link resolve.
- `docs/README.md`'s index no longer points at missing files; every link in README, `docs/README.md`
  and `docs/guides/usage.md` was checked on 2026-08-06 and resolves.
- README no longer claims event sourcing. It documents the `INSERT OR REPLACE` destructive
  upsert explicitly, under "How storage actually works".
- README no longer links `docs/guides/troubleshooting.md`.

---

## EXTERNAL — sourced 2026-08-04 from a web check, not from this repo

Not derived from the tree. **Re-verify before relying on any of it.**

### 1. NNS protocol constants are governance-mutable — never hardcode them

Read them from the governance canister where exposed. Where not exposed, hold them in config with a
**verified-as-of date**, and alarm when observed data contradicts them.

Reason — these changed in 2026, and this codebase predates the change:

| Parameter | Old | New |
|---|---|---|
| Max dissolve delay | 8 years | 2 years |
| Max dissolve-delay bonus | 2x | 3x |
| Voting eligibility floor | 6 months | 2 weeks |
| Dissolve-delay bonus curve | linear | quadratic |

Age bonus is unchanged (up to 1.25x at 4 years).
Source: `docs.internetcomputer.org/concepts/governance/` and the Mission 70 paper.

This repo does not read any of these from the chain, and hardcodes the superseded values — see REPO STATE.

### 2. DFINITY documentation is currently inconsistent

`docs.internetcomputer.org` carries the new parameters. `learn.internetcomputer.org` and
`support.dfinity.org` still carry the old ones. **Always check which page a claim came from** before
acting on it — including claims already written into this repo's docs.

### 3. Scope

This is a personal tool, maintained as time allows.

---

## REPO STATE — verified in-tree 2026-08-04

Observed directly in this working copy, unlike the EXTERNAL block above. A small number of claims here
were verified against a running installation rather than the tree, and are noted inline where they appear.

- Nothing in this repo reads any NNS protocol *parameter* from the governance canister. The
  constants live in the `protocol` module of
  [src/domain/value_objects.rs](src/domain/value_objects.rs), which carries a verified-as-of
  date. **Corrected 2026-08-06** to the Mission 70 curve: quadratic, 1.0x–3.0x over two
  years, replacing the linear 1.0x–2.0x over eight. Source is
  `rs/nns/governance/src/neuron/types.rs` in `dfinity/ic`.

  **Verified at the endpoints only.** Every neuron observed so far sits at the
  730-day maximum, so 0 and 730 are confirmed but nothing between them has been checked
  against an observation. `1 + 2·(d/dmax)²` is the reading of "increases quadratically" that
  satisfies both endpoints; a different quadratic would be wrong mid-range and right at both
  ends, and no data here would show it. If a neuron with a partial dissolve delay ever gets
  tracked, that is the observation to check it against.
- `LICENSE` exists (MIT, added 2026-08-05). The README badge ([README.md:8](README.md#L8))
  and the link in the Licence section both resolve. An earlier note here cited `README.md:449`
  as the link site; README is 325 lines and that reference was never valid.
- The SQLite snapshot history and `config.toml` are gitignored **by design** and exist only on the machine
  where the CLI was actually run. Without them a clone is indistinguishable from the original working copy.
- A history can mix automated snapshots with rows CSV-imported from a spreadsheet under a backdated
  `snapshot_date`. Both are real observations, but imported rows carry transcription risk the automated
  rows do not. `retrieved_timestamp` is what distinguishes them — preserve that column.
- **`voting_power` IS read from the chain** — `info.voting_power` from `get_neuron_info`, at
  [src/infrastructure/ic_client.rs:111](src/infrastructure/ic_client.rs#L111). An earlier
  version of this note claimed it was computed locally; that was wrong, and it was
  load-bearing. The two `*_bonus_multiplier` columns *are* computed locally, and the
  difference is what made the superseded dissolve curve visible at all: the tool printed its
  own 1.46x combined multiplier beside a chain-sourced voting power implying 3.85x.
- The `age_bonus_multiplier` / `dissolve_bonus_multiplier` **columns are written but never
  read back**. `row_to_neuron` recomputes both from the stored `age_days` and
  `dissolve_delay_days`. Consequences: correcting the curve needed no backfill, and stored
  rows still carry the old 1.25 values while reports show 3.0 — the columns are a write-only
  audit trail, not the source of anything displayed.
- Observed from chain: `stake_e8s`, `maturity_e8s`, `staked_maturity_e8s`, `voting_power`,
  `age_days`, `dissolve_delay_days`, `state`. Derived by the tool: the two multiplier columns
  and `daily_rewards` deltas. All sit in the same tables with no provenance marker except
  `retrieved_timestamp`.
