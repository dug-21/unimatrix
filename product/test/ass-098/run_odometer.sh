#!/usr/bin/env bash
# Reverse-QA relevance odometer — end-to-end runner (ass-098).
#
# Chains: sample -> genq -> leakage gate -> build scenarios -> eval run -> known-item
#         -> judge -> graded odometer, writing every artifact into --out.
#
# PREREQUISITES (see README.md):
#   * a PAIRED snapshot: `unimatrix snapshot --out <dir>/snap.db` (never the live DB — FR-44 guard).
#     E is sampled FROM and Q is searched AGAINST the same snapshot state (the #500 KB-drift trap).
#   * `claude` CLI on PATH (Claude Code subscription; judging runs at $0 API spend).
#   * `unimatrix` CLI on PATH.
#   * Python 3 (stdlib only — no numpy/scipy).
#
# USAGE:
#   ./run_odometer.sh --db <snap.db> --out <dir> [-n 80] [-k 10] [--model sonnet] [--workers 8]
#                     [--configs baseline.toml]
#
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DB=""; OUT=""; N=80; K=10; MODEL="sonnet"; WORKERS=8; CONFIGS="${HERE}/baseline.toml"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db)      DB="$2"; shift 2 ;;
    --out)     OUT="$2"; shift 2 ;;
    -n)        N="$2"; shift 2 ;;
    -k)        K="$2"; shift 2 ;;
    --model)   MODEL="$2"; shift 2 ;;
    --workers) WORKERS="$2"; shift 2 ;;
    --configs) CONFIGS="$2"; shift 2 ;;
    -h|--help) grep '^#' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

[[ -n "$DB"  ]] || { echo "error: --db <snapshot.db> is required" >&2; exit 2; }
[[ -n "$OUT" ]] || { echo "error: --out <dir> is required" >&2; exit 2; }
[[ -f "$DB"  ]] || { echo "error: snapshot not found: $DB" >&2; exit 2; }
mkdir -p "$OUT"

echo "== [1/8] sample targets (n=$N) =="
python3 "${HERE}/rqa_sample.py" --db "$DB" --out "$OUT/entries.jsonl" -n "$N"

echo "== [2/8] generate reverse-QA queries (model=$MODEL) =="
python3 "${HERE}/rqa_genq.py" --in "$OUT/entries.jsonl" --out "$OUT/queries.jsonl" \
  --model "$MODEL" --workers "$WORKERS"

echo "== [3/8] leakage premise gate =="
# Report-and-continue: a flagged query means regenerate before trusting the number, but we still
# run the full pipeline so the runner demonstrates end to end. Gate verdict is printed above.
python3 "${HERE}/rqa_leakage.py" --in "$OUT/queries.jsonl" || \
  echo "   (leakage gate flagged queries — see above; regenerate for a trustworthy panel)"

echo "== [4/8] build eval scenarios =="
python3 "${HERE}/rqa_build_scen.py" --in "$OUT/queries.jsonl" --out "$OUT/scenarios.jsonl"

echo "== [5/8] eval run (context_search on the snapshot, k=$K) =="
unimatrix eval run --db "$DB" --scenarios "$OUT/scenarios.jsonl" \
  --configs "$CONFIGS" --out "$OUT/results" --k "$K"

echo "== [6/8] known-item recall@k / MRR (objective anchor) =="
python3 "${HERE}/rqa_knownitem.py" --results "$OUT/results" \
  --scenarios "$OUT/scenarios.jsonl" --out "$OUT/knownitem.json"

echo "== [7/8] judge top-k relevance (model=$MODEL) =="
python3 "${HERE}/rqa_judge_batch.py" --results "$OUT/results" \
  --out "$OUT/grades_${MODEL}" --model "$MODEL" --workers "$WORKERS"

echo "== [8/8] graded odometer (nDCG@5) + discrimination controls =="
python3 "${HERE}/rqa_odometer.py" --grades "$OUT/grades_${MODEL}" --queries "$OUT/queries.jsonl"

echo "== done. artifacts in: $OUT =="
