# Test Plan: Store Layer (Component 6)

Files: `crates/unimatrix-store/src/analytics.rs`, `write_ext.rs`, `db.rs`
Risks: R-02 (Critical), R-11, R-14, AC-09, AC-17

---

## Unit Test Expectations

### `AnalyticsWrite::FeatureEntry` variant (R-11, FR-06.2, C-12)

**`test_analytics_write_feature_entry_has_phase_field`** (compile-time / structural)
- Verify that `AnalyticsWrite::FeatureEntry` can be constructed with an explicit `phase` field
- Assert: `AnalyticsWrite::FeatureEntry { feature_id: "f".into(), entry_id: 1, phase: None }` compiles
- Assert: `AnalyticsWrite::FeatureEntry { feature_id: "f".into(), entry_id: 1, phase: Some("scope".into()) }` compiles

**`test_analytics_write_feature_entry_phase_some_matches_stored`** (R-11)
- Document: when `phase = Some("scope")` is set on the enqueued variant, the drain handler
  must write `"scope"` to `feature_entries.phase` — not `NULL`
- Full verification is in the drain integration test below

**`test_analytics_write_match_arms_exhaustive`** (R-11, C-12)
- The non-exhaustive annotation means internal match arms must not use `..` shortcut
- Verify: the drain handler match arm for `FeatureEntry` explicitly destructures `phase`
- This is a code review assertion; the Rust compiler enforces it at compile time

### `record_feature_entries` signature (R-14, FR-06)

**`test_record_feature_entries_accepts_phase_parameter`** (compile-time / R-14)
- Verify that `record_feature_entries(feature_cycle, entry_ids, phase)` compiles with a
  third `Option<&str>` argument
- Assert: all call sites in `server.rs`, `services/usage.rs`, and tests pass the third arg

---

## Integration Test Expectations

These tests require a real SQLite database via `tempfile::TempDir` and `SqlxStore::open`.

### `insert_cycle_event` (AC-04, AC-08, AC-17, FR-04)

**`test_insert_cycle_event_start_type`** (AC-17)
- Arrange: fresh store (v15 schema)
- Act: `store.insert_cycle_event("crt-025", 0, "cycle_start", None, None, Some("scope"), ts)`
- Assert: row in `cycle_events` with `event_type = "cycle_start"`, `next_phase = "scope"`,
  `phase IS NULL`

**`test_insert_cycle_event_phase_end_type`** (AC-17)
- Act: `store.insert_cycle_event("crt-025", 1, "cycle_phase_end", Some("scope"), None, Some("design"), ts)`
- Assert: row with `event_type = "cycle_phase_end"`, `phase = "scope"`, `next_phase = "design"`

**`test_insert_cycle_event_stop_type`** (AC-17)
- Act: `store.insert_cycle_event("crt-025", 2, "cycle_stop", Some("testing"), Some("all pass"), None, ts)`
- Assert: row with `event_type = "cycle_stop"`, `phase = "testing"`, `outcome = "all pass"`

**`test_insert_cycle_event_three_sequential_seq_values`** (AC-08, R-07)
- Act: insert three events for the same `cycle_id` in sequence
- Query: `SELECT seq FROM cycle_events WHERE cycle_id = ? ORDER BY timestamp ASC, seq ASC`
- Assert: seq values are `[0, 1, 2]`

**`test_insert_cycle_event_optional_nulls`** (AC-04, FR-04.6)
- Act: insert `cycle_start` with `phase = None`, `outcome = None`, `next_phase = None`
- Assert: all three columns are SQL `NULL` in the stored row

**`test_insert_cycle_event_orphaned_phase_end_no_start`** (R-13, FR-04.4)
- Act: insert a `cycle_phase_end` row for a `cycle_id` that has no `cycle_start` row
- Assert: insert succeeds (no error)
- Assert: row is retrievable via `SELECT WHERE cycle_id = ?`

### `record_feature_entries` with phase (AC-09, R-14)

**`test_record_feature_entries_with_phase_some`** (AC-09 non-NULL case)
- Arrange: store an entry, get `entry_id`
- Act: `store.record_feature_entries("crt-025", &[entry_id], Some("scope")).await`
- Query: `SELECT phase FROM feature_entries WHERE entry_id = ?`
- Assert: `phase = "scope"`

**`test_record_feature_entries_with_phase_none`** (AC-09 NULL case)
- Act: `store.record_feature_entries("crt-025", &[entry_id], None).await`
- Assert: `feature_entries.phase IS NULL`

**`test_record_feature_entries_three_arg_signature`** (R-14 compile check)
- Verify: `record_feature_entries` accepts exactly three arguments
- Any call site using the old two-argument signature fails to compile

### Analytics Drain Phase Snapshot (R-02 Critical, AC-09, FR-06.2)

This is the most critical store-layer test. It directly tests the enqueue-time snapshot guarantee.

**`test_analytics_drain_uses_enqueue_time_phase`** (R-02 Critical)
- Arrange: fresh store + analytics drain queue (using test helpers from `analytics.rs`)
- Act:
  1. Enqueue `AnalyticsWrite::FeatureEntry { feature_id: "f", entry_id: E, phase: Some("implementation") }`
  2. Do NOT flush the drain yet
  3. Simulate phase advancing: (conceptually) the `SessionState.current_phase` is now `"testing"`
     — but the enqueued event carries the old value
  4. Flush the drain queue (process all pending events)
- Query: `SELECT phase FROM feature_entries WHERE entry_id = E`
- Assert: `phase = "implementation"` — NOT `"testing"`

This test proves that the drain handler reads `phase` from the `FeatureEntry` variant struct,
not from any live session state. The variant carries the enqueue-time snapshot.

**`test_analytics_drain_phase_none_persists_null`** (R-11, R-02)
- Enqueue `AnalyticsWrite::FeatureEntry { ..., phase: None }`
- Drain
- Assert: `feature_entries.phase IS NULL`

**`test_analytics_drain_phase_some_persists_value`** (R-11)
- Enqueue with `phase: Some("testing")`
- Drain
- Assert: `feature_entries.phase = "testing"`

---

## Edge Cases

| Edge Case | Test | Expected Outcome |
|-----------|------|-----------------|
| `cycle_id` with multiple phase-end events (rework) | `test_insert_cycle_event_three_sequential_seq_values` | seq 0,1,2 in order |
| `entry_id` stored before any phase signal | `test_record_feature_entries_with_phase_none` | `phase IS NULL` |
| Drain fires after `current_phase` has advanced | `test_analytics_drain_uses_enqueue_time_phase` | Enqueue-time value preserved |
| `insert_cycle_event` for orphaned `phase_end` (no prior start) | `test_insert_cycle_event_orphaned_phase_end_no_start` | Insert succeeds |
