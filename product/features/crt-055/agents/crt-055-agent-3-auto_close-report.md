# Agent Report — crt-055 Component 8: auto_close handler arm (#593)

**Agent**: crt-055-agent-3-auto_close
**Component**: 8 — `auto_close` handler arm (ADR-010 #5045)
**Crate**: unimatrix-server

## Summary

Added `auto_close: bool` (default `false`) to `RetrospectiveParams` and a self-contained
`maybe_auto_close()` helper invoked as the FIRST statement inside the full-pipeline block
(`if memo_hit.is_none() {`), before the pipeline reads the `cycle_events` timeline at
step 10g (rank-1 phase reckoning, line ~2649). When `auto_close == true` and the cycle
has no `cycle_stop`, it writes one synchronously via the EXISTING `cycle_events` writer
(`store.insert_cycle_event`) — NOT a second `store_cycle_review`. Idempotent (no-op if a
`cycle_stop` exists). `auto_close == false` leaves the timeline as-is so an open final phase
surfaces as #556 never-closed downstream (fail-loud).

## Files modified

1. `crates/unimatrix-server/src/mcp/tools.rs`
   - `RetrospectiveParams`: added `#[serde(default)] pub auto_close: bool` (~:399).
   - `context_cycle_review` handler: `maybe_auto_close(&store, &feature_cycle, params.auto_close).await;`
     as the first statement in the `if memo_hit.is_none()` block, with a marked comment for
     Component 9 (review pipeline ordering) integration. NOT on memo-hit / cached-MetricVector
     / no-data returns.
   - New module-level `async fn maybe_auto_close(...)` helper (idempotency check via
     `write_pool_server()`, seq = `COALESCE(MAX(seq), -1) + 1`, write via `insert_cycle_event`,
     non-fatal error handling per ADR-010).
   - 6 unit/handler tests in `cycle_review_integration_tests` (reused existing `open_store`).
2. `crates/unimatrix-server/src/infra/validation.rs`
   - Added `auto_close: false` to 3 existing `RetrospectiveParams` test fixtures (E0063 fix).

## Tests

`cargo test -p unimatrix-server --lib auto_close` → **6 passed; 0 failed**.

- `test_auto_close_true_no_stop_writes_before_pipeline` (AC-15a)
- `test_auto_close_true_stop_exists_idempotent` (AC-15b)
- `test_auto_close_false_no_stop_written` (AC-15c)
- `test_auto_close_default_is_false`
- `test_auto_close_writes_via_event_writer_not_store_cycle_review` (R-14/AC-17 — no
  `cycle_review_index` row written; single-writer invariant holds)
- `test_auto_close_true_empty_cycle_writes_seq_zero` (edge: no phases → no error)

`cargo build -p unimatrix-server` clean. `cargo fmt` applied. No new clippy warnings on the
added code (pre-existing collapsible-if hits at tools.rs:3250 are unrelated).

Integration (MCP harness / pytest) tests from the test plan were NOT run/modified per scope.

## Issues / blockers

- **Transient cross-crate**: during one test run, `unimatrix-observe` (Component 3,
  parallel/earlier wave) failed to compile — `cycle_aggregates.rs:257` references
  `phase.phase_total_duration_secs`, a field not yet on the `reckon_phase_aggregates`
  return type. This is in-flight Component 3 working-tree state, NOT in my scope or files.
  It self-resolved (observe lib built clean on the subsequent run) and my 6 tests then passed.
  Flagging so the Delivery Leader confirms Component 3 lands a compiling observe before the
  gate `--workspace` run.

## Scope discipline

Did not reorder or rewrite the broader review pipeline (Component 9). The auto_close call
site is a single marked line at the top of the full-pipeline block; a comment block marks
the integration point and the invariant (must precede the step-10g cycle_events read).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) + context_get(#5045 ADR-010)
  — confirmed: write `cycle_stop` via existing `insert_cycle_event` (8-arg signature), TOP of
  full-pipeline block before rank-1, idempotent, writes `cycle_events` (not `cycle_review_index`),
  informs-not-controls.
- Stored: nothing novel to store — the implementation followed ADR-010 directly; the
  `insert_cycle_event` 8-arg signature and `COALESCE(MAX(seq),-1)+1` advisory-seq convention are
  already documented (pattern #3383 / db.rs doc comments). No new runtime-invisible gotcha surfaced.
