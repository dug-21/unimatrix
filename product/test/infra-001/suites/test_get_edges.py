"""Suite: context_get next-hop edge affordance (vnc-037).

End-to-end coverage of the ranked, capped (<=3) next-hop edge surfacing on
``context_get`` through the MCP JSON-RPC binary. These tests are the MCP-visible
echo of the discriminating store/server unit tests; the store-boundary
"never-materialized" proof and the injected-failure fail-loud RED tests live in
the Rust unit suites (graph_queries_ranked_tests.rs / get_edges_tests.rs) where a
failure-injection seam exists.

Cumulative: reuses the ``server`` fixture and the established
``_compute_db_path`` direct-SQLite seeding pattern from test_tools.py. Symmetric
edges are seeded as BOTH reciprocal graph_edges rows to exercise canonicalization
(ADR-007). Authored edges use ``source='agent'``; inferred edges use a live
statistical source string (e.g. ``co_access``).

Risk coverage: R-01 (canon display+totals), R-02 (ranking), R-03 (split count),
R-04 (high-degree cap), R-07 (list-view byte-identity), R-12 (Supersedes), R-14
(opt-out), R-15 (dangling), AC-01..AC-08, AC-11, AC-14 (zero-vs-failure).
"""

import hashlib
import os
import sqlite3
import time

import pytest

from harness.assertions import (
    assert_tool_success,
    extract_entry_id,
    parse_entry,
)


# --- seeding helpers (cumulative — mirror test_tools._compute_db_path) --------

def _compute_db_path(project_dir):
    """Server SQLite DB path from the project dir (mirrors compute_project_hash)."""
    canonical = os.path.realpath(project_dir)
    digest = hashlib.sha256(canonical.encode()).hexdigest()[:16]
    return os.path.join(os.path.expanduser("~"), ".unimatrix", digest, "unimatrix.db")


# Synthetic target-id base — well above any MCP-stored id. Target entries are
# seeded directly via SQL (see _seed_target) so each is genuinely distinct and the
# server's semantic store-dedup (cosine >= ~0.93 on short templated content) can
# never collapse them. Only the ANCHOR is stored via the real MCP path.
_TARGET_BASE = 900_000


def _store_anchor(server, content):
    """Store the anchor entry via the REAL MCP store path and return its id. The
    anchor must be a genuine entry (the primary entry_store.get reads it)."""
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    return extract_entry_id(resp)


def _seed_target(server, target_id, *, confidence=None, title=None):
    """Insert a target ENTRY directly via SQL with a pinned id + confidence + title.

    Bypasses MCP store dedup (short templated content embeds too similarly and the
    store rejects near-duplicates, cosine >= ~0.93). The ranked LEFT JOIN reads
    entries.confidence and the title join reads entries.title with NO status filter,
    so a directly-seeded row participates correctly regardless of status encoding.
    If confidence is None the target is left absent (dangling) by the caller."""
    db_path = _compute_db_path(server.project_dir)
    conn = sqlite3.connect(db_path, timeout=30)
    now = int(time.time())
    try:
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA busy_timeout=30000")
        conn.execute(
            "INSERT OR REPLACE INTO entries "
            "(id, title, content, topic, category, source, status, confidence, "
            " created_at, updated_at) "
            "VALUES (?, ?, ?, 'testing', 'convention', 'agent', 0, ?, ?, ?)",
            (
                target_id,
                title if title is not None else f"target {target_id}",
                f"target entry {target_id} content",
                confidence if confidence is not None else 0.5,
                now,
                now,
            ),
        )
        conn.commit()
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        conn.close()


def _seed_edges(server, rows):
    """Insert graph_edges rows directly. Each row is a dict with keys
    source_id, target_id, relation_type, source, and optional weight.
    Symmetric pairs must be passed as TWO reciprocal rows by the caller.
    Checkpoints WAL so the server's read pool sees the rows immediately."""
    db_path = _compute_db_path(server.project_dir)
    conn = sqlite3.connect(db_path, timeout=30)
    now = int(time.time())
    try:
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA busy_timeout=30000")
        for r in rows:
            conn.execute(
                "INSERT OR IGNORE INTO graph_edges "
                "(source_id, target_id, relation_type, weight, created_at, "
                " created_by, source) VALUES (?, ?, ?, ?, ?, 'test', ?)",
                (
                    r["source_id"],
                    r["target_id"],
                    r["relation_type"],
                    r.get("weight", 1.0),
                    now,
                    r["source"],
                ),
            )
        conn.commit()
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        conn.close()


