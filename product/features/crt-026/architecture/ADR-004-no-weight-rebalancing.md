## ADR-004: No Weight Rebalancing — Sum 0.95 → 0.955 Is Within Invariant

Feature: crt-026 (WA-2 Session Context Enrichment)

### Context

Adding `w_phase_histogram = 0.005` as a first-class term in `compute_fused_score` (per
ADR-001) increases the total sum of weights from `0.95` to `0.955`. The `FusionWeights`
doc-comment states an invariant of `<= 1.0`, and `InferenceConfig::validate()` enforces
`sum <= 1.0` at startup.

There are two approaches to accommodate the new term:

**Option A — Rebalance existing weights**: Reduce an existing weight (e.g., reduce `w_prov`
from `0.05` to `0.045`, or `w_util` from `0.05` to `0.045`) so the total sum remains
`0.95`. This preserves the existing calibrated sum but alters the relative signal dominance
ordering established by crt-024 ADR-003.

**Option B — Accept sum = 0.955**: Add the new weight on top of the existing defaults,
increasing the sum from `0.95` to `0.955`. The `<= 1.0` invariant holds trivially
(`0.955 < 1.0`). The product vision explicitly documented `sum = 0.95, 0.05 headroom for
WA-2` in anticipation of this scenario.

**Key facts**:
1. `InferenceConfig::validate()` checks `w_sim + w_nli + w_conf + w_coac + w_util + w_prov
   <= 1.0` — the six original fields. It does NOT currently include the new phase weights in
   this sum. Even if the check is extended to include phase weights, `0.955 <= 1.0` passes.
2. No test asserts `sum == 0.95` exactly against the struct defaults. The test
   `test_fusion_weights_effective_nli_active_headroom_weight_preserved` constructs a manual
   `FusionWeights` with `sum = 0.90` to verify that `effective(true)` does not re-normalize
   sub-1.0 sums — this test does not assert any specific value for the default weights.
3. The WA-0 headroom (`0.05`) was pre-budgeted in the product vision. Using `0.005` of it
   for `w_phase_histogram` leaves `0.045` for future terms.
4. W3-1 is the intended mechanism for final weight calibration. Manually rebalancing now
   would produce a hand-tuned compromise that W3-1 will subsequently overwrite — the
   rebalancing cost is paid twice.

**`FusionWeights::effective()` NLI-absent path**: The re-normalization denominator is
`w_sim + w_conf + w_coac + w_util + w_prov`. The new `w_phase_histogram` and
`w_phase_explicit` fields must NOT be added to this denominator. They are additive terms
outside the six-term normalization group. The `effective()` method must pass both new
fields through unchanged (not re-normalized) in both the NLI active and NLI absent paths.

### Decision

Do not rebalance existing weights. Ship `w_phase_histogram = 0.005` as an additive term
alongside the existing six weights. The default weight sum increases from `0.95` to `0.955`.

Concretely:
- `InferenceConfig::validate()` gains per-field range checks for `w_phase_explicit` and
  `w_phase_histogram` ([0.0, 1.0]), but the existing six-field sum check is NOT modified
  to include the new fields. The new fields are architecturally additive dimensions, not
  part of the six-term formula's balance.
- The `FusionWeights` invariant doc-comment is updated: `sum of six core terms <= 1.0;
  w_phase_histogram and w_phase_explicit are additive terms excluded from this constraint`.
- `FusionWeights::effective()` NLI-absent path: the new fields are passed through unchanged
  and excluded from the re-normalization denominator.
- W3-1 will refine all weights (including the new phase terms) from training data.

### Consequences

**Easier**:
- No existing weight defaults change. crt-024 ADR-003's calibrated default ordering
  (`w_nli=0.35` dominant, `w_sim=0.25` second, etc.) is preserved exactly. The ranking
  behavior for sessions without histogram data is bit-for-bit identical to the pre-crt-026
  pipeline (cold-start safety, NFR-02).
- The product vision's pre-budgeted headroom is used as intended. The transition from
  `sum=0.95` to `sum=0.955` is documented and anticipated.
- W3-1 receives a clean baseline: the six core weights plus two new phase weights, all at
  their intended initialization values, with `w_phase_histogram=0.005` as the cold-start
  seed for the histogram dimension.

**Harder**:
- The `FusionWeights` doc-comment distinction (core terms vs. phase terms) must be
  maintained precisely to avoid confusion. Future contributors must understand why the six-
  term sum check does not include the phase weights.
- `FusionWeights::effective()` NLI-absent re-normalization must explicitly exclude the new
  fields from the denominator. This is a correctness invariant: including `w_phase_histogram`
  in the denominator would incorrectly dilute the existing five weights when NLI is absent.
- If `w_phase_histogram` were ever set to a large value (e.g., `0.10`), the combined sum
  could approach `1.0` without the six-term validation catching it. Validation should be
  noted in the `validate()` comments as intentionally decoupled.
