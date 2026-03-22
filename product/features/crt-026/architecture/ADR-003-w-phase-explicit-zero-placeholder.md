## ADR-003: w_phase_explicit=0.0 Placeholder Strategy — Defer to W3-1

Feature: crt-026 (WA-2 Session Context Enrichment)

### Context

The product vision WA-2 specifies two separate affinity boost terms:
1. **Explicit phase signal**: `phase_category_weight(entry.category, current_phase) * 0.015`
   Uses `SessionState.current_phase` (set by WA-1/crt-025) and a static mapping from phase
   string to expected category sets (e.g., phase "design" → expected categories
   `["decision", "pattern"]`).
2. **Implicit histogram signal**: `p(entry.category) * 0.02`
   Uses the session category histogram accumulated during the session.

ASS-028 shipped a single flat term (`p(category) * 0.02`) before the two-term design was
finalized. The product vision supersedes ASS-028 by specifying both terms explicitly.

The explicit phase term presents a coupling problem documented in SCOPE.md §Constraints:
**"Phase string vocabulary is opaque."** WA-1 (crt-025, ADR-001) established that
Unimatrix stores and forwards phase strings without interpreting them. A static mapping
`phase_string → expected_categories` in the codebase would:
- Couple the ranking engine to the SM's phase naming conventions (e.g., "design" vs.
  "architecture" vs. "scoping")
- Require codebase changes whenever the SM vocabulary evolves
- Produce incorrect signals for custom phase strings from external agents

The W3-1 GNN (downstream of WA-2) is designed to learn the `phase → category` relationship
from training data — this is precisely the kind of latent mapping a GNN can learn without
explicit hand-coding. Shipping a static mapping now would provide a weak, brittle signal
that W3-1 would have to override.

The `phase_explicit_norm` field is still required as a W3-1 placeholder so that the feature
vector is stable and the field can be enabled without struct changes when W3-1 is ready.

### Decision

Ship `w_phase_explicit = 0.0` (default) as a placeholder in `InferenceConfig` and
`FusionWeights`. The corresponding `FusedScoreInputs.phase_explicit_norm` field is always
`0.0` in crt-026. No `phase_category_weight(category, phase)` mapping function is
implemented in this feature.

Concretely:
- `InferenceConfig` gets `w_phase_explicit: f64` with `serde(default = "default_w_phase_explicit")`
  returning `0.0`
- `FusionWeights` gets `w_phase_explicit: f64`, read from `InferenceConfig`
- `FusedScoreInputs` gets `phase_explicit_norm: f64` — hardcoded to `0.0` at the call site
  in the scoring loop
- `compute_fused_score` includes `+ weights.w_phase_explicit * inputs.phase_explicit_norm`
  (which evaluates to `0.0 * 0.0 = 0.0` in crt-026 — a no-op, but structurally present)
- AC-07 from the specification is explicitly dropped (see SPECIFICATION.md §AC-07)

W3-1 will:
1. Populate `phase_explicit_norm` using its learned `phase → category` model
2. Initialize `w_phase_explicit` from its training regime (not from crt-026's `0.0` default)
3. Tune both `w_phase_explicit` and `w_phase_histogram` jointly against the full weight vector

### Consequences

**Easier**:
- No static `phase → category` mapping is introduced. The codebase remains decoupled from
  SM vocabulary choices.
- W3-1 can enable the `phase_explicit_norm` dimension by populating it in the scoring loop
  and setting `w_phase_explicit > 0.0` in config — no struct changes required.
- The feature vector contract for W3-1 is established in crt-026 with both dimensions named
  and present; W3-1 training infrastructure can reference the field names immediately.

**Harder**:
- The explicit phase boost (product vision 3× weight of histogram) is not available until
  W3-1 ships. Sessions with an active `current_phase` receive no additional boost above
  the histogram term in crt-026.
- If a manual/static `phase → category` mapping is desired for an interim period before
  W3-1, a separate feature must implement it and supersede this ADR.
- `phase_explicit_norm = 0.0` is hardcoded at the call site, which is a code smell.
  A comment citing this ADR must be present at that site to prevent future removal as
  "dead code".