def _seed_symmetric(server, a, b, relation_type, source="co_access", weight=1.0):
    """Seed a symmetric edge as BOTH reciprocal rows (a->b and b->a)."""
    _seed_edges(server, [
        {"source_id": a, "target_id": b, "relation_type": relation_type,
         "source": source, "weight": weight},
        {"source_id": b, "target_id": a, "relation_type": relation_type,
         "source": source, "weight": weight},
    ])


def _get_json(server, entry_id, include_edges=None):
    resp = server.context_get(
        entry_id, format="json", include_edges=include_edges
    )
    return parse_entry(resp)


# --- AC-01 / AC-11: default-on, freshness, opt-out ---------------------------

@pytest.mark.smoke
def test_get_surfaces_ranked_edges_default(server):
    """AC-01/AC-02: default-on context_get surfaces depth-1 edges (both directions)
    with the exact 5-field shape; no include_edges param needed."""
    anchor = _store_anchor(server, "anchor entry default-on full of distinct words")
    out_t = _TARGET_BASE + 1
    in_t = _TARGET_BASE + 2
    _seed_target(server, out_t, confidence=0.8)
    _seed_target(server, in_t, confidence=0.7)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": out_t, "relation_type": "Supports",
         "source": "agent"},
        {"source_id": in_t, "target_id": anchor, "relation_type": "Prerequisite",
         "source": "agent"},
    ])
    entry = _get_json(server, anchor)
    assert "edges" in entry, "default-on must surface an edges key"
    assert "edge_totals" in entry
    edges = entry["edges"]
    assert len(edges) == 2
    # exact 5-field shape, no enrichment
    for e in edges:
        assert set(e.keys()) == {
            "edge_type", "direction", "target_id", "target_title", "authored"
        }, f"unexpected edge fields: {e.keys()}"
    targets = {e["target_id"]: e for e in edges}
    # anchor->out_t (anchor is source_id) is outbound; in_t->anchor is inbound.
    assert targets[out_t]["direction"] == "outbound"
    assert targets[in_t]["direction"] == "inbound"


def test_get_edges_freshness_no_tick(server):
    """AC-01: an edge written then immediately read appears on the next get with no
    tick wait (live graph_edges SQL read)."""
    anchor = _store_anchor(server, "freshness anchor with its own distinct content")
    tgt = _TARGET_BASE + 10
    _seed_target(server, tgt, confidence=0.5)
    before = _get_json(server, anchor)
    assert before["edges"] == []
    _seed_edges(server, [
        {"source_id": anchor, "target_id": tgt, "relation_type": "Supports",
         "source": "agent"},
    ])
    after = _get_json(server, anchor)
    assert len(after["edges"]) == 1
    assert after["edges"][0]["target_id"] == tgt


def test_get_include_edges_opt_out(server):
    """AC-11/R-14: include_edges=false suppresses both keys (byte-indistinguishable
    from a list-view payload)."""
    anchor = _store_anchor(server, "opt-out anchor with unique descriptive text")
    tgt = _TARGET_BASE + 20
    _seed_target(server, tgt, confidence=0.5)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": tgt, "relation_type": "Supports",
         "source": "agent"},
    ])
    entry = _get_json(server, anchor, include_edges=False)
    assert "edges" not in entry, "opt-out must omit the edges key entirely"
    assert "edge_totals" not in entry


def test_get_include_edges_true_surfaces(server):
    """AC-11: explicit include_edges=true surfaces (same as default)."""
    anchor = _store_anchor(server, "explicit true anchor with separate wording here")
    tgt = _TARGET_BASE + 30
    _seed_target(server, tgt, confidence=0.5)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": tgt, "relation_type": "Supports",
         "source": "agent"},
    ])
    entry = _get_json(server, anchor, include_edges=True)
    assert "edges" in entry
    assert len(entry["edges"]) == 1


