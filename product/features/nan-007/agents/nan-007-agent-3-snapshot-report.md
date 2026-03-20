# Agent Report: nan-007-agent-3-snapshot

**Component**: `snapshot.rs` (D1)
**Feature**: nan-007 W1-3 Evaluation Harness

---

## Files Modified

- `crates/unimatrix-server/src/snapshot.rs` — created (346 lines)
- `crates/unimatrix-server/src/export.rs` — `block_export_sync` promoted from `fn` to `pub(crate) fn`
- `crates/unimatrix-server/src/lib.rs` — added `pub mod snapshot`

---

## Implementation Summary

Implemented `run_snapshot(project_dir, out)` per pseudocode/snapshot.md and ADR-001.

Key behaviours delivered:
- **Pre-tokio sync dispatch**: calls `block_export_sync` from `export.rs` (C-09, ADR-001)
- **Live-DB path guard**: `canonicalize(paths.db_path)` vs `canonicalize_or_parent(out)` comparison (C-13, AC-02, NFR-06)
- **VACUUM INTO via sqlx**: `sqlx::query("VACUUM INTO ?").bind(out_str).execute(&pool)` — no rusqlite, no migration (C-02, ADR-001)
- **Read-only pool with rw fallback**: opens source with `read_only(true)`; falls back to `read_only(false)` if SQLite rejects (ADR-001 note)
- **No SqlxStore::open()**: raw `SqlitePool::connect_with(SqliteConnectOptions)` only (R-02)
- **Content-sensitivity warning**: documented in module-level doc comment (NFR-07, C-12)
- **No --anonymize flag**: removed from scope per C-12

`block_export_sync` in `export.rs` was private (`fn`). Promoted to `pub(crate)` so `snapshot.rs` (and the `eval/` modules) can reuse the async-to-sync bridge without duplication. Updated doc comment to reflect shared ownership.

---

## Tests

**8 unit tests, 8 pass, 0 fail** (all in `snapshot::tests` inline block):

| Test | Plan Coverage | Result |
|------|--------------|--------|
| `test_snapshot_path_guard_same_path` | AC-02, R-06 | pass |
| `test_snapshot_path_guard_symlink` | AC-02, R-06 | pass |
| `test_snapshot_parent_dir_missing` | Error path table | pass |
| `test_snapshot_canonicalize_fails_on_source` | R-06, NFR-06 | pass |
| `test_canonicalize_or_parent_existing_file` | Helper correctness | pass |
| `test_canonicalize_or_parent_nonexistent_file_existing_parent` | Helper correctness | pass |
| `test_canonicalize_or_parent_missing_parent_returns_error` | Helper error path | pass |
| `test_snapshot_no_sqlx_store_open_in_snapshot` | R-02, C-02 (structural doc) | pass |

Integration test for VACUUM INTO happy path (AC-01) is in `test-plan/snapshot.md` as a Python subprocess test (`test_snapshot_creates_valid_sqlite`) — deferred to the tester agent per the test plan.

Full workspace: **1530 lib tests, 0 failures** (pre-existing flaky `uds::listener::tests::col018_prompt_at_limit_not_truncated` passes when run in isolation, fails occasionally under parallel load — not related to this component).

---

## Issues / Blockers

None. All constraints satisfied.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` snapshot patterns — found #1561 (generation-cached snapshot, unrelated), #2602 (ADR-001 VACUUM INTO, directly applicable), #2126 (block_in_place pattern, followed). No gaps.
- Queried: `/uni-query-patterns` for `nan-007 architectural decisions` (category: decision, topic: nan-007) — found all five ADRs (#2585–#2588, #2602). All followed exactly.
- Stored: nothing novel to store — `block_export_sync pub(crate)` promotion and `canonicalize_or_parent` are direct applications of documented ADR-001 + patterns #2126/#1758. No runtime-invisible traps discovered.
