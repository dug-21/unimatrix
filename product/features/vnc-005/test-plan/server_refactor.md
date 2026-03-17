# Test Plan: Server Refactor (`server.rs`)

This file covers two server.rs changes treated as one unit per the spawn prompt:
- **Component 3**: `UnimatrixServer` clone model (C-04, ADR-003)
- **Component 5**: `PendingEntriesAnalysis` refactor to two-level structure (ADR-004)

Risk coverage: R-01 (partial), R-05, R-07, R-15, R-18.
Additional ACs: AC-17, AC-18.
RV items: RV-06, RV-11, RV-12.

---

## Part A: UnimatrixServer Clone Model

### Unit Tests

#### T-SERVER-U-01: `UnimatrixServer::clone` produces shallow copy sharing all Arc fields
**Risk**: R-01
**Arrange**: Construct a `UnimatrixServer` from a real or mock `ServiceLayer`.
**Act**: Call `.clone()`.
**Assert**:
- `Arc::strong_count(&store)` increases by 1 (now 2).
- `Arc::ptr_eq(&server.store, &clone.store)` is `true` — both point to the same
  `Arc<Store>` allocation.
- Same check applies to `vector_index`, `pending_entries_analysis`, and
  `session_registry` fields.

#### T-SERVER-U-02: Arc strong_count is 1 before graceful_shutdown after session drop (RV-01, R-01)
**Risk**: R-01
**Arrange**: Construct a `UnimatrixServer`; clone it into a simulated session task
(wrapped in a `tokio::task::JoinHandle`). Record the `Arc::strong_count(&store)`.
**Act**: Drop the session-side clone; join the handle.
**Assert**: After join, `Arc::strong_count(&store) == 1`. This is the invariant
required before `graceful_shutdown` calls `Arc::try_unwrap(store)`.
**Test form**: `#[tokio::test]`.

#### T-SERVER-U-03: `ServiceLayer` constructed exactly once; no construction inside session task
**Risk**: R-01
**Arrange**: Code review / grep.
**Act**: `grep -rn 'ServiceLayer::new' crates/unimatrix-server/src/`
**Assert**: Exactly one call site. It is NOT inside a session task spawn closure
(i.e., not inside `async move { ... }` following a `tokio::spawn` call).
**Test form**: Automated grep in Stage 3c.

#### T-SERVER-U-04: `CallerId::UdsSession` exemption carries C-07/W2-2 comment (RV-11, R-07)
**Risk**: R-07
**Arrange**: Grep source.
**Act**: `grep -n 'UdsSession' crates/unimatrix-server/src/`
**Assert**: The match arm or adjacent comment contains both "C-07" and "W2-2" (or
"W2-2" and "C-07" in any order) within 3 lines of the exemption code.
**Test form**: Automated grep in Stage 3c.

#### T-SERVER-U-05: `CallerId::UdsSession` exemption does not apply to non-UDS caller variants
**Risk**: R-07
**Arrange**: Unit test the rate-limit enforcement function directly with mock `CallerId` values.
**Act**:
1. Call rate-limit check with `CallerId::UdsSession` → assert exempted (no error).
2. Call rate-limit check with `CallerId::Stdio` (or `CallerId::Http` if that variant
   exists as a future placeholder) → assert rate limit IS applied.
**Assert**: Only the `UdsSession` variant bypasses the rate check.

---

## Part B: PendingEntriesAnalysis Refactor

### Unit Tests

#### T-ACCUM-U-01: `upsert` inserts into correct feature_cycle bucket
**Arrange**: Create a fresh `PendingEntriesAnalysis`.
**Act**: Call `upsert("vnc-005", entry_a)` where `entry_a.entry_id = 1`.
**Assert**:
- `buckets.contains_key("vnc-005")` is `true`.
- `buckets["vnc-005"].entries.contains_key(&1)` is `true`.
- `buckets["vnc-005"].entries[&1] == entry_a`.