# --- R-01: symmetric canonicalization (display AND totals, separately) --------

@pytest.mark.parametrize("rel", ["Contradicts", "CoAccess", "Informs"])
def test_get_symmetric_canonicalized_one_arrow(server, rel):
    """R-01 display: a symmetric reciprocal pair surfaces ONE edge with direction
    'both', not two. Asserted for all three symmetric types."""
    anchor = _store_anchor(server, f"symmetric canon anchor for relation {rel} distinct")
    other = _TARGET_BASE + 40
    _seed_target(server, other, confidence=0.6)
    _seed_symmetric(server, anchor, other, rel)
    entry = _get_json(server, anchor)
    matching = [e for e in entry["edges"] if e["target_id"] == other]
    assert len(matching) == 1, f"{rel} reciprocal pair must collapse to one edge"
    assert matching[0]["direction"] == "both"


def test_get_edge_totals_symmetric_once(server):
    """R-01/R-03 totals: the symmetric pair contributes ONCE to the `both` bucket;
    inbound stays unchanged (#744 inbound-degree integrity). Separate assertion
    from the displayed-set test."""
    anchor = _store_anchor(server, "totals symmetric once anchor unique sentence here")
    other = _TARGET_BASE + 50
    _seed_target(server, other, confidence=0.6)
    _seed_symmetric(server, anchor, other, "Contradicts")
    entry = _get_json(server, anchor)
    totals = entry["edge_totals"]
    assert set(totals.keys()) == {"inbound", "outbound", "both"}, totals
    assert totals["both"] == 1, "symmetric edge counted once in `both`"
    assert totals["inbound"] == 0, "symmetric must NOT fold into inbound (#744)"
    assert totals["outbound"] == 0


# --- R-02: ranking (authored-first, inferred fill, confidence) ---------------

def test_get_authored_priority_under_cap(server):
    """R-02/AC-05a: >3 edges with >=3 authored -> only authored show, no inferred."""
    anchor = _store_anchor(server, "authored priority anchor with unique descriptive body")
    a1, a2, a3 = _TARGET_BASE + 60, _TARGET_BASE + 61, _TARGET_BASE + 62
    inf = _TARGET_BASE + 63
    for t in (a1, a2, a3):
        _seed_target(server, t, confidence=0.1)
    _seed_target(server, inf, confidence=0.99)  # high-conf but inferred
    _seed_edges(server, [
        {"source_id": anchor, "target_id": a1, "relation_type": "Supports",
         "source": "agent"},
        {"source_id": anchor, "target_id": a2, "relation_type": "Supports",
         "source": "agent"},
        {"source_id": anchor, "target_id": a3, "relation_type": "Supports",
         "source": "agent"},
        {"source_id": anchor, "target_id": inf, "relation_type": "Supports",
         "source": "co_access", "weight": 5.0},
    ])
    entry = _get_json(server, anchor)
    shown = {e["target_id"] for e in entry["edges"]}
    assert len(entry["edges"]) == 3
    assert inf not in shown, "high-confidence inferred must be excluded by authored-first"
    assert all(e["authored"] for e in entry["edges"])


def test_get_inferred_fill_when_authored_lt_3(server):
    """R-02/AC-05b/c: <3 authored -> inferred top up to exactly 3, ranked by target
    confidence. Discriminating: the lower-confidence inferred is excluded even
    though it has a HIGHER edge weight (proof outside cap, #3886 — weight must NOT
    decide)."""
    anchor = _store_anchor(server, "inferred fill anchor distinct phrasing for embedding")
    a1 = _TARGET_BASE + 70
    hi1, hi2, lo = _TARGET_BASE + 71, _TARGET_BASE + 72, _TARGET_BASE + 73
    _seed_target(server, a1, confidence=0.1)
    _seed_target(server, hi1, confidence=0.9)
    _seed_target(server, hi2, confidence=0.8)
    _seed_target(server, lo, confidence=0.2)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": a1, "relation_type": "Supports",
         "source": "agent"},
        # lo seeded with a HIGHER weight so weight-ordering != confidence-ordering
        {"source_id": anchor, "target_id": lo, "relation_type": "Supports",
         "source": "co_access", "weight": 9.0},
        {"source_id": anchor, "target_id": hi1, "relation_type": "Supports",
         "source": "co_access", "weight": 1.0},
        {"source_id": anchor, "target_id": hi2, "relation_type": "Supports",
         "source": "co_access", "weight": 1.0},
    ])
    entry = _get_json(server, anchor)
    shown = {e["target_id"] for e in entry["edges"]}
    assert len(entry["edges"]) == 3
    assert a1 in shown, "the authored edge always wins a slot"
    assert hi1 in shown and hi2 in shown, "higher-confidence inferred fill remaining slots"
    assert lo not in shown, "lower-confidence inferred excluded; weight does NOT decide"


