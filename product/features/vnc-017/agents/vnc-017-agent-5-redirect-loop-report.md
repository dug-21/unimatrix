# Agent Report: vnc-017-agent-5-redirect-loop

## Task

Implement the `redirect_loop` component — insert the auto-redirect block into the `context_correct` handler in `tools.rs` as step 8c (after Phase B edge writes, before confidence recompute). Wire `format_redirect_summary` into the response. Add 11 unit tests per the test plan.

## Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs`

## What Was Implemented

### 1. `REDIRECT_CEILING` constant (line ~44)

`pub(super) const REDIRECT_CEILING: usize = 50` — placed before the first struct definition per pseudocode.

### 2. `RedirectSummary` struct and `run_redirect_loop` helper (lines ~4400-4580)

Module-level `pub(super)` struct and async function extracted from the handler for testability. The function:

- Calls `store.query_incoming_edges(original_id)` — returns `None` on SQL error (logs warn, correction already committed)
- Returns `None` for zero-edge case (AC-11)
- Applies ceiling check at `REDIRECT_CEILING=50` with `tracing::warn!` when exceeded (ADR-004)
- Per-edge loop: validates source status (skip Quarantined/Deprecated as `skipped++`, not `failed++`), calls `redirect_graph_edge` (ADR-003 warn+continue), accumulates `RedirectSummary`
- Emits `tracing::info!` summary after loop (only when `found > 0`)

### 3. Step 8c insertion in `context_correct` handler (lines ~1085-1127)

After Phase B `validate_and_write_edges`, before step 9 confidence recompute:
- Calls `run_redirect_loop(&self.entry_store, original_id, correct_result.corrected_entry.id)`
- No `find_terminal_active`, no `TypedGraphState` lock (ADR-001)

### 4. Response append (step 10 modification)

`format_correct_success` result mutated post-call: when `redirect_summary` is `Some` and `format_redirect_summary(...)` returns `Some(text)`, the text is appended to `result.content[0].raw.text` via `rmcp::model::RawContent::Text` match. No change to `format_correct_success` signature.

### 5. Import: `format_redirect_summary` added to the `use crate::mcp::response::{}` block.

## Tests

11 tests in `#[cfg(test)] mod redirect_loop_tests` at module scope in `tools.rs`:

| Test name | AC/R covered |
|-----------|--------------|
| `test_redirect_loop_no_incoming_edges_returns_none` | AC-11 |
| `test_redirect_loop_end_to_end_moves_edge_to_new_target` | AC-14 |
| `test_redirect_loop_quarantined_source_skipped_not_failed` | AC-08 |
| `test_redirect_loop_deprecated_source_skipped_not_failed` | R-06 |
| `test_redirect_loop_unique_conflict_counts_as_success` | AC-09 / R-09 |
| `test_redirect_loop_mixed_status_redirects_valid_skips_invalid` | R-06 mixed |
| `test_redirect_loop_ceiling_truncates_at_50_and_warns` | R-05 (55 edges) |
| `test_redirect_loop_exactly_at_ceiling_no_truncation` | R-05 variant (50 edges) |
| `test_redirect_loop_idempotent_with_pre_existing_edge` | R-10 |
| `test_redirect_ceiling_constant_is_50` | R-01 structural |
| `test_redirect_loop_targets_new_entry_not_chain_traversal` | AC-03 |

All tests use `store.correct_entry()` with dummy `data_id=0, embedding_dim=0` — no ONNX dependency.

## Test Results

- `cargo test -p unimatrix-server test_redirect` — **11 passed, 0 failed**
- `cargo test --workspace` — **all pass, 0 new failures**

## Issues / Blockers

None. One structural gotcha encountered (documented in Knowledge Stewardship below).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-003 (#4462), ADR-004 (#4463), source-validation pattern (#4459), and the vnc-017 stale-edge pattern (#4458). All confirmed architecture alignment.
- Stored: entry #4467 "pub(super) items in tools.rs are not reachable via `use super::*` from child test modules" via `/uni-store-pattern`

  **Root cause of gotcha**: `tools.rs` has multiple stacked `#[cfg(test)]` modules. The last visible one (`vnc014_audit_field_tests`) closes at line ~8970, not at EOF. Inserting test code after the TOOL-U-10 comment placed it inside `vnc014_audit_field_tests`, not at module scope. Additionally, `pub(super)` items are not included in `use super::*` glob imports — explicit `use crate::mcp::tools::{}` in a dedicated module is required.
