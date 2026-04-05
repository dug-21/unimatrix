# bugfix-523 Researcher Report

## Summary

Completed problem-space exploration for the four-item hardening batch. SCOPE.md written to `product/features/bugfix-523/SCOPE.md`.

## Key Findings

### Item 1 — NLI Tick Gate

**Confirmed issue with a nuance**. The issue description says Path B "runs unconditionally when the ONNX model is loaded, ignoring `nli_enabled` config flag." Accurate but incomplete:

- The crt-039 ADR-001 (entry #4017) intentionally removed the outer `if inference_config.nli_enabled` gate from `background.rs` so Phase A (structural Informs HNSW scan) runs unconditionally.
- The current implicit gate is `nli_handle.get_provider().await` returning `Err(NliNotReady)` when `nli_enabled = false`. This IS documented with a comment at line 563.
- The fix is NOT to restore the outer gate in `background.rs`. It is to add `if !config.nli_enabled { return; }` inside `run_graph_inference_tick` at the Path B boundary (after Path C completes, before the `get_provider()` call). This keeps Phase A and Path C unconditional.
- The performance issue is real: under NLI load, a 353-second tick was observed because the function reaches rayon dispatch before failing. The explicit gate prevents even the `get_provider()` async call when disabled.

### Item 2 — Log Downgrade

**Confirmed, but location is Path C (not Path B)**. The `warn!` fires in `run_cosine_supports_path` (Path C, pure cosine), not in Path B (NLI). Two warn sites at lines 796 and 806 for source/target absent from `category_map`. The non-finite cosine warn at line 766 stays as `warn!` — that IS an anomaly.

### Item 3 — NaN Guards

**Confirmed, 11 fields identified** (issue says "8+"). The three crt-046 fields already have `!v.is_finite()` (PR #516). The 11 fields without it: `nli_entailment_threshold`, `nli_contradiction_threshold`, `nli_auto_quarantine_threshold`, `supports_candidate_threshold`, `supports_edge_threshold`, `ppr_alpha`, `ppr_inclusion_threshold`, `ppr_blend_weight`, `nli_informs_cosine_floor`, `nli_informs_ppr_weight`, `supports_cosine_threshold`. The 6 fusion weights (`w_sim`, etc.) and 2 phase weights also lack `!v.is_finite()` but lesson #4132 flags these as out-of-scope — needs confirmation (OQ-01).

### Item 4 — sanitize_session_id Gap

**Confirmed exactly as described**. The `post_tool_use_rework_candidate` arm (lines 656–718) is the last UDS dispatch arm that uses `event.session_id` without calling `sanitize_session_id`. All other arms carry the guard. The fix is 5 lines matching the `RecordEvent` general arm pattern (lines 731–738). No existing test covers invalid session_id in this specific arm.

## Open Questions (see SCOPE.md OQ-01 through OQ-04)

Most important: OQ-01 (fusion weight NaN scope) and OQ-02 (log message on early NLI gate return).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned #4132 (InferenceConfig NaN lesson), #3461 (obs log pattern), #3467 (obs log ADR), #2492 (ONNX NLI integration), #4017 (crt-039 ADR-001 control flow split). All directly relevant.
- Stored: nothing novel to store — the NaN guard pattern is already captured in lesson #4132, the obs log pattern in #3461/#3467, and the crt-039 gate decision in #4017.
