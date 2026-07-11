#!/usr/bin/env python3
"""Reverse-QA golden-set instrument — emit the human-gradeable CSV.

One row per (query, retrieved entry). The human fills `human_grade`; the model's grade is kept in
a SEPARATE `_llm_grade` column a grader must not consult while grading (retained for post-hoc
judge<->human agreement). ~1-2 h of human grading over this CSV yields the real judge<->human kappa
that closes the one remaining calibration gap ("proven" instrument).

The output CSV is sensitive (contains snapshot content) and is git-ignored — never commit it.
"""
import argparse
import json
import glob
import csv
import os


def main():
    ap = argparse.ArgumentParser(description="Emit the human-gradeable golden CSV.")
    ap.add_argument("--grades", required=True, help="Grades directory (from rqa_judge_batch.py).")
    ap.add_argument("--results", required=True, help="Eval-run results dir (for entry title/snippet).")
    ap.add_argument("--out", required=True, help="Output golden CSV (git-ignored — do not commit).")
    args = ap.parse_args()

    rows = []
    for f in sorted(glob.glob(f"{args.grades}/rqa-*.json")):
        r = json.load(open(f))
        sid, q = r["scenario_id"], r["query"]
        resfile = f"{args.results}/{sid}.json"
        if not os.path.exists(resfile):
            continue
        emap = {e["id"]: e for e in json.load(open(resfile))["profiles"]["baseline"]["entries"]}
        for rank, g in enumerate(r["grades"], 1):
            e = emap.get(g["id"], {})
            rows.append({"scenario_id": sid, "query": q, "rank": rank, "entry_id": g["id"],
                         "entry_title": e.get("title", ""),
                         "entry_snippet": (e.get("content") or "").replace("\n", " ")[:300],
                         "human_grade": "", "_llm_grade": g["grade"]})

    with open(args.out, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["scenario_id", "query", "rank", "entry_id", "entry_title",
                                          "entry_snippet", "human_grade", "_llm_grade"])
        w.writeheader()
        w.writerows(rows)
    nq = len(set(r["scenario_id"] for r in rows))
    print(f"wrote {len(rows)} (query,entry) rows across {nq} queries -> {args.out}")


if __name__ == "__main__":
    main()
