# crt-011: Implementation Brief — Confidence Signal Integrity

## Summary

Fix session count over-counting in signal consumers and add handler-level integration tests. Two bug fixes + new tests, all within `unimatrix-server`.

## GitHub Issue

#136 (https://github.com/dug-21/unimatrix/issues/136)
Related: #75 (session over-counting), #32 (handler integration tests)

## Implementation Tasks

### Task 1: Fix run_confidence_consumer (AC-01, AC-03, AC-04, AC-10)

**File:** `crates/unimatrix-server/src/uds/listener.rs`
**Function:** `run_confidence_consumer` (line ~1364)

**Changes:**
1. Add `let mut session_counted: HashSet<(String, u64)> = HashSet::new();` before the three-pass structure.
2. In Pass 1 (lines 1418-1427), wrap the `success_session_count += 1` increment:
   ```rust
   if session_counted.insert((signal.session_id.clone(), entry_id)) {
       existing.success_session_count += 1;
   }
   ```
3. In Pass 3 (lines 1440-1460), apply the same check:
   - Line 1446 ("Added between passes"): check `session_counted.insert(...)` before incrementing.
   - Line 1454 (new entry): the initial value of 1 is correct only if the pair is new in the HashSet (it will be, since this is a new entry).

**Do NOT modify:** Steps 2-3 (helpful_count dedup via HashSet<u64>). These are already correct.

### Task 2: Fix run_retrospective_consumer (AC-02, AC-05)

**File:** `crates/unimatrix-server/src/uds/listener.rs`
**Function:** `run_retrospective_consumer` (line ~1464)

**Changes:**
1. Add `let mut session_counted: HashSet<(String, u64)> = HashSet::new();` before the update loop.
2. In Step 4 (lines 1507-1528), split the increment:
   ```rust
   // Always increment rework_flag_count (event counter, no dedup)
   existing.rework_flag_count += 1;
   // Only increment rework_session_count once per (session_id, entry_id)
   if session_counted.insert((signal.session_id.clone(), entry_id)) {
       existing.rework_session_count += 1;
   }
   ```
3. For new entries (line 1521), set `rework_session_count: 1` only if the pair is new (it will be for new entries, but check for consistency).

**Add code comments** explaining the semantic distinction between `rework_flag_count` (event counter) and `rework_session_count` (session counter).

### Task 3: Consumer Dedup Unit Tests (AC-04)

**File:** `crates/unimatrix-server/src/uds/listener.rs` (extend existing test module)

**Tests to add:**
- `test_confidence_consumer_dedup_same_session` (T-CON-01)
- `test_confidence_consumer_different_sessions_count_separately` (T-CON-02)
- `test_retrospective_consumer_rework_session_dedup` (T-CON-03)
- `test_retrospective_consumer_flag_count_not_deduped` (T-CON-04)

**Setup pattern:** Use the existing test helper in listener.rs tests. Create a test Store, insert signals with overlapping entry_ids, call the consumer function, inspect PendingEntriesAnalysis.

### Task 4: Handler-Level Integration Tests (AC-06, AC-07, AC-08)

**Files:**
- `crates/unimatrix-server/src/services/usage.rs` (extend test module)
- `crates/unimatrix-server/src/server.rs` (extend test module)

**Tests to add:**
- `test_mcp_usage_confidence_recomputed` (T-INT-01)
- `test_mcp_usage_dedup_prevents_double_access` (T-INT-02)
- `test_confidence_path_search_to_store` (T-INT-03) — may already exist as `test_confidence_updated_on_retrieval`
- `test_confidence_path_dedup_across_calls` (T-INT-04) — may already exist as `test_record_usage_for_entries_access_dedup`

**Note:** Review existing tests before adding duplicates. Some of T-INT-03 and T-INT-04 may already be covered by existing tests in server.rs (e.g., `test_confidence_updated_on_retrieval`, `test_record_usage_for_entries_access_dedup`). If so, document the mapping and add only the missing coverage.

### Task 5: Verify No Regressions (AC-09)

Run full test suite: `cargo test --workspace`

## ADRs

- **ADR-001:** Per-session dedup using (session_id, entry_id) HashSet
- **ADR-002:** rework_flag_count remains un-deduplicated
- **ADR-003:** Integration tests at UsageService level

ADRs are documented in `product/features/crt-011/architecture/ARCHITECTURE.md`. Unimatrix storage was attempted but MCP server was unavailable; store when server is available.

## Estimated Scope

- **Modified files:** 1 (`uds/listener.rs`)
- **New test code:** ~150-200 lines across `listener.rs`, `usage.rs`, and `server.rs` test modules
- **Risk:** LOW — focused bug fix with clear before/after behavior
- **Dependencies:** None (no new crate deps, no schema changes)

## Implementation Order

1. Task 1 (confidence consumer fix) — highest priority, fixes #75
2. Task 2 (retrospective consumer fix) — same pattern
3. Task 3 (consumer dedup tests) — verify fixes
4. Task 4 (integration tests) — closes #32
5. Task 5 (regression check) — final gate
