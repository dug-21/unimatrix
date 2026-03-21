## ADR-003: Default Fusion Weights for the Six-Term Formula

### Context

The default weights for `w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov` are
W3-1's initialization point. If NLI is underweighted at defaults, W3-1's training data
will represent a world where NLI barely mattered, permanently biasing the learned function.
If sim is underweighted, user-visible topical match degrades. The scope (SR-01) requires
deriving weights from signal-role reasoning and verifying the results numerically.

**Constraints that bound the decision:**
1. Sum must be ≤ 1.0 (config validation constraint).
2. Leave ≥ 0.05 headroom for WA-2's `w_phase * phase_boost` term (SR-06, AC-13).
3. With defaults and NLI disabled, sim must remain dominant over conf (Constraint 9, 10).
4. AC-11: high-NLI (0.9) entry ranks above high-co-access (max raw=0.03, norm=1.0)
   entry given equal sim=0.5, conf=0.5, util neutral.
5. W3-1 training: NLI should be the dominant signal because it is the precision model
   operating on already-pre-filtered candidates.

**Signal role analysis:**

`w_nli`: NLI cross-encoder measures whether a candidate passage is entailed by the query —
whether it answers it. The bi-encoder that produced the candidate set approximates this; the
cross-encoder computes it directly. Operating after HNSW pre-filtering, NLI resolves semantic
ambiguity that cosine similarity cannot (e.g., two passages equally similar in embedding space
but one answering the query and one restating it). This is the richest signal for precision.
It should be dominant.

`w_sim`: Cosine similarity is the recall anchor. Candidates have already passed HNSW pre-
filtering, so all candidates have non-trivially similar embeddings. Similarity still matters
as a tiebreaker among candidates with similar NLI scores, and as the primary signal on the
NLI-disabled fallback path. It should be the second-largest weight to preserve the semantic
that topical match is the primary filter.

`w_conf`: Historical reliability (Wilson score composite over helpfulness votes). For
semantically equivalent candidates, a battle-tested entry should outrank an untested one.
Good tiebreaker but should not override a semantically better answer. Medium weight.

`w_coac`: Co-access affinity reflects usage patterns — entries that co-occur with high-
confidence query anchors. Useful but lagging (requires accumulated history to populate) and
biased toward frequently-retrieved entries regardless of relevance to the specific query.
Smaller weight.

`w_util`: Effectiveness classification (Effective/Settled/Ineffective/Noisy). Meaningful
signal but sparse: early in a deployment, most entries are Unmatched (neutral). Keeps weight
small to avoid rewarding popular entries simply because they have been classified.

`w_prov`: Category provenance (boosted_categories, e.g. lesson-learned). The weakest signal:
binary, category-level, not query-specific. Smallest weight.

**Candidate weight sets:**

Set A: `w_nli=0.40, w_sim=0.25, w_conf=0.15, w_coac=0.08, w_util=0.05, w_prov=0.02` → sum=0.95
Set B: `w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05` → sum=0.95
Set C: `w_nli=0.35, w_sim=0.30, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.00` → sum=0.95

Set A weights NLI at 0.40 and makes co-access nearly invisible (0.08). This is aggressive
and may reduce diversity in early deployments with sparse NLI data. Set C removes provenance
entirely, making `w_prov` a dead parameter for WA-0. Set B provides rounded, memorable values,
meaningful non-zero weight for all six signals, and equal treatment of the two weakest signals
(util and prov both at 0.05).

The config-driven design means operators can override any weight, including setting `w_prov=0.0`
if they believe provenance is irrelevant. The default should make a principled initial claim
for each signal, not a null claim.

**Set B selected.**

### Decision

Default fusion weights:

