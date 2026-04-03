"""
Tests for run_eval.py snapshot pairing validation (#501).
"""

import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

# Add harness directory to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from run_eval import check_snapshot_pairing


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _write_sidecar(directory: str, source_db_hash: str, generated_at: str = "2026-04-03T10:00:00Z") -> str:
    sidecar_path = os.path.join(directory, "scenarios_meta.json")
    meta = {
        "source_db_hash": source_db_hash,
        "generated_at": generated_at,
        "scenario_count": 100,
    }
    with open(sidecar_path, "w") as f:
        json.dump(meta, f)
    return sidecar_path


def _write_snap_db(directory: str, content: bytes = b"fake-snap-db-content") -> str:
    snap_path = os.path.join(directory, "snap.db")
    with open(snap_path, "wb") as f:
        f.write(content)
    return snap_path


def _sha256(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def _write_scenarios(directory: str) -> str:
    scenarios_path = os.path.join(directory, "scenarios.jsonl")
    with open(scenarios_path, "w") as f:
        f.write('{"id":"obs-00000000-1000-aabbcc","query":"test","context":{},"baseline":null,"source":"observations","expected":[1]}\n')
    return scenarios_path


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_mismatch_exits_1():
    """Mismatched hash with no --allow-snapshot-mismatch flag must call sys.exit(1)."""
    with tempfile.TemporaryDirectory() as tmpdir:
        snap_content = b"current-snap-different-content"
        snap_path = _write_snap_db(tmpdir, snap_content)
        scenarios_path = _write_scenarios(tmpdir)

        # Sidecar records a DIFFERENT hash
        wrong_hash = _sha256(b"different-original-db-content")
        _write_sidecar(tmpdir, wrong_hash)

        import pytest
        with pytest.raises(SystemExit) as exc_info:
            check_snapshot_pairing(
                Path(scenarios_path),
                Path(snap_path),
                allow_mismatch=False,
            )
        assert exc_info.value.code == 1


def test_absent_sidecar_is_warning_not_error(capsys):
    """No scenarios_meta.json should print a WARNING and not exit."""
    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = _write_snap_db(tmpdir)
        scenarios_path = _write_scenarios(tmpdir)
        # Do NOT write sidecar

        # Should not raise
        check_snapshot_pairing(
            Path(scenarios_path),
            Path(snap_path),
            allow_mismatch=False,
        )

        captured = capsys.readouterr()
        assert "WARNING" in captured.out
        assert "backward compat" in captured.out


def test_allow_snapshot_mismatch_flag_suppresses_error(capsys):
    """Mismatch + --allow-snapshot-mismatch should print WARNING but NOT call sys.exit."""
    with tempfile.TemporaryDirectory() as tmpdir:
        snap_content = b"current-snap"
        snap_path = _write_snap_db(tmpdir, snap_content)
        scenarios_path = _write_scenarios(tmpdir)

        # Sidecar records a different hash
        wrong_hash = _sha256(b"different-original-db")
        _write_sidecar(tmpdir, wrong_hash)

        # Should not raise even though hashes differ
        check_snapshot_pairing(
            Path(scenarios_path),
            Path(snap_path),
            allow_mismatch=True,
        )

        captured = capsys.readouterr()
        assert "WARNING" in captured.out
        assert "mismatch" in captured.out.lower()


def test_matching_hash_is_silent(capsys):
    """When hashes match, check_snapshot_pairing should produce no output."""
    with tempfile.TemporaryDirectory() as tmpdir:
        snap_content = b"matching-db-content"
        snap_path = _write_snap_db(tmpdir, snap_content)
        scenarios_path = _write_scenarios(tmpdir)

        correct_hash = _sha256(snap_content)
        _write_sidecar(tmpdir, correct_hash)

        check_snapshot_pairing(
            Path(scenarios_path),
            Path(snap_path),
            allow_mismatch=False,
        )

        captured = capsys.readouterr()
        assert captured.out == ""
