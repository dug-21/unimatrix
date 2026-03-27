# Agent Report: crt-029-agent-2-spec

**Role**: Specification Writer
**Feature**: crt-029 — Background Graph Inference (Supports Edges)

## Output

- `/workspaces/unimatrix/product/features/crt-029/specification/SPECIFICATION.md`

## Decisions Made

1. **AC-18† added** (SR-07 mitigation): Explicit acceptance criterion requiring a grep pass for
   `InferenceConfig {` struct literals before merge. Grep at spec time confirmed 52 occurrences
   across `nli_detection.rs` and `config.rs`. This is a known gate-failure pattern from crt-023
   (Unimatrix entry #2730).

2. **AC-19† added** (SR-01 mitigation): Explicit criterion that the tick's contradiction
   threshold is exactly `nli_contradiction_threshold` — not a softer value. Prevents false-
   positive `Contradicts` edges from silently suppressing results via col-030 `suppress_contradicts`.

3. **C-07 added** (SR-01): Named constraint in the Constraints section making the
   contradiction-threshold floor explicit and traceable. Complements AC-19†.

4. **C-11 added** (SR-07): Named constraint requiring the struct-literal grep pass as a merge
   gate condition, with current count (52) recorded.

5. **C-12 added** (SR-06): Named constraint requiring the architect to confirm
   `compute_graph_cohesion_metrics` pool choice before the implementation brief is written.

6. **FR-10 added** (SR-02 mitigation): Explicit functional requirement that source candidates
   are bounded to `max_graph_inference_per_tick` before any `get_embedding` call. This bound is
   independent of (and not derived from) the NLI pair cap.

7. **NFR-05 revised**: Hard gate is 800 lines (not the 500-line soft target from
   `rust-workspace.md`). The 500-line target is acknowledged but `nli_detection.rs` is already
   ~650 lines; 800 is the practical merge gate.

8. **SR-03 resolved in AC-02**: Confirmed strict `>=` predicate (reject when
   `supports_candidate_threshold >= supports_edge_threshold`), matching the ADR-007 guard
   pattern for `nli_auto_quarantine_threshold`. Equal values are rejected.

## Open Questions for Architect

- **OQ-01**: `compute_graph_cohesion_metrics` — `read_pool()` vs. `write_pool_server()` (SR-06).
- **OQ-02**: `write_edges_with_cap` reuse vs. named variant for tick path (SR-08).
- **OQ-03**: GRAPH_EDGES covering index confirmation and scale boundary (SR-04).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 11 entries returned. Entry #3655 (source-
  candidate bound pattern for background NLI ticks) and entry #3591 (EDGE_SOURCE_NLI constant
  location) were directly incorporated. Entry #2730 (InferenceConfig struct literal trap) was
  referenced for AC-18†/C-11.
