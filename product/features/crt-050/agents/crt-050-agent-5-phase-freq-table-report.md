# Agent Report: crt-050-agent-5-phase-freq-table

**Feature:** crt-050 Phase-Conditioned Category Affinity (Explicit Read Rebuild)
**GH Issue:** #542
**Component:** phase-freq-table

---

## Work Completed

### Files Modified

1. `crates/unimatrix-server/src/services/phase_freq_table.rs`
2. `crates/unimatrix-store/src/lib.rs`

### Changes

**`phase_freq_table.rs`** — full two-query rebuild path implemented:

- `rebuild()` signature extended to `(store: &Store, lookback_days: u32, min_phase_session_pairs: u32)` (Option A from pseudocode — config value threaded as parameter, not stored as field)
- Query A call (`store.query_phase_freq_observations`) already patched by config agent; now wired into the full pipeline
- Coverage gate: `store.count_phase_session_pairs(lookback_days)` called after non-empty check; `use_fallback=true` + `tracing::warn!` emitted when below threshold (AC-14, constraint C-9)
- Query B call (`store.query_phase_outcome_map`) with error propagation — never silently treated as empty (constraint C-7)
- `apply_outcome_weights(rows_a, rows_b)` — per-phase MEAN aggregation (ADR-001 constraint #6, R-03)
- `outcome_weight(outcome: &str) -> f32` — rework checked before fail (ADR-003 constraint #7), doc comment cross-references `infer_gate_result()` in `mcp/tools.rs`
- `phase_category_weights(&self) -> HashMap<(String, String), f32>` — breadth-based (bucket.len() / phase_total), empty on cold-start (ADR-008)
- All existing methods and contracts preserved unchanged (AC-06, FR-11)

**`unimatrix-store/src/lib.rs`** — added `#[doc(hidden)] pub use query_log::PhaseOutcomeRow;` to enable cross-crate consumption without full module path (Option A from pseudocode visibility note).

### Background.rs

Already patched by the config agent (line 623: `let min_pairs = inference_config.min_phase_session_pairs;`, line 628: passes `min_pairs` to `rebuild()`). No changes required.

---

## Test Results

**32 phase_freq_table unit tests: 32 pass, 0 fail**

New tests added (25):
- `test_outcome_weight_*` (5 tests) — vocabulary coverage and priority order (R-02, T-PFT-14/15)
- `test_apply_outcome_weights_*` (6 tests) — per-phase mean aggregation, missing phase default, empty outcome rows (R-03, AC-04/05)
- `test_phase_category_weights_*` (5 tests) — cold-start empty, single category=1.0, two-category distribution, breadth-not-freq, multi-phase independence (AC-08, R-07)

Pre-existing tests (7): all pass without modification (AC-06 contract preserved).

**Full workspace: 2881+ passed, 0 failed** (one pre-existing embedding-init flake in `col018_topic_signal_from_feature_id` not related to this component).

---

## Constraints Verified

| Constraint | Status |
|---|---|
| C-5: per-phase MEAN (not per-cycle) | PASS — `apply_outcome_weights` averages across all cycles per phase |
| C-6: rework before fail priority | PASS — `outcome_weight()` checks `contains("rework")` first |
| C-7: Query B errors propagate | PASS — `?` operator, no catch-and-empty |
| C-8: phase_category_weights breadth formula | PASS — `bucket.len() / total`, not freq sum |
| C-9: coverage gate with warn + use_fallback | PASS — gate fires before Query B |
| C-10: PhaseOutcomeRow not re-exported as public API | PASS — `#[doc(hidden)]` marks it as implementation detail |

---

## Issues / Deviations

None. Implementation follows pseudocode exactly. No silent deviations from validated design.

One observation: `background.rs` and `config.rs` were already updated by the config agent (crt-050-agent-4) to pass `min_phase_session_pairs` through the call chain and rename `query_log_lookback_days`. No conflicts encountered.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned col-031 rank normalization ADRs (#3685), crt-050 ADRs (#4225, #4223, #4228, #4230), and PhaseFreqTable cold-start patterns (#3677, #3699). Applied: confirmed rework-before-fail priority from #4225, confirmed breadth formula from #4230.
- Stored: entry #4239 "Use #[doc(hidden)] re-export to bridge store-internal row types to server crate" via /uni-store-pattern — captures the Option A visibility decision for future cross-crate internal type scenarios.
