# Agent Report: 351b-agent-1-fix

**Agent ID**: 351b-agent-1-fix
**Branch**: bugfix/351-extraction-noise
**Date**: 2026-03-23

## Work Completed

### Fix 2: Two-step session-based observation fetch

**File**: `crates/unimatrix-server/src/background.rs`

Replaced the unbounded `SELECT ... FROM observations ORDER BY id DESC LIMIT 5000` scan
with a two-step approach:

- Added constant `DEAD_KNOWLEDGE_SESSION_THRESHOLD: usize = 20` alongside existing `DEAD_KNOWLEDGE_*` constants.
- **Step A**: `SELECT session_id FROM observations GROUP BY session_id ORDER BY MAX(id) DESC LIMIT ?1` — fetches the 20 most-recent distinct session IDs.
- **Step B**: Builds an IN-clause with indexed placeholders (`?1, ?2, ...`) following the exact pattern from `load_observations_for_sessions` in `observations.rs:131–136`.
- Removed the `limit: i64` parameter from `fetch_recent_observations_for_dead_knowledge`. Call site in `dead_knowledge_deprecation_pass` updated accordingly.
- The inner 5-session window argument to `detect_dead_knowledge_candidates` is unchanged.

### Fix 3: EXISTS query replaces full-topic scan

**File**: `crates/unimatrix-observe/src/extraction/recurring_friction.rs`
**File**: `crates/unimatrix-observe/Cargo.toml`

Replaced `store.query_by_topic("process-improvement")` + Rust-side `.any()` with:
```sql
SELECT EXISTS(
    SELECT 1 FROM entries
    WHERE topic = ?1 AND title = ?2 AND status = 0
)
```

- Uses `sqlx::query_scalar::<_, bool>` with `store.write_pool_server()` as specified.
- `block_in_place` pattern matches the existing usage in `dead_knowledge.rs:146`.
- Transient runtime fallback retained for the no-runtime case.
- Safe-default on error: `false` (allow proposal). No panics.
- Added `sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }` to `unimatrix-observe/Cargo.toml` — sqlx was not previously a direct dependency of this crate.

## Tests Added/Updated

### Fix 2 — new test
- `test_dead_knowledge_pass_session_threshold_boundary`: inserts `DEAD_KNOWLEDGE_SESSION_THRESHOLD + 5` sessions total. Entry accessed only in the 5 oldest sessions (beyond the threshold window) is deprecated; entry accessed in the most-recent session (inside the 5-session inner window) stays Active. Uses `"id": N` snippet format matched by `extract_entry_ids`.

### Fix 3 — new test
- `test_recurring_friction_does_not_skip_for_deprecated_entry`: pre-inserts a deprecated entry (status=1) with the matching title, verifies the dedup guard does NOT suppress proposal generation (EXISTS query is `status = 0` only).

The existing `test_recurring_friction_skips_if_existing_entry` was not changed — it already tests the active-entry suppression path.

## Test Results

```
unimatrix-observe: 388 passed, 0 failed
unimatrix-server lib: 1881 passed, 0 failed
All other targets: pass
```

Full workspace build: clean.

## Issues / Blockers

None. All changes are within the scope defined in the brief.

## Knowledge Stewardship

- Queried: /uni-query-patterns for `unimatrix-observe`, `unimatrix-server` — no prior results retrieved (knowledge search not invoked; brief was self-contained and code patterns were read directly from the codebase).
- Stored: entry via /uni-store-pattern — pattern: "detect_dead_knowledge_candidates expects response_snippet in `\"id\": N` or `#N` format to recognise accessed entry IDs — plain `entry_N` strings are silently ignored, causing test entries to appear unaccessed even when observations exist for their sessions." Topic: `unimatrix-observe`. (Stored after implementing test_dead_knowledge_pass_session_threshold_boundary which initially used `entry_{id}` format and failed for this reason.)
