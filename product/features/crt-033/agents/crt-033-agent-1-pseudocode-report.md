# Agent Report: crt-033-agent-1-pseudocode

## Task

Produce per-component pseudocode for crt-033 (CYCLE_REVIEW_INDEX memoization).
Five components: cycle_review_index store module, migration v17→v18 cascade,
tools.rs handler modifications, status.rs response modifications, services/status.rs
Phase 7b.

## Output Files

- `product/features/crt-033/pseudocode/OVERVIEW.md`
- `product/features/crt-033/pseudocode/cycle_review_index.md`
- `product/features/crt-033/pseudocode/migration.md`
- `product/features/crt-033/pseudocode/tools_handler.md`
- `product/features/crt-033/pseudocode/status_response.md`
- `product/features/crt-033/pseudocode/status_service.md`

## Components Covered

1. `cycle_review_index.rs` (new store module): `CycleReviewRecord`, `SUMMARY_SCHEMA_VERSION`,
   `get_cycle_review`, `store_cycle_review`, `pending_cycle_reviews`
2. Migration v17→v18: all 7 cascade touchpoints documented with exact DDL
3. `tools.rs` handler: `RetrospectiveParams.force`, step 2.5, step 8a,
   `handle_memoization_hit`, `handle_purged_signals_hit`, `build_cycle_review_record`,
   `dispatch_format_with_advisory`
4. `status.rs` response: `StatusReport.pending_cycle_reviews`, `Default`, `StatusReportJson`,
   summary/markdown/JSON formatters
5. `services/status.rs` Phase 7b: `PENDING_REVIEWS_K_WINDOW_SECS`, `compute_report()` insertion

## Self-Check Results

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections — gaps flagged explicitly
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/crt-033/pseudocode/`
- [x] Knowledge Stewardship report block included

## Open Questions / Flags for Implementation Agent

### OQ-A: MemoizationDeserError variant
`handle_memoization_hit` needs to signal "fall through" to the handler when
`serde_json::from_str` fails. Two options:
- Option 1: New `ServerError::MemoizationDeserError(String)` variant in
  `crates/unimatrix-server/src/error.rs`
- Option 2: Return `Result<Option<CallToolResult>, ServerError>` where `None` = fall through

The tools_handler.md documents Option 1. Implementation agent should choose
based on existing error convention in tools.rs and error.rs.

### OQ-B: StatusReportJson construction
The status_response.md notes that `StatusReportJson` may be constructed via a
`From<&StatusReport>` impl or inline in the `Json` match arm. The existing code
at line ~789 shows `StatusReportJson::from(report)` — implementation agent must
verify whether a `From` impl exists and add the field mapping in the correct location.

### OQ-C: format_unix_timestamp in handle_purged_signals_hit
`handle_purged_signals_hit` uses a `format_unix_timestamp(record.computed_at)` call.
The `mcp/response/mod.rs` exports `format_timestamp` — verify this function
accepts `u64` or `i64` and import from the correct location.

### OQ-D: append_text_to_tool_result helper
`dispatch_format_with_advisory` needs to append advisory text to a `CallToolResult`.
Check `mcp/response/mod.rs` for existing helpers before adding a new function.
The pattern may already exist for other advisory messages in the codebase.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 16 entries returned. Most relevant:
  entries #3793/#3794/#3795/#3796 (all crt-033 ADRs confirmed — decisions match
  the architecture and spec exactly); entry #2152/#2270 (dual-pool pattern — confirms
  `read_pool` for reads, `write_pool_server` for integrity writes — applied to pool
  selection in all three store functions); entry #3619 referenced in ADR-004 and
  RISK-TEST-STRATEGY (read_pool for status aggregates — applied to `pending_cycle_reviews`).
- Queried: `context_search(sqlx store module patterns)` — found entries #2147–#2153
  (dual-pool WAL architecture patterns). Confirmed: `read_pool()` for SELECT,
  `write_pool_server()` for INSERT/UPDATE, never `spawn_blocking` for async sqlx.
- Deviations from established patterns: none. All store methods follow the
  `observations.rs` / `sessions.rs` pattern exactly. Migration follows the
  `v16→v17` block pattern. Status service follows the Phase 7 → Phase 7b insertion
  pattern. tools.rs helper extraction follows the 500-line guideline.
