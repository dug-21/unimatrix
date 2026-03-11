# Test Plan: effectiveness-store

Component: `crates/unimatrix-store/src/read.rs` (new methods on Store)
Test location: `#[cfg(test)]` module in `read.rs`, extending existing test patterns
Infrastructure: TestDb, TestEntry from `test_helpers.rs`; session/injection_log insert helpers from `sessions.rs` and `injection_log.rs`

## Setup Pattern

Each test creates a TestDb, inserts entries via TestEntry builder, creates sessions via `store.create_session()`, and populates injection_log via `store.insert_injection_log_batch()`. This follows existing patterns in `injection_log.rs` and `sessions.rs` tests.

## compute_effectiveness_aggregates Tests

### Query 1: Entry Injection Stats

**S-01: COUNT DISTINCT session deduplication** (R-03)
- Setup: 1 entry, 1 session (outcome="success"). Insert 3 injection_log records for (entry_id=1, session_id="s1") with different confidence values.
- Call: `store.compute_effectiveness_aggregates()`
- Assert: `entry_stats[0].injection_count == 1` (not 3). One distinct session, not three injection records.

**S-02: Multiple distinct sessions counted correctly** (R-03)
- Setup: 1 entry, 3 sessions (s1=success, s2=rework, s3=abandoned). One injection per session for the same entry.
- Call: `store.compute_effectiveness_aggregates()`
- Assert: `entry_stats[0].injection_count == 3`, `success_count == 1`, `rework_count == 1`, `abandoned_count == 1`

**S-03: Sessions with NULL outcome excluded** (R-03)
- Setup: 1 entry, 2 sessions. s1 has outcome="success", s2 has outcome=None (active session).
- Insert injection for entry into both sessions.
- Assert: `entry_stats[0].injection_count == 1` (only s1 counted), `success_count == 1`

**S-04: Multiple entries with mixed outcomes**
- Setup: 3 entries, 5 sessions with various outcomes. Inject entries into different subsets of sessions.
- Assert: Each entry's stats match expected counts independently.

### Query 2: Active Topics

**S-05: NULL feature_cycle excluded from active_topics** (R-02)
- Setup: 2 sessions. s1 has feature_cycle=Some("crt-018"), s2 has feature_cycle=None.
- Assert: `active_topics` contains "crt-018" but not "" or any NULL representation.

**S-06: Empty string feature_cycle excluded** (R-02)
- Setup: 1 session with feature_cycle=Some("").
- Assert: `active_topics` is empty.

**S-07: Multiple distinct feature_cycles** (R-02)
- Setup: 3 sessions with feature_cycles "crt-018", "crt-018", "vnc-001".
- Assert: `active_topics == {"crt-018", "vnc-001"}` (deduplicated).

**S-08: NULL feature_cycle session still contributes to injection stats** (R-02)
- Setup: 1 entry, 1 session with feature_cycle=None and outcome="success". Inject entry into session.
- Assert: `entry_stats[0].success_count == 1` (session outcome counted), `active_topics` is empty (feature_cycle excluded).

### Query 3: Calibration Rows

**S-09: Calibration rows include all injection records** (R-06)
- Setup: 1 entry, 1 session (outcome="success"). 3 injection_log records with confidence values 0.3, 0.5, 0.8.
- Assert: `calibration_rows` has 3 entries: (0.3, true), (0.5, true), (0.8, true). Unlike injection_count (DISTINCT), calibration includes every injection record.

### Query 4: Data Window

**S-10: Data window from sessions with outcomes**
- Setup: 3 sessions. s1: started_at=1000, outcome="success". s2: started_at=2000, outcome="rework". s3: started_at=3000, outcome=None.
- Assert: `data_window.session_count == 2` (only sessions with outcomes), `earliest_session_at == Some(1000)`, `latest_session_at == Some(2000)`.

### Connection Lock Scope

**S-11: Single lock_conn scope** (R-07)
- Verification: Code review. Confirm that `compute_effectiveness_aggregates` calls `self.lock_conn()` once and all four queries execute within that scope. No intermediate lock release.

## load_entry_classification_meta Tests

**S-12: Active entries only**
- Setup: 2 entries. Entry 1 is Active, entry 2 is Deprecated.
- Assert: `load_entry_classification_meta()` returns 1 record (entry 1 only).

**S-13: NULL/empty topic mapped to "(unattributed)"** (R-02)
- Setup: Entry with topic="" (empty string).
- Assert: Returned `EntryClassificationMeta.topic == "(unattributed)"`.

**S-14: Fields correctly populated**
- Setup: Entry with title="My Title", topic="auth", trust_source="auto", helpful_count=5, unhelpful_count=2.
- Assert: All fields match in returned EntryClassificationMeta.

**S-15: Entry with no helpful/unhelpful counts**
- Setup: Entry with defaults (helpful_count=0, unhelpful_count=0).
- Assert: `helpful_count == 0`, `unhelpful_count == 0`.

## Empty Database Tests

**S-16: Empty database returns empty aggregates**
- Setup: Fresh TestDb, no entries, no sessions.
- Call: `store.compute_effectiveness_aggregates()`
- Assert: `entry_stats` empty, `active_topics` empty, `calibration_rows` empty, `data_window.session_count == 0`, timestamps are None.

**S-17: Empty entry_classification_meta on empty DB**
- Call: `store.load_entry_classification_meta()`
- Assert: returns empty Vec.

## Performance Test

**S-18: Performance at scale** (R-06)
- Setup: 500 entries, 200 sessions with outcomes, 10,000 injection_log rows distributed across entries and sessions.
- Call: `store.compute_effectiveness_aggregates()` with timing.
- Assert: Completes within 500ms. (Use `std::time::Instant` for wall-clock measurement.)

## Edge Cases

- Entry injected into session that was later GC'd (injection_log row exists but session is gone): JOIN produces no match, entry has zero effective injections. Not an error.
- Session with outcome but zero injection_log rows: contributes to data_window and active_topics but not entry_stats.
- Entry deleted between `compute_effectiveness_aggregates` and `load_entry_classification_meta`: orphaned entry_id in entry_stats with no matching meta. Server layer must handle gracefully (skip orphaned entries).
