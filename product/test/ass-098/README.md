# ass-098 — reverse-QA relevance odometer (harness)

A pipeline-independent **topical-relevance odometer** for `context_search`. It answers one
question: *did `context_search(Q)` return the best answer for Q?* This is the SL-METRIC #5572
odometer half — it complements the nan-018 fixture floor (`docs/testing/eval-harness.md`), it does
not replace it.

Because the `query_log` holds **zero organic `context_search` traffic** (all rows are hook
auto-injection), there is no real search history to grade. The harness therefore builds a
**synthetic reverse-QA** query bank: sample Active entries `E`, ask an LLM for the query `Q` a
developer would type to find each `E`, then measure whether `context_search(Q)` returns `E` (and
other relevant entries) near the top. **Synthetic ≠ real usage** — this validates the instrument
and gives a first honest number on the correct surface; it does not measure the real question
distribution (carry that flag).

Full methodology, interpretation, and gate thresholds: **[`docs/testing/eval-odometer.md`](../../../docs/testing/eval-odometer.md)**.
Proven results this tooling reproduces: **[`product/research/ass-098/FINDINGS.md`](../../research/ass-098/FINDINGS.md)**.

## Prerequisites

- **A paired snapshot** — `unimatrix snapshot --out <dir>/snap.db` (writes a `vector/` sibling with
  the HNSW index). Never point the harness at the live DB — the FR-44 guard refuses it. `E` is
  sampled FROM and `Q` is searched AGAINST the **same** snapshot state (the #500 KB-drift trap: a
  panel built on one state and searched on another measures drift, not retrieval).
- **`claude` CLI on PATH** — Claude Code subscription. Generation and judging run via `claude -p`;
  **$0 API spend**. Every call passes `--strict-mcp-config` (see below).
- **`unimatrix` CLI on PATH** — for `snapshot` and `eval run`.
- **Python 3, stdlib only.** No numpy/scipy. Weighted kappa, binary kappa, Spearman, nDCG, and
  bootstrap CIs are hand-rolled in `metrics.py`. `pip install` is not required.

### `--strict-mcp-config` is load-bearing

Every `claude -p` call (generation + judging) passes `--strict-mcp-config`, which disables MCP.
Without it, each judge/generator call would issue tool calls that write a `query_log` row into the
very snapshot under measurement, polluting the search history being studied. Do not remove the flag.

## Pipeline order

| # | Script | Role | Key args |
|---|--------|------|----------|
| 1 | `rqa_sample.py` | Sample Active entries `E`, stratified by category (Deprecated/Quarantined stay in-corpus as distractors, never targets). | `--db --out -n [--seed --min-content-len]` |
| 2 | `rqa_genq.py` | Generate one reverse-QA query `Q` per `E` via `claude -p`, with anti-leakage rules. | `--in --out [--model --workers]` |
| 3 | `rqa_leakage.py` | Premise gate: Jaccard(Q,E) content-word overlap; flags any `Q` over threshold (exits non-zero if flagged). | `--in [--threshold]` |
| 4 | `rqa_build_scen.py` | Build eval scenarios with `expected=[E_id]` (known-item hard label). | `--in --out` |
| — | `unimatrix eval run` | Replay `Q` through `context_search` on the snapshot → top-k result JSONs. | `--db --scenarios --configs --out --k` |
| 6 | `rqa_knownitem.py` | Objective anchor (no judge): recall@k / MRR of `E` in top-k, bootstrap CIs. | `--results --scenarios [--out]` |
| 7 | `rqa_judge_batch.py` | LLM judge grades top-k relevance 0–3 (independent oracle), cached, parallel. | `--results --out [--model --workers]` |
| 8 | `rqa_odometer.py` | Headline graded **nDCG@5** + CI + per-category + two-sided discrimination controls. | `--grades [--queries --seed]` |

Shared modules: **`judge_one.py`** (the reverse-QA judge core — rubric, prompt builder, one-shot
grader; also a single-file debug driver) and **`metrics.py`** (all ranking + inter-rater metrics,
stdlib-only). Calibration/scale/golden tooling (not in the one-command runner):

- `rqa_calib_stab.py` — judge test-retest + inter-model agreement (`--sonnet --sonnet2 --opus`).
- `rqa_scale.py` — query-subsample convergence → the sampling floor for gate sizing (`--grades`).
- `rqa_golden_csv.py` — emit the human-gradeable golden CSV that closes calibration
  (`--grades --results --out`).

`baseline.toml` is the compiled-defaults eval profile (measures `context_search` as shipped).

## Run

```bash
# 0. paired snapshot (prerequisite — sensitive, never committed)
unimatrix snapshot --out /var/tmp/rqa/snap.db

# one command, end to end (sample → genq → leakage → build → eval run → known-item → judge → odometer)
bash product/test/ass-098/run_odometer.sh \
  --db  /var/tmp/rqa/snap.db \
  --out /var/tmp/rqa/out \
  -n 150 -k 10 --model sonnet --workers 8
```

For a gate panel use `-n 150` (CI half-width < 0.037). For a cheap plumbing smoke use `-n 3`.
All artifacts land in `--out`. See `run_odometer.sh --help`.

To judge extra passes (the gate wants ≥3 averaged) point `rqa_judge_batch.py --out` at a fresh
grades dir per pass, then compare with `rqa_calib_stab.py`.

## What is never committed

The tooling **regenerates everything** from a fresh snapshot. `.gitignore` excludes: snapshots
(`*.db`, `vector/` — contain full `query_log` + agent history, NFR-07), judge grade sets
(`grades*/`), the human golden CSV (`*_golden*.csv`), and output dirs. Commit only the scripts,
this README, the runner, and `baseline.toml`.

## Scope / limits

- **Discriminates list-changing features only** — inert to confidence-weight-only changes (ass-098
  measured 80/80 identical rankings under a degraded-weights profile). The gate sees embeddings /
  recall / rerank / graph / filter changes; a weight-only feature must first be shown to change the
  returned list.
- **Synthetic queries** — no real usage distribution yet (needs `context_search` adoption).
- **Human golden-set calibration still owed** — `rqa_golden_csv.py` produces the instrument; ~1–2 h
  of human grading yields the real judge↔human κ.
- **Corpus-size scaling** needs a per-size HNSW rebuild (documented follow-up in the doc/FINDINGS).
