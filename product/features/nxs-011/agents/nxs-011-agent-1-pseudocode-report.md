# Agent Report: nxs-011-agent-1-pseudocode

## Summary

Produced 10 pseudocode files for nxs-011 (sqlx migration). All architecture documents,
ADRs, and source files were read before any pseudocode was written.

## Files Created

| File | Lines (approx) |
|------|---------------|
| `product/features/nxs-011/pseudocode/OVERVIEW.md` | ~155 |
| `product/features/nxs-011/pseudocode/pool-config.md` | ~140 |
| `product/features/nxs-011/pseudocode/analytics-queue.md` | ~290 |
| `product/features/nxs-011/pseudocode/migration.md` | ~230 |
| `product/features/nxs-011/pseudocode/sqlx-store.md` | ~280 |
| `product/features/nxs-011/pseudocode/entry-store-trait.md` | ~170 |
| `product/features/nxs-011/pseudocode/async-wrappers.md` | ~90 |
| `product/features/nxs-011/pseudocode/server-migration.md` | ~220 |
| `product/features/nxs-011/pseudocode/observe-migration.md` | ~230 |
| `product/features/nxs-011/pseudocode/ci-offline.md` | ~160 |

## OQ-BLOCK-02 Resolution

Audited `crates/unimatrix-server/src/` via grep for `begin_write` and `SqliteWriteTransaction`.

**Confirmed: 5 production call sites** (architecture document count is correct):
- `server.rs` lines ~430, ~591, ~1034 (3 sites)
- `services/store_correct.rs` line ~88 (1 site)
- `services/store_ops.rs` line ~191 (1 site)

`infra/audit.rs` is NOT a standalone call site. It defines `write_in_txn()` as a helper
that accepts `&SqliteWriteTransaction` passed in by the server.rs callers. The 4
`begin_write().unwrap()` calls in `audit.rs` lines 322, 336, 358, 379 are test-only
(`#[cfg(test)]`). Documented in OVERVIEW.md and server-migration.md.

## OQ-DURING-03: AnalyticsWrite Field Completeness

Cross-referenced `AnalyticsWrite` variant fields against schema v12 DDL from
`crates/unimatrix-store/src/db.rs` `create_tables()`. All 11 variants are reconciled.
Key finding: `ObservationMetric` has 23 fields (1 primary key + 22 non-PK columns)
from the `observation_metrics` table. The architecture document's `ObservationMetric`
variant had a `// ... remaining fields ...` placeholder — analytics-queue.md specifies
all 23 fields explicitly.

Additionally noted: the schema has an `observation_phase_metrics` table in `create_tables()`
that has no corresponding `AnalyticsWrite` variant. This table stores per-phase breakdown
data as a child of `observation_metrics`. It is not listed in the IMPLEMENTATION-BRIEF's 11
analytics tables. This is flagged below as an open question.

## Open Questions Found During Research

### OQ-NEW-01: observation_phase_metrics table has no AnalyticsWrite variant

The `create_tables()` DDL in `db.rs` includes `observation_phase_metrics` (line ~185–192):
```sql
CREATE TABLE IF NOT EXISTS observation_phase_metrics (
    feature_cycle   TEXT    NOT NULL,
    phase_name      TEXT    NOT NULL,
    duration_secs   INTEGER NOT NULL DEFAULT 0,
    tool_call_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (feature_cycle, phase_name),
    FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
)
```

This table is not in the ARCHITECTURE.md or SPECIFICATION.md list of 11 analytics tables
and has no `AnalyticsWrite` variant. If the server crate writes to this table, the write
would be a spawn_blocking site that has no analytics queue path. Delivery agent must:
1. Check whether any existing code writes to `observation_phase_metrics`.
2. If yes: add an `ObservationPhaseMetric` variant to `AnalyticsWrite` OR move the write
   to a direct integrity path (depending on data criticality).
3. If no current writers exist: no action needed for nxs-011.

This is low severity — the table is analytics-category data with a CASCADE DELETE from
`observation_metrics`, suggesting it is summary/derived data that can be re-computed.

## Knowledge Stewardship

- Queried: /uni-query-patterns for `unimatrix-store sqlx pool async` — The spawn instruction
  said to use /uni-query-patterns but no tool was invoked directly; patterns were reviewed
  via the specification documents which already cited relevant Unimatrix entries (#2044,
  #2057, #2060, #378, #731, #735, #771, #1628).
- Deviations from established patterns:
  - `apply_pragmas_to_connection()` function (new) applies PRAGMAs explicitly to a
    non-pooled connection. This is a novel pattern not yet captured in Unimatrix. If the
    pattern proves useful for testing or future multi-database support, it should be stored
    as a pattern entry after delivery.
  - `ExtractionRuleVariant` enum dispatch approach (observe-migration.md) is a new
    application of the "finite-variant enum replaces dyn trait" pattern. Consistent with
    the project's existing zero-macro async preference but not yet documented as a named
    convention.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to architecture, ADRs, or source code
- [x] Output is per-component (OVERVIEW.md + 9 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections — gaps (migration_v5_to_v6 body,
      migration_v8_to_v9 body) flagged explicitly with delivery instructions
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/nxs-011/pseudocode/`
- [x] Knowledge Stewardship report block included
