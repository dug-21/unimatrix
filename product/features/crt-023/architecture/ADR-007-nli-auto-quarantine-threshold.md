# ADR-007: NLI-Derived Auto-Quarantine Uses a Higher Confidence Threshold

**Feature**: crt-023 — NLI + Cross-Encoder Re-ranking
**Status**: Accepted
**Supersedes**: None
**Relates to**: ADR-001 (session concurrency), crt-018b entry #1544 (hold-on-error)

---

## Context

The product vision for W1-4 states:

> "NLI-derived auto-quarantine should require a higher confidence threshold than the
> existing manual-correction path."

The architecture implements a circuit breaker (`max_contradicts_per_tick`) that caps the
number of `Contradicts` edges written per `context_store` call. However, the circuit
breaker only limits edge *creation rate*; it does not raise the bar for when those edges
may subsequently trigger auto-quarantine during background tick processing.

Without a higher threshold, a valid entry storing a `Contradicts` edge with NLI score 0.65
(above `nli_contradiction_threshold=0.6`, meaning edge is written) could trigger
auto-quarantine on the next tick under the same criteria as a manually-curated correction.
NLI at inference time is probabilistic and domain-dependent; manually-curated corrections
carry human intent. These two signal sources should not share the same quarantine trigger.

The vision guardian flagged this as a VARIANCE (missing second guard) during design review.
Human decision: "Add it."

---

## Decision

Introduce `nli_auto_quarantine_threshold` as a tenth `InferenceConfig` field (default 0.85,
range `(0.0, 1.0)`, validated > `nli_contradiction_threshold`).

The background tick's auto-quarantine logic applies this threshold when evaluating topology
penalties from NLI-origin edges:

- An entry penalised **only** by NLI-origin `Contradicts` edges triggers auto-quarantine
  only when the NLI scores stored in those edges' `metadata` column all exceed
  `nli_auto_quarantine_threshold` (0.85 default).
- An entry penalised by a mix of NLI-origin and manually-curated edges continues to follow
  the existing auto-quarantine logic unchanged.
- The existing hold-on-error behavior (crt-018b, entry #1544) is unaffected.

The `metadata` column on `GRAPH_EDGES` already stores `{"nli_entailment": f32,
"nli_contradiction": f32}` per AC-11/AC-25; the auto-quarantine check reads
`nli_contradiction` from the metadata of each NLI-origin edge.

---

## Consequences

**Positive**:
- Closes the vision gap: two guards now exist (edge creation rate cap + higher quarantine
  bar), as the product vision intended.
- Miscalibrated NLI at first deployment cannot silently quarantine legitimate entries even
  if the circuit breaker is saturated (10 edges written but all scored at 0.65 — below
  the 0.85 quarantine bar).
- Operators can tune `nli_auto_quarantine_threshold` independently of `nli_contradiction_threshold`
  to match their domain's tolerance for NLI false positives.

**Negative / Trade-offs**:
- One additional config field to document and validate.
- Cross-field invariant (`nli_auto_quarantine_threshold > nli_contradiction_threshold`)
  adds a validation path that requires two-field error messaging.
- The background tick must now distinguish NLI-origin edges from other edges when computing
  topology penalty for auto-quarantine — a modest increase in tick complexity.

---

## Alternatives Considered

**Defer to follow-on feature**: The circuit breaker alone was the existing guard. The vision
explicitly requires the higher threshold; deferral would leave the VARIANCE open and was
rejected by the human reviewer.

**Single threshold for all edges**: Using `nli_contradiction_threshold` for both edge
creation and auto-quarantine would require setting it high enough (≥0.85) to safely gate
quarantine, which would reduce edge creation sensitivity (fewer valid Contradicts edges
written). Separating the thresholds preserves both signal density and quarantine safety.
