# The Reverse-QA Relevance Odometer

A pipeline-independent **topical-relevance odometer** for `context_search`, answering one question:
*did `context_search(Q)` return the best answer for Q?* This is the **SL-METRIC #5572 odometer
half** — it complements, and does not replace, the nan-018 fixture floor. Origin: research spike
ass-098. Pairs with:

- [Evaluation harness overview](./eval-harness.md) — the offline D1–D4 A/B instrument this rides on
- [Fixture-corpus authoring guide](./eval-fixture-authoring.md) — the durable property-assertion floor
- [Two-corpus model](./eval-two-corpus-model.md) — fixture (durable) vs snapshot (realism) roles
- `product/research/ass-098/FINDINGS.md` — the proven methodology + every threshold below
- `product/test/ass-098/` — the promoted harness this runbook drives

---

## Contents

- [What it is / when to use it](#what-it-is--when-to-use-it)
- [Why reverse-QA](#why-reverse-qa)
- [Prerequisites](#prerequisites)
- [Runbook](#runbook)
- [Interpretation](#interpretation)
- [Gate thresholds (crt-feature use)](#gate-thresholds-crt-feature-use)
- [The measured blind spot](#the-measured-blind-spot)
- [Known limits / open gaps](#known-limits--open-gaps)

---

## What it is / when to use it

The [eval harness](./eval-harness.md) measures whether an A/B config change *regresses* retrieval,
scored against **soft** ground truth (the results production previously returned). The odometer is a
different instrument on a different question: an **absolute-ish, judge-graded** read of whether
`context_search(Q)` surfaces the genuinely best entries for a focused developer query, scored
against an **independent LLM oracle** and an **objective known-item** target.

Use it when you need a headline "is search good?" number on the real `context_search` surface, or as
a **crt-feature impact gate** for changes that alter the returned list (embeddings, recall stage,
re-ranking, graph expansion, filters). It is not a workflow gate — like the eval harness, it is a
human-reviewed instrument, run on demand, never wired into CI-on-every-PR.

| Change type | Odometer |
|---|---|
| Embedding / retrieval-model swap | Recommended — moves the list |
| Recall-stage / re-ranking / graph-expansion change | Recommended — moves the list |
| Confidence-weight-only tuning | **Inert** — see [blind spot](#the-measured-blind-spot) |
| Schema / storage only | Not applicable |

---

## Why reverse-QA

The obvious plan — mine `query_log` for real `context_search` queries and grade them — is a
**dead-end**. A live probe confirmed `context_search(Q) → top-k` *is* a clean, logged, gradeable
unit, but the historical log holds **zero organic `context_search` rows**: every row is hook
auto-injection (`uds` whole-prompt turns), not a developer explicitly searching. There is no real
search history to grade.

So the query bank is built by **synthetic reverse-QA**: sample an Active entry `E`, ask an LLM for
the query `Q` a developer *who does not know E exists* would type when they have the problem `E`
answers, then measure whether `context_search(Q)` returns `E` (and other relevant entries) near the
top. `E` is *a* known-correct answer by construction, giving objective ground truth for free.

> **Honesty flag (carry it prominently):** the bank is **synthetic** and stands in for
> `context_search` traffic that does not yet exist. It validates the *instrument* and gives a real
> first number on the correct surface; it does **not** measure the real-world question distribution.
> A real-usage odometer still needs actual `context_search` adoption.

The prior ass-098 run graded the wrong surface (whole `uds` prompts injected verbatim as the search
string) and its headline (nDCG@5 0.615 / "40% retrieve nothing") is **retracted**. Ignore that
surface; the unit is `context_search(Q) → top-k`.

---

## Prerequisites

- **A paired snapshot.** `unimatrix snapshot --out <dir>/snap.db` (writes a `vector/` sibling with
  the HNSW index). **Never the live DB** — the FR-44 guard refuses a `--db` that resolves to the
  active daemon path. Snapshots contain full `query_log` + agent history and are **never committed**
  (NFR-07).
- **The paired-snapshot discipline.** `E` is sampled FROM and `Q` is searched AGAINST the **same**
  snapshot state. A panel built on one DB state and searched against a fresh one measures **KB
  drift**, not retrieval quality — the #500 trap. Queries are snapshot-specific; do not reuse a
  panel across corpus states.
- **`claude` CLI** (Claude Code subscription). Generation and judging run via `claude -p` at **$0
  API spend**. Every call passes `--strict-mcp-config` — this **disables MCP** so a judge/generator
  call never writes a `query_log` row into the snapshot under measurement. The flag is load-bearing.
- **Python 3, stdlib only** (no numpy/scipy — all statistics are hand-rolled in
  `product/test/ass-098/metrics.py`).

---

## Runbook

All scripts live in `product/test/ass-098/`. The one-command runner chains the whole pipeline; the
per-step commands below show what it does against the promoted scripts.

```bash
# 0. paired snapshot (prerequisite — sensitive, never committed)
unimatrix snapshot --out /var/tmp/rqa/snap.db

# one command, end to end
bash product/test/ass-098/run_odometer.sh \
  --db /var/tmp/rqa/snap.db --out /var/tmp/rqa/out -n 150 -k 10 --model sonnet
```

### Step by step

```bash
D=product/test/ass-098; OUT=/var/tmp/rqa/out; DB=/var/tmp/rqa/snap.db

# 1. sample Active targets E, stratified by category (n>=150 for gate use)
python3 $D/rqa_sample.py --db $DB --out $OUT/entries.jsonl -n 150

# 2. generate one reverse-QA query Q per E (anti-leakage rules: no ADR/feature ids, no file
#    paths, no verbatim symbol names, no quoting E's sentences)
python3 $D/rqa_genq.py --in $OUT/entries.jsonl --out $OUT/queries.jsonl --model sonnet

# 3. leakage premise gate — Jaccard(Q,E) content-word overlap; PASS requires 0 flagged
python3 $D/rqa_leakage.py --in $OUT/queries.jsonl --threshold 0.35

# 4. build eval scenarios with expected=[E_id] (the known-item hard label)
python3 $D/rqa_build_scen.py --in $OUT/queries.jsonl --out $OUT/scenarios.jsonl

# 5. run context_search on the SAME snapshot → top-k result JSONs (reuses the eval-harness engine)
unimatrix eval run --db $DB --scenarios $OUT/scenarios.jsonl \
  --configs $D/baseline.toml --out $OUT/results --k 10

# 6. objective known-item anchor (no judge): recall@k / MRR of E, bootstrap CIs
python3 $D/rqa_knownitem.py --results $OUT/results --scenarios $OUT/scenarios.jsonl

# 7. judge top-k relevance 0–3 (independent oracle). Repeat into grades_sonnet, _p2, _p3 for the
#    >=3-pass gate; the judge sees only (query, entry text) — never rank/score/target id.
python3 $D/rqa_judge_batch.py --results $OUT/results --out $OUT/grades_sonnet --model sonnet

# 8. headline graded nDCG@5 + CI + per-category + the two-sided discrimination control
python3 $D/rqa_odometer.py --grades $OUT/grades_sonnet --queries $OUT/queries.jsonl
```

**Anti-leakage rules (step 2), enforced in the generator prompt and gated in step 3:** the query
must be phrased for the underlying problem, in the developer's own words — no ADR numbers, feature
ids, issue numbers, file paths, or exact symbol names, and no quoting E's sentences. High lexical
overlap would make retrieval trivially lexical and stop testing *semantic* search.

**Discrimination control (step 8), the validity check.** `rqa_odometer.py` re-scores the same judge
grades under list manipulations and must produce a monotone ordering:
`IDEAL(sort desc) > HOLD(actual) > shuffle > distractor > truncate`. If HOLD does not sit strictly
between ideal and the degrade variants, the instrument is not discriminating and its number is
untrustworthy for this run.

---

## Interpretation

Report **both** scores — they are correlated (mutually validating) but not redundant
(per-query correlation ≈ 0.56):

- **Known-item recall@k / MRR** (`rqa_knownitem.py`) — the **objective anchor**. `E` is a known
  target, so this needs no judge and can run on every corpus state cheaply. It *under-credits*:
  when `E` is not rank 1 but a *different* equally-relevant entry is, known-item scores it low.
- **Graded nDCG@5** (`rqa_odometer.py`) — the **headline odometer**. The LLM judge grades every
  top-k entry 0–3, so it credits *any* best-answer at the top and captures ordering that binary
  P@5 misses. This is the number to lead with.

**The two-sided discrimination control** is the run's validity check, not a quality number: a
trustworthy odometer must order IDEAL > HOLD > shuffle > distractor > truncate (both nDCG@5 and MRR
move together). ass-098 measured IDEAL 1.00 > HOLD 0.70 > shuffle 0.47 > distractor 0.34 >
truncate 0.00 — the positive/negative control ass-097 declared structurally impossible under
self-ground-truth.

**The leakage premise gate** (`Jaccard(Q,E) < 0.35`) is a precondition, not a result: it proves the
queries test semantic search rather than lexical copying. ass-098 measured mean 0.079, 0/80 flagged.
If anything flags, regenerate before trusting the odometer.

**Reference numbers** (ass-098, n=80 reverse-QA, paired snapshot): graded nDCG@5 **0.699**
(CI 0.646–0.746); known-item recall@5 **0.775**, recall@10 0.863, MRR **0.658**; 96% of queries
surface ≥1 relevant entry in top-5. These are corpus-relative — meaningful as a trend, not an
absolute leaderboard.

---

## Gate thresholds (crt-feature use)

To gate a list-changing crt-feature on the odometer:

1. Freeze a **paired snapshot** (query panel + retrieval from one DB state — #500 discipline).
2. Regenerate a reverse-QA panel of **n ≥ 150** against that snapshot state (CI half-width < 0.037
   at n=150; queries are snapshot-specific — do not reuse across states).
3. Retrieve before/after the change on the **same** snapshot; judge both with **≥ 3 Sonnet passes
   averaged**.
4. Metric = **graded nDCG@5** (headline); known-item recall@5/MRR advisory. Report the **bootstrap
   95% CI on the before/after delta**.
5. **Verdict: real only if the delta's 95% CI excludes 0 AND |Δ nDCG@5| > 0.04.** Smaller is "within
   judge noise — inconclusive."

The floors these clear: judge test-retest jitter ≈ 0.004–0.02; sampling half-width 0.037 at n=150.
Observed reverse-QA effect sizes (shuffle Δ 0.23, distractor Δ 0.36, truncate Δ 0.70) sit far above
the ~0.05 combined floor, so the surface is well-powered for genuine list changes.

---

## The measured blind spot

**The odometer is inert to confidence-weight-only changes.** Re-running the identical bank under a
`degraded-weights` profile (starves semantic/trust, over-weights freshness/usage) produced **80/80
byte-identical rankings** — recall@5 unchanged. The candidate set and order are dominated by the
vector-similarity stage; the confidence-weight lever does not move reverse-QA retrieval. A
crt-feature that only **retunes confidence weights** will register **zero** movement here.

The gate detects features that change the **candidate set or ordering** (embeddings, recall,
re-ranking, graph expansion, filters). Weight-only features need a different instrument, or must
first be shown to change the returned list. **Declare this explicitly** when adopting the odometer
as a gate. (This extends ass-097's "weight-profile A/B is near-degenerate" from soft-GT snapshot-MRR
to the independent odometer.)

---

## Known limits / open gaps

- **Synthetic queries — no real usage distribution.** The bank stands in for `context_search`
  traffic that does not yet exist (0 organic rows). A real-question odometer needs actual
  `context_search` adoption — a product shift, not something this bank resolves.
- **Human golden-set calibration still owed (one gate to "proven").** An AI cannot build a
  non-circular human anchor. `rqa_golden_csv.py` produces the ready instrument (one row per
  query,entry with the model grade held in a separate column); ~1–2 h of human grading yields the
  real judge↔human κ. Until then, trust the **aggregate** metric (judge-model-robust, test-retest
  Δ≈0.01) and treat **per-entry** grades as provisional (κ≈0.78).
- **Corpus-size scaling is mechanical-blocked.** The harness loads one HNSW index per snapshot;
  cross-size comparison needs a per-size index rebuild. The difficulty-normalized design (fixed
  reverse-QA panel, growing corpus, report the odometer as a delta against a held reference target
  set's recall@k) is specified in `FINDINGS.md` G5 but not automated here.
- **`goal`-category entries retrieve poorly** (nDCG ≈ 0.33 vs 0.6–0.87 elsewhere) — broad/abstract
  statements are hard to surface for focused queries. A corpus/retrieval observation, not a harness
  bug.
