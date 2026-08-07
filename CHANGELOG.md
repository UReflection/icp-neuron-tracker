# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-08-06

Correctness and disclosure. No new features.

### Fixed

- **Unobserved reward windows were counted as zero income.** A window with no reward rows
  covering its span was zero-filled, so a collection gap reached the projection as a run of
  zero-income weeks. A 211-day gap produced eleven fabricated zero windows, drove the 10th
  percentile to 0.0, and failed `project` with "zero or negative growth" over 545.60 ICP of
  rewards that had genuinely accrued. The same fabrication handed the whole 545.60 ICP to the
  single window holding the row's `reward_date`, making the 90th percentile 77.94 ICP/day —
  thirty times the real rate. Windows are now populated only when every day in them is
  covered by a reward row; unobserved windows are omitted rather than zeroed, and genuine
  zero-reward weeks still count. `MIN_WINDOWS` counts populated windows, not windows
  produced, which is why a floor meant to reject thin samples had been passing on fabricated
  ones.
- **The dissolve-delay bonus used the pre-Mission-70 curve.** Corrected to quadratic,
  1.0x–3.0x over two years, from linear 1.0x–2.0x over eight. Every neuron in the production
  database reports the new 730-day maximum and was being scored 1.25x where the protocol
  gives 3.0x — a 2.4-fold understatement written to every row since the protocol changed.
  Source: `rs/nns/governance/src/neuron/types.rs` in `dfinity/ic`. The age bonus is unchanged
  and was confirmed against chain voting power rather than assumed.
- **The setup wizard discarded everything on a failed first snapshot.** Config is now written
  before the snapshot, and the snapshot cannot abort setup. A hot key added moments earlier
  has not necessarily propagated, so a failed first snapshot is the expected day-one outcome
  on a correct configuration.
- **`init` failed unintelligibly without a terminal**, after writing an identity, and
  identically on every retry. It now refuses up front and documents the manual route.
- **`report rewards` printed two different daily averages for the same quantity.** The
  portfolio figure divided by the requested `--days` window while each neuron divided by the
  days actually observed; a single-neuron portfolio showed 0.0043 against 0.0100. Both now
  divide by observed days, and the divisor is named in the output.
- **Errors were rendered with Debug**, so messages appeared wrapped in quotes. Fixed once at
  `main` rather than at each propagation site.
- **Exports omitted `retrieved_timestamp_seconds`**, so re-importing a backup — which the
  export's own summary recommends — restamped every row with the import time and destroyed
  the marker distinguishing automated collection from bulk import. Export format is now 1.1
  and the column round-trips. Format 1.0 files remain importable.
- **The income panel's labels contradicted each other and the arithmetic** — a "30-day
  average" header, a trailing 30-day period the figure was not drawn from, and a
  "Data points: 1/30" counter, over a rate whose real divisor was 247 days.

### Changed

- User-facing messages name the binary (`icp-neuron-tracker <command>`) rather than
  `cargo run --`, and reference `sample_import.csv`, which exists, rather than
  `neurons_history_sample.csv`, which never did.
- The insufficient-history message reports where you are and names the real requirement,
  instead of promising that seven days would unblock a projection that needs eleven weeks.
- Neither the README quick start nor the setup wizard recommends `project` any more. On a
  fresh database it cannot succeed for about eleven weeks, so both pointed new users at a
  guaranteed refusal.

### Security

- **The `.csv` gitignore rule matched nothing it claimed to.** Line 6 read `.csv`, which
  matches only a file literally named `.csv`, so every export the tool writes was tracked
  while README and the usage guide both stated `*.csv` was ignored. Exports carry neuron IDs,
  stake, maturity and voting power. Now `*.csv` with `!sample_import.csv`.
- **Real neuron IDs and balances have been replaced with synthetic values** throughout the
  documentation and test fixtures. Neuron IDs are public on-chain identifiers: pseudonymous
  alone, but not when published beside a company name and a named committer. Substitutes keep
  digit length and remain valid `u64`, so the documented formats stay accurate. Earlier
  commits still contain the real values — see [NOTICE.md](NOTICE.md).
- **The committed-key remedy put history rewriting first.** Reordered to rotate first: treat
  the key as compromised from the moment of commit, generate and re-register a replacement,
  confirm the exposed principal controls nothing, and only then weigh a rewrite — which
  changes what is easy to find, not what has already been copied. `git filter-branch` is no
  longer recommended anywhere.

### Documentation

- **README states the eleven-week cold start up front**, in its own section, as a design
  property rather than a caveat to be discovered through repeated refusals.
- **`docs/architecture/domain-model.md` no longer teaches NNS protocol constants.** It carried
  the superseded 8-year/2.0x linear curve in six places, including a reproduction of
  `from_dissolve_seconds` that read as a specification. Those passages were deleted rather
  than rewritten, each replaced by a pointer to the `protocol` module — a second copy of
  governance-mutable constants is what made the file wrong in the first place.
- **`NOTICE.md` added**, covering what this repository is, what its history contains, and
  which data in it is synthetic.
