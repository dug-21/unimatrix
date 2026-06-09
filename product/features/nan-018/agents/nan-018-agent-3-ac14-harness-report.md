# Agent Report — nan-018 AC-14 Capstone Integrator

**Agent:** nan-018-agent-3-ac14-harness
**Scope:** Wave-1 EXIT GATE — wire 9 components into the proof-by-use sweep; make the swept penalty lever LIVE (eval path only).

## Files Modified / Created
- `crates/unimatrix-server/src/services/mod.rs` — `with_rate_config` gains `graph_penalty_params`; `new()` passes `default()`.
- `crates/unimatrix-server/src/eval/profile/layer.rs` — eval layer threads `profile.config_overrides.graph_penalty.resolve_params()` (LEVER LIVE).
- `crates/unimatrix-server/src/test_support.rs` — test harness passes `default()`.
- `crates/unimatrix-server/src/infra/embed_handle.rs` — new `pub(crate) set_ready_with_provider` (provider-injection seam).
- `crates/unimatrix-server/src/eval/runner/sweep.rs` — NEW `run_fixture_sweep` (drift guard + embed-at-load + alias-map replay).
- `crates/unimatrix-server/src/eval/runner/sweep_tests.rs` — NEW `test_ac14_correlated_sweep_non_vacuous`.
- `crates/unimatrix-server/src/eval/runner/mod.rs` — module registration + re-exports.

## Tests
- `test_ac14_correlated_sweep_non_vacuous` PASSES — all five conditions covered.
- Full eval suite: 274 passed / 0 failed. Workspace: rc=0, no failures.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search -- found ADR-006 (#4894 eval-only boundary), ADR-001 (#4897 penalty exposure + clamp coupling), ADR-002 (#4895 drift guard severity).
- Stored: entry #4909 "Eval fixture-sweep non-vacuity trap: query + corpus MUST share one EmbeddingProvider" via context_store (pattern).

## Issues / Notes
- `services/mod.rs` is 656 lines (was ~646 before my +10-line change) — pre-existing over-limit shared file; splitting is out of scope for this capstone. New files are 181 / 360 lines.
- R-04 named human column-manifest review remains the one outstanding human delivery gate (unchanged by this work).
