#!/usr/bin/env python3
"""Shared reverse-QA LLM judge core + single-file debug driver.

The judge is an INDEPENDENT ORACLE: it sees only (query, entry title+content) — never the
pipeline's rank, score, or the known-item target id. It grades topical relevance of each
retrieved entry to the query on a 0-3 scale.

`claude -p ... --strict-mcp-config` is LOAD-BEARING: --strict-mcp-config disables MCP so the
judge never writes a query_log row into the very corpus under measurement (would pollute the
snapshot's search history). Judging runs on the Claude Code subscription => $0 API spend.

rqa_judge_batch.py imports RUBRIC / build_prompt / parse / judge_call from here and drives
them in parallel. Run this file directly to judge ONE eval-run result file (debugging):

    python3 judge_one.py <result.json> <model> <out.json>
"""
import json
import subprocess
import sys
import re

RUBRIC = """You are an impartial relevance judge for a knowledge-retrieval system. A developer typed a
SEARCH QUERY into a code-knowledge search box. The system returned KNOWLEDGE ENTRIES. Grade how well
each entry answers or materially informs that query — judge topical relevance ONLY, ignore ordering.

Grade each entry 0-3:
  3 = directly answers / is a best-answer entry for this query
  2 = relevant: materially helps address the query (partial but on-topic)
  1 = tangential: same broad area, does not actually help answer/act on the query
  0 = irrelevant: no bearing on the query

Judge on substance, not keyword overlap. Do not reward verbosity. Be strict about grade 3.

Output ONLY a JSON object, no prose:
{"grades":[{"id":<entry id>,"grade":<0-3>}, ...]}"""


def build_prompt(query, entries):
    parts = [RUBRIC, "\n\nSEARCH QUERY:\n" + query.strip()[:800], "\n\nKNOWLEDGE ENTRIES:"]
    for e in entries:
        c = (e.get("content") or "").replace("\n", " ")[:800]
        parts.append(f"\n[id {e['id']}] {e.get('title', '')}\n{c}")
    parts.append("\n\nRespond with ONLY the JSON object.")
    return "".join(parts)


def judge_call(prompt, model, timeout=180):
    """One `claude -p` judge call. --strict-mcp-config disables MCP (no query_log write)."""
    p = subprocess.run(
        ["claude", "-p", "--model", model, "--strict-mcp-config"],
        input=prompt, capture_output=True, text=True, timeout=timeout,
    )
    return p.stdout.strip()


def parse(out):
    m = re.search(r"\{.*\}", out, re.DOTALL)
    if not m:
        raise ValueError("no json: " + out[:200])
    return json.loads(m.group(0))


def grade_result(result, model, retries=3):
    """Grade one eval-run result dict. Returns the grade record (or raises on repeated failure)."""
    entries = result["profiles"]["baseline"]["entries"]
    query = result["query"]
    sid = result["scenario_id"]
    if not entries:
        return {"scenario_id": sid, "query": query, "grades": [], "ranked_ids": [], "model": model}
    prompt = build_prompt(query, entries)
    last = "unknown"
    for _ in range(retries):
        try:
            parsed = parse(judge_call(prompt, model))
            gmap = {g["id"]: g["grade"] for g in parsed.get("grades", [])}
            ranked_ids = [e["id"] for e in entries]
            grades = [{"id": e["id"], "grade": int(gmap.get(e["id"], 0)),
                       "final_score": e.get("final_score"), "status": e.get("status")}
                      for e in entries]
            return {"scenario_id": sid, "query": query, "grades": grades,
                    "ranked_ids": ranked_ids, "model": model}
        except Exception as ex:  # noqa: BLE001 — retry any judge/parse failure
            last = str(ex)
    raise RuntimeError(f"judge failed for {sid}: {last[:120]}")


def main():
    resfile, model, outfile = sys.argv[1], sys.argv[2], sys.argv[3]
    rec = grade_result(json.load(open(resfile)), model)
    json.dump(rec, open(outfile, "w"))
    print(f"graded {rec['scenario_id']} ({len(rec['grades'])} entries) -> {outfile}")


if __name__ == "__main__":
    main()