- The usage guide's percentile description, which still described the replaced 30-reward-date
  method, now matches the shipped 90-day non-overlapping windows.

### Known limitations

Added to those recorded for 0.1.0:

- **`project` needs about eleven weeks of daily tracking before it will produce a number.**
  Risk scenarios require 11 populated non-overlapping 7-day windows within a 90-day lookback;
  a new database has none, and the tool will not manufacture them. Reports work from the
  first snapshot. This is a deliberate consequence of refusing to estimate from unobserved
  data, not a defect, and `project` reports how far along you are each time it declines.

- **Historical rows are rendered under current protocol rules.** Bonus multipliers are
  recomputed on load from the stored dissolve delay and age, and the tool has no
  protocol-effective-date model — it knows only today's curve. A 2922-day dissolve delay
  observed in 2023 therefore displays as 3.0x, though the protocol in force at the time gave
  2.0x. Nothing is rewritten and no backfill was performed; the stored multiplier columns
  still hold what was computed when the row was written. The same applies to re-importing the
  historical CSV, whose metadata records `dissolve_delay_years: 8` from the old protocol.
  This is a known property of how history is displayed, not a defect to be discovered later:
  the tool renders the past as if today's rules had always applied.
- **The eight-year-gang bonus is not modelled.** Neurons created at genesis with 8-year
  dissolve delays carry a legacy bonus worth roughly 9.7% of voting power, computed by the
  governance canister as a separate term. The tool does not account for it, so for affected
  neurons the displayed combined multiplier reads about 3.51x against a chain voting power
  implying about 3.85x. Deliberate: modelling it means decoding
  `eight_year_gang_bonus_base_e8s`, which requires changing the hand-declared Candid types in
  `ic_client.rs` — the one file that crosses a trust boundary with no `.did` file to check
  against, where a decode error loses data silently.
- **The quadratic dissolve curve is verified at its endpoints only.** Every neuron available
  sits at the 730-day maximum, so 0 and 730 are confirmed against observation and nothing
  between them is. A different quadratic would be indistinguishable on this data.

## [0.1.0] - 2026-08-05

First tagged release.

> A `[0.1.0]` entry dated 2025-11-07 previously appeared below this one, but no tag was ever
> created and nothing was published — the repository carried zero tags until this release.
> The two entries are merged here rather than left as a contradiction. Development ran
> 2025-10-25 to 2025-11-26, then stopped; work resumed 2026-08-04 for assessment, correctness
> and release. Entries below cover both periods.

### Fixed

- **Age-bonus inversion used a 365-day year.** `csv_parser::calculate_age_from_bonus`
  inverted `age_bonus_multiplier` back to seconds using `4 × 365 × 86400`, while
  `BonusMultiplier::from_age_seconds` divides by `365.25 × 86400`. Round-tripping scaled the
  bonus portion by 365/365.25 = 0.9993155373, so an imported `1.25` was stored as
  `1.2498288843258043` and the maximum age bonus could not be represented at all.
- **Dissolve-delay conversion used a 365-day year.** Same defect, separate constant:
  metadata `dissolve_delay_years` was multiplied by `31_536_000`. An 8-year delay stored a
  `1.999315537303217` bonus over 2920 days instead of `2.0` over 2922.
- **Risk percentiles were computed over overlapping windows.** Bands came from the most
  recent 30 reward dates, each expanded into a *trailing* 7-day rate, so one day's reward
  appeared in up to seven samples. The upper tail was worst affected — a single strong week
  could populate the top decile with copies of itself. Replaced with non-overlapping 7-day
  windows over a 90-day lookback, with a floor of 11 windows (below which the 10th percentile
  is merely the single worst window) and the sample size printed alongside the bands. On real
  data the optimistic scenario moved 1.3 years later; the pessimistic barely moved.
- **`project` wrote to `config.toml` as a side effect.** Every projection persisted
  `--target` as `retirement.default_target_income`. Nothing ever read it — `--target` is
  required and has no default — and `Config::save` re-serialises the whole file, silently
  dropping comments. A target chosen against stale offline data was also recorded
  indistinguishably from one chosen against a live fetch.
- **`report rewards` misdiagnosed stale data as misconfiguration.** With no snapshots in the
  requested window it built a full ranking at 0.00 ICP, printed
  "No rewards earned (check configuration)" against every correctly-configured neuron, and
  exited 0. It now distinguishes an empty window, genuinely zero rewards with data present,
  and no reward history at all — and exits 1 on the first.
- **Configuration and database failures printed a raw `Os { code: 2, … }` struct**, naming
  neither the file nor a remedy. Both now report the path, the cause, and what to do.

### Added

- **`project --offline`.** Projects from the newest stored snapshot without touching the
  network. Without the flag a live query is attempted and falls back to stored data on
  failure rather than aborting. Previously no projection could be produced at all without a
  successful live fetch, however much history was stored.
