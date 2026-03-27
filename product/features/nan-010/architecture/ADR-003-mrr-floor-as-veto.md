## ADR-003: mrr_floor Is a Veto, Not a Co-Equal Distribution Target

### Context

The distribution gate has three numeric thresholds: `cc_at_k_min`, `icd_min`, and `mrr_floor`.
Two design options were considered for how these three values combine into a pass/fail:

**Option A — Three co-equal targets**: The distribution gate passes when all three are met.
Fail condition is any one of the three being missed. The report shows a single PASSED/FAILED
verdict with a three-row table.

**Option B — Two diversity targets plus one veto**: CC@k and ICD measure whether diversity
improved. `mrr_floor` is an absolute ranking quality floor — "did ranking collapse?" These are
orthogonal concerns. Diversity can improve by shuffling the ranking and tanking MRR. The
correct semantics are: the diversity gate (CC@k + ICD) is evaluated independently of whether
the MRR floor is breached.

Option B is specified in SCOPE.md §Design Decisions #3:
> CC@k and ICD measure whether diversity improved. mrr_floor measures whether ranking quality
> didn't collapse. [...] The report renders the distribution gate result (CC@k + ICD pass/fail)
> first, then the MRR floor as a separate line that can independently block.

The failure mode "Diversity targets met, but ranking floor breached" is meaningfully different
from "Diversity targets not met." Option A collapses both into a single FAILED verdict,
destroying diagnostic information. Option B distinguishes them.

`mrr_floor` is also intentionally an absolute floor, not a delta relative to baseline MRR. A
distribution-change feature may have lower MRR than the baseline (this is the expected and
intended effect of re-ranking). The floor is a quality minimum: "the feature must not tank MRR
below this absolute value, regardless of how much lower it is than baseline."

### Decision

`mrr_floor` is a veto over the distribution gate, not a co-equal target.

`DistributionGateResult` has:
- `diversity_passed: bool` — true when `mean(cc_at_k) >= cc_at_k_min` AND
  `mean(icd) >= icd_min`.
- `mrr_floor_passed: bool` — true when `mean(mrr) >= mrr_floor`.
- `overall_passed: bool` — true when `diversity_passed && mrr_floor_passed`.

The renderer (`render_distribution_gate_section`) renders these in two separate sub-blocks:
1. The diversity table (CC@k, ICD rows) with verdict "Diversity gate: PASSED / FAILED".
2. The MRR floor as a separate table/line with its own verdict.
3. An overall verdict.

Distinct failure messages:
- Diversity failed, MRR floor passed: "Diversity targets not met."
- Diversity passed, MRR floor failed: "Diversity targets met, but ranking floor breached."
- Both failed: both messages shown.

### Consequences

Easier:
- Diagnostic value is preserved: the operator can distinguish "diversity didn't improve" from
  "diversity improved but ranking collapsed."
- `DistributionGateResult` models the actual semantics rather than collapsing them.
- Test cases can cover each of the four states (pass/pass, pass/fail, fail/pass, fail/fail)
  independently.

Harder:
- The `DistributionGateResult` struct has more fields than a single bool would.
- The renderer must produce two separate sub-blocks with their own headers, which is slightly
  more complex than a single table.
