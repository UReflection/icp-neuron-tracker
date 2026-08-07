# Provenance marker confirmed — 2026-08-05

Verification gate for the 365/365.25 constant fix. Run against a **copy** of the
production database; the original was never opened (md5 `513151ba…`, mtime
`1767816305`, unchanged before and after). Copies deleted after the run.

## What was tested

`retrieved_timestamp` is now read back through `SqliteRepository::row_to_neuron`
(commit `a2c712d`). The question this gate answers: does the durable column
partition the data identically to the two markers that are about to become
unreliable?

## Result — identical, zero disagreements

| Marker | Bulk import | Automated | Durable? |
|---|---|---|---|
| `retrieved_timestamp` = 1762487131 | **4,408** | **252** | **Yes** |
| `dissolve_bonus_multiplier` < 2.0 | 4,408 | 252 | No — depends on the 365/365.25 bug |
| `created_at` ∈ {03:45:31, 03:45:32} | 4,408 | 252 | No — SQLite `DEFAULT CURRENT_TIMESTAMP` |

Cross-tabulated, only two combinations occur across all 4,660 rows:

```
retrieved=bulk  bonus=bulk  created=bulk  ->  4408
retrieved=auto  bonus=auto  created=auto  ->   252
```

Rows where the durable marker disagrees with either corroborator: **0**.

## Read path verified on both partitions

Bulk-era snapshots, via `report summary --format json`:

```
neuron 10000000000000000002  retrieved_at=2025-11-07T03:45:31Z
neuron 10000000000000000001  retrieved_at=2025-11-07T03:45:31Z
neuron 100000000000000004    retrieved_at=2025-11-07T03:45:31Z
neuron 1000000000000000003   retrieved_at=2025-11-07T03:45:31Z
```

Automated snapshots (current latest), same command:

```
neuron 10000000000000000002  retrieved_at=2026-01-07T20:05:03Z
neuron 10000000000000000001  retrieved_at=2026-01-07T20:05:00Z
neuron 100000000000000004    retrieved_at=2026-01-07T20:05:01Z
neuron 1000000000000000003   retrieved_at=2026-01-07T20:05:05Z
```

Both previously reported the report-generation time. The per-neuron stagger on
the automated side (`:00 :01 :03 :05`) is the sequential chain fetch, and is
exactly the signal a CSV import cannot produce.

Terminal staleness line, bulk-era data:

```
Report Generated: 2026-08-05 19:18:18 UTC
Data Retrieved:   2025-11-07 03:45:31 UTC (271 days ago)
```

## Conclusion

**The gate passes.** Provenance no longer depends on the arithmetic bug. The
365/365.25 constant fix in `csv_parser.rs::calculate_age_from_bonus`
(`FOUR_YEARS_SECONDS`) may proceed without destroying the 4,408/252 split.

Still HELD pending an explicit decision — this record authorises nothing.

## Not settled here

Correcting the constant changes stored `age_bonus_multiplier` and
`dissolve_bonus_multiplier` for future writes only. The 4,408 existing rows keep
their current values unless backfilled. Whether to backfill, recompute on read,
or drop the columns is a separate proposal.
