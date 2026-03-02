# Test Plan: signal-store

## Component Scope

`crates/unimatrix-store/src/signal.rs`, `schema.rs`, `migration.rs`, `db.rs` (Store methods), `lib.rs`

## Unit Tests

### SignalRecord Serialization (R-10)

**`test_signal_record_roundtrip`**
- Arrange: `SignalRecord { signal_id: 1, session_id: "s1", created_at: 1000, entry_ids: vec![10, 20], signal_type: Helpful, signal_source: ImplicitOutcome }`
- Act: serialize_signal → deserialize_signal
- Assert: round-trip produces identical struct

**`test_signal_record_roundtrip_flagged`**
- Same with `signal_type: Flagged, signal_source: ImplicitRework`

**`test_signal_record_field_order_layout_frozen_comment`**
- Documentation test: verify `// LAYOUT FROZEN` comment exists in signal.rs source
- (Can be a compile-time check or simple string scan in the test)

**`test_signal_type_discriminants`**
- Assert `SignalType::Helpful as u8 == 0`, `SignalType::Flagged as u8 == 1`

**`test_signal_source_discriminants`**
- Assert `SignalSource::ImplicitOutcome as u8 == 0`, `SignalSource::ImplicitRework as u8 == 1`

### Schema Version (R-02)

**`test_current_schema_version_is_4`**
- Assert `CURRENT_SCHEMA_VERSION == 4`

### Store::insert_signal (R-06)

**`test_insert_signal_returns_monotonic_ids`**
- Open fresh test db
- Insert two signals
- Assert returned IDs are 0, 1 (monotonically increasing)

**`test_insert_signal_data_persists`**
- Insert signal, reopen (or read), drain, assert record fields match

**`test_signal_queue_len_counts_all_types`**
- Insert 2 Helpful + 1 Flagged
- Assert signal_queue_len() == 3

**`test_signal_queue_cap_at_boundary_no_drop`**
- Insert 9,999 signals
- Assert signal_queue_len() == 9,999 (no drop at 9,999)

**`test_signal_queue_cap_at_10000_no_drop`**
- Insert 10,000 signals
- Assert signal_queue_len() == 10,000 (exactly at cap — no drop yet)

**`test_signal_queue_cap_at_10001_drops_oldest`** (AC-10, R-06)
- Insert 10,001 signals sequentially (signal_ids 0..10000)
- Assert signal_queue_len() == 10,000
- Drain all, assert no record with signal_id == 0 (first/oldest dropped)
- Assert record with signal_id == 10000 present (newest kept)

**`test_signal_queue_cap_at_11000_drops_1000`**
- Insert 11,000 signals
- Assert len == 10,000
- Drain, assert signal_ids 0..999 absent, 1000..10999 present

### Store::drain_signals (R-05)

**`test_drain_signals_idempotent_on_empty`** (R-05 scenario 1)
- Empty db, drain Helpful
- Assert Ok(empty Vec), no error

**`test_drain_signals_returns_matching_type`**
- Insert 3 Helpful + 2 Flagged
- drain_signals(Helpful) → returns 3 records
- drain_signals(Flagged) → returns 2 records

**`test_drain_signals_deletes_drained_records`** (R-05 scenario 2)
- Insert 2 Helpful
- drain_signals(Helpful)
- drain_signals(Helpful) again → empty Vec
- signal_queue_len() == 0

**`test_drain_signals_leaves_other_type`**
- Insert 1 Helpful + 1 Flagged
- drain_signals(Helpful)
- signal_queue_len() == 1 (Flagged remains)

### Schema Migration (R-02)

**`test_migration_v3_to_v4`** (AC-01, R-02 scenario 1)
- Create v3 test db with 10 entries (using test_helpers or manual setup)
- Open with Store::open() (triggers migrate_if_needed)
- Assert: schema_version == 4
- Assert: SIGNAL_QUEUE table exists (can open it without error)
- Assert: next_signal_id == 0 (COUNTERS has the key)
- Assert: all 10 entries intact (count = 10, spot-check one entry)

**`test_migration_v4_idempotent`** (R-02 scenario 2)
- Open v4 db (already migrated)
- Open again with Store::open()
- Assert: schema_version still 4, no writes performed (check SIGNAL_QUEUE is still empty)

**`test_migration_next_signal_id_not_overwritten`** (R-02 scenario 3)
- Manually set next_signal_id = 5 in an already-v4 db (simulate partial state)
- Open with Store::open()
- Assert: next_signal_id still 5 (not reset to 0)
- This tests FR-01.2: "only if key does not already exist"

## Integration Tests

**`test_insert_drain_full_roundtrip`**
- Insert Helpful signal for entry_ids [1, 2, 3]
- Drain Helpful
- Assert drained record has entry_ids == [1, 2, 3], session_id matches, signal_type == Helpful

**`test_concurrent_inserts_monotonic_ids`**
- Insert 100 signals; assert signal_ids are unique and span 0..99

## Edge Cases

- `test_signal_record_empty_entry_ids` — serialize signal with entry_ids=[] → roundtrip OK
- `test_drain_from_mixed_db` — 5 Helpful + 5 Flagged; drain Helpful → 5 records; drain Flagged → 5 records; queue empty
