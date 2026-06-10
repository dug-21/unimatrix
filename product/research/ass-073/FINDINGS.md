# ASS-073 — FINDINGS

**Spike:** ass-073 (GH #720) — measure crt-053's Q5/Q8 on the nan-018 eval harness
**Date:** 2026-06-10 · **Type:** Measurement (empirical) · **Confidence:** Empirical (real ONNX embeddings + real prod snapshot)
**Goal (human):** establish whether the search algorithm is **correct**. The Q8/Q5 tuning numbers are outputs of that assessment, recorded for crt-053 (TABLED pending this).

## How it was run

- **Track A — fixture corpus (trust/correctness):** drove `run_fixture_sweep` (`eval/runner/sweep.rs:115`) over the five shipped fixtures with the **real ONNX provider** (`all-MiniLM-L6-v2`, `OnnxProvider::new(EmbedConfig::default())`) — not the deterministic test provider — embedding corpus + query with the same model so retrieval is realistic. Graduated sweep, baseline + coarse `clean_replacement` {0.40→0.30→0.20→0.10→0.05} + `multiplier` {1.0→0.5→0.25→0.10}, k=5. Trust read from `ProfileResult.trust` (populated by `replay.rs:179`, `evaluate_trust`). Strict mode probed by re-running each scenario through the same `SearchService.search` with `RetrievalMode::Strict`. There is **no CLI for the fixture sweep** — it is reachable only via the runner; the measurement was driven by a scratch `#[ignore]` test harness (since reverted — no code left behind).
- **Track B — realism snapshot (relevance/cost):** fresh `unimatrix snapshot` of the **prod DB** (89.7 MB, HNSW index ~6.7k vectors) → `eval scenarios` (675 real query_log scenarios) → `eval run` baseline → `eval report`.

---

## RQ-1 — Q8 steepness + leak-vs-steepness diagnosis  ⟶ **STEEPNESS-ADDRESSABLE; the leak path is NOT exercised by this corpus**

On the fixture corpus, every deprecated entry arrives via the **penalized HNSW path**: its `final_score` responds monotonically to the levers. The leak (injection at penalty=1.0, #4887) **does not reproduce** — the fixtures author only `Supersedes` edges, no positive/co-access edges, so `graph_expand`/PPR injection has nothing to inject. The sweep therefore measures the penalized path only and is **blind to the leak by construction**.

Evidence (`deprecated-connected.rank-below-band`, real ONNX, Flexible) — penalty is live and monotonic:

| entry (sim) | baseline | cr_0.30 | cr_0.20 | cr_0.10 | mult_0.5 | mult_0.25 | mult_0.10 |
|---|---|---|---|---|---|---|---|
| db.dep1 (0.692, orphan) | 0.2597 | 0.2597 | 0.2597 | 0.2597 | 0.1298 | 0.0649 | 0.0260 |
| db.dep2 (0.673, clean-repl d1) | 0.1347 | 0.1010 | 0.0673 | 0.0337 | 0.0673 | 0.0337 | 0.0135 |
| db.actStrong (0.660, Active) | 0.3298 | 0.3298 | 0.3298 | 0.3298 | 0.3298 | 0.3298 | 0.3298 |

`clean_replacement` moves only chain entries (dep2), `multiplier` moves all deprecated severities, actives are untouched — exactly as designed. **No flat-at-1.0 (leaked) deprecated entry appeared in any shape.**

**Crossover / guarantee threshold:** *not findable as authored.* At **baseline penalties** every deprecated entry already ranks **below** the active it is paired against **whenever that active is present**. So for the present-anchor pairs the rank-below property holds at steepness = 0 — there is no crossover to bracket. Steepness is not the load-bearing variable for ranking on this corpus.

---

## RQ-4 — Trust in both modes  ⟶ **the modes differ structurally; this is the core correctness finding**

| Mode | Mechanism | Stale entries | Trust outcome on real shapes |
|---|---|---|---|
| **Flexible** (search) | multiplicative **penalty** only | **demoted, retained** in top-k | absence can FAIL; rank holds for present anchors |
| **Strict** (briefing) | hard **status filter** | **evicted** | passes (results are Active-only) |

Strict-mode results (baseline penalties): `dangling-deprecated` PASS (stale `cache.stale` gone), `deprecated-connected` PASS (deps evicted), `multi-correction` PASS, `superseded-active` PASS. The one `TRUST_FAIL` is `dead-end-chain` → head `flag.terminal` absent — and that is the **defined fail the fixture name promises** (a dead-end chain has no Active terminal head; correct behavior, not a bug).

**Implication for crt-053:** the load-bearing correctness lever is **eviction (status filter), not penalty steepness**. Strict/briefing already evicts; Flexible/search only demotes. A multiplicative penalty cannot satisfy a `forbidden_absent` property in a small result set — it lowers score but does not remove the entry from top-k (see RQ-1 `dangling-deprecated`: `cache.stale` stays in top-5 even at `mult_0.10`). This matches #4887/#4888: trust/absence needs enforcement at admission, not a post-hoc multiplier.

---

## RQ-3 — #406 graph-snapshot multi-hop chain  ⟶ **carried; #406 does NOT reproduce in the eval rebuild**

`multi-correction-chain.redirect` (A→B→C→head, depth>1) **passes at every steepness**: `jwt.head` ranks #0 and every present superseded member ranks below it. `redirect_to_head` resolves the terminal head via `find_terminal_active`, which can only succeed if the rebuilt `TypedGraphState` carries the **full multi-hop `Supersedes` chain** at search time. It does. Confidence: high (end-to-end behavioral proof through the real search path). A direct graph dump was unnecessary — the redirect verdict exercises the chain.

---

## RQ-2 — Q5 relevance/cost baseline + regression bound  ⟶ **baselined**

Fresh prod snapshot, 675 query_log scenarios, baseline (compiled defaults), 2026-06-10:

| P@5 | MRR | CC@5 | ICD | Avg latency | Mean cost (tokens/top-5) | Median | Max |
|---|---|---|---|---|---|---|---|
| **0.3695** | **0.5212** | 0.3263 | 0.6752 | 12.9 ms | **2228.8** | 1838 | 14643 |

Above the recorded 2026-03-26 platform baseline (P@5 0.3058 / MRR 0.4181). **Drift caution (#4886/#500):** soft ground truth = the prod system's own prior output, so it ages; re-snapshot when measuring crt-053.

**Recommended regression bound for crt-053 AC-11:** guard on **MRR**, not P@5. A status-filter/leak-fix *correctly evicts* stale entries that were in the baseline's own output (= soft GT), so **P@5 against soft GT will mechanically drop** — a P@5 decline alone is **not** a regression for this change (it is the #500 KB-drift trap in another guise). Proposed: **MRR drop ≤ ~0.05 absolute (~10%) is acceptable** when paired with a trust improvement and non-increasing cost; a larger MRR drop = "silently tanked recall" → fail. Re-snapshot to re-establish soft GT after the leak fix before judging recall.

---

## RQ-5 — Corpus sufficiency  ⟶ **INSUFFICIENT — corpus-revision request back to nan-018 (anticipated; ADR-004 §5)**

Two concrete defects in `deprecated_connected.toml`:

1. **`rank_below(db.dep3, db.actMid)` can never pass in Flexible.** `db.actMid` ("Connection pool health checks") has too-low similarity to the query "how should I size the database connection pool" and **never enters top-5**. The asymmetric rule (B-absent while A present ⇒ FAIL) then fails the assertion at **every** steepness — a corpus artifact masquerading as a steepness/trust failure. (This is the only Flexible failure for this shape.)
2. **No crossover to bracket.** At baseline, deps already rank below present actives, and the leak path is unreachable (no positive edges). ADR-004 §5's "bracketed range of crossover points" is not realizable on this corpus.

**Revision request to nan-018:** (a) re-pair `rank_below` anchors so each B-anchor is actually retrievable at k for its query, or raise k / tune fixture text so `actMid` enters the set; (b) to make the corpus able to exercise the **leak** (the real crt-053 defect), author **positive/co-access edges** on the deprecated band so `graph_expand`/PPR injection can re-admit a stale entry at penalty=1.0 — without this, the corpus structurally cannot test the load-bearing change.

---

## Recommendations Summary

**Search-correctness verdict: the ranking core is correct; the gap is an enforcement-shape gap, not a steepness gap.**

1. **Ranking is correct on the penalized path.** Actives outrank deprecated at baseline; penalty levers are live, monotonic, and topology-correct; multi-hop correction chains resolve to the active head (RQ-3); no leaked penalty=1.0 entry appeared.
2. **Eviction, not steepness, is load-bearing.** Strict/briefing evicts stale entries and trust holds; Flexible/search only demotes, so `forbidden_absent` cannot be guaranteed by any penalty magnitude in a small result set. crt-053 should treat the **status filter at every admission stage** (the #4887 leak fix) as the primary change; steepness is secondary tuning. Q8 has **no empirically required steepness** from this corpus (baseline already holds rank-below for present anchors).
3. **The leak is unmeasured, not absent.** This corpus cannot exercise the injection/PPR leak path; ass-073 can neither confirm nor quantify ES-4/5/6. Confirming the leak fix needs the corpus revision in RQ-5 (positive edges) — or a real-distribution probe (ASS-037 class).
4. **Q5 bound:** baseline MRR 0.5212 / P@5 0.3695 / mean cost 2229 tok. Gate crt-053 on MRR (≤~0.05 abs drop) with cost non-increasing; do **not** gate on soft-GT P@5 (mechanically drops when stale entries are correctly evicted); re-snapshot after the fix.
5. **#406 does not reproduce** in the eval graph rebuild.

**crt-053 can come off HOLD** with: leak-fix (status filter at injection) as the primary work, steepness as optional secondary tuning, MRR-based regression bound, and a corpus revision requested from nan-018 before the fixture corpus is used to *prove* the leak fix.

### Unanswered / blocked
- **Quantified leak severity (ES-4/5/6):** blocked — fixture corpus cannot trigger the injection path (RQ-5 revision needed). Not steepness-addressable.
- **Deployment steepness authority:** out of scope — ASS-037 real-distribution decision (#3984, ADR-006 #4894); this fixture sweep is never the deployment authority.

### Out-of-Scope Discoveries
- `eval report` logs `WARN: skipping results/profile-meta.json (parse error ... missing field scenario_id)` — the report's results-dir scan tries to parse the `profile-meta.json` sidecar as a scenario result. Cosmetic (report still generated correctly); flag to nan-018 as a minor harness nit.
