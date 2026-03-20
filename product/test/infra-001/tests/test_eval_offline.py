"""D1–D4 offline test suite for the eval harness CLI subcommands.

AC coverage:
  AC-01  snapshot creates valid SQLite with all expected tables
  AC-02  snapshot refuses --out path that resolves to the active daemon DB
  AC-04  eval scenarios --source mcp|uds|all filters correctly
  AC-05  SHA-256 of snapshot file is unchanged after eval run
  AC-06  eval run result JSON contains all required fields
  AC-08  eval report Markdown contains all five section headers
  AC-15  --help text lists snapshot, scenarios, run, report subcommands
  AC-16  eval run refuses --db path that resolves to the active daemon DB

Risk coverage:
  R-01  SHA-256 subprocess verification (AC-05)
  R-06  snapshot --out path guard (AC-02)
  R-07  offline/live test separation — this file passes without a daemon
  R-11  block_export_sync bridge exercised by subprocess invocations
  R-17  section headers present in report output (AC-08)

All tests are subprocess-only: no running daemon required.
"""

import hashlib
import json
import os
import sqlite3
import subprocess
import sys
from pathlib import Path

import pytest


# ===========================================================================
# Binary resolution
# ===========================================================================

_WORKSPACE_ROOT = Path(__file__).resolve().parent.parent.parent.parent.parent


