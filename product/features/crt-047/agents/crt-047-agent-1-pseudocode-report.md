# Agent Report: crt-047-agent-1-pseudocode

## Task

Produce per-component pseudocode for crt-047 (Curation Health Metrics) covering
five components: cycle_review_index, migration v23→v24, services/curation_health,
context_cycle_review handler extension, and context_status Phase 7c.

## Output Files

| File | Component |
|------|-----------|
| `product/features/crt-047/pseudocode/OVERVIEW.md` | Component map, data flow, shared types, sequencing |
| `product/features/crt-047/pseudocode/cycle_review_index.md` | Schema/store layer — 7 new fields, two-step upsert, baseline window reader |
| `product/features/crt-047/pseudocode/migration.md` | v23→v24 migration block, db.rs DDL update, cascade test list |
| `product/features/crt-047/pseudocode/curation_health.md` | All types, constants, 5 functions (1 async + 4 pure) |
| `product/features/crt-047/pseudocode/context_cycle_review.md` | Step 8a extension: snapshot compute → store → baseline |
| `product/features/crt-047/pseudocode/context_status_phase7c.md` | ~15-line Phase 7c: baseline window read + compute_curation_summary |

## Components Covered

1. `schema/cycle_review_index` — `CycleReviewRecord` extended; two-step upsert replacing
   plain `INSERT OR REPLACE`; `CurationBaselineRow`; `get_curation_baseline_window()`;
   `SUMMARY_SCHEMA_VERSION` bumped to 2.
2. `migration v23→v24` — seven-column `pragma_table_info` pre-check-all-then-alter pattern;
   `CURRENT_SCHEMA_VERSION` bumped to 24; `db.rs` DDL update; cascade test list.
3. `services/curation_health` — all six structs, one enum, three constants, one async function
   (`compute_curation_snapshot`), four pure functions.
4. `context_cycle_review handler` — Step 8a-pre (snapshot compute), updated
   `build_cycle_review_record` signature, Step 8a-post (baseline comparison),
   `RetrospectiveReport.curation_health` population.
5. `context_status Phase 7c` — `CURATION_BASELINE_WINDOW` constant; Phase 7c block;
   `StatusReport.curation_health` field.

## Open Questions / Gaps Found

**OQ-1 (context_cycle_review, minor)**: The handler's existing `cycle_events_vec`
structure type was not fully inspected. `extract_cycle_start_ts` pseudocode assumes
the vec stores rows with `event_type` and `timestamp` fields. If the actual type
differs (e.g., it is a `Vec<String>` of cycle IDs only), the implementor should fall
back to a separate scalar SQL query:
```sql
SELECT MIN(timestamp) FROM cycle_events
WHERE cycle_id = ?1 AND event_type = 'cycle_start'
```
This is a single read_pool() call; acceptable overhead.

**OQ-2 (context_cycle_review, design choice)**: The pseudocode reads the baseline
window AFTER `store_cycle_review()`, meaning the just-written row for the current
cycle may be included in the baseline comparison if `first_computed_at > 0`. The
architecture diagram shows the same ordering. The implementor should document this
in a comment at the call site for clarity, but no behavior change is required.

**OQ-3 (cascade)**: The implementation brief lists `response/cycle_review.rs` and
`response/status.rs` as needing `curation_health` fields. The exact serde
annotations (e.g., `#[serde(skip_serializing_if = "Option::is_none")]`) and Display
formatting for sigma values (e.g., `"2.1σ (4 cycles of history)"`) are not specified
in pseudocode — implementation agent should follow the existing pattern for optional
fields in the response types and format the sigma annotation as a string field within
`CurationBaselineComparison`.

No blocking gaps. All ADR decisions are encoded in the pseudocode with explicit
comments where the "obvious" approach would be wrong (e.g., INSERT OR REPLACE
clobbering first_computed_at, corrections_system excluded from total).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "schema migration cycle_review_index patterns"
  — returned ADR #4182 (crt-047 ADR-004 migration atomicity), #4088 (crt-043 outer transaction),
  #3794 (crt-033 SUMMARY_SCHEMA_VERSION), #760 (independent migration versioning). All directly
  applicable to the migration block structure.
- Queried: `mcp__unimatrix__context_search` for "crt-047 architectural decisions"
  — returned ADR #4179 (ADR-001 first_computed_at), #4180 (ADR-002 trust_source), #4182 (ADR-004).
  Confirmed ADRs are stored in Unimatrix and match the ADR files on disk.
- Read: all five ADR files, ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md,
  IMPLEMENTATION-BRIEF.md, existing `cycle_review_index.rs`, `migration.rs` (full),
  `status.rs` (Phase 7 section), `services/mod.rs`, `tools.rs` (Step 8a and cycle_review sections).
- Deviations from established patterns: none. The two-step upsert for `first_computed_at`
  is a novel pattern for this codebase, but it is explicitly mandated by ADR-001 and
  documented in the pseudocode with anti-"fix" comments.
