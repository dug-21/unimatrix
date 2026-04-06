# 491-agent-1-fix — Implementation Report

## Files Modified

- `crates/unimatrix-store/src/read.rs` — SQL filter replaced with `NOT IN (EDGE_SOURCE_CO_ACCESS, '')`
  via `format!()`. Struct doc updated with full semantic contract (exclusive filter intent, empty-string
  guard is defensive, co_access exclusion rationale).
- `crates/unimatrix-server/src/mcp/response/status.rs` — field doc updated to remove NLI-only framing;
  label updated from `"Inferred (NLI) edges: {}"` to `"Inferred edges: {}"`.
- `crates/unimatrix-server/src/services/nli_detection_tick.rs` — TC-15 rewritten as table-driven test
  covering nli, cosine_supports, S1, S2, S8, behavioral (all counted) and co_access (excluded).

## New Tests

- `nli_detection_tick::tests::test_inferred_edge_count_table_driven` (TC-15 rewrite)

## Tests

4,540 passed, 0 failed (full workspace).

## Issues

None.

## Knowledge Stewardship

Queried:
- `context_briefing` (task-scoped) — returned entries #4167 (lesson: root cause pattern), #3591
  (col-029 ADR-001: EDGE_SOURCE constant mandate), #4056 (write_graph_edge pattern), #4046
  (EDGE_SOURCE test types pattern). Used to confirm constant-over-literal requirement and TC-15 scope.

Stored:
- Entry #4168: Reusable pattern for SQL exclusive-filter approach (`source NOT IN (EDGE_SOURCE_CO_ACCESS, '')`)
  with rationale and format!() usage — stored for future implementers extending graph metrics.