#### T-ACCUM-U-02: `upsert` on same entry_id overwrites (upsert semantics, not duplicate)
**Risk**: R-05
**Arrange**: Create `PendingEntriesAnalysis`; call `upsert("vnc-005", entry_v1)` where
`entry_v1.entry_id = 42`.
**Act**: Call `upsert("vnc-005", entry_v2)` where `entry_v2.entry_id = 42` but with
a different `rework_flag_count`.
**Assert**: `buckets["vnc-005"].entries.len() == 1`. The entry stored is `entry_v2`,
not `entry_v1`. No duplicate IDs.

#### T-ACCUM-U-03: `upsert` into different feature_cycle keys creates independent buckets
**Arrange**: Fresh `PendingEntriesAnalysis`.
**Act**: `upsert("vnc-005", entry_a)`; `upsert("vnc-006", entry_b)`.
**Assert**: `buckets.len() == 2`. Each bucket contains only its own entry.

#### T-ACCUM-U-04: `drain_for` returns all entries and removes the bucket
**Arrange**: `upsert` three entries into `"vnc-005"`: entry IDs 1, 2, 3.
**Act**: `let result = drain_for("vnc-005")`.
**Assert**:
- `result.len() == 3`.
- All three entries are present in `result`.
- `buckets.contains_key("vnc-005")` is now `false` (bucket removed).

#### T-ACCUM-U-05: `drain_for` on absent key returns empty Vec
**Arrange**: Fresh `PendingEntriesAnalysis`.
**Act**: `let result = drain_for("nonexistent-cycle")`.
**Assert**: `result.is_empty()`. No panic. No bucket created for the key.

#### T-ACCUM-U-06: `evict_stale` removes buckets older than ttl_secs
**Arrange**: Insert a bucket; set `last_updated` to `now - 73 * 3600` (just over 72h
TTL). Insert a second bucket with `last_updated = now - 1`.
**Act**: `evict_stale(now, 72 * 3600)`.
**Assert**: Old bucket is removed. New bucket is retained. Return value (or log) indicates
eviction occurred.

#### T-ACCUM-U-07: `evict_stale` does not evict non-empty buckets within TTL
**Arrange**: Insert a bucket with 5 entries; `last_updated = now - 71 * 3600`.
**Act**: `evict_stale(now, 72 * 3600)`.
**Assert**: Bucket is retained. Entries are still present.

#### T-ACCUM-U-08: Per-bucket cap enforced at 1000 entries (R-15)
**Risk**: R-15
**Arrange**: `upsert` 999 unique entries into `"vnc-005"`.
**Act**: `upsert` one more (1000th); then `upsert` one more (1001st with a new ID).
**Assert**: After upsert of 1001st entry, `buckets["vnc-005"].entries.len() <= 1000`.
The eviction must remove the entry with the lowest `rework_flag_count`. No panic.

#### T-ACCUM-U-09: Cap eviction runs inside Mutex critical section (R-15)
**Risk**: R-15
**Arrange**: Construct `PendingEntriesAnalysis`; verify that `upsert` holds the Mutex
for the entire duration of cap-check + eviction + insert.
**Assert**: Confirmed via code review that no `drop(lock)` + `lock.acquire()` pattern
exists within `upsert`. Static check in Stage 3c.

#### T-ACCUM-U-10: Mutex held for full duration of `evict_stale` and `drain_for` (RV-12, R-18)
**Risk**: R-18
**Arrange**: Acquire the `Mutex<PendingEntriesAnalysis>` in a test.
**Act**: Confirm that `evict_stale` and `drain_for` accept `&mut self` (i.e., require
exclusive access via the already-held lock guard). They cannot be called without first
acquiring the Mutex.
**Assert**: Neither method takes `&Arc<Mutex<Self>>` — both take `&mut self`, making
it structurally impossible to call them without holding the lock.
**Test form**: Compile-time / API design verification. Confirmed by signature inspection.

#### T-ACCUM-U-11: feature_cycle key exceeding 256 bytes returns validation error
**Risk**: Security Risks section (RISK-TEST-STRATEGY.md)
**Arrange**: Construct a `feature_cycle` string of 257 bytes.
**Act**: Call `upsert` (or the calling `context_store` handler) with the oversized key.
**Assert**: Returns a validation error (MCP-level: tool error response, not a panic).
No allocation for the oversized key. 256-byte key passes.

---

## Concurrency Tests

