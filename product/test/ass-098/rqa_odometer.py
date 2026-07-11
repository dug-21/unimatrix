#!/usr/bin/env python3
"""Reverse-QA step 8 — the graded ODOMETER (nDCG@5) + two-sided discrimination controls.

Grades are read in pipeline rank order and scored against the JUDGE ORACLE (never pipeline ids).
The headline is graded nDCG@5 with a bootstrap 95% CI; P@5(>=2), MRR(>=2), and best-answer-at-rank-1
fractions accompany it. Per-category breakdown surfaces which knowledge kinds retrieve well.

The discrimination control is the VALIDITY check: a trustworthy odometer must order
IDEAL > HOLD(actual) > shuffle > distractor > truncate. If HOLD does not sit strictly between
ideal and the degrade variants, the instrument is not discriminating and the number is untrustworthy.
"""
import argparse
import json
import glob
import os
import random
import sys
import statistics as st
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from metrics import ndcg, p_at_k, mrr, bootstrap_ci  # noqa: E402


def load(grades_dir):
    out = {}
    for f in glob.glob(f"{grades_dir}/rqa-*.json"):
        r = json.load(open(f))
        out[r["scenario_id"]] = r
    return out


def glist(r):
    return [g["grade"] for g in r["grades"]]


def odometer(recs, keys=None):
    ks = keys if keys is not None else list(recs.keys())
    gl = [glist(recs[k]) for k in ks]
    gl = [g for g in gl if g]
    if not gl:
        return {"n": 0}
    return {"n": len(gl),
            "nDCG@5": round(st.mean(ndcg(g) for g in gl), 4),
            "P@5(>=2)": round(st.mean(p_at_k(g) for g in gl), 4),
            "MRR(>=2)": round(st.mean(mrr(g) for g in gl), 4),
            "mean_top1_grade": round(st.mean(g[0] for g in gl), 3),
            "frac_any_rel@5": round(st.mean(1.0 if any(x >= 2 for x in g[:5]) else 0.0 for g in gl), 4),
            "frac_grade3_top1": round(st.mean(1.0 if g[0] == 3 else 0.0 for g in gl), 4)}


def controls(recs, seed=7):
    rng = random.Random(seed)
    gls = [glist(recs[k]) for k in recs if glist(recs[k])]

    def ideal(g):
        return sorted(g, reverse=True)

    def shuf(g):
        g2 = g[:]
        rng.shuffle(g2)
        return g2

    def distract(g):  # replace top-3 slots with grade-0 distractors
        return [0, 0, 0] + sorted(g, reverse=True)[3:] if len(g) >= 3 else [0] * len(g)

    def trunc(g):  # drop the good head; simulates truncated retrieval
        return ([0] * max(0, len(g) - 2)) + sorted(g)[:2]

    variants = {"HOLD(actual)": lambda g: g, "IDEAL(sortdesc)": ideal,
                "DEGRADE-shuffle": shuf, "DEGRADE-distractor": distract,
                "DEGRADE-truncate": trunc}
    res = {}
    for name, fn in variants.items():
        reps = 20 if "shuffle" in name else 1
        acc = {"nDCG@5": [], "P@5": [], "MRR": []}
        for _ in range(reps):
            tg = [fn(g) for g in gls]
            acc["nDCG@5"].append(st.mean(ndcg(g) for g in tg))
            acc["P@5"].append(st.mean(p_at_k(g) for g in tg))
            acc["MRR"].append(st.mean(mrr(g) for g in tg))
        res[name] = {m: round(st.mean(v), 4) for m, v in acc.items()}
    return res


def main():
    ap = argparse.ArgumentParser(description="Reverse-QA graded odometer + discrimination controls.")
    ap.add_argument("--grades", required=True, help="Grades directory (from rqa_judge_batch.py).")
    ap.add_argument("--queries", help="queries JSONL (enables per-category breakdown).")
    ap.add_argument("--seed", type=int, default=7, help="Bootstrap/shuffle RNG seed (default 7).")
    args = ap.parse_args()

    recs = load(args.grades)
    gls = [glist(recs[k]) for k in recs if glist(recs[k])]
    print(f"############ REVERSE-QA GRADED ODOMETER ({args.grades}) ############")
    print("ALL:", odometer(recs))
    print("95% CI nDCG@5:", bootstrap_ci(gls, ndcg, seed=args.seed))
    print("95% CI P@5   :", bootstrap_ci(gls, p_at_k, seed=args.seed))
    print("95% CI MRR   :", bootstrap_ci(gls, mrr, seed=args.seed))

    if args.queries:
        cat = {f"rqa-{json.loads(l)['id']}": json.loads(l)["category"] for l in open(args.queries)}
        bycat = defaultdict(list)
        for k in recs:
            bycat[cat.get(k, "?")].append(k)
        print("\n-- per category --")
        for c, ks in sorted(bycat.items()):
            print(f"  {c:16s}", odometer(recs, ks))

    print("\n############ GRADED DISCRIMINATION CONTROLS ############")
    for name, m in controls(recs, seed=args.seed).items():
        print(f"  {name:20s} {m}")


if __name__ == "__main__":
    main()
