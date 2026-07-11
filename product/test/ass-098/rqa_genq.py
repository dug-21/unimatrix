#!/usr/bin/env python3
"""Reverse-QA step 2 — generate one search query Q per entry E via `claude -p`.

Q is phrased for the PROBLEM/knowledge E addresses, NOT a paraphrase of E's text. Anti-leakage
rules forbid copying distinctive identifiers (ADR-ids, feature-ids, issue numbers, file paths,
symbol names) or quoting E's sentences — otherwise retrieval is trivially lexical and does not
test semantic search. rqa_leakage.py gates the result with a Jaccard(Q,E) premise check.

`--strict-mcp-config` is LOAD-BEARING on every call: it disables MCP so generation never writes a
query_log row into the snapshot under measurement. Subscription CLI => $0 API spend.
"""
import argparse
import json
import subprocess
import concurrent.futures as cf

GEN = """You write realistic search queries a developer would type into a code-knowledge search box.
Below is ONE knowledge entry from an internal engineering knowledge base. Write the single search
query that a developer WHO DOES NOT KNOW THIS ENTRY EXISTS would type when they have the problem or
question that this entry answers.

Rules:
- Phrase it for the underlying PROBLEM or QUESTION, not as a summary of the entry.
- Natural developer phrasing (a question or a terse search phrase), 4-16 words.
- Do NOT copy distinctive identifiers verbatim: no ADR numbers, feature ids (like col-031, vnc-040),
  issue numbers, file paths, or exact function/struct names from the entry. Describe the concept in
  your own words instead.
- Do NOT quote sentences from the entry. Someone searching would not know its wording.
- Output ONLY the query text on a single line. No quotes, no preamble, no explanation.

KNOWLEDGE ENTRY:
title: {title}
{content}
"""


def gen(e, model):
    prompt = GEN.format(title=e["title"], content=(e["content"] or "")[:1600])
    for _ in range(3):
        try:
            p = subprocess.run(["claude", "-p", "--model", model, "--strict-mcp-config"],
                               input=prompt, capture_output=True, text=True, timeout=180)
            lines = p.stdout.strip().splitlines()
            q = (lines[-1] if lines else "").strip().strip('"').strip()
            if q and len(q) > 8:
                return {"id": e["id"], "category": e["category"], "title": e["title"],
                        "content": e["content"], "query": q}
        except Exception:  # noqa: BLE001 — retry any subprocess/parse failure
            pass
    return {"id": e["id"], "category": e["category"], "title": e["title"],
            "content": e["content"], "query": None}


def main():
    ap = argparse.ArgumentParser(description="Generate reverse-QA queries via claude -p.")
    ap.add_argument("--in", dest="infile", required=True, help="entries JSONL (from rqa_sample.py).")
    ap.add_argument("--out", required=True, help="Output queries JSONL.")
    ap.add_argument("--model", default="sonnet", help="Judge/generator model (default sonnet).")
    ap.add_argument("--workers", type=int, default=8, help="Parallel claude -p workers (default 8).")
    args = ap.parse_args()

    entries = [json.loads(line) for line in open(args.infile)]
    out = []
    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        for r in ex.map(lambda e: gen(e, args.model), entries):
            out.append(r)
    with open(args.out, "w") as f:
        for r in out:
            f.write(json.dumps(r) + "\n")
    ok = sum(1 for r in out if r["query"])
    print(f"generated {ok}/{len(out)} queries -> {args.out}")


if __name__ == "__main__":
    main()
