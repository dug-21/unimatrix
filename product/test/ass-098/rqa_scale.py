#!/usr/bin/env python3
"""Reverse-QA scale study — query-subsample convergence (the sampling floor for gate use).

Shows how the odometer estimate (mean nDCG@5) and its CI half-width shrink as the query-bank size
n grows. This sets the sampling floor a trustworthy gate must clear: ass-098 standardized on
n >= 150 (CI half-width < 0.037).

NOTE: this varies QUERY-bank size at FIXED corpus. Corpus-size scaling is a separate, harder study
that needs a per-size HNSW index rebuild (documented follow-up), not doable from grades alone.
"""
import argparse
import json
import glob
import os
import random
import sys
import statistics as st

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from metrics import ndcg  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description="Query-subsample convergence of the odometer.")
    ap.add_argument("--grades", required=True, help="Grades directory (from rqa_judge_batch.py).")
    ap.add_argument("--seed", type=int, default=11, help="RNG seed (default 11).")
    ap.add_argument("--reps", type=int, default=500, help="Subsamples per n (default 500).")
    args = ap.parse_args()

    rng = random.Random(args.seed)
    vals = []
    for f in glob.glob(f"{args.grades}/rqa-*.json"):
        g = [x["grade"] for x in json.load(open(f))["grades"]]
        if g:
            vals.append(ndcg(g))
    if not vals:
        print("no grades found")
        return

    print(f"full-bank nDCG@5 mean={st.mean(vals):.4f} (N={len(vals)}), sd={st.pstdev(vals):.4f}")
    print("n   | mean(nDCG@5) over subsamples | CI half-width (2.5-97.5%)")
    for n in (10, 20, 30, 40, 50, 60, 70, 80, 100, 150, 250):
        if n > len(vals):
            continue
        means = []
        for _ in range(args.reps):
            means.append(st.mean(rng.sample(vals, n)))
        means.sort()
        lo, hi = means[int(.025 * len(means))], means[int(.975 * len(means))]
        print(f"{n:3d} | {st.mean(means):.4f}                    | {(hi - lo) / 2:.4f}")


if __name__ == "__main__":
    main()
