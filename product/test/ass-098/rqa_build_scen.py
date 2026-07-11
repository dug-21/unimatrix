#!/usr/bin/env python3
"""Reverse-QA step 4 — build the eval-scenarios JSONL from the query bank.

Each scenario: query=Q, expected=[E_id] (the known-item hard label — pipeline-independent ground
truth), retrieval_mode=flexible (matches context_search's mcp mode), baseline=null (no prior
output). These scenarios feed `unimatrix eval run` on the SAME paired snapshot E was sampled from.
"""
import argparse
import json


def main():
    ap = argparse.ArgumentParser(description="Build eval scenarios from the reverse-QA query bank.")
    ap.add_argument("--in", dest="infile", required=True, help="queries JSONL (from rqa_genq.py).")
    ap.add_argument("--out", required=True, help="Output scenarios JSONL.")
    args = ap.parse_args()

    rows = [json.loads(line) for line in open(args.infile)]
    n = 0
    with open(args.out, "w") as f:
        for r in rows:
            if not r.get("query"):
                continue
            scen = {"id": f"rqa-{r['id']}", "query": r["query"],
                    "context": {"agent_id": "rqa", "feature_cycle": "",
                                "session_id": "rqa", "retrieval_mode": "flexible"},
                    "baseline": None, "source": "synthetic",
                    "expected": [r["id"]], "assertions": None}
            f.write(json.dumps(scen) + "\n")
            n += 1
    print(f"wrote {n} scenarios -> {args.out}")


if __name__ == "__main__":
    main()
