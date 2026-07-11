#!/usr/bin/env python3
"""Reverse-QA step 7 — judge every eval-run result in parallel via the shared judge core.

Uses judge_one.py's RUBRIC / build_prompt / parse (INDEPENDENT oracle: judge sees only
query + entry title/content, never rank/score/target id). Writes one grade file per scenario into
--out, cached (skips scenarios already graded there) so re-runs and >=3-pass judging are cheap.

Every `claude -p` call carries --strict-mcp-config (MCP disabled => no query_log write under
measurement). Subscription CLI => $0 API spend.
"""
import argparse
import json
import glob
import os
import sys
import concurrent.futures as cf

# import the shared judge core from the same directory
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from judge_one import grade_result  # noqa: E402


def judge_file(args):
    resfile, model, outdir = args
    d = json.load(open(resfile))
    sid = d["scenario_id"]
    outfile = f"{outdir}/{sid}.json"
    if os.path.exists(outfile):
        return sid, "cached"
    try:
        rec = grade_result(d, model)
    except Exception as ex:  # noqa: BLE001
        return sid, f"FAIL:{str(ex)[:60]}"
    json.dump(rec, open(outfile, "w"))
    return sid, ("empty" if not rec["grades"] else "ok")


def main():
    ap = argparse.ArgumentParser(description="Batch reverse-QA judge over eval-run results.")
    ap.add_argument("--results", required=True, help="Directory of eval-run result JSONs (rqa-*.json).")
    ap.add_argument("--out", required=True, help="Output grades directory (one JSON per scenario).")
    ap.add_argument("--model", default="sonnet", help="Judge model (default sonnet).")
    ap.add_argument("--workers", type=int, default=8, help="Parallel claude -p workers (default 8).")
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    files = sorted(glob.glob(f"{args.results}/rqa-*.json"))
    work = [(f, args.model, args.out) for f in files]
    ok = 0
    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        for sid, status in ex.map(judge_file, work):
            if status in ("ok", "cached", "empty"):
                ok += 1
            else:
                print(f"  {sid}: {status}", file=sys.stderr)
    print(f"judged {ok}/{len(files)} -> {args.out}")


if __name__ == "__main__":
    main()
