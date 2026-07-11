# FINDINGS: ass-098 — The topical-relevance ODOMETER on the corrected unit (`context_search(Q) → top-k`)

**Spike**: ass-098 (RE-RUN)
**Date**: 2026-07-11
**Approach**: measurement + proof-of-concept (G1–G5); investigation (G6–G8)
**Confidence**: empirical (G1–G5), directional (G6–G8)

---

> **This document SUPERSEDES the prior ass-098 findings, which are RETRACTED.** The prior run
> graded the **wrong surface** — whole-prompt `uds` UserPromptSubmit turns injected verbatim as the
> search string — and reported it as `context_search` quality. Its headline G4 number (nDCG@5
> **0.615** / "40% of prompts retrieve nothing relevant") is withdrawn per the **#951 correction
> comment**. A live probe (row 5498) proved `context_search(Q) → top-k` *is* a clean, logged,
> gradeable unit, but the historical log holds **zero real `context_search` traffic**, so there is
> no real search history to grade. This re-run therefore **builds the query set by synthetic
> reverse-QA** and grades the correct unit. The judge, metric, and discrimination machinery from the
> prior run were sound and are reused unchanged. **The `uds`/`cycle_events.goal` surfaces are demoted
> to separate lesser cross-checks and are NEVER pooled with the headline below.**

**Headline (correct surface, n=80 reverse-QA queries, paired snapshot):**

| Metric | Value | 95% CI |
|---|---|---|
| **Graded nDCG@5** (judge odometer) | **0.699** | 0.646 – 0.746 |
| **Known-item recall@5** (objective, E-as-target) | **0.775** | 0.675 – 0.863 |
| **Known-item recall@10** | 0.863 | — |
| **Known-item MRR** | 0.658 | 0.564 – 0.746 |
| frac queries with ≥1 relevant in top-5 | 0.963 | — |
| frac queries with a best-answer (grade 3) at rank 1 | 0.625 | — |

**Discrimination (two-sided control):** IDEAL **1.00** > HOLD(actual) **0.70** > shuffle **0.47** >
distractor **0.34** > truncate **0.00** — monotone; both nDCG@5 and MRR move together. This is the
positive/negative control ass-097 declared structurally impossible under self-GT.

**Honesty flag (carried prominently):** the query bank is **synthetic** (reverse-QA), standing in for
`context_search` traffic that does not yet exist. It validates the *instrument* and gives a real
first number on the correct surface; it does **not** measure real-world question distribution. A
real-usage odometer still requires actual `context_search` adoption (see Unanswered Questions).

---

## Findings

### Q: G1 — Query-bank construction via synthetic reverse-QA + the `query_log` dead-end (empirical)

**Answer.** The `query_log` mining plan is **empirically dead** for the surface that matters, and a
**low-leakage synthetic reverse-QA bank that discriminates** was built successfully. **Premise gate:
PASS.**

**Evidence.**
- **`query_log` dead-end confirmed on the snapshot.** Source split in `eval/snap.db`:
  `uds`/strict = **3,101**, `mcp`/flexible = **0**. (Prod DB: 3,105 `uds` vs the single `mcp` probe
  row 5498.) `context_search(Q) → top-k` *is* the correct gradeable unit (probe row 5498 logged
  request→exact top-5), but it has **zero organic history** — nobody explicitly searches; all rows
  are hook auto-injection. Mining the log for real queries is not viable.
- **Bank built by reverse-QA.** Sampled **80 Active (status=0) entries** from the paired snapshot,
  stratified across category: decision 19, pattern 16, lesson-learned 15, procedure 10, capability 8,
  convention 6, goal 6. **Deprecated (893) and Quarantined (2,504) excluded as targets** — they should
  recede, not be sought — but retained **in-corpus as distractors** (they are searchable and can be
  returned). For each E, `claude -p` (Sonnet) generated a focused developer **search query** phrased
  for the *problem* E answers, with explicit anti-leakage instructions (no ADR/feature ids, no file
  paths, no verbatim symbol names, no quoting).