| Field | Default | Reasoning |
|-------|---------|-----------|
| `w_nli` | **0.35** | Dominant signal: cross-encoder precision, semantically richest |
| `w_sim` | **0.25** | Second: recall anchor, topical match preservation |
| `w_conf` | **0.15** | Third: historical reliability, tiebreaker |
| `w_coac` | **0.10** | Fourth: useful lagging usage pattern |
| `w_util` | **0.05** | Fifth: sparse early on, meaningful once populated |
| `w_prov` | **0.05** | Sixth: weakest signal, non-zero to be learnable by W3-1 |
| **Sum** | **0.95** | **0.05 headroom for WA-2 phase boost** |

**Numerical verification of all binding constraints:**

**AC-11 (NLI dominance over max co-access):**
Entry A: nli=0.9, coac_norm=0.0, sim=0.5, conf=0.5, util_norm=0.5 (neutral), prov=0
  score_A = 0.35×0.9 + 0.25×0.5 + 0.15×0.5 + 0.10×0.0 + 0.05×0.5 + 0.05×0.0
           = 0.315 + 0.125 + 0.075 + 0.000 + 0.025 + 0.000 = 0.540

Entry B: nli=0.3, coac_norm=1.0, sim=0.5, conf=0.5, util_norm=0.5, prov=0
  score_B = 0.35×0.3 + 0.25×0.5 + 0.15×0.5 + 0.10×1.0 + 0.05×0.5 + 0.05×0.0
           = 0.105 + 0.125 + 0.075 + 0.100 + 0.025 + 0.000 = 0.430

**0.540 > 0.430. AC-11 holds.**

**Constraint 10 (sim dominant over conf at full defaults, no NLI):**
Entry A: sim=0.9, conf=0.3, nli=0.0 (disabled), coac=0, util neutral, prov=0
  score_A = 0.25×0.9 + 0.15×0.3 = 0.225 + 0.045 = 0.270

Entry B: sim=0.5, conf=0.9, nli=0.0, coac=0, util neutral, prov=0
  score_B = 0.25×0.5 + 0.15×0.9 = 0.125 + 0.135 = 0.260

**0.270 > 0.260. Constraint 10 holds (sim dominant).**

**Constraint 9 (NLI disabled: sim dominant, conf secondary after re-normalization):**
Re-normalization denominator = w_sim + w_conf + w_coac + w_util + w_prov
  = 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 0.60

Re-normalized weights: w_sim'=0.4167, w_conf'=0.2500, w_coac'=0.1667, w_util'=0.0833, w_prov'=0.0833

Entry A: sim=0.9, conf=0.3 → 0.4167×0.9 + 0.2500×0.3 = 0.375 + 0.075 = 0.450
Entry B: sim=0.5, conf=0.9 → 0.4167×0.5 + 0.2500×0.9 = 0.208 + 0.225 = 0.433

**0.450 > 0.433. Constraint 9 holds: sim dominant, conf secondary after re-normalization.**

The re-normalized sim/conf ratio is 1.67:1. The pre-crt-024 formula (`rerank_score` at cw=0.18)
has ratio 0.82:0.18 = 4.56:1. The ratio changes but the ordering property holds — a
high-similarity entry still outranks a high-confidence entry at these defaults. This satisfies
the behavioral invariant even though the exact ratio differs.

### Consequences

Easier:
- W3-1 gets a well-reasoned initialization vector; NLI-dominated training data from day one.
- All six signals have non-zero defaults, making all six learnable dimensions meaningful from
  the first training run.
- Operators who want to restore pre-NLI behavior can set `w_nli=0.0` in config and the
  re-normalization distributes remaining weight proportionally.
- The 0.05 headroom makes WA-2 deployable without config retune for most operators.

Harder:
- `w_nli=0.35` is only achievable when the NLI model is loaded and warm. Cold-start deployments
  without NLI will see the re-normalized five-signal formula until the model loads.
- Early sparse deployments (few votes, low co-access data) will see `w_coac` and `w_util`
  terms near 0.5 (neutral utility) and 0.0 (no co-access), meaning those weights contribute
  little initially. This is correct behavior — they earn influence over time.
- Operators who previously had tightly-tuned behavior from the rerank_score formula may need
  to re-validate their expected ranking behavior at these defaults.
