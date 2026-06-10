# FINDINGS: PPR Re-Evaluation + Retrieval Tuning Re-Validation at Current Corpus Scale

**Spike**: ass-074 (GH #721)
**Date**: 2026-06-10
**Approach**: Measurement — edge inventory + **live nan-018 harness A/B run** (baseline vs expander-on) on the prod realism snapshot
**Confidence**: empirical (direct census + 683-scenario harness run) for the edge inventory, the expander-activity finding, and the relationship-mechanism finding; the relevance verdict is **deliberately not reduced to the soft-truth P@K/MRR metrics** — see the methodology note.

> **Methodology correction (read first).** An earlier draft of this FINDINGS.md
> stopped at a Phase-0 edge census (SQL only) and concluded the expander was
> *dormant / "nothing to expand into" / defer ES-4/5/6*. That was wrong: it never
> ran the harness the scope is built around. The harness was subsequently run
> (`unimatrix snapshot` → `eval scenarios` → `eval run` baseline vs expander-on →
> `eval report`, plus a graph-edge trace of every injection). **The empirical run
> overturns the desk-analysis verdict.** This document reflects the run.

---

## Primary discovery — the platform cannot measure its own graph layer

**The single most important finding of this spike is not about PPR. It is that
Unimatrix has no automated way to measure the one capability that differentiates
it.** The product thesis is *similarity **+** graph relationships > similarity
alone*. Yet every automated quality signal available collapses back to cosine
similarity, so the graph-relational layer is effectively unmeasurable:

- **P@K / MRR** are scored against soft ground truth that *is* the cosine-ranked,
  expander-off baseline. A relationally-useful but *dissimilar* entry — exactly
  what PPR exists to surface — scores as a **regression by construction**. The
  harder PPR works, the worse it looks.
- **The edges themselves are similarity-derived.** `cosine_supports` mints **238 of
  282** `Supports` edges from cosine. Much of the graph is similarity wearing a
  relationship hat — which is why expanding along it produced no diversity gain.
- **Even the "noise removal" signal in this very run** (near-zero-sim entries
  237→37, mean similarity 0.358→0.373) measures *"better" as "higher cosine."* The
  instrument cannot escape its own ruler.
- **Even the nan-018 fixture corpus** — the project's only labeled-correctness
  asset — asserts *status/penalty* behavior (`forbidden_absent`,
  `redirect_to_head`, `rank_below` for deprecated suppression). It has **no
  assertion for "this dissimilar entry was relationally relevant."**

**Consequence:** PPR / graph-relational retrieval can only ever look
neutral-to-negative under the metrics that exist, regardless of how well it works.
The only way this investigation could establish that the expander works was to
**read results and trace graph edges by hand** (qlog-1073, qlog-611). That is not a
scalable evaluation method. Until a labeled *relational-relevance* corpus exists,
the platform is steering its graph features blind. See **Future work — the
relational-relevance corpus** below.

---

## Executive verdict

**The PPR expander works as designed and does not overpower. PPR is not dead.**

Turning `ppr_expander_enabled = true` at current prod edge density changes the
result set in **~48% of queries** (329/683), and **every injection traces to a
graph edge connecting the injected entry to a seed** — the expander follows
*relationships*, not cosine. It behaves selectively: it removes near-zero-relevance
noise, replaces, or fills sparse result sets, without flooding. On the examples
traced by hand, the added entries were defensibly relevant.

The aggregate soft-truth metrics (P@5 −0.0419, MRR −0.0091) read as a regression,
but **those metrics cannot judge a retrieval-changing feature** — soft ground truth
*is* the expander-off baseline, so any change scores as a regression by
construction. The honest evaluation is the relationship trace, not the P@K delta.

**The one real limitation is the edge MIX, not the expander.** The corpus's
positive edges are dominated by `cosine_supports`-minted `Supports` edges (a
similarity proxy), so much of the expansion re-surfaces semantic neighbors and
diversity (CC@5/ICD) is flat. The relationship-native edges that make PPR
worthwhile — `CoAccess` (real co-retrieval), `Informs`, and especially
`RelatedTo` (**0 edges**, never generated) — are thin. This is **historical**
(CoAccess predates the typed-edge set; the protocol does not currently encourage
`RelatedTo`), not a defect in the expander.

**crt-053 (#717): ES-4/5/6 is dormant ONLY because the flag ships off — NOT because
injection can't happen.** Flip the flag and PPR/graph_expand injection fires in
~48% of queries. So crt-053 is a genuine **guardrail that must land before the
expander is enabled**, not a safe defer.

---

## The harness run (the load-bearing evidence)

**Corpus**: prod snapshot `~/.unimatrix/0d62f3bf1bf46a0a` → `/tmp/eval/snap.db`
(89.8 MB; HNSW index `unimatrix-6687`, ~6,687 vectors; 1,898 `vector_map` rows;
2,352 entries with `embedding_dim>0`). **683 scenarios** mined from `query_log`.

**Profiles**: `baseline` (compiled defaults = production, `ppr_expander_enabled =
false`) vs `expander-on` (`[inference] ppr_expander_enabled = true`,
`ppr_blend_weight = 0.15`, `ppr_max_expand = 50`). Plumbing verified end to end:
`parse_profile_toml` → `config_overrides.inference` → `layer.rs:378` Arc →
`services/mod.rs:438` → `search.rs:911 if self.ppr_expander_enabled { …graph_expand BFS… }`.

### Aggregate (Section 1 of the report)

| Metric | baseline (off) | expander-on | Δ |
|---|---|---|---|
| P@5 | 0.3705 | 0.3286 | −0.0419 |
| MRR | 0.5239 | 0.5148 | −0.0091 |
| CC@5 (coverage) | 0.3265 | 0.3255 | −0.0010 |
| ICD (diversity) | 0.6752 | 0.6709 | −0.0043 |
| Avg latency | 13.4 ms | 14.1 ms | +0.6 ms |

**Soft-truth caveat (decisive for interpretation):** P@5/MRR are scored against
soft ground truth = the baseline's own returned entries. Any entry the expander
changes is by definition "not in ground truth" and scores as a regression. The
−4.2pt P@5 is therefore mostly *change vs production*, not *worse than production*.
CC@5/ICD are absolute (not baseline-relative) and are flat — that is the
trustworthy aggregate, and it says **no diversity gain** (see edge-mix below).

### Behavior (direct diff of the 683 result JSONs)

- Result set changed in **329/683 (48.2%)**; expander injected ≥1 entry not in the
  baseline top-5 in **328 (48%)**; **151 distinct** entries newly surfaced.
- **Pure-fill** (added, dropped nothing): 79 scenarios. **Replacement** (added AND
  dropped): 249. Baseline returned **<5 entries in 237/683 (35%)** — the
  HNSW+penalty pipeline is frequently sparse; the expander fills it.
- **It removes noise:** near-zero-similarity (<0.05) entries in the top-5s drop
  **237 → 37**; mean similarity of returned entries 0.358 → 0.373. So it
  preferentially displaces semantically-irrelevant entries the baseline ranked via
  graph/confidence penalty terms.
- It does **not** flood: median injected vs dropped similarity is ~equal
  (0.333 vs 0.339); the change is selective, not a dump.

### Mechanism — every injection is relationship-driven (graph-edge trace)

For each injected entry, the edge connecting it to a baseline seed:

| Scenario | Query (abbrev) | Seed | Injected | Edge |
|---|---|---|---|---|
| qlog-1073 | "...fix/benchmark-realpath-trap branch, why?" | 525 *Worktree Isolation ADR* | 553 *How to validate worktree isolation* (proc) | `525 ⟷ CoAccess ⟷ 553` |
| qlog-611 | context_cycle / session attribution | 3373 *col-024 Structured Log ADR* | 981 + 3382 (session-attribution cluster) | `CoAccess` to 3373 (and to each other) |
| qlog-17 | "recheck the production config.toml" | 2395 *Two-Level TOML Config Merge* | 2286 *dsn-001 Config Merge Replace Semantics* | `2395 → Supports → 2286` |
| qlog-1116 | vnc-030 AC-04 close/convert | 4761 *sweep_stale_sessions* | 4742 *vnc-025 Purge Points ADR* | `4761 → Supports → 4742` |
| qlog-1024 | gate 3b review | 4315, 4791 | 4309, 4797 | `Supports` to 4315 / 4791 |

`qlog-1073` and `qlog-611` are **pure PPR** — `CoAccess` (actual co-retrieval)
surfaces related-not-similar entries (a worktree-validation *procedure* next to a
worktree *ADR*; a co-accessed session-attribution *cluster*). Those are the wins
the feature exists to produce.

### Why diversity is flat — the edge MIX

The `Supports`-driven injections (qlog-17/1116/1024) ride edges minted by
`cosine_supports` — **238 of 282 Supports edges are auto-generated from cosine
similarity**. Expanding along them re-finds semantic neighbors → no new categories
→ CC@5/ICD flat. The relationship-native signal (`CoAccess` co-retrieval,
`Informs`, `RelatedTo`) is the part that surfaces genuinely novel material, and it
is thin: `RelatedTo` = 0, and only 444 of 31,124 raw `co_access` rows are promoted
to edges. **The lever is edge mix, not edge count.**

---

## Findings (against SCOPE Goal questions)

### Q1 — PPR verdict: keep (expander on) / zero out / build-edges-first?

**Answer: the expander works and is a viable activation candidate; its value is
gated on relationship-native edge maturity, not on the algorithm.** This is *not*
the earlier "build-edges-first because PPR is dead" — the expander demonstrably
expands along relationships and does not overpower. The remaining work to make
activation *worthwhile* (vs merely correct) is to grow the relationship-native
edges:

1. **Enable the `RelatedTo` write path** (0 edges today despite vnc-015/ADR-006
   #4429 adding it to the positive set) — the cheapest diversity lever.
2. **Weight `CoAccess`/`Informs` over `cosine_supports`** in the PPR walk so
   expansion follows behavior/meaning rather than re-derived similarity (would
   move CC@5/ICD, which `cosine_supports` cannot).
3. Do **not** formally `ppr_blend_weight = 0.0` (Q3b's literal rec) — that drops
   co-access (crt-032 #3785 made PPR its sole carrier) and is contradicted by the
   run. Leave the expander off for now (it's correct but diversity-neutral until
   the edge mix matures); revisit activation after (1)/(2).

A formal zero-vs-enable production decision remains an ASS-037-class real-distribution
decision per ADR-006 (#4894) — out of scope here. This spike supplies the evidence.

### Q2 — crt-053 scope: is ES-4/5/6 (injection-path leak) live or dormant?

**Answer: DORMANT TODAY, but only because `ppr_expander_enabled = false` ships off
— it is NOT structurally safe.** (Reverses the earlier draft.) The run proves the
injection path fires in ~48% of queries the instant the flag flips. Therefore:

- crt-053's ES-4/5/6 is the **required guardrail before the expander is enabled**,
  not a safe defer. It can be sequenced *with* expander-enablement work, but must
  land *before* the flag flips.
- ass-073's confirmed HNSW-path eviction fix remains crt-053's other load-bearing
  half. crt-053 can come off HOLD; ES-4/5/6 stays coupled to expander-enablement.

**The leak, confirmed in `main` (`services/search.rs`):** `penalty_map` is built at
Step 6a (`:721–754`) over the HNSW set *only*; the expander injects at Step 6d /
Phase 0 (`:911–967`) *after* it. Injection skips **quarantined** (`:950`) but not
**deprecated/superseded**. Final scoring (`:1284`) reads
`penalty_map.get(&id).copied().unwrap_or(1.0)` → an injected stale entry gets
**penalty 1.0**, bypassing the entire crt-014 steepness penalty.

**Operational decision (2026-06-10, human):** the expander is being **enabled in
prod now**, ahead of crt-053, as a *capped-risk* call. Rationale, accepted
eyes-open: all 1,146 current PPR edges are **Active→Active** — there is **no
positive edge pointing at a deprecated/superseded entry today**, so the leak has no
live carrier; the generators are not minting stale-targeted edges; the flag is
reversible in one line; and **crt-053 is the immediately-next feature**, bounding
exposure to one cycle. The leak is a *latent* correctness gap, not a live one, on
the current graph. crt-053 must still land before edge generation could plausibly
create a stale-targeted edge. (Optional belt-and-suspenders until then: a one-line
log if any injected entry is deprecated/superseded, to make the capped risk
observable.)

### Q3 — Edge inventory (verified census, prod DB, 2026-06-10)

| relation_type | count | provenance | PPR-positive |
|---|---|---|---|
| CoAccess | 444 | `co_access/tick` 440, agent 4 | ✅ |
| Informs | 419 | `S1` 411, agent 8 | ✅ |
| Supports | 282 | `cosine_supports` 238, agent 44 | ✅ |
| Prerequisite | 1 | agent | ✅ |
| **RelatedTo** | **0** | — | ✅ (never generated) |
| Advances | 2 | agent | ❌ (excluded) |
| **Total** | **1,148** | (1,146 PPR-positive) | |

- Entries: 4,915 (Active 1,781 / Deprecated 633 / Quarantined 2,501; Proposed 0).
- All 1,146 PPR edges are **Active→Active**; 0 dangling, 0 self-loops; 0 bootstrap-only.
- Recency: CoAccess/Informs last `2026-06-10` (today), Supports `2026-06-07` —
  generators are live.
- Connectivity: **600 entries (12.2%)** have ≥1 PPR out-edge; 759 touch any PPR
  edge; **4,156 (84.6%) isolated**; out-degree mean 1.91 / median 1 / max 8.
- Density: **0.233/entry** (all), 0.475 (non-quarantined), 0.643 (active) — below
  Q3b's ~1.0/entry. Informs/entry 0.085 (all) / 0.235 (active).
- **`co_access` table holds 31,124 raw rows; only 444 are promoted to CoAccess
  edges** — a throttled promotion threshold is a second density/mix lever.

Durable answer: *we generate CoAccess/Informs/Supports automatically and live, but
under-dense and similarity-skewed; `RelatedTo` is not generated at all.*

### Q4 — Formula-still-holds verdict at current scale

**No drift signal; no re-ablation run or warranted.** This spike's harness run was
a PPR/expander A/B, not a confidence-formula sweep. The standing evidence: ass-073's
fresh-snapshot baseline (P@5 0.3695 / MRR 0.5212) is *above* the 2026-03-26 platform
baseline (0.3058 / 0.4181), so the ASS-037 sim/conf core still holds at current
scale. Phase-3 escalation condition (drift) not met. Re-fit the PPR term only when
the expander is actually introduced to the deployed formula.

---

## Future work — the relational-relevance corpus (NEXT SESSION)

**This is the highest-value follow-up and the direct consequence of the Primary
Discovery. The human will take it up in a separate session.** A candidate spike:

- **Goal**: build the platform's first *relational-relevance* labeled corpus — a set
  of `(query → entries that are related-but-**dissimilar** and genuinely useful)`
  judgments — and a harness assertion that scores it. This is the instrument the
  platform currently lacks: it measures the graph layer on its own terms instead of
  through a cosine proxy. It is distinct from the nan-018 fixture corpus, which
  asserts *status/penalty* behavior, **not** relational usefulness.
- **Why it matters**: without it, every graph-relational feature (PPR/expander,
  CoAccess weighting, `RelatedTo`, future Informs/edge work) is unmeasurable and can
  only be validated by hand-tracing — as this spike had to. It is also the only way
  to *quantify* the expander benefit just enabled in prod, rather than trusting
  three hand-traced examples.
- **Shape (sketch, to refine in scope)**: 30–50 queries with human-judged
  related-but-dissimilar `expected` entries; a new property assertion (e.g.
  `relationally_present` / `related_rank_above_noise`) in the eval runner;
  baseline-vs-expander scored against it. Pairs with the nan-018 fixture-corpus
  authoring path (`crates/unimatrix-server/src/eval/corpus/`).
- **Companion experiment** (cheap, can fold in): re-run expander-on with the PPR
  positive set restricted to `CoAccess`/`Informs`/`RelatedTo` (drop the
  `cosine_supports`-derived `Supports`) and measure whether CC@5/ICD finally move —
  isolating the relationship-native signal from the similarity proxy.

## Unanswered / deferred

- **Does a relationship-native walk move diversity?** (Folded into the companion
  experiment above — not run in this spike.)
- **Quantified relevance better/worse for the expander** — blocked on the
  relational-relevance corpus above. Query-log soft truth structurally cannot
  answer it.

## Out-of-scope discoveries

- **`RelatedTo` write path is effectively unused** (0 edges) and the protocol does
  not currently encourage the relationship. Human will review separately. Highest-
  leverage diversity lever for PPR.
- **`co_access` promotion is heavily throttled** (31,124 raw → 444 edges). Tuning
  this is a cheap relationship-native density lever.
- **Live searchable corpus (~1,898 vector_map / 2,352 embedded)** is smaller than
  the "~6.7k vectors" HNSW headline; the index retains historical data IDs. Worth a
  one-off reconciliation before future "passed the retest threshold" judgments.

## Recommendations Summary

- **PRIMARY — measurement gap**: the platform **cannot measure its graph layer** —
  every automated signal (P@K/MRR soft truth, the `cosine_supports` edges, even the
  fixture corpus) reduces to cosine, so graph-relational retrieval looks
  neutral-to-negative no matter how well it works. **Build a relational-relevance
  labeled corpus** (see Future work) — it is the prerequisite for measuring any
  graph feature, including the expander just enabled.
- **PPR**: the expander **works as designed and does not overpower** (injects in
  ~48% of queries, every injection edge-traced to a seed, selective not flooding;
  hand-proven benefit on qlog-1073/qlog-611). **Now enabled in prod** (capped-risk
  decision — see Q2). Value is still gated on relationship-native edges: enable
  `RelatedTo` (0 today) and weight `CoAccess`/`Informs` over the similarity-derived
  `cosine_supports`. Do NOT zero `ppr_blend_weight` (drops co-access).
- **crt-053**: the ES-4/5/6 leak is **latent, not live** on today's all-Active→Active
  graph, which is why enabling the expander now is a bounded risk. crt-053 is the
  **immediately-next feature** and remains the required guardrail before edge
  generation could create a stale-targeted edge. HNSW-path eviction remains its
  other load-bearing half.
- **Edge inventory**: 1,146 PPR-positive edges, similarity-skewed
  (`cosine_supports` dominant), `RelatedTo`=0, 0.23/entry, 85% isolated. The gap is
  **mix more than count**.
- **Formula**: no drift; no re-ablation; ass-037 sim/conf core holds at scale on
  ass-073's above-baseline numbers.
