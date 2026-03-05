# Test Plan: migration-compat (Wave 0)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-01 (Migration Data Fidelity) | RT-09 |
| RISK-07 (Enum-to-Integer Mapping) | RT-41 |

## Unit Tests

### UT-compat-01: deserialize_entry_v5 round-trip (RT-09)
```
Setup: Create EntryRecord with all 24 fields set to non-default values
Action: Serialize with bincode (v2 serde path), then deserialize_entry_v5
Assert: All 24 fields match original
```

### UT-compat-02: deserialize_entry_v5 with serde(default) fields
```
Setup: Create minimal EntryRecord (v0-era shape: ~16 fields), serialize with bincode
Action: deserialize_entry_v5
Assert: Default fields (helpful_count=0, unhelpful_count=0, confidence=0.5, etc.) populated correctly
```

### UT-compat-03: deserialize_co_access_v5 round-trip
```
Setup: Create CoAccessRecord, serialize with bincode
Action: deserialize_co_access_v5
Assert: count and last_updated match
```

### UT-compat-04: deserialize_session_v5 round-trip
```
Setup: Create SessionRecord with all fields, serialize with bincode
Action: deserialize_session_v5
Assert: All 9 fields match, SessionLifecycleStatus preserved
```

### UT-compat-05: deserialize_injection_log_v5 round-trip
```
Setup: Create InjectionLogRecord, serialize with bincode
Action: deserialize_injection_log_v5
Assert: All 5 fields match
```

### UT-compat-06: deserialize_signal_v5 round-trip
```
Setup: Create SignalRecord with entry_ids=[1,2,3], serialize with bincode
Action: deserialize_signal_v5
Assert: All fields match including entry_ids Vec
```

### UT-compat-07: deserialize_agent_record_v5 round-trip (RT-09)
```
Setup: Create AgentRecord with capabilities and allowed_topics, serialize with bincode
Action: deserialize_agent_record_v5
Assert: All fields match, TrustLevel and Capability enums correct
```

### UT-compat-08: deserialize_audit_event_v5 round-trip (RT-09)
```
Setup: Create AuditEvent with target_ids and Outcome, serialize with bincode
Action: deserialize_audit_event_v5
Assert: All fields match, Outcome enum correct
```

### UT-compat-09: bincode discriminant matches repr(u8) for all 7 enums (RT-41)
```
For each enum (Status, SessionLifecycleStatus, SignalType, SignalSource, Outcome, TrustLevel, Capability):
  For each variant:
    Setup: Serialize variant with bincode
    Action: Extract discriminant byte
    Assert: discriminant == variant as u8
```

## Type Movement Verification

Verify these types moved from server crate to store::schema and are accessible:
- `AgentRecord`
- `TrustLevel`
- `Capability`
- `AuditEvent`
- `Outcome`

Server crate must re-import from `unimatrix_store::{AgentRecord, TrustLevel, ...}`.
