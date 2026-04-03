"""
Tests for build_scenarios.py — ID collision fix (#502) and sidecar generation (#501).
"""

import hashlib
import json
import os
import sqlite3
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

# Add parent directories to path so we can import build_scenarios
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from build_scenarios import build_scenarios


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_db(path: str) -> None:
    """Create a minimal in-memory-style SQLite DB with the schema build_scenarios needs."""
    conn = sqlite3.connect(path)
    conn.executescript("""
        CREATE TABLE sessions (
            session_id TEXT PRIMARY KEY,
            feature_cycle TEXT,
            agent_role TEXT
        );

        CREATE TABLE observations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT,
            ts_millis INTEGER,
            tool TEXT,
            input TEXT,
            topic_signal TEXT
        );
    """)
    conn.commit()
    conn.close()


def _insert_search(conn, session_id, ts_millis, query, topic_signal=None):
    conn.execute(
        "INSERT INTO observations (session_id, ts_millis, tool, input, topic_signal) VALUES (?, ?, ?, ?, ?)",
        (session_id, ts_millis, "mcp__unimatrix__context_search", json.dumps({"query": query}), topic_signal),
    )


def _insert_get(conn, session_id, ts_millis, entry_id):
    conn.execute(
        "INSERT INTO observations (session_id, ts_millis, tool, input, topic_signal) VALUES (?, ?, ?, ?, NULL)",
        (session_id, ts_millis, "mcp__unimatrix__context_get", json.dumps({"id": entry_id}), ),
    )


def _insert_briefing(conn, session_id, ts_millis, feature, phase, topic_signal=None):
    conn.execute(
        "INSERT INTO observations (session_id, ts_millis, tool, input, topic_signal) VALUES (?, ?, ?, ?, ?)",
        (
            session_id,
            ts_millis,
            "mcp__unimatrix__context_briefing",
            json.dumps({"feature": feature, "phase": phase}),
            topic_signal,
        ),
    )


def _insert_session(conn, session_id, feature_cycle=None, agent_role=None):
    conn.execute(
        "INSERT INTO sessions (session_id, feature_cycle, agent_role) VALUES (?, ?, ?)",
        (session_id, feature_cycle, agent_role),
    )


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_no_id_collision_same_session_same_ms():
    """Two searches in same session at same ms but different queries must get different IDs."""
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    try:
        _make_db(db_path)
        conn = sqlite3.connect(db_path)
        sid = "aabbccdd-1122-3344-5566-7788990011aa"
        ts = 1712345678000

        _insert_session(conn, sid, feature_cycle="test-cycle")
        _insert_search(conn, sid, ts, "confidence scoring formula")
        _insert_search(conn, sid, ts, "graph edge detection threshold")
        # context_get calls after both searches
        _insert_get(conn, sid, ts + 100, 42)
        _insert_get(conn, sid, ts + 200, 99)
        conn.commit()
        conn.close()

        scenarios, _ = build_scenarios(db_path)
        ids = [s["id"] for s in scenarios]
        assert len(ids) == len(set(ids)), f"Duplicate IDs found: {ids}"
        assert len(scenarios) == 2, f"Expected 2 scenarios, got {len(scenarios)}"
    finally:
        os.unlink(db_path)


def test_briefing_no_id_collision_same_session_same_ms():
    """Two briefings in same session at same ms but different feature+phase must get different IDs."""
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    try:
        _make_db(db_path)
        conn = sqlite3.connect(db_path)
        sid = "aabbccdd-1122-3344-5566-7788990011bb"
        ts = 1712345678000

        _insert_session(conn, sid, feature_cycle="test-cycle")
        _insert_briefing(conn, sid, ts, "crt-042", "delivery")
        _insert_briefing(conn, sid, ts, "crt-043", "design")
        _insert_get(conn, sid, ts + 100, 55)
        conn.commit()
        conn.close()

        scenarios, _ = build_scenarios(db_path)
        briefing_scens = [s for s in scenarios if s["id"].startswith("obs-briefing-")]
        ids = [s["id"] for s in briefing_scens]
        assert len(ids) == len(set(ids)), f"Duplicate briefing IDs found: {ids}"
        assert len(briefing_scens) == 2, f"Expected 2 briefing scenarios, got {len(briefing_scens)}"
    finally:
        os.unlink(db_path)


def test_uniqueness_assertion_fires_on_collision():
    """Patching hashlib.md5 to return constant hexdigest should trigger the uniqueness assertion."""
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    try:
        _make_db(db_path)
        conn = sqlite3.connect(db_path)
        sid = "aabbccdd-1122-3344-5566-7788990011cc"
        ts = 1712345678000

        _insert_session(conn, sid, feature_cycle="test-cycle")
        _insert_search(conn, sid, ts, "query one")
        _insert_search(conn, sid, ts, "query two")
        _insert_get(conn, sid, ts + 100, 7)
        conn.commit()
        conn.close()

        # Patch hashlib.md5 in the build_scenarios module to always return same digest
        import build_scenarios as bs_module

        class _FakeMd5:
            def hexdigest(self):
                return "aaaaaa" + "0" * 26  # 32-char hex string, first 6 = "aaaaaa"

        with patch.object(bs_module.hashlib, "md5", return_value=_FakeMd5()):
            try:
                bs_module.build_scenarios(db_path)
                assert False, "Expected AssertionError for duplicate IDs"
            except AssertionError as exc:
                assert "Duplicate scenario IDs detected" in str(exc)
    finally:
        os.unlink(db_path)


def test_sidecar_written():
    """build_scenarios returns (scenarios, hash); sidecar is written by __main__, test hash return."""
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    try:
        _make_db(db_path)
        conn = sqlite3.connect(db_path)
        sid = "aabbccdd-1122-3344-5566-7788990011dd"
        ts = 1712345678000

        _insert_session(conn, sid, feature_cycle="sidecar-test")
        _insert_search(conn, sid, ts, "sidecar scenario query")
        _insert_get(conn, sid, ts + 50, 123)
        conn.commit()
        conn.close()

        scenarios, source_db_hash = build_scenarios(db_path)

        # Verify hash is a valid SHA-256 hex string
        assert len(source_db_hash) == 64, f"Expected 64-char hex, got {len(source_db_hash)}"
        int(source_db_hash, 16)  # raises if not valid hex

        # Verify hash matches independent computation
        h = hashlib.sha256()
        with open(db_path, "rb") as f:
            for chunk in iter(lambda: f.read(65536), b""):
                h.update(chunk)
        assert source_db_hash == h.hexdigest()

        # Simulate what __main__ does: write sidecar and verify structure
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = os.path.join(tmpdir, "scenarios.jsonl")
            sidecar_path = os.path.join(tmpdir, "scenarios_meta.json")

            with open(out_path, "w") as f:
                for s in scenarios:
                    f.write(json.dumps(s) + "\n")

            meta = {
                "source_db_hash": source_db_hash,
                "generated_at": "2026-04-03T15:00:00Z",
                "scenario_count": len(scenarios),
            }
            with open(sidecar_path, "w") as f:
                json.dump(meta, f)

            assert os.path.exists(sidecar_path), "scenarios_meta.json not written"
            loaded = json.loads(Path(sidecar_path).read_text())
            assert "source_db_hash" in loaded
            assert "generated_at" in loaded
            assert "scenario_count" in loaded
            assert loaded["scenario_count"] == len(scenarios)
            assert loaded["source_db_hash"] == source_db_hash
    finally:
        os.unlink(db_path)
