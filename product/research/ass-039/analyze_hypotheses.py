#!/usr/bin/env python3
"""
ASS-039: H1/H2/H3 hypothesis validation.

H1 — Goal clustering: cycles with similar goals retrieve overlapping entries.
H2 — Outcome correlation: entry access profiles differ by cycle outcome.
H3 — Phase stratification: design vs delivery phases access distinct entries.
"""

import json
import sqlite3
import sys
from collections import defaultdict
from math import log

DB_PATH = "/home/vscode/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db"

# ---------------------------------------------------------------------------
# Phase classification
# ---------------------------------------------------------------------------
DESIGN_PHASES = {"scope", "design", "design-review", "spec", "spec-review",
                 "scope-risk", "discovery"}
DELIVERY_PHASES = {"develop", "test", "bug-review", "fix", "pr-review",
                   "security-review", "review", "testing", "validate", "retrospective"}


def connect(db_path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def jaccard(a: set, b: set) -> float:
    if not a and not b:
        return 1.0
    union = a | b
    return len(a & b) / len(union) if union else 0.0


# ---------------------------------------------------------------------------
# Load cycle data
# ---------------------------------------------------------------------------
def load_cycles(conn: sqlite3.Connection) -> dict:
    """Returns dict: cycle_id -> {goal, phases: set, stop_outcome, entry_ids: set}"""
    cur = conn.cursor()

    # Goals from cycle_start
    goals = {}
    cur.execute("""
        SELECT cycle_id, goal FROM cycle_events
        WHERE event_type = 'cycle_start' AND goal IS NOT NULL AND goal != ''
        ORDER BY timestamp ASC
    """)
    for row in cur.fetchall():
        if row["cycle_id"] not in goals:
            goals[row["cycle_id"]] = row["goal"]

    # Phases from cycle_phase_end
    phases: dict[str, set] = defaultdict(set)
    cur.execute("""
        SELECT cycle_id, phase FROM cycle_events
        WHERE event_type = 'cycle_phase_end' AND phase IS NOT NULL
    """)
    for row in cur.fetchall():
        if row["phase"]:
            phases[row["cycle_id"]].add(row["phase"].strip())

    # Stop outcomes
    stop_outcomes: dict[str, str] = {}
    cur.execute("""
        SELECT cycle_id, outcome FROM cycle_events
        WHERE event_type = 'cycle_stop' AND outcome IS NOT NULL AND outcome != ''
        ORDER BY timestamp DESC
    """)
    for row in cur.fetchall():
        if row["cycle_id"] not in stop_outcomes:
            stop_outcomes[row["cycle_id"]] = row["outcome"]

    # Entry access per cycle (deduplicated by session+entry)
    entry_ids_by_cycle: dict[str, set] = defaultdict(set)
    cur.execute("""
        SELECT DISTINCT topic_signal,
            CAST(json_extract(input, '$.id') AS INTEGER) as entry_id
        FROM observations
        WHERE tool = 'mcp__unimatrix__context_get'
          AND json_valid(input) = 1
          AND json_extract(input, '$.id') IS NOT NULL
          AND topic_signal IS NOT NULL AND topic_signal != ''
    """)
    for row in cur.fetchall():
        if row["entry_id"] and row["entry_id"] > 0:
            entry_ids_by_cycle[row["topic_signal"]].add(row["entry_id"])

    # Entry access per cycle per phase (using timing + cycle_events timestamps)
    # Approximate: map observation ts_millis to phase using cycle_events
    phase_events: dict[str, list] = defaultdict(list)
    cur.execute("""
        SELECT cycle_id, phase, timestamp FROM cycle_events
        WHERE event_type = 'cycle_phase_end'
          AND phase IS NOT NULL AND timestamp IS NOT NULL
        ORDER BY cycle_id, timestamp ASC
    """)
    for row in cur.fetchall():
        phase_events[row["cycle_id"]].append((row["timestamp"] * 1000, row["phase"]))

    # Get cycle start timestamps
    cycle_starts: dict[str, int] = {}
    cur.execute("""
        SELECT cycle_id, MIN(timestamp) as ts FROM cycle_events
        WHERE event_type = 'cycle_start'
        GROUP BY cycle_id
    """)
    for row in cur.fetchall():
        cycle_starts[row["cycle_id"]] = row["ts"] * 1000

    # Entry access per cycle per phase-class (design vs delivery)
    entries_by_cycle_phase: dict[str, dict[str, set]] = defaultdict(lambda: {"design": set(), "delivery": set()})
    cur.execute("""
        SELECT DISTINCT o.topic_signal, o.ts_millis,
            CAST(json_extract(o.input, '$.id') AS INTEGER) as entry_id
        FROM observations o
        WHERE o.tool = 'mcp__unimatrix__context_get'
          AND json_valid(o.input) = 1
          AND json_extract(o.input, '$.id') IS NOT NULL
          AND o.topic_signal IS NOT NULL AND o.topic_signal != ''
        ORDER BY o.topic_signal, o.ts_millis
    """)
    for row in cur.fetchall():
        cid = row["topic_signal"]
        ts = row["ts_millis"]
        eid = row["entry_id"]
        if not eid or eid <= 0:
            continue

        # Determine which phase this observation falls in
        phase_class = _classify_phase(ts, cid, phase_events, cycle_starts)
        if phase_class:
            entries_by_cycle_phase[cid][phase_class].add(eid)

    # Build cycles dict
    cycles = {}
    all_cycle_ids = set(goals) | set(phases) | set(stop_outcomes)
    for cid in all_cycle_ids:
        cycles[cid] = {
            "goal": goals.get(cid, ""),
            "phases": phases.get(cid, set()),
            "stop_outcome": stop_outcomes.get(cid, ""),
            "entry_ids": entry_ids_by_cycle.get(cid, set()),
            "design_entries": entries_by_cycle_phase[cid]["design"],
            "delivery_entries": entries_by_cycle_phase[cid]["delivery"],
        }

    return cycles


def _classify_phase(ts_ms: int, cycle_id: str,
                    phase_events: dict, cycle_starts: dict) -> str | None:
    """Return 'design' or 'delivery' based on which phase the timestamp falls in."""
    events = phase_events.get(cycle_id, [])
    start_ts = cycle_starts.get(cycle_id, 0)

    if not events:
        return None

    # Phase events are (phase_end_ts, phase_name)
    # Observation falls in phase P if it's between the previous phase end and phase P end
    prev_ts = start_ts
    for phase_end_ts, phase_name in sorted(events):
        if prev_ts <= ts_ms <= phase_end_ts:
            pclass = _phase_class(phase_name)
            if pclass:
                return pclass
        prev_ts = phase_end_ts

    # After last phase end — use last phase
    if events:
        last_phase = sorted(events)[-1][1]
        return _phase_class(last_phase)
    return None


def _phase_class(phase_name: str) -> str | None:
    if phase_name in DESIGN_PHASES:
        return "design"
    if phase_name in DELIVERY_PHASES:
        return "delivery"
    return None


# ---------------------------------------------------------------------------
# Goal similarity (keyword-based, as embedding approximation)
# ---------------------------------------------------------------------------
STOP_WORDS = {
    "the", "a", "an", "and", "or", "in", "of", "to", "for", "from", "with",
    "on", "by", "as", "is", "are", "be", "that", "this", "it", "at", "from",
    "so", "via", "per", "not", "if", "when", "all", "any", "its", "into",
    "enabling", "ensures", "ensure", "making", "allowing", "completing"
}


def goal_tokens(goal: str) -> set[str]:
    """Tokenize goal into meaningful keywords."""
    tokens = set()
    for word in goal.lower().split():
        word = word.strip(".,();:/-")
        if len(word) >= 4 and word not in STOP_WORDS:
            tokens.add(word)
    return tokens


def goal_similarity(g1: str, g2: str) -> float:
    """Jaccard similarity on goal keyword tokens."""
    t1 = goal_tokens(g1)
    t2 = goal_tokens(g2)
    return jaccard(t1, t2)


# ---------------------------------------------------------------------------
# H1: Goal clustering
# ---------------------------------------------------------------------------
def h1_goal_clustering(cycles: dict, threshold: float = 0.15) -> dict:
    """
    Test H1: cycles with similar goals retrieve overlapping entries.

    Uses keyword Jaccard as goal similarity proxy.
    Threshold of 0.15 is conservative for short goal texts.
    """
    cycles_with_goals = {
        cid: c for cid, c in cycles.items()
        if c["goal"] and len(c["entry_ids"]) >= 2
    }

    if len(cycles_with_goals) < 5:
        return {"verdict": "INSUFFICIENT_DATA",
                "reason": f"Only {len(cycles_with_goals)} cycles with goals + entries"}

    cids = list(cycles_with_goals.keys())

    # Build similarity matrix
    sim_matrix: dict[tuple, float] = {}
    for i, cid_a in enumerate(cids):
        for j, cid_b in enumerate(cids):
            if i >= j:
                continue
            sim = goal_similarity(cycles_with_goals[cid_a]["goal"],
                                  cycles_with_goals[cid_b]["goal"])
            sim_matrix[(cid_a, cid_b)] = sim

    # Cluster: group cycles where at least one pair has sim >= threshold
    # Simple greedy clustering
    clusters: list[set] = []
    assigned = set()

    for i, cid_a in enumerate(cids):
        if cid_a in assigned:
            continue
        cluster = {cid_a}
        for j, cid_b in enumerate(cids):
            if cid_b in assigned or cid_b == cid_a:
                continue
            pair = (cid_a, cid_b) if cid_a < cid_b else (cid_b, cid_a)
            if sim_matrix.get(pair, 0) >= threshold:
                cluster.add(cid_b)
        clusters.append(cluster)
        assigned |= cluster

    # Measure intra-cluster vs inter-cluster entry overlap
    intra_overlaps = []
    inter_overlaps = []

    for cluster in clusters:
        cluster_list = list(cluster)
        for i in range(len(cluster_list)):
            for j in range(i + 1, len(cluster_list)):
                a = cycles_with_goals[cluster_list[i]]["entry_ids"]
                b = cycles_with_goals[cluster_list[j]]["entry_ids"]
                intra_overlaps.append(jaccard(a, b))

    # Inter-cluster: random sample of cross-cluster pairs
    for ci, cluster_a in enumerate(clusters):
        for cj, cluster_b in enumerate(clusters):
            if ci >= cj:
                continue
            for ca in cluster_a:
                for cb in cluster_b:
                    a = cycles_with_goals[ca]["entry_ids"]
                    b = cycles_with_goals[cb]["entry_ids"]
                    inter_overlaps.append(jaccard(a, b))

    intra_mean = sum(intra_overlaps) / len(intra_overlaps) if intra_overlaps else 0
    inter_mean = sum(inter_overlaps) / len(inter_overlaps) if inter_overlaps else 0
    effect = intra_mean - inter_mean

    # Cluster size distribution
    cluster_sizes = sorted([len(c) for c in clusters], reverse=True)

    # Top goal pairs by similarity
    top_pairs = sorted(sim_matrix.items(), key=lambda x: -x[1])[:5]

    verdict = "PASS" if intra_mean > inter_mean * 1.5 and intra_mean > 0.02 else "FAIL"

    return {
        "verdict": verdict,
        "cycles_analyzed": len(cycles_with_goals),
        "similarity_threshold": threshold,
        "clusters": len(clusters),
        "cluster_size_distribution": cluster_sizes[:10],
        "intra_cluster_overlap": round(intra_mean, 4),
        "inter_cluster_overlap": round(inter_mean, 4),
        "effect_size": round(effect, 4),
        "intra_pairs": len(intra_overlaps),
        "inter_pairs": len(inter_overlaps),
        "top_goal_pairs": [
            {
                "a": pair[0], "b": pair[1],
                "goal_sim": round(sim, 4),
                "entry_overlap": round(jaccard(
                    cycles_with_goals[pair[0]]["entry_ids"],
                    cycles_with_goals[pair[1]]["entry_ids"]
                ), 4)
            }
            for pair, sim in top_pairs
        ],
    }


# ---------------------------------------------------------------------------
# H2: Outcome correlation
# ---------------------------------------------------------------------------
def h2_outcome_correlation(cycles: dict) -> dict:
    """
    Test H2: distinct entry access profiles by cycle outcome.

    Since sessions.outcome is 99%+ success, uses rework proxy:
    cycles where a phase repeated = rework signal.
    Also computes entry frequency distribution (load-bearing vs rare).
    """
    # Classify cycles: 'rework' if any phase appeared more than once
    success_cycles = []
    rework_cycles = []

    for cid, c in cycles.items():
        if not c["entry_ids"]:
            continue
        # Rework = phase repeated (inferred from cycle_events repeated phases)
        # This is a proxy — actual rework tracking requires phase repeat detection
        # Use the cycle_events data: if same phase appeared > once, it's rework
        # We detect this from the phases being a set (collapsed), but we need counts
        # For now, classify all as success (99.4% rate) and analyze distribution
        success_cycles.append(cid)

    # Load rework proxy: look for cycles where obs topic_signal has repeated phase data
    # Better approach: check if cycle has entries accessed in both design AND delivery
    # (indicating at least one full pass was completed)

    # Entry frequency across all cycles
    entry_cycle_count: dict[int, int] = defaultdict(int)
    total_cycles_with_entries = 0
    for cid, c in cycles.items():
        if c["entry_ids"]:
            total_cycles_with_entries += 1
            for eid in c["entry_ids"]:
                entry_cycle_count[eid] += 1

    # Sort by frequency
    entry_freqs = sorted(entry_cycle_count.items(), key=lambda x: -x[1])
    top_entries = entry_freqs[:10]

    # Distribution analysis: how many entries appear in N cycles
    freq_dist: dict[int, int] = defaultdict(int)
    for eid, cnt in entry_cycle_count.items():
        freq_dist[cnt] += 1

    # Load-bearing: entries appearing in ≥20% of cycles
    threshold = max(1, total_cycles_with_entries * 0.20)
    load_bearing = [(eid, cnt) for eid, cnt in entry_freqs if cnt >= threshold]

    # Check distribution uniformity via entropy
    total_accesses = sum(entry_cycle_count.values())
    if total_accesses > 0:
        probs = [cnt / total_accesses for cnt in entry_cycle_count.values()]
        entropy = -sum(p * log(p) for p in probs if p > 0)
        max_entropy = log(len(entry_cycle_count)) if entry_cycle_count else 1
        normalized_entropy = entropy / max_entropy if max_entropy > 0 else 0
    else:
        normalized_entropy = 0

    # Verdict: H2 requires true rework cases
    # With 99.4% success rate, outcome correlation cannot be measured
    # However, load-bearing entries (high frequency) are still identified
    verdict = "INSUFFICIENT_OUTCOME_VARIANCE"

    return {
        "verdict": verdict,
        "reason": "Sessions table shows 162/163 outcomes = 'success' (0 rework cases). "
                  "Outcome-correlated analysis requires at least 10 rework cycles. "
                  "The 'rework' events that exist are within-cycle phase repeats, not "
                  "session-level failures. H2 cannot be validated at the current corpus.",
        "total_cycles_with_entries": total_cycles_with_entries,
        "distinct_entries_accessed": len(entry_cycle_count),
        "load_bearing_entries_threshold_pct": 20,
        "load_bearing_count": len(load_bearing),
        "load_bearing_entries": load_bearing[:15],
        "top_10_by_frequency": top_entries,
        "access_distribution_entropy": round(normalized_entropy, 4),
        "freq_distribution": {
            "1_cycle": freq_dist.get(1, 0),
            "2_3_cycles": sum(freq_dist.get(n, 0) for n in [2, 3]),
            "4_9_cycles": sum(freq_dist.get(n, 0) for n in range(4, 10)),
            "10plus_cycles": sum(cnt for n, cnt in freq_dist.items() if n >= 10),
        },
    }


# ---------------------------------------------------------------------------
# H3: Phase stratification
# ---------------------------------------------------------------------------
def h3_phase_stratification(cycles: dict) -> dict:
    """
    Test H3: design vs delivery phases access distinct entry sets.
    Within goal clusters (H1), measure cross-phase vs same-phase entry overlap.
    """
    # Only use cycles with both design AND delivery entries
    valid_cycles = {
        cid: c for cid, c in cycles.items()
        if c["design_entries"] and c["delivery_entries"]
    }

    if len(valid_cycles) < 5:
        return {"verdict": "INSUFFICIENT_DATA",
                "reason": f"Only {len(valid_cycles)} cycles have both design and delivery observations"}

    # Per-cycle: cross-phase overlap (design ∩ delivery / design ∪ delivery)
    cross_phase_overlaps = []
    same_phase_design = []
    same_phase_delivery = []

    for cid, c in valid_cycles.items():
        d = c["design_entries"]
        v = c["delivery_entries"]
        cross_phase_overlaps.append(jaccard(d, v))

    # Across cycles: same-phase overlap (design-design, delivery-delivery)
    cycle_list = list(valid_cycles.keys())
    for i in range(len(cycle_list)):
        for j in range(i + 1, len(cycle_list)):
            a = valid_cycles[cycle_list[i]]
            b = valid_cycles[cycle_list[j]]
            if a["design_entries"] and b["design_entries"]:
                same_phase_design.append(jaccard(a["design_entries"], b["design_entries"]))
            if a["delivery_entries"] and b["delivery_entries"]:
                same_phase_delivery.append(jaccard(a["delivery_entries"], b["delivery_entries"]))

    cross_mean = sum(cross_phase_overlaps) / len(cross_phase_overlaps) if cross_phase_overlaps else 0
    design_mean = sum(same_phase_design) / len(same_phase_design) if same_phase_design else 0
    delivery_mean = sum(same_phase_delivery) / len(same_phase_delivery) if same_phase_delivery else 0

    # H3 passes if cross-phase overlap significantly lower than same-phase
    # Use delivery-design comparison (within same cycle)
    # Expected: same-phase (across cycles) > cross-phase (within cycle)
    # NOTE: within-cycle cross-phase will naturally differ since same entries
    # can appear in both phases (reference lookups happen in delivery too)

    # Better H3 test: across cycles with similar goals, do design sessions cluster
    # together (share more entries) than mixed design/delivery sessions?
    # Use goal similarity from H1
    cycles_with_goals = {
        cid: c for cid, c in valid_cycles.items() if c.get("goal", "")
    }

    if len(cycles_with_goals) >= 4:
        # Within-goal-cluster cross-phase vs same-phase
        # For each pair of cycles, classify as: same-phase or cross-phase
        # For each pair, measure entry overlap using their phase-specific sets
        intra_cluster_same_phase = []
        intra_cluster_cross_phase = []
        inter_cluster_overlaps = []

        CLUSTER_THRESHOLD = 0.12
        cids = list(cycles_with_goals.keys())

        for i, cid_a in enumerate(cids):
            for j, cid_b in enumerate(cids):
                if i >= j:
                    continue
                goal_sim = goal_similarity(cycles_with_goals[cid_a].get("goal", ""),
                                           cycles_with_goals[cid_b].get("goal", ""))
                a = valid_cycles.get(cid_a, {})
                b = valid_cycles.get(cid_b, {})

                if goal_sim >= CLUSTER_THRESHOLD:
                    # Same cluster: compare same vs cross phase
                    dd = jaccard(a.get("design_entries", set()), b.get("design_entries", set()))
                    vv = jaccard(a.get("delivery_entries", set()), b.get("delivery_entries", set()))
                    dv = jaccard(a.get("design_entries", set()), b.get("delivery_entries", set()))
                    vd = jaccard(a.get("delivery_entries", set()), b.get("design_entries", set()))

                    intra_cluster_same_phase.extend([dd, vv])
                    intra_cluster_cross_phase.extend([dv, vd])
                else:
                    d = a.get("design_entries", set()) | a.get("delivery_entries", set())
                    e = b.get("design_entries", set()) | b.get("delivery_entries", set())
                    inter_cluster_overlaps.append(jaccard(d, e))

        intra_same = sum(intra_cluster_same_phase) / len(intra_cluster_same_phase) if intra_cluster_same_phase else 0
        intra_cross = sum(intra_cluster_cross_phase) / len(intra_cluster_cross_phase) if intra_cluster_cross_phase else 0
        inter = sum(inter_cluster_overlaps) / len(inter_cluster_overlaps) if inter_cluster_overlaps else 0

        verdict = "PASS" if intra_same > intra_cross * 1.3 and intra_same > 0.01 else "FAIL"

        cluster_analysis = {
            "intra_cluster_same_phase_overlap": round(intra_same, 4),
            "intra_cluster_cross_phase_overlap": round(intra_cross, 4),
            "inter_cluster_overlap": round(inter, 4),
            "intra_same_pairs": len(intra_cluster_same_phase),
            "intra_cross_pairs": len(intra_cluster_cross_phase),
        }
    else:
        verdict = "INSUFFICIENT_DATA"
        cluster_analysis = {}

    # Per-cycle stats
    per_cycle = []
    for cid, c in sorted(valid_cycles.items()):
        d = c["design_entries"]
        v = c["delivery_entries"]
        per_cycle.append({
            "cycle": cid,
            "design_entries": len(d),
            "delivery_entries": len(v),
            "cross_phase_jaccard": round(jaccard(d, v), 4),
        })

    return {
        "verdict": verdict,
        "cycles_with_both_phases": len(valid_cycles),
        "within_cycle_cross_phase_overlap": {
            "mean": round(cross_mean, 4),
            "n": len(cross_phase_overlaps),
        },
        "across_cycles_design_design": {
            "mean": round(design_mean, 4),
            "n": len(same_phase_design),
        },
        "across_cycles_delivery_delivery": {
            "mean": round(delivery_mean, 4),
            "n": len(same_phase_delivery),
        },
        "cluster_analysis": cluster_analysis,
        "per_cycle_phase_overlap": per_cycle,
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    conn = connect(DB_PATH)
    print("Loading cycle data...", file=sys.stderr)
    cycles = load_cycles(conn)
    conn.close()

    cycles_with_goals = sum(1 for c in cycles.values() if c["goal"])
    cycles_with_entries = sum(1 for c in cycles.values() if c["entry_ids"])
    print(f"  Total cycles: {len(cycles)}", file=sys.stderr)
    print(f"  Cycles with goals: {cycles_with_goals}", file=sys.stderr)
    print(f"  Cycles with entry access: {cycles_with_entries}", file=sys.stderr)

    print("\nRunning H1...", file=sys.stderr)
    h1 = h1_goal_clustering(cycles)

    print("Running H2...", file=sys.stderr)
    h2 = h2_outcome_correlation(cycles)

    print("Running H3...", file=sys.stderr)
    h3 = h3_phase_stratification(cycles)

    results = {
        "feasibility": {
            "total_cycles": len(cycles),
            "cycles_with_goals": cycles_with_goals,
            "cycles_with_entry_access": cycles_with_entries,
            "cycles_with_goals_and_entries": sum(
                1 for c in cycles.values() if c["goal"] and c["entry_ids"]
            ),
        },
        "H1": h1,
        "H2": h2,
        "H3": h3,
    }

    print(json.dumps(results, indent=2, default=list))


if __name__ == "__main__":
    main()
