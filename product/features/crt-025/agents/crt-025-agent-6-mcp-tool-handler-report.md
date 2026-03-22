# Agent Report: crt-025-agent-6-mcp-tool-handler

## Task

Component 2 (Wave 4): MCP Tool Handler (`mcp/tools.rs` + `mcp/response/retrospective.rs`)

## Status: COMPLETE

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/response/retrospective.rs`

## Changes Made

### 1. `CycleParams` (already correct on arrival)

On inspection, `CycleParams` already had `phase`, `outcome`, `next_phase` fields and `context_cycle` already called `validate_cycle_params` with the new signature. These were completed by a prior Wave 2/3 agent. No changes needed here.

### 2. `context_cycle_review` phase narrative assembly (`tools.rs`)

Added step `10g` after the existing col-020 session data block and before audit:

- **Query 1**: `SELECT seq, event_type, phase, outcome, next_phase, timestamp FROM cycle_events WHERE cycle_id = ? ORDER BY timestamp ASC, seq ASC`
- **Query 2**: `SELECT fe.phase, e.category, COUNT(*) AS cnt FROM feature_entries JOIN entries ... WHERE fe.feature_id = ? AND fe.phase IS NOT NULL GROUP BY fe.phase, e.category`
- **Query 3**: Cross-feature baseline query (excludes current feature, only features with phase data), grouped by `feature_id, phase, category`

Assembly:
- Query 1 empty → `phase_narrative` remains `None` (AC-12/13)
- Cross dist keyed by `feature_id → PhaseCategoryDist` for `sample_features` computation
- Calls `unimatrix_observe::build_phase_narrative(events, current_dist, cross_dist)` (pure function from Wave 1)
- Sets `report.phase_narrative = Some(narrative)`
- Query 2/3 failures are logged and fall back to empty `HashMap` (non-fatal)

### 3. Phase narrative markdown rendering (`retrospective.rs`)

Added import for `PhaseCategoryComparison` and `PhaseNarrative`. Added section 10 call in `format_retrospective_markdown`:

```rust
if let Some(narrative) = &report.phase_narrative {
    output.push_str(&render_phase_narrative(narrative));
}
```

New functions:
- `render_phase_narrative(narrative: &PhaseNarrative) -> String`: renders `## Phase Narrative` header, phase sequence (arrow-joined), rework flags (when non-empty), per-phase category distribution table
- `render_cross_cycle_table(out: &mut String, comparisons: &[PhaseCategoryComparison])`: renders `### Cross-Cycle Comparison` markdown table when `cross_cycle_comparison` is `Some`

AC-13 compliance: when `phase_narrative` is `None`, the guard prevents any section rendering — no placeholder, no empty header.
AC-14 compliance: `render_cross_cycle_table` is only called when `cross_cycle_comparison` is `Some` (controlled by `build_phase_narrative` threshold of ≥ 2 prior features).

## Tests

### New unit tests (retrospective.rs)
8 tests added to `mcp::response::retrospective::tests`:

| Test | Coverage |
|------|---------|
| `test_render_phase_narrative_none_omits_section` | AC-13: no section when None |
| `test_render_phase_narrative_some_emits_section` | section appears when Some |
| `test_render_phase_narrative_phase_sequence_rendered` | arrow-joined sequence |
| `test_render_phase_narrative_rework_phases_rendered` | rework flag line |
| `test_render_phase_narrative_no_rework_omits_rework_line` | no rework line when empty |
| `test_render_phase_narrative_per_phase_categories` | category counts in section |
| `test_render_phase_narrative_cross_cycle_absent_when_none` | AC-14: table absent |
| `test_render_phase_narrative_cross_cycle_table_rendered` | table content correct |
| `test_render_phase_narrative_empty_sequence_no_crash` | graceful empty sequence |

### Existing tests (tools.rs, already present from prior agents)
`CycleParams` deserialization tests (11 tests) already covered all test plan items including `test_cycle_params_keywords_silently_discarded`.

## Test Results

- `cargo build --workspace`: PASS (zero errors, pre-existing warnings only)
- `cargo test --workspace`: PASS (all suites, zero failures)
- New phase narrative tests: 21 passed (10 CycleParams + 9 render_phase_narrative + 2 existing)

## Commit

`impl(mcp-tool-handler): phase narrative assembly in context_cycle_review and markdown rendering (#330)`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found dual-path validation pattern (#1265) and spawn_blocking_with_timeout pattern (#1367), both consistent with this implementation.
- Stored: entry via `/uni-store-pattern` — see below for details.

### Pattern stored

"SQLx row hydration in context_cycle_review: use `try_get` not direct field access; `write_pool_server()` is the correct pool for read queries in MCP handlers."