# --- AC-05e/AC-08: capped pointer + uncapped totals --------------------------

def test_get_capped_pointer_and_uncapped_totals(server):
    """AC-05d/e: with >3 edges the displayed set is <=3 but edge_totals report the
    true uncapped split; markdown carries the '...N more — use context_graph'
    pointer."""
    anchor = _store_anchor(server, "capped pointer anchor with its own unique narrative")
    targets = [_TARGET_BASE + 80 + i for i in range(6)]
    for i, t in enumerate(targets):
        _seed_target(server, t, confidence=0.5 + i * 0.05)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": t, "relation_type": "Supports",
         "source": "agent"} for t in targets
    ])
    entry = _get_json(server, anchor)
    assert len(entry["edges"]) == 3, "displayed set capped at 3"
    totals = entry["edge_totals"]
    assert totals["outbound"] == 6, "totals are uncapped"
    # markdown pointer
    md = server.context_get(anchor, format="markdown")
    text = assert_tool_success(md).text
    assert "more" in text and "context_graph" in text, (
        "capped markdown must carry the '...N more — use context_graph' pointer"
    )


# --- AC-06 / AC-14: zero-edge empty state is a SUCCESS, distinct from failure --

def test_get_zero_edge_empty_state_all_formats(server):
    """AC-06/DNB-3 + AC-14(b): a genuine zero-edge entry returns a SUCCESS with an
    explicit empty state in all three formats — never an error and never an
    omitted key. (Zero-vs-failure distinction: a real zero-edge get is a SUCCESS.)"""
    anchor = _store_anchor(server, "zero edge anchor entirely unique content body text")
    # json
    entry = _get_json(server, anchor)
    assert entry["edges"] == []
    assert entry["edge_totals"] == {"inbound": 0, "outbound": 0, "both": 0}
    # markdown + summary are successes with explicit empty state
    for fmt in ("markdown", "summary"):
        resp = server.context_get(anchor, format=fmt)
        result = assert_tool_success(resp)  # SUCCESS, not error
        assert result.text  # non-empty rendered body


# --- AC-04: Supersedes excluded ----------------------------------------------

def test_get_supersedes_absent_display_and_totals(server):
    """AC-04/R-12: a Supersedes edge never appears in surfaced edges or totals."""
    anchor = _store_anchor(server, "supersedes exclusion anchor with separate wording")
    sup = _TARGET_BASE + 90
    keep = _TARGET_BASE + 91
    _seed_target(server, sup, confidence=0.9)
    _seed_target(server, keep, confidence=0.5)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": sup, "relation_type": "Supersedes",
         "source": "agent"},
        {"source_id": anchor, "target_id": keep, "relation_type": "Supports",
         "source": "agent"},
    ])
    entry = _get_json(server, anchor)
    shown = {e["target_id"] for e in entry["edges"]}
    assert sup not in shown, "Supersedes must be excluded from surfaced edges"
    assert keep in shown
    assert entry["edge_totals"]["outbound"] == 1, "Supersedes excluded from totals"


# --- AC-03: authored flag ----------------------------------------------------

def test_get_authored_flag_agent_vs_inferred(server):
    """AC-03/R-09: authored true iff source=='agent'; inferred sources -> false."""
    anchor = _store_anchor(server, "authored flag anchor with distinct sentence content")
    agent_t = _TARGET_BASE + 100
    inf_t = _TARGET_BASE + 101
    _seed_target(server, agent_t, confidence=0.5)
    _seed_target(server, inf_t, confidence=0.5)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": agent_t, "relation_type": "Supports",
         "source": "agent"},
        {"source_id": anchor, "target_id": inf_t, "relation_type": "Prerequisite",
         "source": "co_access"},
    ])
    entry = _get_json(server, anchor)
    by_t = {e["target_id"]: e for e in entry["edges"]}
    assert by_t[agent_t]["authored"] is True
    assert by_t[inf_t]["authored"] is False


