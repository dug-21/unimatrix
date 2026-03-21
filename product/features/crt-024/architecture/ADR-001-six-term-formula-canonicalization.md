## ADR-001: Six-Term Formula Canonicalization

### Context

The product vision document (PRODUCT-VISION.md) states a four-term fusion formula:
`w_sim * sim + w_nli * nli + w_conf * conf + w_coac * coac`. SCOPE.md's Proposed Approach
and Acceptance Criteria define a six-term formula that adds `w_util * util_norm` and
`w_prov * prov_norm`. This divergence is the SR-02 risk in the scope risk assessment and
matches the config key semantic divergence pattern documented in Unimatrix entry #2298.

The product vision formula is the public-facing summary of the ranking approach. The scope
document represents implementation-level requirements derived from the actual current pipeline.
The current pipeline (post crt-023) already applies `utility_delta` and `PROVENANCE_BOOST` as
additive terms in Step 8. These signals exist and influence ranking today.

The question is: should `utility_delta` and `PROVENANCE_BOOST` stay as pre-fusion adjustments
to other signals, or become explicit weighted terms in the fused formula?

Two options:
- **A (four-term)**: Keep utility and provenance as additive post-fusion adjustments, maintaining
  the product vision's formula shape. These signals remain outside the learnable feature vector.
- **B (six-term)**: Include utility and provenance as weighted terms in the formula. Every signal
  that currently influences ranking becomes a learnable dimension for W3-1.

The scope explicitly resolves this (AC-10): "utility_delta and PROVENANCE_BOOST are included in
the fused formula as `w_util` and `w_prov` weighted terms, normalized to [0, 1]. They are not
additive afterthoughts outside the formula. This makes them learnable dimensions for W3-1."

Additionally, SCOPE.md Constraint 2 establishes that the formula is W3-1's feature vector
interface: "Adding a signal to WA-0 is adding a learnable dimension to W3-1." Signals outside
the formula cannot have their contribution tuned by GNN training.

### Decision

The six-term formula is the canonical implementation target for crt-024:

```
fused_score =
    w_sim  * similarity_score
  + w_nli  * nli_entailment_score
  + w_conf * confidence_score
  + w_coac * (raw_boost / MAX_CO_ACCESS_BOOST)
  + w_util * ((utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY))
  + w_prov * (prov_boost / PROVENANCE_BOOST)

final_score = fused_score * status_penalty
```

The product vision's four-term formula was illustrative — it named the primary signals without
enumerating every term. Implementation derives from SCOPE.md, which is the authoritative source
for AC-level requirements. The six-term formula is recorded here as the canonical definition
so future documents, agents, and W3-1 training assume six inputs, not four.

TOML config exposes six weight keys: `w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`
under `[inference]`. These match the formula terms exactly — no key diverges from its semantic.

**`utility_delta` normalization**: The raw signal from `utility_delta()` is in [-0.05, +0.05].
The shift-and-scale `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)` maps this to
[0, 1] where: -0.05 (Ineffective/Noisy) → 0.0, 0.0 (unclassified) → 0.5, +0.05 (Effective) → 1.0.
Division by `UTILITY_BOOST` alone would yield [-1, 1], which breaks the range guarantee.

**`prov_boost` normalization**: `PROVENANCE_BOOST = 0.02` is a binary signal (0 or 0.02).
After `÷ PROVENANCE_BOOST`, the result is {0.0, 1.0}. Implementation must guard against
`PROVENANCE_BOOST == 0.0` (produce 0.0 in that case, never divide).

### Consequences

Easier:
- W3-1 receives a complete feature vector — all six signals are learnable dimensions.
- Formula is explicit and auditable; no signal influences ranking invisibly.
- AC-11 regression test can assert NLI dominance over co-access at any default weight config.
- Spec and tests operate on a single canonical formula with no additive exceptions.

Harder:
- Six config fields instead of four; operators have more parameters to tune.
- `utility_norm` requires shift-and-scale rather than simple division; implementer must
  use the correct formula.
- If W3-1 learns to assign near-zero weight to `w_util` or `w_prov`, those signals were
  a false cost of complexity. Acceptable: config allows operators to set those to 0.0.
- Product vision docs will need a footnote clarifying that the six-term implementation is
  the accurate specification.
