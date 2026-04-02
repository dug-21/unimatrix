# ASS-039: Behavioral Signal Validation — Goal × Phase × Entries × Outcome

**Date**: 2026-04-01  
**Spike type**: Data mining + design — validate signal, then spec delivery mechanism  
**Depends on**: ASS-038 (edge generation, GNN readiness), col-024 (context_cycle_review)  
**Data source**: Live DB — observations + sessions + outcome_index (50 feature cycles post-#409)

---

## Context

ASS-038 identified that the PPR bottleneck is architectural (re-ranker within k=20, not an
expander) and that the labeled edge set is GNN-ready. The remaining question is whether
behavioral signals — what agents actually retrieve to accomplish specific goals — can drive
a qualitatively different retrieval mechanism than semantic similarity alone.

A foundational flaw in the ASS-037 eval harness was also identified: the 2,356 scenarios
were built from human orchestration commands typed at the prompt ("merge", "start bugfix-438",
"take a look at issue 91"). These are not knowledge retrieval queries. Furthermore, the
`expected` field in every scenario was `null` — the harness was using `baseline.entry_ids`
(what the old system returned) as soft ground truth, measuring self-consistency rather than
actual retrieval quality. All P@5/MRR numbers from ASS-037 must be treated as directionally
informative at best and are not reliable for formula calibration.

This spike produces the correct eval scenario set and re-runs the ASS-037 ablations as an
explicit output, in addition to validating the behavioral hypotheses.

The infrastructure to answer both questions already exists and is populated:

- `observations`: every context_search, context_get, context_briefing call with session_id,
  tool input, and topic_signal (feature_cycle)
- `sessions`: session_id → feature_cycle, agent_role, outcome
- `outcome_index`: feature_cycle → delivery outcome
- 50 feature cycles of real delivery data post-#409, covering design and delivery phases

This spike mines that data to validate three hypotheses. If they hold, the delivery
mechanism is an extension to `context_cycle_review` — not a new pipeline.

---

## Central Question

**Does the Goal × Phase × Entries × Outcome signal exist in 50 cycles of observation data,
and is it learnable enough to improve context_briefing quality for future cycles?**

---

## Hypotheses to Validate

### H1 — Goal clustering is real

Feature cycles with semantically similar goals (cosine on GH issue title + description
embeddings) retrieve overlapping entry sets.

**Test**: For each of the 50 cycles, fetch the GH issue and embed the goal statement.
Cluster cycles by goal similarity (cosine ≥ 0.65). Within each cluster, measure entry
overlap across cycles: what fraction of entries retrieved in cycle A also appear in cycle B?

**Pass**: Intra-cluster entry overlap significantly exceeds inter-cluster overlap.  
**Fail**: Entry sets are goal-independent — cycles retrieve similar entries regardless of
what they were trying to accomplish.

**Implication if pass**: Goal similarity is a viable briefing signal. Future cycles can be
pre-warmed with entries from successful past cycles with similar goals.

---

### H2 — Outcome correlation is real

Successful cycles have a distinct knowledge access profile from rework cycles. Certain
entries consistently appear in successful cycles; others appear primarily in rework cycles
(gap candidates — knowledge that existed but was insufficient or absent).

**Test**: Partition the 50 cycles by outcome (success vs. rework). For each active entry,
compute: (appearances in success cycles) / (total appearances). Entries with high success
ratio are load-bearing. Entries that appear in rework cycles but rarely in success cycles
are gap candidates.

Note: ~90% success / ~10% rework in this corpus. The rework cases carry a
before/after signal — initial delivery (what was used) + correction (what was additionally
needed). Treat rework as a gap signal, not a failure signal.

**Pass**: Success-ratio distribution is non-uniform — a subset of entries clusters at high
success ratio, distinct from the rest.  
**Fail**: Success ratio is uniformly distributed — no entry is more associated with success
than chance.

**Implication if pass**: Outcome-correlated retrieval is valid. context_cycle_review can
write reinforcing edges for entries that co-appeared in successful cycles.

---

### H3 — Phase stratification is real

The same goal cluster requires different knowledge in design phase vs. delivery phase.
Phase is a meaningful filter on the entry access profile, not just a label.

**Test**: Within goal clusters (from H1), split sessions by agent_role (proxy for phase:
architect/specification → design; rust-dev/tester → delivery). Measure entry overlap
between design-phase sessions and delivery-phase sessions within the same cluster.

**Pass**: Intra-cluster, cross-phase entry overlap is significantly lower than intra-cluster,
same-phase overlap. Design sessions and delivery sessions retrieve distinct entry sets even
for the same goal.  
**Fail**: Phase does not stratify entry access — design and delivery sessions retrieve the
same entries.

**Implication if pass**: context_briefing should return phase-conditioned entries from
goal-similar past cycles, not a phase-agnostic union.

---

## Data Reconstruction

All analysis is read-only against the live DB. No snapshot required — this is behavioral
data, not eval harness work.

**Primary query** (reconstruct Goal × Phase × Entries per cycle):

```sql
SELECT 
  s.feature_cycle,
  s.agent_role,
  s.outcome,
  o.tool,
  o.input,
  o.ts_millis,
  o.topic_signal
FROM observations o
JOIN sessions s ON o.session_id = s.session_id
WHERE o.tool IN ('context_get', 'context_search', 'context_briefing')
  AND s.feature_cycle IS NOT NULL
ORDER BY s.feature_cycle, o.ts_millis ASC
```

Extract entry IDs from `observations.input` JSON for context_get calls.
Fetch GH issue title + description for each feature_cycle via `gh issue view`.
Embed goal statements using the existing embedding pipeline (same model as entries).

**Feasibility gate**: Before proceeding, verify:
- Count of distinct feature_cycles with ≥3 observations
- Count of context_get calls with parseable entry IDs in input
- Count of sessions with non-null outcome

If fewer than 30 cycles have sufficient observations and parseable entry access records,
mark H1/H2/H3 as INSUFFICIENT DATA and document what threshold is needed.

---

## Design Output (conditional on hypothesis validation)

If ≥2 of 3 hypotheses pass, produce a concrete design for the context_cycle_review
extension — what it should emit at cycle close to grow the graph and improve future
briefing quality.

### context_cycle_review additions

**Edge emission**:
- For each pair of entries co-accessed in the completed cycle (via context_get): write or
  reinforce an Informs edge with `signal_origin='behavioral'` and weight proportional to
  co-access frequency within the cycle
- Weight: success outcome → full weight; rework outcome → half weight (gap signal, not
  confirmation)
- Additive only — never remove or reduce existing edges based on a single cycle

**Goal-cluster store**:
- Store the cycle's goal embedding + phase-stratified entry access profile as a new
  example in a goal_clusters table (design the schema)
- Key: goal_embedding (vector), phase, feature_cycle
- Value: entry_ids accessed, outcome

**Gap signal**:
- If rework outcome: store the delta between initial delivery entry set and correction
  entry set as a gap record — what knowledge was missing or insufficient

**Briefing enhancement**:
- When context_briefing fires, retrieve goal-similar past cycles (cosine on goal embedding)
  filtered by same phase, sorted by outcome
- Return a blended result: semantic retrieval (current) + goal-cluster entries (new)
- Cold-start graceful degradation: if no goal-similar past cycles exist, fall back to
  semantic retrieval only — no behavior change for new deployments

### Domain-agnostic design constraints

The system must work at any outcome distribution:
- High-success corpus (this one, ~90%): positive signal dominates; rework provides gap
  signal
- High-failure corpus (new team, complex domain): failure signal becomes primary learning
  source; same mechanism, different weight distribution
- New corpus (zero history): cold-start → pure semantic retrieval → behavioral edges
  accumulate organically as cycles complete

Edge weights must be normalized to corpus size and cycle count, not absolute counts —
otherwise large corpora with many cycles will always dominate over small corpora.

---

## Eval Scenario Construction

This is a required output regardless of hypothesis results. The ASS-037 scenarios.jsonl
must be replaced with behaviorally-grounded scenarios before any future formula or
architecture testing is meaningful.

### How to build scenarios

For each `context_search` call in the observations table:
1. Extract the query text from `observations.input` JSON (the `query` field)
2. Find subsequent `context_get` calls in the same session within 30 minutes
3. Extract the entry IDs from those `context_get` input JSON records
4. Those entry IDs are the behavioral ground truth — entries the agent found worth reading

For `context_briefing` calls, the query is the feature + phase combination. Ground truth
is all `context_get` calls within that session following the briefing.

Filter: exclude sessions where no `context_get` followed within 30 minutes — those are
searches that found nothing useful and do not produce reliable ground truth.

### Scenario format

Each scenario is one JSONL record. The `expected.entry_ids` field MUST be populated with
behavioral ground truth — never null, never copied from `baseline.entry_ids`:

```json
{
  "id": "obs-{session_id}-{ts_millis}",
  "query": "{context_search input text or 'briefing:{feature}:{phase}'}",
  "context": {
    "agent_id": "{agent_id from sessions table}",
    "feature_cycle": "{feature_cycle from sessions table}",
    "session_id": "{session_id}",
    "retrieval_mode": "strict"
  },
  "baseline": null,
  "source": "observations",
  "expected": {
    "entry_ids": [123, 456, 789]
  }
}
```

**The `expected.entry_ids` field is the ground truth.** This is what P@5 and MRR are
measured against. Do not leave it null. Do not copy from baseline. These must be entry IDs
the agent actually retrieved in that session.

### Where to save

Save the new scenarios to: `product/research/ass-039/harness/scenarios.jsonl`

Then **delete** `product/research/ass-037/harness/scenarios.jsonl` — it is invalid and
must not be used for any future eval run. Also delete the stale results:
`product/research/ass-037/harness/results/` — all results in that directory were computed
against the invalid scenario set and should not be referenced.

Copy the new scenarios to `product/research/ass-037/harness/scenarios.jsonl` so the
ASS-037 harness infrastructure (snapshot.db, vector files) can be reused for the
ablation re-run below.

### Minimum viable scenario set

Before running hypothesis tests, verify the scenario set contains:
- ≥ 50 context_search-derived scenarios with populated expected.entry_ids
- ≥ 10 context_briefing-derived scenarios
- Scenarios spanning ≥ 10 distinct feature_cycles
- ≥ 1 scenario from each major agent_role (architect, rust-dev, tester, specification)

If fewer than 50 total scenarios are recoverable from observations, document the count
and the gap — do not proceed to formula ablations with an insufficient scenario set.

### ASS-037 ablation re-run

Once the proper scenario set is built, re-run the ASS-037 Q4 signal ablations using the
ASS-037 snapshot (snapshot.db + vector files) and the new scenarios.jsonl.

Run at minimum:
- `conf-boost-c` (w_sim=0.50, w_conf=0.35, w_nli=0.00) — the current recommendation
- `ablation-conf-zero` (w_sim=0.85, w_conf=0.00, w_nli=0.00) — validate confidence contribution
- `ablation-cosine-only` (w_sim=0.85, all others zero) — cosine floor
- `baseline-nli` (original production weights) — was this ever actually better?

Also run the two phase signal ablations that were missing from ASS-037:
- `ablation-phase-zero` (w_phase_histogram=0.00, w_phase_explicit=0.00)
- `ablation-util-prov-zero` (w_util=0.00, w_prov=0.00)

Update `product/research/ass-037/FINDINGS.md` Q4 table with the re-run results.
Update `product/research/ass-037/RECOMMENDATION.md` formula section if the findings change.

---

## Output

1. **Scenario set**: `product/research/ass-039/harness/scenarios.jsonl` — behaviorally
   grounded, expected.entry_ids populated, ≥50 scenarios from real agent queries
2. **Scenario construction report**: count by source type, feature_cycle coverage,
   agent_role distribution, any gaps in the observations data
3. **ASS-037 ablation re-run**: updated Q4 signal attribution table with valid ground truth;
   confirmed or revised formula recommendation
4. **H1 result**: Goal cluster overlap table — intra-cluster vs. inter-cluster entry overlap,
   with pass/fail verdict and effect size
5. **H2 result**: Success-ratio distribution — entry success ratios, load-bearing entry
   candidates, gap signal entries from rework cases
6. **H3 result**: Phase stratification table — intra-cluster cross-phase overlap vs.
   same-phase overlap
7. **Feasibility assessment**: Observations data quality — how many cycles have sufficient
   data, what gaps exist
8. **context_cycle_review design** (if ≥2 hypotheses pass): edge emission spec, goal-cluster
   store schema, briefing enhancement design, cold-start behavior
9. **Delivery sequence**: what ships in the context_cycle_review extension vs. what is gated
   on PPR expander architecture (ASS-038 Recommendation 2)

---

## Constraints

- **Read-only against live DB.** No writes, no schema changes, no production impact.
- **Harness and scripting only.** No app code changes. Analysis scripts in
  `product/research/ass-039/`.
- **Delete the invalid scenario set.** `product/research/ass-037/harness/scenarios.jsonl`
  and all contents of `product/research/ass-037/harness/results/` must be deleted before
  any eval re-run. They were built on human orchestration commands with null ground truth
  and must not be referenced in any future work.
- **Never leave expected null.** Every scenario in the new set must have
  `expected.entry_ids` populated with behavioral ground truth. A scenario with null expected
  is not a scenario — it is a retrieval log.
- **GH issue fetch is required.** Goal statements are not in the DB — they are in GH issues.
  Use `gh issue view {number} --json title,body` for each feature_cycle that maps to an
  issue number.
- **Do not implement the context_cycle_review extension.** The design output is a spec,
  not code. Implementation belongs in a delivery session scoped from this spike's findings.
- **Domain-agnostic interpretation required.** Every finding must include an assessment of
  whether it would hold for a corpus with different outcome distribution, domain vocabulary,
  or team size.

---

## What This Is Not

This spike does not implement the PPR expander architecture (ASS-038 Recommendation 2).
It does not modify context_cycle_review. It does not build a GNN. It validates three
hypotheses against existing data and, if they hold, produces the design spec for one
targeted extension to an existing feature.
