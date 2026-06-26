## ADR-004 nan-022: One Embedding/Ranking Tolerance Policy Single-Sourced Across Retrieval + Briefing (Stable-Prefix + Tie-Class)

### Context
Retrieval (`context_search`/`lookup`/`get`) and proactive delivery (`context_briefing`) are the
SAME nondeterminism class: both are embedding/cluster-ranked. SR-03 names the trap — two
dimensions share ONE failure mode, so one entropy source can falsely RED BOTH, and authoring two
divergent tie policies is the #5302 drift hazard. SR-01/#4990 is the hard floor: HNSW approximate
top-k MEMBERSHIP flips from per-process OS entropy (no seed API in hnsw_rs 0.3.4, verified
bugfix-742, deferred to GH#746), on top of HashMap iteration-order (#2610) and `sort_unstable`
tie-breaks (`tools.rs:1598,10758`) and embedding cold-start. A naive exact-result-id-order
assertion (AC-02/AC-05) WILL flake. The fixed dispositions (OQ-2) require that intra-transport
ranking nondeterminism is NOT a cloud-parity defect; the tolerance must therefore be defined ONCE
and define exactly what "ranking match" means for the cross-leg comparison.

### Decision
ONE ranking tolerance policy module, `harness/ranking_tolerance.py`, consumed by BOTH
`RetrievalComparator` and `BriefingComparator` (no second tie policy anywhere — SR-03/#5302).

(1) **`ranking_parity(https_ids, uds_ids, *, scores=None) -> RankingVerdict`** is the single
policy function. The parity SIGNAL is the **stable ranked prefix**: the longest leading run of
result ids that is order-identical across the two legs. Membership/order churn BELOW the stable
prefix — the HNSW-approximate tail (#4990) — is TOLERATED per the closed, justified policy; it is
NOT a parity defect. `RankingVerdict` carries `matched: bool`, `stable_prefix_len: int`,
`tail_churn: list`, `tie_classes: list` for the evidence record.

(2) **Ties compare as an unordered tie-class, not positionally.** Equal-score results (the #2610
HashMap-order / `sort_unstable` instability) are grouped into a tie-class derived from the scores
the server returns; within a tie-class, membership equality is asserted, position is NOT. This
absorbs the #2610 trap without widening the cross-transport signal.

(3) **Single-sourced by construction.** There is exactly one `ranking_parity`; both comparators
import it. A change to the tolerance changes both consumers atomically — the #5302 lesson applied
at the architecture level. The closed exclusion the comparators declare (`EXCLUDED`) names the
tail/tie tolerance as a justified entry (AC-09), audited by the ADR-003 drift guard.

(4) **Non-degenerate ranking required** (pairs with ADR-007): the policy is only meaningful over a
seed corpus large enough that the stable prefix IS a real ranking signal, not a single hit (SR-06).
The corpus + query set are load-bearing inputs to this policy.

(5) **Disposition — "measured parity," not "tolerant parity":** retrieval is THE dimension where
"measured parity" is most tempted to soften into "tolerant parity." The tie-class / stable-prefix
tolerance MUST be scrutinized at first live run (the nan-021 ADR-003 first-live-run gate, per
dimension) so it CANNOT swallow a real cross-transport ranking divergence — the stable prefix and
tie-class boundaries are examined field-by-field and product-disposed before the gate is trusted. A
cross-leg divergence WITHIN the stable prefix (after both legs are intra-stable per ADR-002) is a
real `PARITY_FAIL` → GH bug. If exact ordering proves unachievable WITHOUT a production determinism
fix (#4990/GH#746 HNSW, no seed API in hnsw_rs 0.3.4), the disposition is a FILED BUG plus a
HUMAN-SIGNED DOCUMENTED C0 exception — NEVER a quiet widening of the tolerance to green a red
(product/human disposition only, nan-021 NFR-8).

### Consequences
Easier: retrieval and briefing share ONE tested tolerance, so the SR-03 "one entropy source reds
both" hazard is closed and the two policies cannot diverge; the stable-prefix framing makes the
GH#746 HNSW tail flip a tolerated, documented non-signal rather than a flake; #2610 ties are handled
once. Harder: "stable prefix length" + "tie-class boundary" are themselves tunables that must be
justified and minimized (a too-short prefix yields a vacuous pass — the SR-06 hazard; a too-long
prefix re-introduces flake); the policy depends on the server returning scores to derive tie-classes
(if scores are absent the policy degrades to membership-only on the prefix, which must be a
documented, justified fallback); the tolerance is the one place test-infra defines what "retrieval
parity" MEANS, so its contents are product-disposed, not implementer-chosen.

Related: SR-01, SR-03, OQ-2; AC-02, AC-05. Absorbs #4990/GH#746, #2610. Single-sources per the
#5302 lesson. Consumed by the retrieval + briefing comparators (ADR-003) and the intra-transport
stability check (ADR-002). Depends on the non-degenerate corpus (ADR-007).
