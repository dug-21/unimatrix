# Test Plan: C3 Quarantine Tool

## File: crates/unimatrix-server/src/tools.rs

### Integration Tests (10 tests)

All tests use `make_server()` + `insert_test_entry()` helpers. The context_quarantine tool handler is invoked directly on the server instance.

#### Test 1: test_quarantine_active_entry
```
#[tokio::test]
async fn test_quarantine_active_entry():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    let result = server.context_quarantine(params {
        id, action: "quarantine", reason: "test quarantine", agent_id: "system"
    }).await
    // Verify: success response
    assert result is Ok
    // Verify: entry status changed to Quarantined
    let entry = server.store.get(id)
    assert entry.status == Status::Quarantined
```
**AC**: AC-02, AC-03
**Risk**: R-10

#### Test 2: test_quarantine_updates_status_index
```
#[tokio::test]
async fn test_quarantine_updates_status_index():
    let server = make_server()
    let id = insert_test_entry(&server.store)  // Active

    server.context_quarantine(params { id, action: "quarantine" }).await

    // Verify STATUS_INDEX: old Active entry removed, new Quarantined added
    let txn = server.store.begin_read()
    let status_table = txn.open_table(STATUS_INDEX)

    // Active entry should NOT exist
    assert status_table.get((Status::Active as u8, id)) is None
    // Quarantined entry SHOULD exist
    assert status_table.get((Status::Quarantined as u8, id)) is Some
```
**AC**: AC-03
**Risk**: R-10 (STATUS_INDEX orphan entries)

#### Test 3: test_quarantine_updates_counters
```
#[tokio::test]
async fn test_quarantine_updates_counters():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Before: total_active includes the entry
    let before_active = read_counter(&server.store, "total_active")
    let before_quarantined = read_counter(&server.store, "total_quarantined")

    server.context_quarantine(params { id, action: "quarantine" }).await

    // After: total_active decremented, total_quarantined incremented
    let after_active = read_counter(&server.store, "total_active")
    let after_quarantined = read_counter(&server.store, "total_quarantined")

    assert_eq!(after_active, before_active - 1)
    assert_eq!(after_quarantined, before_quarantined + 1)
```
**AC**: AC-03
**Risk**: R-03 (counter desync)

#### Test 4: test_restore_quarantined_entry
```
#[tokio::test]
async fn test_restore_quarantined_entry():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Quarantine first
    server.context_quarantine(params { id, action: "quarantine" }).await

    // Restore
    let result = server.context_quarantine(params { id, action: "restore" }).await
    assert result is Ok

    // Verify: entry status back to Active
    let entry = server.store.get(id)
    assert entry.status == Status::Active

    // Verify counters
    let active = read_counter(&server.store, "total_active")
    let quarantined = read_counter(&server.store, "total_quarantined")
    // Should be back to original values
```
**AC**: AC-04
**Risk**: R-03, R-10

#### Test 5: test_quarantine_idempotent
```
#[tokio::test]
async fn test_quarantine_idempotent():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Quarantine once
    server.context_quarantine(params { id, action: "quarantine" }).await
    let quarantined_count_1 = read_counter(&server.store, "total_quarantined")

    // Quarantine again (same entry)
    let result = server.context_quarantine(params { id, action: "quarantine" }).await
    assert result is Ok  // idempotent success

    // Verify: counter unchanged
    let quarantined_count_2 = read_counter(&server.store, "total_quarantined")
    assert_eq!(quarantined_count_1, quarantined_count_2)

    // Verify: entry still quarantined
    let entry = server.store.get(id)
    assert entry.status == Status::Quarantined
```
**AC**: AC-05
**Risk**: R-09 (idempotency violation)

#### Test 6: test_restore_non_quarantined_fails
```
#[tokio::test]
async fn test_restore_non_quarantined_fails():
    let server = make_server()
    let id = insert_test_entry(&server.store)  // Active

    // Attempt to restore an Active entry
    let result = server.context_quarantine(params { id, action: "restore" }).await
    // Verify: error
    assert result is Err
    assert error message contains "not quarantined"
```
**AC**: AC-06

#### Test 7: test_quarantine_nonexistent_entry
```
#[tokio::test]
async fn test_quarantine_nonexistent_entry():
    let server = make_server()

    let result = server.context_quarantine(params { id: 99999, action: "quarantine" }).await
    // Verify: error (EntryNotFound)
    assert result is Err
```
**AC**: AC-22

#### Test 8: test_quarantine_requires_admin
```
#[tokio::test]
async fn test_quarantine_requires_admin():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Use a restricted agent (not Admin)
    let result = server.context_quarantine(params {
        id, action: "quarantine", agent_id: "restricted-agent"
    }).await
    // Verify: error (InsufficientCapability)
    assert result is Err
```
**AC**: AC-02

#### Test 9: test_quarantine_confidence_changes
```
#[tokio::test]
async fn test_quarantine_confidence_changes():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Record initial confidence
    let before = server.store.get(id).confidence

    // Quarantine
    server.context_quarantine(params { id, action: "quarantine" }).await
    // Wait briefly for confidence recomputation (spawn_blocking)
    tokio::time::sleep(Duration::from_millis(50)).await

    let after_quarantine = server.store.get(id).confidence
    // Confidence should be lower (base_score drops from 0.5 to 0.1)
    assert!(after_quarantine < before)

    // Restore
    server.context_quarantine(params { id, action: "restore" }).await
    tokio::time::sleep(Duration::from_millis(50)).await

    let after_restore = server.store.get(id).confidence
    // Confidence should approximately recover
    assert!(after_restore > after_quarantine)
```
**AC**: AC-23
**Risk**: R-08 (confidence drift)

#### Test 10: test_quarantine_response_formats
```
#[tokio::test]
async fn test_quarantine_response_formats():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Test summary format
    let result = server.context_quarantine(params {
        id, action: "quarantine", format: "summary"
    }).await
    assert result text contains "Quarantined"

    // Restore first for next test
    server.context_quarantine(params { id, action: "restore" }).await

    // Test markdown format
    let result = server.context_quarantine(params {
        id, action: "quarantine", format: "markdown"
    }).await
    assert result text contains "## Entry Quarantined"

    // Test json format (restore + re-quarantine)
    server.context_quarantine(params { id, action: "restore" }).await
    let result = server.context_quarantine(params {
        id, action: "quarantine", format: "json"
    }).await
    assert result text contains "\"quarantined\": true"
```
**AC**: AC-24
**Risk**: --

## Risk Coverage

| Risk | Scenarios Covered |
|------|-------------------|
| R-03 | Tests 3, 4: counter arithmetic after quarantine and restore |
| R-08 | Test 9: confidence changes after quarantine/restore cycle |
| R-09 | Test 5: idempotent quarantine does not modify counters |
| R-10 | Tests 2, 4: STATUS_INDEX verified after quarantine and restore |