- **Leakage control (reported as a premise gate).** Content-word **Jaccard(Q, E)**: mean **0.079**,
  median 0.073, p90 0.127, **max 0.258** — **0 of 80 exceeded the 0.35 flag threshold**, so no
  regeneration was needed. (Query-word coverage-in-E mean 0.544 is expected — a good query shares
  domain vocabulary — but Jaccard confirms Q does not copy E's distinctive wording.) The bank is
  low-leakage AND it discriminates (G4), so the premise holds.

**Caveats carried:** (a) **synthetic ≠ real usage** — flagged as the primary future item; (b)
**known-item non-uniqueness** — E is *a* correct answer, not the only one (G4 shows this matters:
11/11 targets that fell outside top-10 still had a *different* relevant entry retrieved).

**Recommendation.** Use reverse-QA on Active entries as the standing query-bank construction method;
report Jaccard leakage stats every regeneration; keep deprecated/quarantined as distractors, never
targets. The `query_log` is a dead-end query source — do not revisit it until real `context_search`
adoption exists.

### Q: G2 — LLM-judge oracle PoC (empirical)

**Answer.** A throwaway `claude -p` Sonnet judge grades top-k relevance to Q on a 0–3 scale and is
**highly stable** on the reverse-QA task — more stable than the prior run's whole-prompt surface.

**Evidence.**
- **Judge shape:** stdin prompt, `--strict-mcp-config` (MCP disabled so judging never pollutes
  `query_log`), independent oracle (sees only query + entry title/content — never rank, score, or the
  target id). Output: `{"grades":[{"id":N,"grade":0-3}]}`.
- **Stability (same input, two Sonnet passes, 791 entry-grade pairs):** exact-match **0.808**,
  adjacent (≤1) **0.989**, **linear-weighted κ 0.833**, binary-relevance (≥2) κ **0.853**, Spearman
  **0.887**; **aggregate nDCG@5 moved 0.699 → 0.703 (Δ 0.004)**.
- **Cost/latency:** ~4–7 s per call single-threaded; ~30–50 s per 80-query pass at 8 workers.
  This spike ran ~480 `claude -p` calls total (80 generation + 400 judge across 5 passes). **$0 API
  spend** (subscription CLI) — the $75 API-fallback ceiling was never approached.

**Recommendation.** Ship the judge as `claude -p` Sonnet with MCP disabled; the aggregate nDCG@5 is the
stable headline (±≈0.01 test-retest). Keep the rubric one-shot 0–3, strict about grade 3.

### Q: G3 — Human golden-set calibration (empirical; human anchor BLOCKED)

**Answer.** The human anchor remains **BLOCKED** — an AI cannot construct a non-circular human golden
set. The instrument is delivered ready, and an **inter-model proxy** shows the aggregate metric is
robust to judge choice with high-moderate per-entry agreement.

**Evidence.**
- **Inter-model proxy (Sonnet vs Opus, 791 pairs):** exact **0.743**, adjacent **0.995**, weighted κ
  **0.784**, binary κ **0.781**, Spearman **0.877**; **aggregate nDCG@5: Sonnet 0.699 vs Opus 0.709
  (Δ 0.010)**. The odometer *number* barely moves with judge model; per-entry grades agree at κ≈0.78.
- **Golden instrument delivered:** `rqa_golden_for_human.csv` (scratch) — **791 (query, entry) rows
  across the 80 queries**, human `human_grade` column blank, Sonnet grade held in a separate column
  for post-hoc agreement. ~1–2 h of human grading yields the real κ. **This CSV is regenerated for the
  reverse-QA queries** (the prior golden CSV was for the retracted uds queries and must not be reused).

**Recommendation.** Trust the **aggregate** odometer now (judge-model-robust, test-retest Δ≈0.01);
treat **per-entry** grades as provisional (κ≈0.78) until a human grades `rqa_golden_for_human.csv`.
That human pass is the one gate remaining before the instrument is "proven."

### Q: G4 — The success metric + discrimination test (empirical — the crux)

**Answer.** On the correct surface, retrieval is **good** for focused queries, both scores agree and
complement, and the metric **discriminates cleanly**.

**Evidence.**
- **Known-item (objective, no judge):** recall@1 **0.550**, recall@3 0.738, recall@5 **0.775**,
  recall@10 0.863, **MRR 0.658**; 69/80 targets in top-10, median rank **1**.
- **Graded nDCG@5 (odometer):** **0.699** (CI 0.646–0.746); P@5(≥2) 0.458; MRR(≥2) 0.842; mean top-1
  grade **2.263**; 96.3% of queries surface ≥1 relevant entry in top-5; 62.5% get a best-answer at
  rank 1. Per-category: convention 0.87, procedure 0.78, pattern 0.77, decision 0.74, lesson-learned
  0.64, capability 0.60, **goal 0.33** (broad/abstract entries are hardest to retrieve — informative).
- **Complementarity (why both are needed):** per-query corr(known-item RR, judge nDCG@5) = **0.563**
  — correlated (mutually validating) but not redundant. **7/80** queries have the target off rank 1
  yet a grade-3 best-answer *at* rank 1 (known-item under-credits). **All 11/11** queries whose target
  fell outside top-10 still retrieved a different grade-≥2 entry (known-item scores 0; graded credits
  them). This is the non-uniqueness effect the graded score exists to catch.
- **Discrimination — two-sided control (graded, list-manipulation):**

  | Variant | nDCG@5 | P@5 | MRR |
  |---|---|---|---|
  | IDEAL (sort desc) | 1.000 | 0.675 | 1.000 |
  | HOLD (actual) | 0.699 | 0.458 | 0.842 |
  | DEGRADE-shuffle | 0.467 | 0.370 | 0.578 |
  | DEGRADE-distractor | 0.337 | 0.150 | 0.113 |
  | DEGRADE-truncate | 0.000 | 0.000 | 0.003 |

  Monotone across all three metrics — degrade drops it, ideal lifts it. **PASS.**
- **Config-level degrade is INERT (honest null).** Re-running the identical bank under the
  `degraded-weights` confidence profile (starves semantic/trust, over-weights freshness/usage)
  produced **80/80 byte-identical rankings** — recall@5 0.775 unchanged. The confidence-weight lever
  does **not** move reverse-QA retrieval; the candidate set/order is dominated by the
  vector-similarity stage. This **reinforces ass-097** (weight-profile A/B is near-degenerate) and
  is load-bearing for G6: the odometer discriminates changes to the *returned list*, not to
  post-retrieval weights.

**Recommendation.** Report **both** scores; **graded nDCG@5 is the headline odometer** (binary P@5 is
blind to ordering). Known-item recall@5/MRR is the cheap objective anchor to run on every corpus state
without judge spend.

### Q: G5 — Scale-comparability via subsampling (empirical + directional)

**Answer.** The estimate is stable and its uncertainty shrinks as ~sd/√n. **Query-bank** scaling is
delivered; **corpus-size** scaling is BLOCKED (needs a per-size index rebuild) with a difficulty-aware
design specified.

**Evidence.**
- **Query-subsample convergence** (mean nDCG@5 stable ~0.698 at every n; sd=0.230):
  analytic 95% CI half-width = 1.96·sd/√n → **n=60: 0.058, n=80: 0.050, n=100: 0.045, n=150: 0.037,
  n=250: 0.029**. (Far tighter than the retracted uds surface's 0.089 at n=60 — reverse-QA nDCG has
  lower variance because focused queries have a clear best answer.)
- **Corpus-size scaling BLOCKED:** the harness loads one HNSW index per snapshot; testing the metric
  at increasing corpus sizes needs a **per-size index rebuild**, out of scope for the spike window.

**Difficulty-normalization design (directional).** A larger corpus is *harder* (more distractors), so
a flat cross-size nDCG comparison is confounded. Use a **fixed-question growing-corpus panel**: hold
the reverse-QA query set constant, rebuild the index at each corpus size, and report the odometer as a
**delta against a difficulty baseline** — the known-item recall@k of a *held reference target set*
present at every size. Holding nDCG flat while recall@k of the reference set falls under growth is
itself the "improving" signal.

**Recommendation.** Standardize on **n ≥ 150** panels for gate use (CI half-width < 0.037). Run the
corpus-size sweep as a one-time follow-up with the fixed-question panel + per-size rebuild.

### Q: G6 — crt-feature gate protocol + minimum discriminating power (directional)

**Answer.** The reverse-QA odometer is a **genuine, well-powered gate for retrieval-list changes**,
with one architectural blind spot: features that only touch post-retrieval confidence weights are
invisible to it.

**Minimum discriminating power (from measured floors):**
- **Judge jitter** (test-retest, aggregate nDCG@5): **≈0.004–0.02**.
- **Sampling floor** (G5): half-width **0.037 at n=150**, 0.029 at n=250.
- **Observed reverse-QA effect sizes** (G4): shuffle Δ **0.23**, distractor Δ **0.36**, truncate Δ
  **0.70** — all **far above** the ~0.05 combined floor. The reverse-QA surface is markedly more
  sensitive than the retracted uds surface (whose shuffle Δ was ~0.05, at the noise floor).

**Gate protocol.**
1. Freeze a **paired snapshot** (query panel + retrieval from one DB state — #500 discipline).
2. Regenerate a **reverse-QA panel of n ≥ 150** against that snapshot state (queries are
   snapshot-specific; do not reuse a panel across corpus states).
3. Retrieve before/after the crt change on the **same snapshot**; judge both with **≥3 Sonnet passes
   averaged**.
4. Metric = **graded nDCG@5** (headline); known-item recall@5/MRR advisory. Report **bootstrap 95% CI
   on the before/after delta**.
5. **Verdict:** real only if the delta's 95% CI **excludes 0** AND **|Δ nDCG@5| > 0.04**. Smaller =
   "within judge noise — inconclusive."

**Critical limitation (measured, not assumed).** The `degraded-weights` config produced identical
rankings (G4) — a crt-feature that only **retunes confidence weights** will register **zero** movement
on this odometer. The gate detects features that change the **candidate set or ordering** (embeddings,
recall stage, reranking, graph expansion, filters). Weight-only features need a different instrument
(or must be shown to change the returned list first).

**Recommendation.** Adopt the odometer as a crt-feature **impact gate for list-changing features** at
|Δ nDCG@5| ≳ 0.04, n ≥ 150, ≥3-pass judging; explicitly declare it **inert to confidence-weight-only
changes**.

### Q: G7 — Product-capability shape (directional)

**Answer.** Ship as a per-project **"evaluate your own corpus"** relative-trend report; one metric,
two oracle bindings; corpus-agnostic by construction; the gate-vs-product tension is the judge binding.

**Sketch.**
- **Surface:** a per-project CLI/report — samples the project's own Active entries, generates
  reverse-QA queries, runs `context_search`, judges, and reports **known-item recall@k + graded
  nDCG@5 as a trend over time**, not an absolute leaderboard (numbers are corpus-relative).
- **Per-project fit:** matches the multi-project independent-config-and-db model — each project
  evaluates its own DB in isolation; nothing crosses project boundaries. Reverse-QA is inherently
  corpus-agnostic (targets are the user's entries).
- **Gate-vs-product tension (the seam that pulls apart):** the internal crt-gate uses the
  **subscription `claude -p`** judge (no key, $0). A shipped product cannot assume a Claude Code
  subscription — it needs a **user-supplied API key** or a **local judge model**. That is the one
  place the two designs diverge; gate the product build on a **local-judge feasibility spike**.

**Recommendation.** Build the internal gate first (subscription judge). Treat the product as a later
capability gated on local/user-key judge feasibility. Keep the metric definitions identical across
both so the only swap is the oracle binding.

### Q: G8 — Go/no-go + recommended build shape (directional)

**Answer. GO.** A trustworthy, pipeline-independent topical-relevance odometer exists on the corpus we
have now, on the **correct surface**, and it discriminates cleanly.

**What the corrected number changes.** The retracted "search is weak (0.62, 40% get nothing)" read was
an artifact of grading whole conversational turns. On the correct unit, **focused-query retrieval is
good**: nDCG@5 **0.70**, recall@5 **0.78**, 96% of queries surface something relevant. The honest
oracle does **not** indict retrieval quality here — it validates the instrument.

**Recommended build shape:** judge = `claude -p` **Sonnet**, MCP disabled; metric = **graded nDCG@5**
(headline) + **known-item recall@5/MRR** (cheap objective anchor, no judge); query source =
**reverse-QA on Active entries**, n ≥ 150 for gate use, Jaccard leakage reported each regeneration;
calibration cadence = one human golden-set pass now (`rqa_golden_for_human.csv`) then re-anchor when
the judge model changes; lives as a **throwaway/scratch harness** feeding the crt-gate protocol (G6),
not committed product code. **One gap before "proven": the human golden-set grading.**

---

## Unanswered Questions

- **Real-usage odometer (primary future item).** With **0 organic `context_search` rows**, the bank is
  synthetic. A real question-distribution odometer needs actual `context_search` adoption — a product
  shift (G7 should drive it), not something this bank resolves.
- **Human golden-set calibration (BLOCKED — one gate to "proven").** An AI cannot build a non-circular
  human anchor. `rqa_golden_for_human.csv` (791 rows) is delivered; needs ~1–2 h human grading to
  produce the real judge↔human κ.
- **Corpus-size scale study (BLOCKED — mechanical).** Needs a one-time per-size HNSW rebuild to run the
  fixed-question growing-corpus panel; the difficulty-normalization design (G5) is ready.
- **Weight-only crt-feature gating.** The odometer is inert to confidence-weight changes (measured).
  Gating such features needs a separate instrument or a demonstrated list-change.

## Out-of-Scope Discoveries

- **Confidence-weight profiles are inert on reverse-QA retrieval (80/80 identical rankings).** Extends
  ass-097's "weight-profile A/B is near-degenerate" from soft-GT snapshot-MRR to the independent
  odometer. The retrieval architecture's discriminating levers are the recall/embedding/rerank stages,
  not the confidence weights — relevant to any crt-feature that plans to move quality via weights.
- **`goal`-category entries retrieve poorly (nDCG 0.33 vs 0.6–0.87 elsewhere).** Broad/abstract goal
  statements are hard to surface for focused queries — a candidate corpus/retrieval issue worth its
  own look, not pursued here.
- **`claude -p` with `--strict-mcp-config` reliably disables MCP** so judging/generation never writes a
  `query_log` row under measurement — reusable pattern for any in-repo LLM-judge harness (avoids
  polluting the very log being studied).

## Recommendations Summary

- **G1:** Build the query bank by reverse-QA on Active entries (deprecated/quarantined as distractors only); report Jaccard leakage each run. Premise gate PASS (Jaccard mean 0.079, 0 flagged). `query_log` mining is a dead-end.
- **G2:** Judge = `claude -p` Sonnet, MCP disabled; aggregate nDCG@5 is the stable headline (test-retest Δ≈0.004, κ 0.83). $0 API.
- **G3:** Trust the aggregate metric now (judge-model-robust, Δ0.01); human calibration BLOCKED — grade the delivered `rqa_golden_for_human.csv` (~1–2 h) to close it.
- **G4:** Headline = graded **nDCG@5 0.699** + known-item **recall@5 0.775 / MRR 0.658**; discrimination PASSES (ideal 1.0 → hold 0.70 → shuffle 0.47 → distractor 0.34 → truncate 0.0). Retrieval is good on focused queries.
- **G5:** Standardize n ≥ 150 panels (CI half-width < 0.037); run the corpus-size sweep as a follow-up with a fixed-question growing-corpus panel + per-size rebuild.
- **G6:** Use as a crt-gate for list-changing features at |Δ nDCG@5| > 0.04, n ≥ 150, ≥3-pass judging; it is **inert to confidence-weight-only** changes.
- **G7:** Ship later as a per-project "evaluate your own corpus" trend report; one metric, two oracle bindings; gate the product on a local/user-key judge feasibility spike.
- **G8: GO.** Trustworthy discriminating odometer in hand on the correct surface; one gap before "proven" — the human golden-set grading.

*Lesser cross-checks (separate constructs — NEVER pooled with the reverse-QA headline): the `uds`
whole-prompt surface (conversational-turn relevance) and `cycle_events.goal` goal-relevance replay
measured by the prior run are different units; the prior uds 0.615 is retracted as an odometer number.*

*Footprint: FINDINGS.md only. Judge scripts, grades, golden CSV, and snapshot are scratch-uncommitted
and never committed (query_log/snapshot are sensitive). $0 API spend. No Unimatrix writes. No PR.*