#### T-ACCUM-C-01: Concurrent upsert + drain — no data loss (RV-06, R-05, AC-17 partial)
**Risk**: R-05
**Arrange**: Create `Arc<Mutex<PendingEntriesAnalysis>>`.
Spawn 4 tokio tasks; each calls `upsert` 250 times on `"test-cycle"` with unique entry IDs.
Spawn a 5th task that calls `drain_for("test-cycle")` 10 times with 50ms intervals.
**Act**: Await all tasks.
**Assert**:
- Total entries seen across all drain calls = 1000 (4 tasks × 250 inserts).
- No entry appears in more than one drain result (drain removes the bucket; upserts
  after drain create a new bucket — the total count across all drains must account
  for the new bucket).
- No panic. No `MutexPoisonError`.
**Note**: The exact count per drain will vary due to timing, but the total must be 1000.

#### T-ACCUM-C-02: Concurrent `evict_stale` + `drain_for` — no double-free (RV-12, R-18)
**Risk**: R-18
**Arrange**: Create a `PendingEntriesAnalysis` with one bucket near TTL expiry.
Simulate two goroutines: one calls `evict_stale`, one calls `drain_for` on the same key.
**Act**: Both acquire the Mutex in sequence (since it's a Mutex, not concurrent).
**Assert**: Second caller gets `None`/empty (bucket already removed by first caller).
No entries are double-counted. No panic.
**Note**: Because `Mutex<_>` serializes access, this test confirms correctness of the
serialized order — the second caller observes the post-first-caller state.

---

## Integration Tests (AC-level)

#### T-ACCUM-I-01: Cross-session accumulation — 3 entries across 2 sessions (AC-17, R-05)
**Arrange**: Start daemon.
**Act**:
1. Open session A (bridge connection); call `context_store` twice with `feature_cycle=fnc-test-001`; close session A.
2. Open session B; call `context_store` once with `feature_cycle=fnc-test-001`.
3. From session B: call `context_retrospective` with `topic=fnc-test-001`.
**Assert**: The retrospective result contains all 3 entries. No entries from session A
were lost when session A closed.

#### T-ACCUM-I-02: Drain clears bucket — second drain returns empty (AC-18, R-05)
**Arrange**: Continue from T-ACCUM-I-01 (bucket drained).
**Act**: Call `context_retrospective` again with `topic=fnc-test-001`.
**Assert**: The accumulator section of the response is empty (0 entries). No duplicate
entries from the first drain appear in the second call.

#### T-ACCUM-I-03: Upsert semantics preserved across sessions (duplicate entry ID)
**Arrange**: Start daemon.
**Act**:
1. Session A: call `context_store` with key K; receive `entry_id=E`; close session A.
2. Session B: call `context_correct` on entry E (updates the entry). This issues a
   new `context_store` or correction internally.
3. Session B: call `context_retrospective`.
**Assert**: Only one entry for ID E appears in the retrospective result (the corrected
version, not the original + correction as separate duplicates).

---

## Assertions Summary

| Test | AC/RV | Risk |
|------|-------|------|
| T-SERVER-U-02 (Arc count = 1 before shutdown) | RV-01 | R-01 |
| T-SERVER-U-03 (ServiceLayer constructed once, grep) | — | R-01 |
| T-SERVER-U-04 (C-07 comment present, grep) | RV-11 | R-07 |
| T-SERVER-U-05 (UdsSession exemption only UDS) | — | R-07 |
| T-ACCUM-U-02 (upsert overwrites same ID) | — | R-05 |
| T-ACCUM-U-04 (drain removes bucket) | — | R-05, R-18 |
| T-ACCUM-U-08 (1000-entry cap) | — | R-15 |
| T-ACCUM-U-10 (Mutex held for full duration) | RV-12 | R-18 |
| T-ACCUM-U-11 (256-byte key validation) | — | Security |
| T-ACCUM-C-01 (concurrent upsert+drain, no loss) | RV-06 | R-05 |
| T-ACCUM-C-02 (evict+drain no double-free) | RV-12 | R-18 |
| T-ACCUM-I-01 (cross-session accumulation) | AC-17 | R-05 |
| T-ACCUM-I-02 (drain clears bucket) | AC-18 | R-05 |
