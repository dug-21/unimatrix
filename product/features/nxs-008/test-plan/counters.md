# Test Plan: counters (Wave 0)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-15 (Counter Consolidation) | RT-67, RT-68 |

## Unit Tests

### UT-counters-01: read_counter returns 0 for missing key
```
Setup: Empty counters table
Action: read_counter(&conn, "nonexistent")
Assert: Returns 0
```

### UT-counters-02: set_counter + read_counter round-trip
```
Setup: Empty counters table
Action: set_counter(&conn, "test_key", 42)
Assert: read_counter(&conn, "test_key") == 42
```

### UT-counters-03: increment_counter from zero
```
Setup: Empty counters table
Action: increment_counter(&conn, "key", 5)
Assert: read_counter(&conn, "key") == 5
```

### UT-counters-04: increment_counter accumulates
```
Setup: set_counter(&conn, "key", 10)
Action: increment_counter(&conn, "key", 3)
Assert: read_counter(&conn, "key") == 13
```

### UT-counters-05: decrement_counter does not go below zero
```
Setup: set_counter(&conn, "key", 2)
Action: decrement_counter(&conn, "key", 5)
Assert: read_counter(&conn, "key") == 0 (saturating)
```

### UT-counters-06: next_entry_id returns sequential IDs (RT-67)
```
Setup: Fresh database
Action: Call next_entry_id 3 times
Assert: Returns 1, 2, 3 (sequential, starting from 1)
```

## Integration Tests

### IT-counters-01: Status counters accurate after insert/update/delete (RT-68)
```
Setup: Fresh Store
Action:
  1. Insert 3 entries (Active status)
  2. Update 1 to Deprecated
  3. Delete 1
Assert:
  - status counter "total_active" == 1
  - status counter "total_deprecated" == 1
```

### IT-counters-02: Counter functions work with &Connection (not &SqliteWriteTransaction)
```
Setup: Fresh Store, lock_conn()
Action: Call all 5 counter functions with &Connection
Assert: All succeed, no type errors
```

## Function Signature Verification

All counter functions take `&Connection` (not `&SqliteWriteTransaction`):
- `read_counter(conn: &Connection, name: &str) -> u64`
- `set_counter(conn: &Connection, name: &str, value: u64) -> Result<()>`
- `increment_counter(conn: &Connection, name: &str, delta: u64) -> Result<()>`
- `decrement_counter(conn: &Connection, name: &str, delta: u64) -> Result<()>`
- `next_entry_id(conn: &Connection) -> Result<u64>`
