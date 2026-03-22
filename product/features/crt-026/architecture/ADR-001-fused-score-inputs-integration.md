## ADR-001: FusedScoreInputs Integration vs. Post-Pipeline Additive Boost

Feature: crt-026 (WA-2 Session Context Enrichment)

### Context

The WA-2 category affinity boost can be applied in two positions relative to the existing
search pipeline:

**Option A — Post-pipeline additive step**: Apply `phase_histogram_norm * w_phase_histogram`
as a final additive term to `final_score` AFTER `compute_fused_score` returns and AFTER
`status_penalty` is applied. This matches the ASS-028 spike design and the SCOPE.md
description at the time of the research spike.

**Option B — FusedScoreInputs integration**: Add `phase_histogram_norm: f64` and
`phase_explicit_norm: f64` as first-class fields to `FusedScoreInputs`, add
`w_phase_histogram: f64` and `w_phase_explicit: f64` to `FusionWeights`, and extend
`compute_fused_score` with the additional terms. The `status_penalty` multiplier is then
applied after the full fused score (including the histogram term).

The WA-2 extension stubs in `search.rs` (lines 55, 89, 179) preexist from crt-024 and
anticipate Option B. Entry #2964 in Unimatrix documents the prior risk that motivated
pre-commitment to a unified scoring function: multiple sequential sort passes caused NLI
results to be overridden by additive boosts in the WA-0 pre-crt-024 pipeline.

**W3-1 dependency**: The product vision specifies that W3-1 (GNN relevance function) learns
from the feature vector produced by `FusedScoreInputs`. Each field is a named, learnable
dimension. A post-pipeline boost term is invisible to W3-1's training; it cannot be tuned
by the GNN. An integrated field is a first-class W3-1 dimension that W3-1 can refine from
its cold-start initialization.

**`status_penalty` ordering (SR-09)**: Option A (post-pipeline) produces
`final_score = (fused * penalty) + boost`, which means the boost ignores the penalty
multiplier. Option B (integrated) produces `final_score = (fused + boost) * penalty`,
which correctly subjects the boost to the same penalty as all other terms. The specification
C-06 mandates Option B ordering.

### Decision

Integrate the histogram affinity term as a first-class dimension in `FusedScoreInputs` /
`FusionWeights` / `compute_fused_score` (Option B).

Concretely:
- `FusedScoreInputs` gains `phase_histogram_norm: f64` and `phase_explicit_norm: f64`
- `FusionWeights` gains `w_phase_histogram: f64` and `w_phase_explicit: f64`
- `compute_fused_score` adds `+ weights.w_phase_histogram * inputs.phase_histogram_norm`
  and `+ weights.w_phase_explicit * inputs.phase_explicit_norm`
- The WA-2 extension stubs (lines 55, 89, 179 of `search.rs`) are resolved; stub comments
  are removed/replaced with the implemented fields and doc-comments

The boost participates in the fused score before the `status_penalty` multiplier:
`final_score = compute_fused_score(&inputs, &weights) * penalty`.

### Consequences

**Easier**:
- W3-1 sees `phase_histogram_norm` as a named, learnable dimension with `w_phase_histogram`
  as its cold-start initialization weight. The GNN can tune this term alongside all other
  six terms from a single unified weight vector.
- The WA-2 stubs that have existed since crt-024 are now resolved — no lingering
  `// WA-2 extension:` comments remain in `search.rs`.
- `status_penalty` applies uniformly to all score terms, including the histogram boost.
  Deprecated entries that happen to match the session histogram do not receive an uncapped
  boost relative to their penalty.
- The sum-invariant concern (the six-term sum was `<= 1.0`; adding `w_phase_histogram=0.005`
  brings the total to `0.955`) is resolved cleanly: the existing validation in
  `InferenceConfig::validate()` checks only the six original weight fields, so no test
  asserts `sum == 0.95` exactly against the defaults. The FusionWeights doc-comment is
  updated to reflect the new total and the distinction between core terms and phase terms.

**Harder**:
- `FusedScoreInputs` and `FusionWeights` grow by two fields each. All existing struct
  literal constructions of these types must be updated (primarily in tests). The
  `InferenceConfig` struct literal in `Default::default()` must also be extended.
- If a future feature needs to bypass `status_penalty` for the histogram term specifically,
  it cannot do so without restructuring the pipeline.
