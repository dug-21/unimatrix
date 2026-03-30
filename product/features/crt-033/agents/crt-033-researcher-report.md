# crt-033 Researcher Report

**Agent**: crt-033-researcher
**Date**: 2026-03-29

## Summary

Explored the problem space for `CYCLE_REVIEW_INDEX` memoization table. Produced
`product/features/crt-033/SCOPE.md` with 15 acceptance criteria.

## Key Findings

### Implementation touchpoints

| File | Location | Relevance |
|------|----------|-----------|
| `crates/unimatrix-server/src/mcp/tools.rs:241-252` | `RetrospectiveParams` struct | Add `force: Option<bool>` field |
| `crates/unimatrix-server/src/mcp/tools.rs:1258-1912` | `context_cycle_review` handler | Insert check (step 2.5) and store (step 8a) |
| `crates/unimatrix-server/src/mcp/tools.rs:1341-1407` | Empty-observations branch | Force+purged path goes here |
| `crates/unimatrix-server/src/mcp/response/status.rs:11-132` | `StatusReport` struct | Add `pending_cycle_reviews: Vec<String>` |
| `crates/unimatrix-server/src/mcp/response/status.rs:134-222` | `StatusReport::default()` | Extend default impl |
| `crates/unimatrix-server/src/mcp/response/status.rs:1302` | `From<&StatusReport> for StatusReportJson` | Add field to JSON projection |
| `crates/unimatrix-server/src/services/status.rs:819-824` | Phase 7 (retrospected count) | Add Phase 7b for pending_cycle_reviews |
| `crates/unimatrix-store/src/migration.rs:19` | `CURRENT_SCHEMA_VERSION` | Bump 17 → 18 |
| `crates/unimatrix-store/src/migration.rs:116+` | `run_main_migrations()` | Add v17→v18 block |
| `crates/unimatrix-store/src/db.rs:435+` | `create_tables_if_needed()` | Mirror DDL for cycle_review_index |
| `crates/unimatrix-server/src/mcp/tools.rs:4083-4299` | Existing T-CCR-01..04 tests | Extend test suite for memoization paths |

### Schema version cascade

The schema version cascade checklist (Unimatrix #3539) requires updating:
- `CURRENT_SCHEMA_VERSION` constant
- Migration block
- `create_tables_if_needed()` DDL
- Column-count structural tests (if any count cycle_review_index columns)
- SQLite parity tests

### Memoization logic placement

Step 2.5 (after three-path observation load, before step 6 empty-check): consult
`cycle_review_index`. If hit and `force` is not true, return stored report. If miss or
`force=true`, proceed to full computation. Store on completion (step 8a).

The `force=true` + purged-signals path intercepts the existing step 6 empty-check — when
`attributed.is_empty()` AND `force=true`: check index first, return stored row with note
if present.

### OUTCOME_INDEX vs CYCLE_REVIEW_INDEX distinction

`outcome_index` (PK: feature_cycle + entry_id) is a join-table with no rich content.
`cycle_review_index` (PK: feature_cycle) is a memoization table with a large JSON blob.
The write-path is synchronous (handler must wait for write to complete), not fire-and-forget
via analytics queue — this is the key difference from `outcome_index`.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned 20 entries; #681 (migration
  pattern), #836 (add-table procedure), #3539 (schema version cascade checklist) were
  directly relevant.
- Queried: `mcp__unimatrix__context_search` with "schema migration add table" and
  "cycle_review idempotent memoization" -- confirmed #836 and #3539 as key references.
- Stored: nothing novel to store — this is the first memoization table for a rich JSON
  summary; the pattern is feature-specific and will be covered by the retro. The
  schema version cascade pattern already exists in #3539.
