# Gate Bugfix Report: bugfix-360

> Gate: Bugfix Validation
> Date: 2026-03-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | `Handle::current().block_on()` fully removed from `check_entry_contradiction`; entries pre-fetched in Tokio context before rayon dispatch |
| No todo!/unimplemented!/FIXME | PASS | None found in changed files |
| New regression test present | PASS | `test_check_entry_contradiction_does_not_panic_in_rayon_pool` in `background::tests` |
| Test would have caught original bug | PASS | Test asserts `result.is_ok()` — before fix, `Handle::current()` panic -> `RayonError::Cancelled` -> `Err` |
| All tests pass | PASS | 3,383 passed, 0 failed (tester report) |
| No new clippy warnings | PASS | Clippy -D warnings clean |
| No unsafe code introduced | PASS | No unsafe blocks in the diff |
| Fix is minimal | PASS | Changes scoped to the rayon/tokio boundary issue only |
| Smoke suite | PASS | 20 passed, 0 failed |
| Contradiction suite | PASS | 12 passed, 0 failed |
| Lifecycle suite | PASS | 35 passed, 2 xfailed (pre-existing GH#303, GH#305) |
| xfail markers have GH Issues | PASS | GH#303 and GH#305 are pre-existing, not introduced by this commit |
| Investigator KS block | PASS | Queried #3339, #2126; Declined (existing entries cover pattern) |
| Rust-dev KS block | PASS | Queried #2742, #2126; Declined (direct application of existing pattern) |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**:

In `contradiction.rs`, the original call site:
```rust
// BEFORE (lines 110-114): panics in rayon worker
let neighbor_entry =
    match tokio::runtime::Handle::current().block_on(store.get(neighbor.entry_id)) {
        Ok(e) => e,
        Err(_) => continue,
    };
```

In commit `98dfbaa`:
- `check_entry_contradiction` signature changed from `store: &Store` to `entries: &[EntryRecord]`
- `HashMap<u64, &EntryRecord>` built from pre-fetched slice; neighbor lookup is pure in-memory
- `Handle::current().block_on()` call removed entirely
- `use unimatrix_core::Store;` import removed (no longer needed)

In `background.rs`, before the rayon spawn:
```rust
// GH #360: fetch in Tokio context before rayon dispatch; rayon threads have no Tokio runtime.
let active_entries_for_gate: Vec<EntryRecord> = match store
    .query_by_status(Status::Active)
    .await
{
    Ok(v) => v,
    Err(e) => {
        tracing::warn!(error = %e, "quality-gate contradiction check skipped: could not fetch entries");
        vec![]
    }
};
```

`store_for_gate` Arc clone removed; `&active_entries_for_gate` passed to the function.

Fix mirrors the same pattern applied in GH #358 for `scan_contradictions` and `check_embedding_consistency`.

### No todo!/unimplemented!/FIXME/placeholder

**Status**: PASS

No prohibited patterns found in the changed files (`contradiction.rs`, `background.rs` diff).

### New Regression Test

**Status**: PASS

`background::tests::test_check_entry_contradiction_does_not_panic_in_rayon_pool` (background.rs:3868):
- Creates a `RayonPool` with 1 thread
- Calls `check_entry_contradiction` from inside `pool.spawn(...)`
- Pre-fetches an empty entry slice (no store needed; neighbor lookup returns None and continues)
- Asserts `result.is_ok()` — `Err(RayonError::Cancelled)` would indicate a rayon worker panic
- Asserts `result.unwrap().unwrap().is_none()` — empty entry list yields no contradiction pair

Before the fix, `Handle::current()` inside the rayon worker would panic on every call, which rayon's no-op panic handler would discard, causing the oneshot sender to drop, returning `RayonError::Cancelled`. The test would have caught this (`result.is_err()`).

### Fix Minimality

**Status**: PASS

The diff is scoped to:
1. Changing the signature of `check_entry_contradiction` and removing the async store call
2. Pre-fetching entries in Tokio context before the rayon spawn in `background.rs`
3. Removing the now-unused `store_for_gate` Arc clone
4. Removing the now-unused `unimatrix_core::Store` import
5. Adding the regression test and expanding `NoopVectorStore`/`NoopEmbedService` mock trait implementations to match new trait requirements

No unrelated changes included.

### Pre-existing Concern: background.rs File Size

**Status**: WARN (pre-existing, not introduced by this fix)

`background.rs` is 3,908 lines in commit `98dfbaa` (was 3,795 before, 3,707 two commits back). This file was already well above the 500-line gate threshold before this bugfix. This commit added 113 lines (tests + pre-fetch logic). The violation is not new.

### Knowledge Stewardship

**Status**: PASS

**Investigator** (from spawn prompt):
- Queried: #3339 (rayon/tokio lesson), #2126 (block_in_place pattern)
- Stored: Declined — existing entries cover the pattern

**Rust-dev** (from `360-agent-1-fix-report.md`):
- Queried: #2742 ("Collect owned data before rayon_pool.spawn_with_timeout"), #2126 ("Use block_in_place not Handle::current().block_on")
- Stored: Declined — direct application of existing GH #358 pattern; entry #2742 already captures the convention

Both KS blocks are present with Queried evidence and reasoned Declined entries.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — this is a standard application of the rayon/Tokio boundary validation pattern. No new failure class was discovered. Existing entries (#2742, #2126, #3339) cover the pattern.
