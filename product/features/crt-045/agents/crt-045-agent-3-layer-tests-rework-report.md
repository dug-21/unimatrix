# Agent Report: crt-045-agent-3-layer-tests-rework

**Agent ID**: crt-045-agent-3-layer-tests-rework
**Task**: Gate 3b rework — extract `seed_graph_snapshot()` helper and bring layer_tests.rs under 500 lines
**Feature**: crt-045

## Work Completed

### Problem

`layer_tests.rs` was 677 lines after crt-045 added two new tests with fully
inlined seeding logic (293 new lines). Gate 3b flagged this as a FAIL against
the 500-line cap.

### Solution

Extracted `seed_graph_snapshot()` as a shared async helper encapsulating:
- `SqlxStore::open_at_path` (with migrations)
- Inserting two Active entries via `store.insert()`
- Inserting one CoAccess edge via raw SQL (`INSERT OR IGNORE INTO graph_edges`)
- Dumping an empty `VectorIndex` into `vector/` for `from_profile()` Step 5

Both new tests (`test_from_profile_typed_graph_rebuilt_after_construction` and
`test_from_profile_returns_ok_on_cycle_error`) call `seed_graph_snapshot()`.
The cycle test adds the Supersedes mutation after calling the helper.

After `cargo fmt` expanded the inlined content beyond 500 lines even with
the helper in `layer_tests.rs`, the two crt-045 tests and `seed_graph_snapshot`
were moved to a new dedicated module:

**New file**: `crates/unimatrix-server/src/eval/profile/layer_graph_tests.rs`

`mod.rs` was updated to register `#[cfg(test)] mod layer_graph_tests`.

### Final Line Counts

| File | Lines |
|------|-------|
| `layer_tests.rs` | 384 (restored to pre-crt-045 size) |
| `layer_graph_tests.rs` | 201 |

Both files are under 500 lines. All test logic preserved exactly — only the
file boundary changed.

## Files Modified

- `crates/unimatrix-server/src/eval/profile/layer_tests.rs` — removed crt-045 section (back to 384 lines)
- `crates/unimatrix-server/src/eval/profile/layer_graph_tests.rs` — new file with seed_graph_snapshot() + two crt-045 tests (201 lines)
- `crates/unimatrix-server/src/eval/profile/mod.rs` — added `#[cfg(test)] mod layer_graph_tests`

## Test Results

```
test eval::profile::layer_tests::layer_tests::* — 9 passed; 0 failed
test eval::profile::layer_graph_tests::layer_graph_tests::* — 2 passed; 0 failed
Total: 11 passed; 0 failed
```

Both new crt-045 tests pass:
- `test_from_profile_typed_graph_rebuilt_after_construction` (three-layer assertion)
- `test_from_profile_returns_ok_on_cycle_error` (cycle-abort-safety)

## Self-Check

- [x] `cargo build --workspace` passes (zero errors, 17 pre-existing warnings)
- [x] `cargo test` — all 11 layer tests pass
- [x] No `todo!()`, `unimplemented!()`, TODO, FIXME, or HACK
- [x] Scope limited to layer_tests.rs, layer_graph_tests.rs (new), mod.rs
- [x] No `.unwrap()` in non-test code
- [x] New structs: n/a (no new production structs)
- [x] Code follows validated pseudocode — `seed_graph_snapshot()` matches OVERVIEW.md spec
- [x] Test assertions preserved exactly (three-layer + cycle-abort-safety)
- [x] No file exceeds 500 lines
- [x] `cargo fmt --check` passes on both files

## Knowledge Stewardship

- Queried: skipped — rework task has no ambiguous implementation patterns; the fix
  is mechanical (file split). Briefing would return the same store/vector patterns
  already applied in the original crt-045 implementation.
- Stored: nothing novel to store — the lesson (cargo fmt expands compact struct
  literals, making line count unpredictable before formatting) is a general Rust
  workflow observation already implicit in the 500-line rule. The module-split
  approach for oversized test files is the standard resolution and does not require
  a separate Unimatrix entry.