# --- AC-02 / DNB-1 / R-15: dangling target retained --------------------------

def test_get_dangling_title_null_retained(server):
    """AC-02/DNB-1/R-15: an edge to a non-existent target_id is retained with
    target_title null (LEFT JOIN), no panic, surfaced as a success."""
    anchor = _store_anchor(server, "dangling target anchor with its own unique words here")
    missing = 99_999_999  # no entries row — intentionally NOT seeded
    _seed_edges(server, [
        {"source_id": anchor, "target_id": missing, "relation_type": "Supports",
         "source": "co_access"},
    ])
    entry = _get_json(server, anchor)
    matching = [e for e in entry["edges"] if e["target_id"] == missing]
    assert len(matching) == 1, "dangling edge must be retained, not dropped"
    assert matching[0]["target_title"] is None, "unresolved target -> null title"


# --- R-04 / AC-12: high-degree node caps at three ----------------------------

def test_get_high_degree_node_caps_at_three(server):
    """R-04/AC-12 (MCP-visible echo): a node with many edges returns <=3 displayed
    edges with honest uncapped totals. The store-boundary 'never materialized'
    proof is a Rust unit test (graph_queries_ranked_tests.rs)."""
    anchor = _store_anchor(server, "high degree hub anchor with a uniquely worded body")
    n = 50
    targets = [_TARGET_BASE + 200 + i for i in range(n)]
    for t in targets:
        _seed_target(server, t, confidence=0.5)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": t, "relation_type": "Supports",
         "source": "co_access"} for t in targets
    ])
    entry = _get_json(server, anchor)
    assert len(entry["edges"]) == 3, "hub get caps the displayed set at 3"
    assert entry["edge_totals"]["outbound"] == n, "totals stay honest/uncapped"


# --- R-07 / AC-07: list-view byte-identity (no edges key) --------------------

def test_list_view_tools_no_edges_key(server):
    """AC-07/R-07: context_search / lookup / store / correct carry NO edges key,
    no edge_totals, no '### Related' — captured via the real MCP response (the
    harness IS the real producer, #1268). Even though the anchor HAS edges,
    list-view tools must not surface them."""
    anchor = _store_anchor(server, "byte identity anchor with edges and unique descriptive body")
    tgt = _TARGET_BASE + 300
    _seed_target(server, tgt, confidence=0.6)
    _seed_edges(server, [
        {"source_id": anchor, "target_id": tgt, "relation_type": "Supports",
         "source": "agent"},
    ])

    # store (json) — fresh store response (its own distinct content)
    store_resp = server.context_store(
        "byte identity fresh store response distinct narrative content",
        "testing", "convention", agent_id="human", format="json",
    )
    store_entry = parse_entry(store_resp)
    assert "edges" not in store_entry and "edge_totals" not in store_entry

    # search (json)
    search_resp = server.context_search("byte identity", format="json")
    search_result = assert_tool_success(search_resp)
    assert '"edges"' not in (search_result.text or ""), "search must not emit edges key"
    assert '"edge_totals"' not in (search_result.text or "")

    # lookup (json)
    lookup_resp = server.context_lookup(topic="testing", format="json")
    lookup_result = assert_tool_success(lookup_resp)
    assert '"edges"' not in (lookup_result.text or ""), "lookup must not emit edges key"

    # correct (json) — produces a new entry; no edges key on the correction payload
    correct_resp = server.context_correct(
        anchor, "corrected byte identity content with its own unique phrasing",
        reason="test", agent_id="human", format="json",
    )
    correct_entry = parse_entry(correct_resp)
    assert "edges" not in correct_entry and "edge_totals" not in correct_entry

    # markdown list views carry no "### Related" section
    search_md = assert_tool_success(server.context_search("byte identity", format="markdown"))
    assert "### Related" not in (search_md.text or "")
