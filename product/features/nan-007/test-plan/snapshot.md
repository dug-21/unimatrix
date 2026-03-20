# Test Plan: `snapshot.rs` (D1)

**Component**: `crates/unimatrix-server/src/snapshot.rs`
**Function under test**: `run_snapshot(project_dir: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>`
**AC coverage**: AC-01, AC-02, AC-15
**Risk coverage**: R-01 (analytics boundary), R-02 (SqlxStore guard), R-06 (path canonicalization)

---

## Unit Tests

Location: `crates/unimatrix-server/src/snapshot.rs` (inline `#[cfg(test)]` block)

### Test: `test_snapshot_no_sqlx_store_open_in_snapshot`

**Purpose**: Structural — ensure `SqlxStore::open()` is never called in `snapshot.rs`.
**Arrange**: No runtime needed.
**Act**: Grep / code review assertion embedded as a compile-time lint or doc comment.
**Assert**: The function body of `run_snapshot` contains no call to `SqlxStore::open`. Use a `#[deny(dead_code)]`-style comment or a doc assertion that references the rule.
**Risk**: R-02

### Test: `test_snapshot_path_guard_same_path`

**Purpose**: Verify `run_snapshot` returns an error when `out` canonicalizes to the same path as the active DB.
**Arrange**: Create a temp directory with a mock SQLite file. Create a `ProjectPaths` resolving to that file. Pass the same path as `out`.
**Act**: Call `run_snapshot(Some(&project_dir), &same_path)`.
**Assert**: Returns `Err(...)`. Error message contains both resolved paths.
**Risk**: R-06 (AC-02)

### Test: `test_snapshot_path_guard_symlink`

**Purpose**: Verify symlinks to the active DB are resolved and rejected.
**Arrange**: Create temp directory with a source SQLite file. Create a symlink pointing to it. Pass the symlink path as `out`.
**Act**: Call `run_snapshot(Some(&project_dir), &symlink_path)`.
**Assert**: Returns `Err(...)`. Error message names the resolved path (not the symlink path). Exit with non-zero code when dispatched from main.
**Risk**: R-06 (AC-02)

### Test: `test_snapshot_path_guard_relative_path`

**Purpose**: Verify a relative path that resolves to the active DB is rejected via canonicalize.
**Arrange**: `cd` to parent dir; supply `./subdir/db.sqlite` that resolves to the active DB path.
**Act**: Call `run_snapshot`.
**Assert**: Returns `Err(...)` with descriptive message.
**Risk**: R-06

### Test: `test_snapshot_parent_dir_missing`

**Purpose**: Verify that a missing parent directory causes a descriptive error before VACUUM INTO executes.
**Arrange**: Supply `out = "/nonexistent/parent/snapshot.db"`.
**Act**: Call `run_snapshot(None, &path)`.
**Assert**: Returns `Err(...)` with message mentioning the path. No partial file is created.
**Risk**: Edge case (failure mode table in RISK-TEST-STRATEGY.md)

### Test: `test_snapshot_canonicalize_fails_on_source`

**Purpose**: If `canonicalize` fails on the source DB path (e.g., source file not found), the command fails descriptively rather than proceeding.
**Arrange**: Supply a project directory that doesn't contain an initialized DB.
**Act**: Call `run_snapshot(Some(&invalid_project_dir), &out)`.
**Assert**: Returns `Err(...)` with descriptive message. Does not create output file.
**Risk**: R-06 (NFR-06)

---

## Integration Tests (Python Subprocess)

Location: `product/test/infra-001/tests/test_eval_offline.py`

### Test: `test_snapshot_creates_valid_sqlite`

**Purpose**: AC-01 — snapshot creates a valid SQLite file containing all schema tables.
**Arrange**: Build `unimatrix-server` binary. Use a pre-seeded test database (via `server` fixture or pre-built fixture file).
**Act**: Invoke `unimatrix snapshot --project-dir <dir> --out <out.db>` as subprocess.
**Assert**:
- Exit code 0.
- `out.db` exists.
- `sqlite3 out.db "SELECT name FROM sqlite_master WHERE type='table'"` returns all expected table names: `entries`, `query_log`, `graph_edges`, `co_access`, `sessions`, `shadow_evaluations`, `entry_tags`, `feature_entries`, `outcome_index`, `agent_registry`, `audit_log`, `counters`.
**Risk**: AC-01

### Test: `test_snapshot_refuses_live_db_path`

**Purpose**: AC-02 — refuses with non-zero exit and descriptive error when `--out` equals the live DB path.
**Arrange**: Start a daemon in a temp project dir. Locate the active DB path.
**Act**: `unimatrix snapshot --project-dir <dir> --out <live-db-path>`.
**Assert**:
- Exit code != 0.
- stderr contains both resolved paths.
**Risk**: R-06 (AC-02)

### Test: `test_snapshot_refuses_symlink_to_live_db`

**Purpose**: AC-02 (symlink edge case) — symlink pointing to live DB is resolved and rejected.
**Arrange**: Create symlink at `tmp/link.db` pointing to `<live-db-path>`.
**Act**: `unimatrix snapshot --project-dir <dir> --out tmp/link.db`.
**Assert**: Exit code != 0. stderr contains both resolved paths.
**Risk**: R-06

### Test: `test_snapshot_is_valid_sqlite3`

**Purpose**: AC-01 complement — snapshot is a valid SQLite3 file openable without write access.
**Arrange**: Take a snapshot from a test project.
**Act**: `sqlite3 <snapshot> "PRAGMA integrity_check"` as subprocess.
**Assert**: Output is `"ok"` (case-insensitive). Exit code 0.
**Risk**: AC-01 (FR-06)

### `--help` visibility test: see `cli-wiring.md` — AC-15

---

## Specific Assertions and Expected Behaviors

- `run_snapshot` must use `block_export_sync` (not a full tokio runtime). Verified structurally by inspecting the dispatch arm in `main.rs` — `run_snapshot` is called inside the sync block before `tokio::main` starts.
- The output file must be a complete SQLite copy (not a subset): `VACUUM INTO` is the only acceptable mechanism. No `export`-style filtering.
- The content-sensitivity warning must appear in `unimatrix snapshot --help` output (NFR-07). Tested in `cli-wiring.md`.
- No analytics tables are excluded: unlike `export.rs`, this is an unconditional full copy (FR-03).

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "snapshot database testing patterns" — found entries #748 (TestHarness Server Integration Pattern), #2326 (Bug fix verification async pattern), #128 (Risk drives testing), #157 (Test infrastructure is cumulative)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 (VACUUM INTO via sqlx + block_on, live-DB path guard), ADR-004 (eval infrastructure in unimatrix-server)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #157 (Test infrastructure is cumulative), #129 (Concrete assertions)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
