# Test Plan: C2 Retrieval Filtering

## File: crates/unimatrix-server/src/tools.rs

### Integration Tests (6 tests)

All tests use `make_server()` + `insert_test_entry()` helper pattern. Each test stores an entry, quarantines it via the store directly (bypassing the tool handler to isolate retrieval testing), then verifies retrieval behavior.

#### Test 1: test_context_search_excludes_quarantined
```
#[tokio::test]
async fn test_context_search_excludes_quarantined():
    let server = make_server()
    let id = insert_test_entry(&server.store)  // Active entry

    // Quarantine the entry directly
    server.store.update_status(id, Status::Quarantined)

    // Search with a query that would match the entry
    let result = server.context_search(params with query matching entry).await
    // Verify: result does NOT contain the quarantined entry
    assert result text does not contain entry title or id
```
**AC**: AC-07
**Risk**: R-02 (quarantine status leak)

#### Test 2: test_context_lookup_excludes_quarantined_default
```
#[tokio::test]
async fn test_context_lookup_excludes_quarantined_default():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    server.store.update_status(id, Status::Quarantined)

    // Lookup with no status param (defaults to Active)
    let result = server.context_lookup(params without status).await
    // Verify: quarantined entry NOT returned
    assert result is empty or does not contain entry
```
**AC**: AC-08
**Risk**: R-02

#### Test 3: test_context_lookup_includes_quarantined_explicit
```
#[tokio::test]
async fn test_context_lookup_includes_quarantined_explicit():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    server.store.update_status(id, Status::Quarantined)

    // Lookup with explicit status="quarantined"
    let result = server.context_lookup(params with status="quarantined").await
    // Verify: quarantined entry IS returned
    assert result contains entry
```
**AC**: AC-08
**Risk**: R-02

#### Test 4: test_context_briefing_excludes_quarantined
```
#[tokio::test]
async fn test_context_briefing_excludes_quarantined():
    let server = make_server()
    let id = insert_test_entry_with_topic(&server.store, "conventions", "developer")

    server.store.update_status(id, Status::Quarantined)

    // Briefing for the role that would match the entry
    let result = server.context_briefing(params with role="developer").await
    // Verify: quarantined entry NOT in conventions or relevant context
    assert result does not contain quarantined entry
```
**AC**: AC-09
**Risk**: R-02

#### Test 5: test_context_get_returns_quarantined
```
#[tokio::test]
async fn test_context_get_returns_quarantined():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    server.store.update_status(id, Status::Quarantined)

    // Get by ID returns entry regardless of status
    let result = server.context_get(params with id).await
    // Verify: entry IS returned with Quarantined status
    assert result contains entry with status "quarantined" or "Quarantined"
```
**AC**: AC-10
**Risk**: R-02

#### Test 6: test_context_correct_rejects_quarantined
```
#[tokio::test]
async fn test_context_correct_rejects_quarantined():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    server.store.update_status(id, Status::Quarantined)

    // Attempt to correct the quarantined entry
    let result = server.context_correct(params with original_id=id, content="corrected").await
    // Verify: error returned mentioning quarantine
    assert result is Err
    assert error message contains "quarantined" or "restore"
```
**AC**: AC-07 (extended), R-12
**Risk**: R-12 (context_correct on quarantined entry)

## Risk Coverage

| Risk | Scenarios Covered |
|------|-------------------|
| R-02 | Tests 1-5: all four retrieval tools tested (search, lookup default, briefing exclude; lookup explicit, get include) |
| R-12 | Test 6: context_correct rejection for quarantined entries |
