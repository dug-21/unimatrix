# Test Plan: operational-tables (Wave 2)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-07 (Enum-to-Integer Mapping) | RT-42, RT-43 |
| RISK-08 (JSON Array Deser) | RT-46, RT-50 |
| RISK-11 (Session GC Cascade) | RT-56, RT-57, RT-58 |
| RISK-12 (Signal Drain Parity) | RT-59, RT-60 |
| RISK-19 (serde_json Dep) | RT-73 |

## Unit Tests

### UT-ops-01: TryFrom<u8> for SessionLifecycleStatus (RT-42)
```
Assert: TryFrom(0) == Active, TryFrom(1) == Completed, TryFrom(2) == TimedOut, TryFrom(3) == Abandoned
Assert: TryFrom(4) returns Err(StoreError::InvalidStatus(4))
```

### UT-ops-02: TryFrom<u8> for SignalType (RT-42)
```
Assert: TryFrom(0) == Helpful, TryFrom(1) == Flagged
Assert: TryFrom(2) returns Err
```

### UT-ops-03: TryFrom<u8> for SignalSource (RT-42)
```
Assert: All variants map correctly, invalid values return Err
```

### UT-ops-04: JSON malformed handling (RT-50)
```
Setup: Manually insert signal_queue row with entry_ids = "not json"
Action: Attempt to read
Assert: Graceful error or default (unwrap_or_default), not panic
```

## Integration Tests — Sessions

### IT-ops-01: Session insert + get round-trip
```
Setup: Create SessionRecord with all 9 fields
Action: insert_session, get_session
Assert: All fields match, status enum correct
```

### IT-ops-02: Session update
```
Setup: Insert Active session
Action: update_session (change status to Completed, set ended_at)
Assert: get_session returns updated fields
```

### IT-ops-03: scan_sessions_by_feature uses indexed query (RT-58)
```
Setup: Insert sessions for feature_cycle "feat-1" and "feat-2"
Action: scan_sessions_by_feature("feat-1")
Assert: Only feat-1 sessions returned
```

### IT-ops-04: GC deletes expired sessions + injection_logs (RT-56)
```
Setup:
  - Session S1: started_at = now - 200_000 (old)
  - Session S2: started_at = now - 100 (recent)
  - InjectionLog records for both S1 and S2
Action: gc_sessions(timed_out_threshold=50_000, delete_threshold=150_000)
Assert:
  - S1 deleted, S2 retained
  - Injection logs for S1 deleted
  - Injection logs for S2 retained
```

### IT-ops-05: GC active sessions untouched (RT-57)
```
Setup: Active session with recent started_at
Action: gc_sessions with aggressive thresholds
Assert: Active session still exists, not timed out
```

### IT-ops-06: GC marks timed-out sessions
```
Setup: Active session with old started_at (< timed_out_boundary)
Action: gc_sessions
Assert: Session status changed to TimedOut
```

### IT-ops-07: Status enum roundtrip in sessions (RT-43 partial)
```
Setup: Insert sessions with each SessionLifecycleStatus
Action: get_session for each
Assert: Status enum correctly preserved as integer column
```

## Integration Tests — Injection Log

### IT-ops-08: Batch insert + scan by session
```
Setup: Insert batch of 5 InjectionLogRecords for session "s1"
Action: scan_injection_log_by_session("s1")
Assert: 5 records returned with correct fields, ordered by log_id
```

### IT-ops-09: Scan returns empty for unknown session
```
Action: scan_injection_log_by_session("nonexistent")
Assert: Empty vec
```

## Integration Tests — Signal Queue

### IT-ops-10: Signal insert with JSON entry_ids (RT-46)
```
Setup: Create SignalRecord with entry_ids=[10, 20, 30]
Action: insert_signal
Action: Read back (via drain)
Assert: entry_ids == [10, 20, 30] (JSON round-trip)
```

### IT-ops-11: drain_signals by type (RT-59)
```
Setup: Insert 3 Helpful signals, 2 Flagged signals
Action: drain_signals(SignalType::Helpful)
Assert: Returns 3 Helpful signals with correct entry_ids
Assert: Flagged signals still in queue
```

### IT-ops-12: drain_signals atomic delete (RT-60)
```
Setup: Insert 2 Helpful signals
Action: drain_signals(Helpful)
Assert: Returns 2 signals
Action: drain_signals(Helpful) again
Assert: Returns empty (previously drained signals deleted)
```

### IT-ops-13: Signal queue cap enforcement
```
Setup: Insert 10_001 signals
Assert: Queue length <= 10_000 (oldest evicted)
```

### IT-ops-14: Empty entry_ids stored as [] not NULL
```
Setup: Insert signal with entry_ids=[]
Action: Read back
Assert: entry_ids == [] (empty Vec, not None/NULL)
```

## Build Verification

### BV-ops-01: serde_json dependency (RT-73)
```
Action: cargo build --workspace after Wave 2
Assert: Builds successfully with serde_json in store crate
```

## Schema Verification

### SV-ops-01: sessions table structure (AC-06)
```
Action: PRAGMA table_info(sessions)
Assert: 9 columns, no data BLOB
```

### SV-ops-02: injection_log table structure (AC-07)
```
Action: PRAGMA table_info(injection_log)
Assert: 5 columns with correct types
```

### SV-ops-03: signal_queue table structure (AC-08)
```
Action: PRAGMA table_info(signal_queue)
Assert: 6 columns, entry_ids is TEXT type
```

### SV-ops-04: Indexes exist
```
Action: Check sqlite_master for:
  - idx_sessions_feature_cycle
  - idx_sessions_status
  - idx_injection_log_session
  - idx_injection_log_entry
Assert: All present
```