def _binary_has_snapshot(path: Path) -> bool:
    """Return True if the binary at `path` supports the snapshot subcommand."""
    try:
        r = subprocess.run(
            [str(path), "--help"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        return "snapshot" in r.stdout
    except Exception:
        return False


def _find_binary() -> Path:
    """Resolve the unimatrix binary for offline eval tests.

    Prefers binaries that have the `snapshot` subcommand (nan-007). Checks
    debug before release because the debug binary is more likely to reflect
    the current branch build. UNIMATRIX_BINARY is intentionally not used
    here — the installed binary on the PATH may predate nan-007.
    """
    candidates = [
        _WORKSPACE_ROOT / "target" / "debug" / "unimatrix",
        _WORKSPACE_ROOT / "target" / "release" / "unimatrix",
    ]
    # First pass: prefer any binary that has snapshot subcommand
    for c in candidates:
        if c.is_file() and _binary_has_snapshot(c):
            return c
    # Second pass: fall back to any existing binary (tests will fail descriptively)
    for c in candidates:
        if c.is_file():
            return c
    raise RuntimeError(
        "Cannot find unimatrix binary with snapshot support in workspace target/. "
        "Run: cargo build --workspace"
    )


@pytest.fixture(scope="session")
def unimatrix_bin() -> str:
    """Session-scoped resolved path to the unimatrix binary."""
    return str(_find_binary())


# ===========================================================================
# Fixture: initialised project directory with a real database
#
# Calls `unimatrix --project-dir <tmpdir> version` which runs Store::open()
# (applying all migrations) and writes the DB to
#   ~/.unimatrix/{sha256(tmpdir)[:16]}/unimatrix.db
# Returns a tuple: (tmpdir, db_path, hash_hex)
# ===========================================================================


def _compute_project_hash(path: Path) -> str:
    """Replicate Rust compute_project_hash for a given directory."""
    h = hashlib.sha256(str(path).encode("utf-8")).hexdigest()
    return h[:16]


@pytest.fixture()
def init_project(tmp_path, unimatrix_bin):
    """Initialise a project dir and return (project_dir, live_db_path).

    The unimatrix binary's `version` subcommand runs Store::open() which
    applies all migrations and creates the database at the expected path.
    """
    project_dir = tmp_path / "project"
    project_dir.mkdir()

    result = subprocess.run(
        [unimatrix_bin, "--project-dir", str(project_dir), "version"],
        capture_output=True,
        text=True,
        cwd=str(_WORKSPACE_ROOT),
    )
    assert result.returncode == 0, f"version failed: {result.stderr}"

    project_hash = _compute_project_hash(project_dir)
    home = Path.home()
    db_path = home / ".unimatrix" / project_hash / "unimatrix.db"
    assert db_path.exists(), f"DB not created at expected path: {db_path}"

    return project_dir, db_path


@pytest.fixture()
def snapshot_db(init_project, tmp_path, unimatrix_bin):
    """Create a WAL-mode snapshot of an initialised project DB.

    Also inserts one session + one query_log row so that `eval scenarios`
    can extract a non-empty JSONL file.

    Returns the Path to the snapshot file.
    """
    project_dir, live_db = init_project

    # Add a session and one query_log row so scenarios extraction is non-empty
    conn = sqlite3.connect(str(live_db))
    conn.execute(
        "INSERT INTO sessions (session_id, feature_cycle, agent_role, started_at, ended_at, status)"
        " VALUES (?, ?, ?, ?, ?, ?)",
        ("sess-offline-001", "nan-007", "tester", 1_000_000, 1_000_001, 1),
    )
    conn.execute(
        "INSERT INTO query_log (session_id, query_text, ts, result_count, source)"
        " VALUES (?, ?, ?, ?, ?)",
        ("sess-offline-001", "offline eval test query unique-xyz", 1_000_000, 0, "mcp"),
    )
    conn.commit()
    conn.close()

    snap_out = tmp_path / "test_snap.db"
    result = subprocess.run(
        [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(snap_out)],
        capture_output=True,
        text=True,
        cwd=str(_WORKSPACE_ROOT),
    )
    assert result.returncode == 0, f"snapshot failed: {result.stderr}"
    assert snap_out.exists(), "snapshot file not created"

    # Apply WAL journal mode so SqlxStore::open_readonly() can set WAL without
    # hitting SQLITE_READONLY (open_readonly uses build_connect_options which
    # issues PRAGMA journal_mode=WAL; applying it here pre-empts that write).
    conn2 = sqlite3.connect(str(snap_out))
    conn2.execute("PRAGMA journal_mode = WAL")
    conn2.commit()
    conn2.close()

    return snap_out


@pytest.fixture()
def snapshot_with_both_sources(init_project, tmp_path, unimatrix_bin):
    """Snapshot with both mcp and uds source rows in query_log.

    Returns the Path to the snapshot file.
    """
    project_dir, live_db = init_project

    conn = sqlite3.connect(str(live_db))
    conn.execute(
        "INSERT INTO sessions (session_id, feature_cycle, agent_role, started_at, ended_at, status)"
        " VALUES (?, ?, ?, ?, ?, ?)",
        ("sess-src-001", "nan-007", "tester", 2_000_000, 2_000_001, 1),
    )
    conn.execute(
        "INSERT INTO query_log (session_id, query_text, ts, result_count, source)"
        " VALUES (?, ?, ?, ?, ?)",
        ("sess-src-001", "mcp source query", 2_000_000, 0, "mcp"),
    )
    conn.execute(
        "INSERT INTO query_log (session_id, query_text, ts, result_count, source)"
        " VALUES (?, ?, ?, ?, ?)",
        ("sess-src-001", "uds source query", 2_000_001, 0, "uds"),
    )
    conn.commit()
    conn.close()

    snap_out = tmp_path / "snap_two_sources.db"
    result = subprocess.run(
        [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(snap_out)],
        capture_output=True,
        text=True,
        cwd=str(_WORKSPACE_ROOT),
    )
    assert result.returncode == 0, f"snapshot failed: {result.stderr}"
    assert snap_out.exists()

    conn2 = sqlite3.connect(str(snap_out))
    conn2.execute("PRAGMA journal_mode = WAL")
    conn2.commit()
    conn2.close()

    return snap_out


@pytest.fixture()
def baseline_profile_toml(tmp_path) -> Path:
    """Write a minimal baseline eval profile TOML and return its path."""
    toml = tmp_path / "baseline.toml"
    toml.write_text("[profile]\nname = \"baseline\"\n")
    return toml


@pytest.fixture()
def eval_run_results(snapshot_db, baseline_profile_toml, tmp_path, unimatrix_bin):
    """Run a full eval pipeline (scenarios → run → results dir).

    Returns (scenarios_path, results_dir) so individual tests can inspect them.
    """
    scenarios_path = tmp_path / "scenarios.jsonl"
    result = subprocess.run(
        [unimatrix_bin, "eval", "scenarios", "--db", str(snapshot_db), "--out", str(scenarios_path)],
        capture_output=True,
        text=True,
        cwd=str(_WORKSPACE_ROOT),
    )
    assert result.returncode == 0, f"eval scenarios failed: {result.stderr}"

    results_dir = tmp_path / "eval_results"
    result2 = subprocess.run(
        [
            unimatrix_bin, "eval", "run",
            "--db", str(snapshot_db),
            "--scenarios", str(scenarios_path),
            "--configs", str(baseline_profile_toml),
            "--out", str(results_dir),
            "--k", "5",
        ],
        capture_output=True,
        text=True,
        cwd=str(_WORKSPACE_ROOT),
    )
    assert result2.returncode == 0, f"eval run failed: {result2.stderr}"

    return scenarios_path, results_dir


# ===========================================================================
# Helpers
# ===========================================================================


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


_EXPECTED_TABLES = frozenset({
    "entries",
    "entry_tags",
    "query_log",
    "sessions",
    "co_access",
    "agent_registry",
    "audit_log",
    "vector_map",
    "counters",
    "feature_entries",
    "outcome_index",
    "shadow_evaluations",
    "graph_edges",
    "injection_log",
    "signal_queue",
    "observations",
    "topic_deliveries",
})

_REPORT_SECTION_HEADERS = [
    "## 1. Summary",
    "## 2. Notable Ranking Changes",
    "## 3. Latency Distribution",
    "## 4. Entry-Level Analysis",
    "## 5. Zero-Regression Check",
]


# ===========================================================================
# AC-15: Help text visibility
# ===========================================================================


class TestHelpVisibility:
    """AC-15: subcommands are visible in --help output without a daemon."""

    def test_snapshot_in_top_level_help(self, unimatrix_bin):
        """unimatrix --help lists the snapshot subcommand."""
        result = subprocess.run(
            [unimatrix_bin, "--help"],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"--help failed: {result.stderr}"
        assert "snapshot" in result.stdout, (
            f"'snapshot' not found in --help output:\n{result.stdout}"
        )

    def test_eval_in_top_level_help(self, unimatrix_bin):
        """unimatrix --help lists the eval subcommand."""
        result = subprocess.run(
            [unimatrix_bin, "--help"],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0
        assert "eval" in result.stdout

    def test_eval_scenarios_in_eval_help(self, unimatrix_bin):
        """unimatrix eval --help lists the scenarios subcommand."""
        result = subprocess.run(
            [unimatrix_bin, "eval", "--help"],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval --help failed: {result.stderr}"
        assert "scenarios" in result.stdout

    def test_eval_run_in_eval_help(self, unimatrix_bin):
        """unimatrix eval --help lists the run subcommand."""
        result = subprocess.run(
            [unimatrix_bin, "eval", "--help"],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0
        assert "run" in result.stdout

    def test_eval_report_in_eval_help(self, unimatrix_bin):
        """unimatrix eval --help lists the report subcommand."""
        result = subprocess.run(
            [unimatrix_bin, "eval", "--help"],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0
        assert "report" in result.stdout

    def test_all_eval_subcommands_in_eval_help(self, unimatrix_bin):
        """unimatrix eval --help lists scenarios, run, and report in one invocation."""
        result = subprocess.run(
            [unimatrix_bin, "eval", "--help"],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0
        for cmd in ("scenarios", "run", "report"):
            assert cmd in result.stdout, (
                f"'{cmd}' not found in eval --help output:\n{result.stdout}"
            )


# ===========================================================================
# AC-01: Snapshot creates valid SQLite
# ===========================================================================


class TestSnapshotCreatesValidSqlite:
    """AC-01: snapshot --out produces a readable SQLite file with all tables."""

    def test_snapshot_creates_file(self, init_project, tmp_path, unimatrix_bin):
        """snapshot --out writes a file to the given path."""
        project_dir, _ = init_project
        out = tmp_path / "snap.db"
        result = subprocess.run(
            [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(out)],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"snapshot failed: {result.stderr}"
        assert out.exists(), "snapshot output file not created"

    def test_snapshot_output_is_valid_sqlite(self, init_project, tmp_path, unimatrix_bin):
        """Snapshot file is a readable SQLite database."""
        project_dir, _ = init_project
        out = tmp_path / "snap_valid.db"
        subprocess.run(
            [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(out)],
            capture_output=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert out.exists()
        # Must open without errors
        conn = sqlite3.connect(str(out))
        tables_rows = list(conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        ))
        conn.close()
        assert tables_rows, "snapshot has no tables"

    def test_snapshot_contains_expected_tables(self, init_project, tmp_path, unimatrix_bin):
        """Snapshot SQLite file contains all expected table names (AC-01)."""
        project_dir, _ = init_project
        out = tmp_path / "snap_tables.db"
        result = subprocess.run(
            [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(out)],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"snapshot failed: {result.stderr}"
        assert out.exists()

        conn = sqlite3.connect(str(out))
        actual_tables = {
            row[0]
            for row in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            )
        }
        conn.close()

        missing = _EXPECTED_TABLES - actual_tables
        assert not missing, (
            f"Snapshot is missing expected tables: {sorted(missing)}\n"
            f"Actual tables: {sorted(actual_tables)}"
        )

    def test_snapshot_exit_code_zero_on_success(self, init_project, tmp_path, unimatrix_bin):
        """snapshot exits 0 on successful completion."""
        project_dir, _ = init_project
        out = tmp_path / "snap_rc.db"
        result = subprocess.run(
            [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(out)],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0


# ===========================================================================
# AC-02: Snapshot refuses live-DB path
# ===========================================================================


class TestSnapshotRefusesLiveDb:
    """AC-02: snapshot --out rejects paths that resolve to the active daemon DB."""

    def test_snapshot_refuses_direct_live_db_path(self, init_project, unimatrix_bin):
        """snapshot --out <live-db> exits non-zero (AC-02)."""
        project_dir, live_db = init_project
        result = subprocess.run(
            [
                unimatrix_bin,
                "--project-dir", str(project_dir),
                "snapshot",
                "--out", str(live_db),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0, (
            "Expected snapshot to reject --out=<live-db> but got exit code 0"
        )

    def test_snapshot_refuses_live_db_error_message(self, init_project, unimatrix_bin):
        """snapshot prints a descriptive error when --out resolves to active DB."""
        project_dir, live_db = init_project
        result = subprocess.run(
            [
                unimatrix_bin,
                "--project-dir", str(project_dir),
                "snapshot",
                "--out", str(live_db),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0
        # stderr should name both paths
        combined = result.stderr + result.stdout
        assert "active" in combined.lower() or "live" in combined.lower() or "resolves" in combined.lower(), (
            f"Expected error message to describe path conflict; got:\n{combined}"
        )

    @pytest.mark.skipif(sys.platform == "win32", reason="symlinks require Unix")
    def test_snapshot_refuses_symlink_to_live_db(self, init_project, tmp_path, unimatrix_bin):
        """snapshot rejects a symlink --out path that resolves to the active DB (R-06)."""
        project_dir, live_db = init_project
        symlink_path = tmp_path / "link_to_live.db"
        symlink_path.symlink_to(live_db)

        result = subprocess.run(
            [
                unimatrix_bin,
                "--project-dir", str(project_dir),
                "snapshot",
                "--out", str(symlink_path),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0, (
            "Expected snapshot to reject symlink pointing to live DB"
        )

    def test_snapshot_refuses_missing_parent_dir(self, init_project, tmp_path, unimatrix_bin):
        """snapshot --out with missing parent directory exits non-zero."""
        project_dir, _ = init_project
        out = tmp_path / "nonexistent_parent" / "snap.db"
        result = subprocess.run(
            [
                unimatrix_bin,
                "--project-dir", str(project_dir),
                "snapshot",
                "--out", str(out),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0, "Expected error for missing parent directory"
        assert not out.exists(), "No partial output file should be created"


# ===========================================================================
# AC-04: eval scenarios --source filter
# ===========================================================================


class TestEvalScenariosSourceFilter:
    """AC-04: eval scenarios --source mcp|uds|all returns correctly filtered records."""

    def test_source_mcp_returns_only_mcp_records(
        self, snapshot_with_both_sources, tmp_path, unimatrix_bin
    ):
        """--source mcp returns only scenarios with source='mcp'."""
        out = tmp_path / "scenarios_mcp.jsonl"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "scenarios",
                "--db", str(snapshot_with_both_sources),
                "--source", "mcp",
                "--out", str(out),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval scenarios --source mcp failed: {result.stderr}"
        assert out.exists()

        lines = [l.strip() for l in out.read_text().splitlines() if l.strip()]
        assert len(lines) >= 1, "Expected at least one mcp scenario"
        for line in lines:
            obj = json.loads(line)
            assert obj["source"] == "mcp", (
                f"Expected source='mcp', got source='{obj['source']}'"
            )

    def test_source_uds_returns_only_uds_records(
        self, snapshot_with_both_sources, tmp_path, unimatrix_bin
    ):
        """--source uds returns only scenarios with source='uds'."""
        out = tmp_path / "scenarios_uds.jsonl"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "scenarios",
                "--db", str(snapshot_with_both_sources),
                "--source", "uds",
                "--out", str(out),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval scenarios --source uds failed: {result.stderr}"
        lines = [l.strip() for l in out.read_text().splitlines() if l.strip()]
        assert len(lines) >= 1, "Expected at least one uds scenario"
        for line in lines:
            obj = json.loads(line)
            assert obj["source"] == "uds", (
                f"Expected source='uds', got source='{obj['source']}'"
            )

    def test_source_all_returns_both_sources(
        self, snapshot_with_both_sources, tmp_path, unimatrix_bin
    ):
        """--source all (default) returns scenarios from both mcp and uds."""
        out = tmp_path / "scenarios_all.jsonl"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "scenarios",
                "--db", str(snapshot_with_both_sources),
                "--source", "all",
                "--out", str(out),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0
        lines = [l.strip() for l in out.read_text().splitlines() if l.strip()]
        sources = {json.loads(l)["source"] for l in lines}
        assert "mcp" in sources, "Expected mcp source in --source all output"
        assert "uds" in sources, "Expected uds source in --source all output"

    def test_empty_snapshot_produces_empty_jsonl(self, init_project, tmp_path, unimatrix_bin):
        """eval scenarios on a DB with no query_log rows produces an empty JSONL file."""
        project_dir, _ = init_project
        snap = tmp_path / "empty_snap.db"
        subprocess.run(
            [unimatrix_bin, "--project-dir", str(project_dir), "snapshot", "--out", str(snap)],
            capture_output=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        # Apply WAL mode
        conn = sqlite3.connect(str(snap))
        conn.execute("PRAGMA journal_mode = WAL")
        conn.close()

        out = tmp_path / "empty_scenarios.jsonl"
        result = subprocess.run(
            [unimatrix_bin, "eval", "scenarios", "--db", str(snap), "--out", str(out)],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval scenarios failed on empty snapshot: {result.stderr}"
        assert out.exists(), "scenarios output file should be created even when empty"
        lines = [l.strip() for l in out.read_text().splitlines() if l.strip()]
        assert lines == [], f"Expected empty JSONL for empty query_log, got {len(lines)} lines"


# ===========================================================================
# AC-05: eval run does not modify snapshot (SHA-256 integrity)
# ===========================================================================


class TestEvalRunReadOnly:
    """AC-05: eval run --db <snapshot> does not write to the snapshot file."""

    def test_sha256_unchanged_after_eval_run(
        self, snapshot_db, baseline_profile_toml, tmp_path, unimatrix_bin
    ):
        """SHA-256 of snapshot file is byte-for-byte identical before and after eval run (AC-05)."""
        scenarios_path = tmp_path / "scenarios_sha256.jsonl"
        subprocess.run(
            [unimatrix_bin, "eval", "scenarios", "--db", str(snapshot_db), "--out", str(scenarios_path)],
            capture_output=True,
            cwd=str(_WORKSPACE_ROOT),
        )

        before_hash = _sha256_file(snapshot_db)

        results_dir = tmp_path / "results_sha256"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "run",
                "--db", str(snapshot_db),
                "--scenarios", str(scenarios_path),
                "--configs", str(baseline_profile_toml),
                "--out", str(results_dir),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval run failed: {result.stderr}"

        after_hash = _sha256_file(snapshot_db)
        assert before_hash == after_hash, (
            "Snapshot file was modified by eval run — AC-05 violation.\n"
            f"  before: {before_hash}\n"
            f"  after:  {after_hash}"
        )

    def test_eval_run_empty_scenarios_snapshot_unchanged(
        self, snapshot_db, baseline_profile_toml, tmp_path, unimatrix_bin
    ):
        """eval run with no scenarios leaves snapshot unchanged."""
        empty_scenarios = tmp_path / "empty.jsonl"
        empty_scenarios.write_text("")

        before_hash = _sha256_file(snapshot_db)

        results_dir = tmp_path / "results_empty"
        subprocess.run(
            [
                unimatrix_bin, "eval", "run",
                "--db", str(snapshot_db),
                "--scenarios", str(empty_scenarios),
                "--configs", str(baseline_profile_toml),
                "--out", str(results_dir),
            ],
            capture_output=True,
            cwd=str(_WORKSPACE_ROOT),
        )

        after_hash = _sha256_file(snapshot_db)
        assert before_hash == after_hash, (
            "Snapshot modified by eval run with empty scenarios"
        )


# ===========================================================================
# AC-06: eval run result JSON contains required fields
# ===========================================================================


class TestEvalRunResultJson:
    """AC-06: per-scenario JSON result files contain all required fields."""

    def test_result_json_top_level_fields(self, eval_run_results, tmp_path):
        """Each result JSON has scenario_id, query, profiles, comparison fields."""
        _, results_dir = eval_run_results
        result_files = list(results_dir.glob("*.json"))
        assert result_files, "No result JSON files produced by eval run"

        for rf in result_files:
            obj = json.loads(rf.read_text())
            for field in ("scenario_id", "query", "profiles", "comparison"):
                assert field in obj, (
                    f"Missing field '{field}' in result file {rf.name}: {list(obj.keys())}"
                )

    def test_result_json_comparison_fields(self, eval_run_results, tmp_path):
        """comparison object contains kendall_tau, mrr_delta, p_at_k_delta, latency_overhead_ms."""
        _, results_dir = eval_run_results
        result_files = list(results_dir.glob("*.json"))
        assert result_files

        for rf in result_files:
            obj = json.loads(rf.read_text())
            comparison = obj["comparison"]
            for field in ("kendall_tau", "mrr_delta", "p_at_k_delta", "latency_overhead_ms"):
                assert field in comparison, (
                    f"Missing comparison field '{field}' in {rf.name}: {list(comparison.keys())}"
                )

    def test_result_json_profiles_numeric_metrics(self, eval_run_results, tmp_path):
        """Each profile entry contains numeric p_at_k, mrr, latency_ms."""
        _, results_dir = eval_run_results
        result_files = list(results_dir.glob("*.json"))
        assert result_files

        for rf in result_files:
            obj = json.loads(rf.read_text())
            for profile_name, profile_result in obj["profiles"].items():
                for field in ("p_at_k", "mrr", "latency_ms"):
                    assert field in profile_result, (
                        f"Profile '{profile_name}' missing field '{field}' in {rf.name}"
                    )
                    assert isinstance(profile_result[field], (int, float)), (
                        f"Profile '{profile_name}'.{field} is not numeric in {rf.name}"
                    )

    def test_result_json_comparison_values_are_numeric(self, eval_run_results, tmp_path):
        """comparison numeric fields are int or float."""
        _, results_dir = eval_run_results
        result_files = list(results_dir.glob("*.json"))
        assert result_files

        for rf in result_files:
            obj = json.loads(rf.read_text())
            c = obj["comparison"]
            for field in ("kendall_tau", "mrr_delta", "p_at_k_delta", "latency_overhead_ms"):
                assert isinstance(c[field], (int, float)), (
                    f"comparison.{field} is not numeric in {rf.name}: got {type(c[field])}"
                )


# ===========================================================================
# AC-08: eval report contains five section headers
# ===========================================================================


class TestEvalReportSections:
    """AC-08: eval report Markdown output contains all five required sections."""

    def test_report_contains_all_five_sections(self, eval_run_results, tmp_path, unimatrix_bin):
        """eval report produces a Markdown file with all five section headers (AC-08)."""
        _, results_dir = eval_run_results
        out_md = tmp_path / "report.md"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "report",
                "--results", str(results_dir),
                "--out", str(out_md),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval report failed: {result.stderr}"
        assert out_md.exists(), "Report Markdown file not created"

        content = out_md.read_text()
        missing = [h for h in _REPORT_SECTION_HEADERS if h not in content]
        assert not missing, (
            f"Report missing section headers: {missing}\n"
            f"Report content (first 1000 chars):\n{content[:1000]}"
        )

    def test_report_from_minimal_result_json(self, tmp_path, unimatrix_bin):
        """eval report on a hand-crafted results dir produces all five sections."""
        results_dir = tmp_path / "minimal_results"
        results_dir.mkdir()

        # Construct a minimal valid result JSON matching ScenarioResult shape
        minimal_result = {
            "scenario_id": "test-001",
            "query": "minimal test query",
            "profiles": {
                "baseline": {
                    "entries": [],
                    "latency_ms": 10,
                    "p_at_k": 0.5,
                    "mrr": 0.5,
                }
            },
            "comparison": {
                "kendall_tau": 1.0,
                "rank_changes": [],
                "mrr_delta": 0.0,
                "p_at_k_delta": 0.0,
                "latency_overhead_ms": 0,
            },
        }
        (results_dir / "test-001.json").write_text(json.dumps(minimal_result))

        out_md = tmp_path / "minimal_report.md"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "report",
                "--results", str(results_dir),
                "--out", str(out_md),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval report failed: {result.stderr}"
        content = out_md.read_text()

        for section in _REPORT_SECTION_HEADERS:
            assert section in content, (
                f"Section '{section}' missing from report.\n"
                f"Report (first 1000 chars):\n{content[:1000]}"
            )

    def test_report_empty_results_dir_exits_zero(self, tmp_path, unimatrix_bin):
        """eval report on an empty results dir exits 0 with section headers present."""
        results_dir = tmp_path / "empty_results"
        results_dir.mkdir()

        out_md = tmp_path / "empty_report.md"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "report",
                "--results", str(results_dir),
                "--out", str(out_md),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, f"eval report failed on empty dir: {result.stderr}"
        assert out_md.exists()
        content = out_md.read_text()
        for section in _REPORT_SECTION_HEADERS:
            assert section in content, f"Section '{section}' missing from empty-dir report"

    def test_report_exit_code_is_always_zero(self, tmp_path, unimatrix_bin):
        """eval report always exits 0 regardless of result content (C-07, FR-29)."""
        results_dir = tmp_path / "results_always_zero"
        results_dir.mkdir()
        # Put a result with a regression to ensure the report still exits 0
        regression_result = {
            "scenario_id": "regress-001",
            "query": "regression query",
            "profiles": {
                "baseline": {"entries": [], "latency_ms": 5, "p_at_k": 1.0, "mrr": 1.0},
                "candidate": {"entries": [], "latency_ms": 50, "p_at_k": 0.5, "mrr": 0.5},
            },
            "comparison": {
                "kendall_tau": 0.5,
                "rank_changes": [],
                "mrr_delta": -0.5,
                "p_at_k_delta": -0.5,
                "latency_overhead_ms": 45,
            },
        }
        (results_dir / "regress-001.json").write_text(json.dumps(regression_result))

        out_md = tmp_path / "regress_report.md"
        result = subprocess.run(
            [
                unimatrix_bin, "eval", "report",
                "--results", str(results_dir),
                "--out", str(out_md),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode == 0, (
            "eval report must always exit 0 (C-07/FR-29). "
            f"Got {result.returncode}. stderr: {result.stderr}"
        )


# ===========================================================================
# AC-16: eval run refuses active daemon DB
# ===========================================================================


class TestEvalRunRefusesLiveDb:
    """AC-16: eval run --db <active-db> exits non-zero with a descriptive error."""

    def _get_active_db_for_cwd(self) -> Path:
        """Return the DB path for the workspace root (run from /workspaces/unimatrix)."""
        project_hash = _compute_project_hash(_WORKSPACE_ROOT)
        home = Path.home()
        return home / ".unimatrix" / project_hash / "unimatrix.db"

    def test_eval_run_refuses_active_db_exit_nonzero(self, tmp_path, unimatrix_bin, baseline_profile_toml):
        """eval run --db <active-daemon-db> exits non-zero (AC-16)."""
        active_db = self._get_active_db_for_cwd()
        if not active_db.exists():
            pytest.skip(f"Active daemon DB not found at {active_db} — cannot test AC-16")

        empty_scenarios = tmp_path / "scenarios_livedb.jsonl"
        empty_scenarios.write_text("")

        result = subprocess.run(
            [
                unimatrix_bin, "eval", "run",
                "--db", str(active_db),
                "--scenarios", str(empty_scenarios),
                "--configs", str(baseline_profile_toml),
                "--out", str(tmp_path / "results_livedb"),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0, (
            "Expected eval run to refuse active daemon DB but got exit code 0"
        )

    def test_eval_run_refuses_active_db_error_message(self, tmp_path, unimatrix_bin, baseline_profile_toml):
        """eval run --db <active-db> prints a descriptive error message."""
        active_db = self._get_active_db_for_cwd()
        if not active_db.exists():
            pytest.skip(f"Active daemon DB not found at {active_db}")

        empty_scenarios = tmp_path / "scenarios_livedb_msg.jsonl"
        empty_scenarios.write_text("")

        result = subprocess.run(
            [
                unimatrix_bin, "eval", "run",
                "--db", str(active_db),
                "--scenarios", str(empty_scenarios),
                "--configs", str(baseline_profile_toml),
                "--out", str(tmp_path / "results_livedb_msg"),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0
        combined = result.stderr + result.stdout
        # Error should mention the path or "live" / "active"
        assert any(
            keyword in combined
            for keyword in ("live", "active", "LiveDb", str(active_db))
        ), (
            f"Expected error to name the live DB path or describe it as live/active.\n"
            f"Combined output:\n{combined}"
        )

    @pytest.mark.skipif(sys.platform == "win32", reason="symlinks require Unix")
    def test_eval_run_refuses_symlink_to_active_db(self, tmp_path, unimatrix_bin, baseline_profile_toml):
        """eval run rejects a --db symlink that resolves to the active daemon DB."""
        active_db = self._get_active_db_for_cwd()
        if not active_db.exists():
            pytest.skip(f"Active daemon DB not found at {active_db}")

        symlink = tmp_path / "link_to_active.db"
        symlink.symlink_to(active_db)

        empty_scenarios = tmp_path / "scenarios_sym.jsonl"
        empty_scenarios.write_text("")

        result = subprocess.run(
            [
                unimatrix_bin, "eval", "run",
                "--db", str(symlink),
                "--scenarios", str(empty_scenarios),
                "--configs", str(baseline_profile_toml),
                "--out", str(tmp_path / "results_sym"),
            ],
            capture_output=True,
            text=True,
            cwd=str(_WORKSPACE_ROOT),
        )
        assert result.returncode != 0, (
            "Expected eval run to reject symlink to active DB"
        )