- **Staleness banner.** Shown on every fallback and every `--offline` run, never suppressed.
  Reports which portfolio value was used, when it was observed and how old that is, and
  states the direction of the error — a stale portfolio understates holdings, so the
  projected date is *later* than reality, not earlier.
- **`retrieved_timestamp` is read back.** The column was written on every save but never
  selected, so a neuron loaded from the database was stamped with the current time. JSON
  export reported `retrieved_at` as "now" for snapshots months old, and `report summary`
  printed "Last Updated: <today>" over data collected in January. It is the only durable
  provenance marker in the schema — `created_at` is SQLite's `DEFAULT CURRENT_TIMESTAMP` and
  does not survive a rebuild.
- **Two-axis data quality.** Grading counted distinct reward dates and said nothing about
  recency, so three years of history that stopped updating still read "Excellent (180+ days)".
  Depth and freshness are now graded separately, the overall verdict is the worse of the two,
  and both are shown with the limiting axis named — thin history is fixed by waiting, stale
  history by running the tracker.
- **`report summary`** separates "Report Generated" from "Data Retrieved", with an age in days.
- **MIT `LICENSE` file.** The README had advertised MIT with a badge and a `§License` link
  since 2025 with no such file in the tree. `Cargo.toml` now also declares
  `license`, `description`, `authors` and `repository`.
- **`rust-version = "1.78"`** — a lower bound derived from the lockfile (v4 cannot be parsed
  below Cargo 1.78), not a tested MSRV. Verified working on 1.90.0.
- Retirement projection with pessimistic / realistic / optimistic scenarios, and `--compare`
  for what-if analysis across alternative targets.
- Multi-neuron portfolio tracking, identity management (`generate`, `verify`, `info`),
  CSV import and export, SQLite persistence with `refinery` migrations, and the
  `summary` / `history` / `rewards` / `neuron` reports in terminal, JSON and CSV.
- Interactive first-run setup wizard (`init`).

### Changed

- Tests grew from 49 to 89. `sqlite_repository.rs` and `main.rs`, previously untested,
  now have coverage — including the first tests in the repository to exercise a real
  on-disk SQLite file rather than a mock.
- Entity name corrected throughout to the registered **U Reflection Design & Build Inc.**

### Removed

- **Auto-save of target income to `config.toml`** — listed as a feature while unreleased,
  removed before release for the reasons under *Fixed*. `Config::save` and
  `update_retirement_target` remain in the tree but are unused.
- `docs/features/` (six specifications, 8,539 lines) — describing either work that had
  shipped or a CSV import format that was never implemented.
- `docs/strategy/` (2,998 lines) — removed; not product documentation.
- `docs/guides/cli-reference.md` (1,547 lines) — replaced by a 201-line
  [usage guide](docs/guides/usage.md) covering what `--help` cannot. `--help` is generated
  from the command definitions and cannot drift; a hand-maintained parallel copy had.

### Documentation

- `docs/` reduced from 18,025 lines to 5,076. Every relative link in the tree now resolves,
  apart from two in this file that deliberately point at removed documents.
- README rewritten: the claim of "event-sourced, immutable state tracking" was disproved and
  removed rather than softened, and replaced with an account of what the storage actually
  does. Quick Start now begins at `init` rather than a bare `cargo run`, which cannot work
  without a registered hot key.
- `domain-model.md` carries a dated header marking its NNS protocol constants superseded.
- `system-overview.md` corrected from four layers to three plus the CLI.
- `database-schema.md` documents the destructive-upsert write semantics and the role of
  `retrieved_timestamp`.

### Known limitations

Recorded plainly rather than left to be rediscovered:

- **`BonusMultiplier` still hardcodes the pre-2026 NNS curve** — an 8-year maximum dissolve
  delay with a linear 2.0x cap. The protocol changed in 2026 (2-year maximum, 3x cap,
  quadratic curve, 2-week voting eligibility floor). Nothing in this tool reads any protocol
  parameter from the governance canister; all are compiled in. See EXTERNAL 1 in
  [CLAUDE.md](CLAUDE.md).
- **Stored `age_bonus_multiplier` and `dissolve_bonus_multiplier` carry historical values
  from before the inversion fixes.** Rows imported prior to this release retain the
  365-day-basis figures; the corrections apply to new writes only, and no backfill was
  performed. No read path consumes these columns — both are recomputed on load — so the
  stored values affect only direct SQL queries.
- The `stake_e8s` CSV column accepts both raw e8s integers and decimal ICP, distinguished
  only by the presence of a decimal point: `100` is 0.000001 ICP, `100.0` is 100 ICP. No
  warning is issued. See the [usage guide](docs/guides/usage.md).
- `infrastructure/ic_client.rs` remains untested. Its Candid types are hand-declared with no
  `.did` file to check against, and testing requires a live network and a real identity.
- ADR-003 is marked Accepted but is not implemented; the tree is a single crate.

[Unreleased]: https://github.com/UReflection/icp-neuron-tracker/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/UReflection/icp-neuron-tracker/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/UReflection/icp-neuron-tracker/releases/tag/v0.1.0
