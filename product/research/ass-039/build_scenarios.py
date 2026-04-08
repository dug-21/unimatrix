#!/usr/bin/env python3
"""
ASS-039: Build behaviorally-grounded eval scenarios from observations.

For each context_search call, find context_get calls in the same session
within 30 minutes. Those entry IDs are the behavioral ground truth.

For each context_briefing call, find context_get calls in the same session
within 30 minutes. Ground truth = entries agent found worth reading after briefing.

Output: JSONL with expected.entry_ids populated (never null).
"""

import hashlib
import json
import os
import sqlite3
import sys
from collections import defaultdict
from datetime import datetime, timezone

DB_PATH = "/home/vscode/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db"
THIRTY_MIN_MS = 30 * 60 * 1000

def build_scenarios(db_path: str) -> tuple[list[dict], str]:
    """Build scenarios from observations. Returns (scenarios, source_db_hash)."""
    # Compute source DB hash (streaming, handles large files)
    h = hashlib.sha256()
    with open(db_path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    source_db_hash = h.hexdigest()

    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    # ------------------------------------------------------------------
    # Step 1: Build deduped context_search set
    # Deduplicate on (session_id, ts_millis, query_text) to collapse
    # duplicate hook-fire records.
    # ------------------------------------------------------------------
    cur.execute("""
        SELECT DISTINCT session_id, ts_millis,
            json_extract(input, '$.query') as query_text,
            topic_signal
        FROM observations
        WHERE tool = 'mcp__unimatrix__context_search'
          AND json_valid(input) = 1
          AND json_extract(input, '$.query') IS NOT NULL
          AND json_extract(input, '$.query') != ''
        ORDER BY session_id, ts_millis
    """)
    searches = cur.fetchall()
    print(f"  Deduped context_search calls: {len(searches)}", file=sys.stderr)

    # ------------------------------------------------------------------
    # Step 2: Build deduped context_get lookup per session
    # Map session_id -> list of (ts_millis, entry_id)
    # ------------------------------------------------------------------
    cur.execute("""
        SELECT DISTINCT session_id, ts_millis,
            CAST(json_extract(input, '$.id') AS INTEGER) as entry_id
        FROM observations
        WHERE tool = 'mcp__unimatrix__context_get'
          AND json_valid(input) = 1
          AND json_extract(input, '$.id') IS NOT NULL
        ORDER BY session_id, ts_millis
    """)
    gets_raw = cur.fetchall()
    gets_by_session: dict[str, list[tuple[int, int]]] = defaultdict(list)
    for row in gets_raw:
        if row["entry_id"] and row["entry_id"] > 0:
            gets_by_session[row["session_id"]].append(
                (row["ts_millis"], row["entry_id"])
            )
    print(f"  Deduped context_get calls: {len(gets_raw)}", file=sys.stderr)

    # ------------------------------------------------------------------
    # Step 3: Build sessions lookup for agent_role / feature_cycle
    # ------------------------------------------------------------------
    cur.execute("""
        SELECT session_id, feature_cycle, agent_role
        FROM sessions
        WHERE session_id IS NOT NULL
    """)
    sessions_map: dict[str, dict] = {}
    for row in cur.fetchall():
        sessions_map[row["session_id"]] = {
            "feature_cycle": row["feature_cycle"] or "",
            "agent_role": row["agent_role"] or "",
        }

    # ------------------------------------------------------------------
    # Step 4: Build context_search scenarios
    # ------------------------------------------------------------------
    scenarios = []
    for row in searches:
        sid = row["session_id"]
        ts = row["ts_millis"]
        query = row["query_text"]
        topic = row["topic_signal"] or ""

        session_gets = gets_by_session.get(sid, [])
        entry_ids = sorted(set(
            eid for (get_ts, eid) in session_gets
            if get_ts > ts and get_ts <= ts + THIRTY_MIN_MS
        ))

        if not entry_ids:
            continue  # no behavioral ground truth — skip

        session_info = sessions_map.get(sid, {})
        feature_cycle = session_info.get("feature_cycle") or topic
        agent_role = session_info.get("agent_role") or "eval"

        query_hash = hashlib.md5(query.encode()).hexdigest()[:12]  # Short hash for disambiguation only — not cryptographic
        scenario_id = f"obs-{sid[:8]}-{ts}-{query_hash}"
        scenarios.append({
            "id": scenario_id,
            "query": query,
            "context": {
                "agent_id": agent_role if agent_role != "eval" else sid,
                "feature_cycle": feature_cycle,
                "session_id": sid,
                "retrieval_mode": "strict",
            },
            "baseline": None,
            "source": "observations",
            "expected": entry_ids,
        })

    print(f"  context_search scenarios with ground truth: {len(scenarios)}", file=sys.stderr)

    # ------------------------------------------------------------------
    # Step 5: Build context_briefing scenarios
    # ------------------------------------------------------------------
    cur.execute("""
        SELECT DISTINCT session_id, ts_millis,
            json_extract(input, '$.feature') as feature_val,
            json_extract(input, '$.phase') as phase_val,
            topic_signal
        FROM observations
        WHERE tool = 'mcp__unimatrix__context_briefing'
          AND json_valid(input) = 1
        ORDER BY session_id, ts_millis
    """)
    briefings = cur.fetchall()
    briefing_scenarios = []
    seen_briefing_keys: set[tuple] = set()  # deduplicate on (sid, ts, query_str) — feature vs topic_signal can differ while producing same query_str
    for row in briefings:
        sid = row["session_id"]
        ts = row["ts_millis"]
        feature = row["feature_val"] or ""
        phase = row["phase_val"] or ""
        topic = row["topic_signal"] or ""

        session_gets = gets_by_session.get(sid, [])
        entry_ids = sorted(set(
            eid for (get_ts, eid) in session_gets
            if get_ts > ts and get_ts <= ts + THIRTY_MIN_MS
        ))

        if not entry_ids:
            continue

        query_str = f"briefing:{feature or topic}:{phase}" if (feature or topic) else "briefing:unknown:unknown"

        briefing_key = (sid, ts, query_str)
        if briefing_key in seen_briefing_keys:
            continue  # feature_val vs topic_signal can differ but produce same query_str — keep first occurrence
        seen_briefing_keys.add(briefing_key)

        session_info = sessions_map.get(sid, {})
        feature_cycle = session_info.get("feature_cycle") or feature or topic
        agent_role = session_info.get("agent_role") or "eval"

        query_hash = hashlib.md5(query_str.encode()).hexdigest()[:12]  # Short hash for disambiguation only — not cryptographic
        scenario_id = f"obs-briefing-{sid[:8]}-{ts}-{query_hash}"
        briefing_scenarios.append({
            "id": scenario_id,
            "query": query_str,
            "context": {
                "agent_id": agent_role if agent_role != "eval" else sid,
                "feature_cycle": feature_cycle,
                "session_id": sid,
                "retrieval_mode": "strict",
            },
            "baseline": None,
            "source": "observations",
            "expected": entry_ids,
        })

    print(f"  context_briefing scenarios with ground truth: {len(briefing_scenarios)}", file=sys.stderr)
    conn.close()

    all_scenarios = scenarios + briefing_scenarios
    dup_count = len(all_scenarios) - len({s["id"] for s in all_scenarios})
    assert dup_count == 0, f"Duplicate scenario IDs detected: {dup_count} duplicates. Fix the ID formula in build_scenarios.py."
    return all_scenarios, source_db_hash


def print_stats(scenarios: list[dict]) -> None:
    search_scen = [s for s in scenarios if not s["id"].startswith("obs-briefing-")]
    briefing_scen = [s for s in scenarios if s["id"].startswith("obs-briefing-")]

    cycles = set(s["context"]["feature_cycle"] for s in scenarios if s["context"]["feature_cycle"])
    agent_roles = set(s["context"]["agent_id"] for s in scenarios)

    gt_sizes = [len(s["expected"]) for s in scenarios]

    print(f"\n=== Scenario Set Statistics ===")
    print(f"Total scenarios:          {len(scenarios)}")
    print(f"  context_search-derived: {len(search_scen)}")
    print(f"  context_briefing-derived: {len(briefing_scen)}")
    print(f"Distinct feature_cycles:  {len(cycles)}")
    print(f"Distinct agent_ids:       {len(agent_roles)}")
    print(f"GT entries per scenario:  min={min(gt_sizes)}, max={max(gt_sizes)}, avg={sum(gt_sizes)/len(gt_sizes):.1f}")
    print(f"\nMinimum viable checks:")
    print(f"  ≥50 search scenarios:   {'PASS' if len(search_scen) >= 50 else 'FAIL'} ({len(search_scen)})")
    print(f"  ≥10 briefing scenarios: {'PASS' if len(briefing_scen) >= 10 else 'FAIL'} ({len(briefing_scen)})")
    print(f"  ≥10 distinct cycles:    {'PASS' if len(cycles) >= 10 else 'FAIL'} ({len(cycles)})")


if __name__ == "__main__":
    out_path = "product/research/ass-039/harness/scenarios.jsonl"
    os.makedirs(os.path.dirname(out_path), exist_ok=True)

    print("Building scenarios from observations...", file=sys.stderr)
    scenarios, source_db_hash = build_scenarios(DB_PATH)

    # Write scenarios atomically (temp file + rename)
    tmp_scenarios = out_path + ".tmp"
    with open(tmp_scenarios, "w") as f:
        for s in scenarios:
            f.write(json.dumps(s) + "\n")
    os.rename(tmp_scenarios, out_path)

    # Write sidecar scenarios_meta.json atomically alongside scenarios.jsonl
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    meta = {
        "source_db_hash": source_db_hash,
        "generated_at": generated_at,
        "scenario_count": len(scenarios),
    }
    sidecar_path = os.path.join(os.path.dirname(out_path), "scenarios_meta.json")
    tmp_sidecar = sidecar_path + ".tmp"
    with open(tmp_sidecar, "w") as f:
        json.dump(meta, f, indent=2)
        f.write("\n")
    os.rename(tmp_sidecar, sidecar_path)

    print(f"\nWrote {len(scenarios)} scenarios to {out_path}", file=sys.stderr)
    print(f"Wrote sidecar to {sidecar_path}", file=sys.stderr)
    print_stats(scenarios)
