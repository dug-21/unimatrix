#!/usr/bin/env python3
"""Reverse-QA step 3 — the leakage PREMISE GATE.

Measures lexical leakage between each query Q and its source entry E:
  (1) content-word Jaccard(Q, title+content)
  (2) fraction of Q's content-words present in E (coverage)

High Jaccard means Q copied E's distinctive wording => the retrieval task is trivially lexical and
does not test SEMANTIC search. Any Q with Jaccard > --threshold is flagged for regeneration. The
gate PASSES only when nothing is flagged (ass-098 measured: mean 0.079, 0/80 flagged at 0.35).

Annotates each row in place with `_jaccard` / `_coverage` and prints the distribution + flag list.
Exits non-zero if any query is flagged, so a runner can treat it as a gate.
"""
import argparse
import json
import re
import sys
import statistics as st

STOP = set(
    "the a an and or of to in for on with is are be as at by it this that these those you your "
    "we our i they them from into how what when where why which who does do can should must "
    "if then than so not no yes use used using via per its it's about over under between "
    "each any all some one two more most such also but was were has have had will would".split())


def toks(s):
    return [w for w in re.findall(r"[a-z0-9]+", (s or "").lower()) if w not in STOP and len(w) > 2]


def main():
    ap = argparse.ArgumentParser(description="Reverse-QA leakage premise gate (Jaccard(Q,E)).")
    ap.add_argument("--in", dest="infile", required=True, help="queries JSONL (annotated in place).")
    ap.add_argument("--threshold", type=float, default=0.35, help="Jaccard flag threshold (default 0.35).")
    args = ap.parse_args()

    rows = [json.loads(line) for line in open(args.infile)]
    jac, cov, flagged = [], [], []
    for r in rows:
        if not r.get("query"):
            continue
        qt = set(toks(r["query"]))
        et = set(toks(r["title"] + " " + (r["content"] or "")[:1500]))
        if not qt:
            continue
        j = len(qt & et) / len(qt | et)
        c = len(qt & et) / len(qt)
        r["_jaccard"], r["_coverage"] = round(j, 3), round(c, 3)
        jac.append(j)
        cov.append(c)
        if j > args.threshold:
            flagged.append(r["id"])

    with open(args.infile, "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")

    print(f"n={len(jac)}")
    print(f"Jaccard(Q,E)  mean={st.mean(jac):.3f} median={st.median(jac):.3f} "
          f"p90={sorted(jac)[int(0.9 * len(jac))]:.3f} max={max(jac):.3f}")
    print(f"Coverage(Q in E) mean={st.mean(cov):.3f} median={st.median(cov):.3f} max={max(cov):.3f}")
    print(f"flagged (Jaccard>{args.threshold}): {len(flagged)} ids={flagged}")
    if flagged:
        print("PREMISE GATE: FAIL — regenerate flagged queries before trusting the odometer.",
              file=sys.stderr)
        sys.exit(1)
    print("PREMISE GATE: PASS")


if __name__ == "__main__":
    main()
