#!/usr/bin/env python3
"""Reverse-QA calibration + stability — judge test-retest and inter-model agreement.

Two comparisons over shared (query, entry) grade pairs:
  * STABILITY (test-retest): the same model judged twice (e.g. sonnet pass1 vs pass2).
  * CALIBRATION (inter-model): two different judge models (e.g. sonnet vs opus).

Reports exact-match, adjacent(<=1), linear-weighted kappa, binary-relevance kappa, Spearman, and
the aggregate nDCG@5 under each judge. A robust odometer moves little between passes/models even
when per-entry kappa is only high-moderate. Requires >=2 grade directories to compare.
"""
import argparse
import json
import glob
import os
import sys
import statistics as st

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from metrics import ndcg, linear_weighted_kappa, binary_kappa, spearman  # noqa: E402


def load(grades_dir):
    out = {}
    for f in glob.glob(f"{grades_dir}/rqa-*.json"):
        r = json.load(open(f))
        out[r["scenario_id"]] = r
    return out


def glist(r):
    return [g["grade"] for g in r["grades"]]


def pairs(a, b):
    out = []
    for sid in set(a) & set(b):
        ga = {g["id"]: g["grade"] for g in a[sid]["grades"]}
        gb = {g["id"]: g["grade"] for g in b[sid]["grades"]}
        for eid in set(ga) & set(gb):
            out.append((ga[eid], gb[eid]))
    return out


def report(a, b, la, lb):
    pr = pairs(a, b)
    sh = list(set(a) & set(b))
    if not pr:
        print(f"-- {la} vs {lb} -- no shared (query,entry) pairs")
        return
    exact = st.mean(1.0 if x == y else 0 for x, y in pr)
    adj = st.mean(1.0 if abs(x - y) <= 1 else 0 for x, y in pr)
    oa = st.mean(ndcg(glist(a[s])) for s in sh if glist(a[s]))
    ob = st.mean(ndcg(glist(b[s])) for s in sh if glist(b[s]))
    print(f"-- {la} vs {lb} -- shared_scen={len(sh)} entry_pairs={len(pr)}")
    print(f"   exact={exact:.3f} adjacent(<=1)={adj:.3f} "
          f"wkappa(0-3)={linear_weighted_kappa(pr):.3f} binkappa(>=2)={binary_kappa(pr):.3f} "
          f"spearman={spearman(pr):.3f}")
    print(f"   nDCG@5 odometer: {la}={oa:.4f}  {lb}={ob:.4f}  |delta|={abs(oa - ob):.4f}")


def main():
    ap = argparse.ArgumentParser(description="Reverse-QA judge stability + calibration proxy.")
    ap.add_argument("--sonnet", required=True, help="Primary judge grades dir (pass 1).")
    ap.add_argument("--sonnet2", help="Same-model second pass grades dir (test-retest stability).")
    ap.add_argument("--opus", help="Second-model grades dir (inter-model calibration proxy).")
    args = ap.parse_args()

    son = load(args.sonnet)
    if args.sonnet2:
        print("############ STABILITY (test-retest) ############")
        report(son, load(args.sonnet2), "pass1", "pass2")
    if args.opus:
        print("\n############ CALIBRATION PROXY (inter-model) ############")
        report(son, load(args.opus), "sonnet", "opus")
    if not args.sonnet2 and not args.opus:
        print("nothing to compare — pass --sonnet2 and/or --opus")


if __name__ == "__main__":
    main()
