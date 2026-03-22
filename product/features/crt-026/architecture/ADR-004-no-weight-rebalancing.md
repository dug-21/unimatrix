## ADR-004: w_phase_histogram=0.02 — Full Session Signal Budget, Sum 0.95 → 0.97

Feature: crt-026 (WA-2 Session Context Enrichment)

### Context

The product vision WA-2 specified two separate session signal terms:
1. **Explicit phase term**: `phase_category_weight(entry.category, current_phase) * 0.015`
2. **Implicit histogram term**: `p(entry.category) * 0.005`

Combined session signal budget: `0.015 + 0.005 = 0.02`.

**OQ-03 resolved to defer the explicit phase term** (`w_phase_explicit = 0.0`) because phase
strings are opaque to Unimatrix and a static mapping would couple ranking to SM vocabulary.
W3-1 will learn the phase→category relationship from training data.

With the explicit term at `0.0`, shipping `w_phase_histogram = 0.005` delivers only **¼ of
the intended session signal budget**. ASS-028, the research spike that informed this feature,
originally specified a flat `AFFINITY_WEIGHT = 0.02` as the calibrated weight for the single
histogram term. The product vision's two-term split was designed with both terms active; the
`0.005` value was never intended to stand alone.

At `0.005`, the histogram boost is detectable only in synthetic tests with ≥60% histogram
concentration (`0.005 × 0.6 = 0.003` — barely above floating-point noise in production
sessions). The signal is effectively invisible in real sessions with distributed stores.

Two options for the histogram term default:

**Option A — Ship at `w_phase_histogram = 0.005`**: Minimal boost now; W3-1 learns the
correct weight later. Risk: W3-1 initializes from a near-zero signal that may not generate
useful training gradients. The feature is live but practically invisible.

**Option B — Ship at `w_phase_histogram = 0.02`**: Carry the full `0.02` session signal
budget on the histogram term since the explicit term is deferred. Matches ASS-028's calibrated
value. At `0.02`, the boost is `0.02` at p=1.0 concentration and `0.012` at 60%
concentration — detectable at realistic session distributions. W3-1 initializes from a
meaningful starting value and is expected to split the budget between histogram and explicit
terms once it learns the phase→category mapping.

**Key facts**:
1. `InferenceConfig::validate()` checks `w_sim + w_nli + w_conf + w_coac + w_util + w_prov
   <= 1.0` — the six original fields only. The new phase weight is NOT included in this sum.
   Even if extended, `0.95 + 0.02 = 0.97 <= 1.0` passes.
2. No test asserts `sum == 0.95` exactly against struct defaults.
3. The WA-0 headroom (`0.05`) was pre-budgeted for WA-2. Using `0.02` of it leaves `0.03`
   for future terms.
4. W3-1 is the intended mechanism for final weight calibration. When W3-1 eventually enables
   `w_phase_explicit`, it will learn the appropriate split between the two terms — likely
   reducing `w_phase_histogram` as `w_phase_explicit` gains weight from training data. The
   `0.02` starting point is the ASS-028 calibrated baseline, not a permanent value.

**`FusionWeights::effective()` NLI-absent path**: The re-normalization denominator is
`w_sim + w_conf + w_coac + w_util + w_prov`. The new `w_phase_histogram` and
`w_phase_explicit` fields must NOT be added to this denominator. They are additive terms
outside the six-term normalization group. The `effective()` method must pass both new
fields through unchanged in both the NLI active and NLI absent paths.

### Decision

Ship `w_phase_histogram = 0.02` (Option B). No existing weight defaults are changed.
The default weight sum increases from `0.95` to `0.97` (including the new phase term).

Concretely:
- `InferenceConfig::validate()` gains per-field range checks for `w_phase_explicit` and
  `w_phase_histogram` ([0.0, 1.0]), but the existing six-field sum check is NOT modified
  to include the new fields.
- The `FusionWeights` invariant doc-comment is updated: `sum of six core terms <= 1.0;
  w_phase_histogram and w_phase_explicit are additive terms excluded from this constraint`.
- `FusionWeights::effective()` NLI-absent path: the new fields are passed through unchanged
  and excluded from the re-normalization denominator.
- W3-1 will refine all weights (including the new phase terms) from training data.

### Consequences

**Easier**:
- No existing weight defaults change. crt-024 ADR-003's calibrated default ordering is
  preserved exactly. Sessions without histogram data produce identical rankings to pre-crt-026
  (cold-start safety, NFR-02).
- The histogram signal is detectable at realistic session distributions. A session with ≥30%
  concentration in one category produces a `≥ 0.006` boost; p=1.0 produces exactly `0.02`.
  AC-12 test fixtures do not require extreme concentration conditions.
- W3-1 receives a meaningful cold-start seed (`0.02`) matching ASS-028's calibration. The
  GNN has sufficient gradient signal to learn from the histogram dimension from day one.
- The product vision's 0.05 headroom is used as intended: `0.02` for histogram (now),
  remainder available for explicit phase term and future dimensions.

**Harder**:
- The `FusionWeights` doc-comment distinction (core terms vs. phase terms) must be maintained
  precisely. The sum check in `validate()` must remain on the six core fields only.
- `FusionWeights::effective()` NLI-absent re-normalization must explicitly exclude the new
  fields from the denominator. This is a correctness invariant (R-06).
- When W3-1 enables `w_phase_explicit`, the combined boost (`w_phase_histogram + w_phase_explicit`)
  may exceed `0.02` if W3-1 learns non-trivial values for both. W3-1's training regime must
  account for this joint budget. The remaining `0.03` of the 0.05 headroom provides clearance.
