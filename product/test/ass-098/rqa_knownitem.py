#!/usr/bin/env python3
"""Reverse-QA step 6 — objective known-item scoring (NO judge).

E is the target by construction, so recall@k and MRR-on-known-target are computed directly from
the pipeline's ranked top-k — pipeline-independent ground truth, zero LLM spend. This is the cheap
objective anchor to run on every corpus state; the graded nDCG@5 (rqa_odometer.py) is the headline.

Bootstrap 95% CIs on recall@5 and MRR. Optionally dumps per-scenario ranks for reuse.
"""
import argparse
import json
import glob
import random
import statistics as st


def main():
    ap = argparse.ArgumentParser(description="Known-item recall@k / MRR from eval-run results.")
    ap.add_argument("--results", required=True, help="Directory of eval-run result JSONs (rqa-*.json).")
    ap.add_argument("--scenarios", required=True, help="Scenarios JSONL (for expected target ids).")
    ap.add_argument("--out", help="Optional path to dump per-scenario ranks JSON.")
    ap.add_argument("--seed", type=int, default=7, help="Bootstrap RNG seed (default 7).")
    args = ap.parse_args()

    scen = {json.loads(line)["id"]: json.loads(line)["expected"][0]
            for line in open(args.scenarios)}
    ranks = []
    for f in sorted(glob.glob(f"{args.results}/rqa-*.json")):
        d = json.load(open(f))
        sid = d["scenario_id"]
        target = scen[sid]
        ids = [e["id"] for e in d["profiles"]["baseline"]["entries"]]
        rank = ids.index(target) + 1 if target in ids else None
        ranks.append((sid, target, rank))

    n = len(ranks)
    if n == 0:
        print("no results found")
        return

    def recall_at(k):
        return sum(1 for _, _, r in ranks if r and r <= k) / n

    mrr = st.mean((1.0 / r if r else 0.0) for _, _, r in ranks)
    print(f"n={n}")
    for k in (1, 3, 5, 10):
        print(f"known-item recall@{k} = {recall_at(k):.3f}")
    print(f"known-item MRR = {mrr:.3f}")
    found = [r for _, _, r in ranks if r]
    print(f"targets found in top-10: {len(found)}/{n}; "
          f"median rank when found = {st.median(found) if found else 'NA'}")

    rng = random.Random(args.seed)

    def boot(fn, m=2000):
        vals = []
        for _ in range(m):
            samp = [rng.choice(ranks) for _ in ranks]
            vals.append(fn(samp))
        vals.sort()
        return round(vals[int(.025 * m)], 3), round(vals[int(.975 * m)], 3)

    r5 = boot(lambda s: sum(1 for _, _, r in s if r and r <= 5) / len(s))
    mr = boot(lambda s: st.mean((1.0 / r if r else 0.0) for _, _, r in s))
    print(f"95% CI recall@5={r5}  MRR={mr}")

    if args.out:
        json.dump([{"sid": s, "target": t, "rank": r} for s, t, r in ranks], open(args.out, "w"))
        print(f"ranks dumped -> {args.out}")


if __name__ == "__main__":
    main()
