# Agent Report: crt-047-agent-1-architect

## Deliverables

- `product/features/crt-047/architecture/ARCHITECTURE.md`
- `product/features/crt-047/architecture/ADR-001-baseline-ordering-key.md`
- `product/features/crt-047/architecture/ADR-002-trust-source-bucketing.md`
- `product/features/crt-047/architecture/ADR-003-orphan-deprecation-attribution.md`
- `product/features/crt-047/architecture/ADR-004-migration-strategy.md`
- `product/features/crt-047/architecture/ADR-005-curation-health-module-extraction.md`

## Unimatrix Entry IDs

| ADR | Entry ID |
|-----|---------|
| ADR-001: Baseline ordering key (first_computed_at) | #4179 |
| ADR-002: trust_source bucketing | #4180 |
| ADR-003: Orphan attribution (ENTRIES-based) | #4181 |
| ADR-004: Migration strategy v23→v24 | #4182 |
| ADR-005: services/curation_health.rs extraction | #4183 |

## Key Findings from Source Analysis

### SR-01 Resolved (AUDIT_LOG operation strings)

Three deprecation write paths verified:

1. `context_deprecate` (tools.rs line ~901): logs `operation: "context_deprecate"` in AUDIT_LOG.
2. `context_correct` (store_correct.rs): the chain-deprecation of the original entry is done inside `correct_entry()` via direct SQL UPDATE — NO separate `"context_deprecate"` AUDIT_LOG row. The audit event is `"context_correct"` with `target_ids: [original_id, new_id]`. Critically: `correct_entry()` always sets `superseded_by = new_id`, so chain-deprecation via correction CANNOT produce orphans.
3. Lesson-learned auto-supersede (tools.rs ~line 2947): calls `store.update_status()` directly — NO AUDIT_LOG row at all. Also always sets `superseded_by`, so cannot produce orphans.

Conclusion: AUDIT_LOG join is unnecessary. All orphan deprecations originate from explicit `context_deprecate`. ENTRIES-based query using `updated_at` within cycle window is complete and correct.

### SR-07 Resolved (baseline ordering key)

Both `computed_at` and SQLite `rowid` are mutable under INSERT OR REPLACE. The decision adds `first_computed_at` as a new column — set once on initial write, never overwritten. This adds one column to the v24 migration (seven total, not five).

### Schema v24 Column Count

The SCOPE.md proposed five new columns. The architecture adds two more:
- `corrections_system` (ADR-002): informational bucket for system/direct trust_source values
- `first_computed_at` (ADR-001): stable ordering key for baseline window

Total: **seven new columns** in the v24 migration block.

## Open Questions

None.
