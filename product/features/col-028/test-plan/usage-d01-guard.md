# Test Plan: D-01 Guard (Component 4)
# File: crates/unimatrix-server/src/services/usage.rs

## Risks Addressed

| Risk | AC | Priority |
|------|-----|----------|
| R-01 D-01 dedup collision (briefing burns dedup slot) | AC-06, AC-07 | Critical |
| R-08 context_briefing weight not corrected | AC-06 | High |
| R-16 D-01 guard future bypass | (canary only) | Low |

---

## Background

`UsageDedup.access_counted` is a `HashSet<(String, u64)>` (agent_id, entry_id) pairs
shared across ALL `AccessSource` variants. Without the D-01 guard, calling
`record_briefing_usage` with `access_weight: 0` still invokes `filter_access`, which
inserts `(agent_id, entry_id)` into `access_counted`. A subsequent `context_get` call
(which routes through `record_mcp_usage`) then hits the dedup filter — the slot is
already taken — and produces `access_count += 0` instead of `access_count += 2`.

The guard is: `if ctx.access_weight == 0 { return; }` at the very top of
`record_briefing_usage`, before `filter_access` is called (ADR-003, C-03).

---

## Unit Test Expectations

All tests in this section exercise `UsageService` directly, not through an MCP handler.
They require access to the `UsageDedup` state — either via a test accessor or by
observing `access_count` in the database.

### AC-07 (POSITIVE ARM): briefing does not consume dedup slot; subsequent context_get increments

```rust
#[tokio::test]
async fn test_d01_guard_briefing_weight_zero_does_not_consume_dedup_slot() {
    // Arrange
    let store = open_test_store().await;
    let entry_id = insert_test_entry(&store).await; // access_count starts at 0
    let usage_service = UsageService::new(store.clone(), ...); // real UsageService
    let agent_id = "test-agent".to_string();
    let session_id = "sess-d01-positive";

    // Step 1: call record_briefing_usage with weight=0
    let briefing_ctx = UsageContext {
        access_weight: 0,
        agent_id: Some(agent_id.clone()),
        session_id: Some(session_id.to_string()),
        current_phase: None,
        ..Default::default()
    };
    usage_service.record_access(
        &[entry_id],
        briefing_ctx,
        AccessSource::Briefing,
    );
    // Allow fire-and-forget to complete
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Step 2: Assert dedup slot was NOT consumed
    // (access_count must still be 0 — briefing did nothing)
    let entry = store.get_entry(entry_id).await.expect("entry");
    assert_eq!(
        entry.access_count, 0,
        "D-01: briefing with weight=0 must not increment access_count"
    );

    // Step 3: call record_mcp_usage with weight=2 (context_get path)
    let get_ctx = UsageContext {
        access_weight: 2,
        agent_id: Some(agent_id.clone()),
        session_id: Some(session_id.to_string()),
        current_phase: None,
        helpful: Some(true),
        ..Default::default()
    };
    usage_service.record_access(
        &[entry_id],
        get_ctx,
        AccessSource::McpGet,
    );
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Step 4: Assert access_count = 2 (guard preserved the dedup slot)
    let entry = store.get_entry(entry_id).await.expect("entry");
    assert_eq!(
        entry.access_count, 2,
        "D-01: context_get after briefing must increment access_count by 2 \
         (briefing did not consume the dedup slot)"
    );
}
```

### AC-07 (NEGATIVE ARM — CRITICAL): without guard, briefing DOES consume dedup slot

This test PROVES the guard is load-bearing, not redundant. It must be present.

The negative arm can be implemented in one of two ways:
1. **Direct dedup test**: Construct a `UsageDedup` without going through the guard,
   call `filter_access` on it with the same entry and agent_id that briefing would
   have used, then call `record_mcp_usage`. This simulates what happens when the
   guard is absent — the dedup slot is consumed.
2. **Counterfactual comment**: If the UsageDedup internal state is not directly
   accessible in tests, the negative arm may be implemented as a comment block
   documenting what would happen, paired with a test that asserts the guard code
   path (the `if ctx.access_weight == 0 { return; }` branch).

Preferred implementation (option 1):

```rust
#[tokio::test]
async fn test_d01_absent_guard_would_consume_dedup_slot_negative_arm() {
    // This test demonstrates WHY the D-01 guard is load-bearing.
    // It simulates the guard-absent scenario by calling filter_access directly.

    // Arrange
    let store = open_test_store().await;
    let entry_id = insert_test_entry(&store).await;
    let agent_id = "test-agent-neg";

    // Simulate what record_briefing_usage does WITHOUT the guard:
    // calling filter_access with access_weight=0 marks the slot as seen.
    let mut dedup = UsageDedup::new(); // or however dedup is constructed
    // Without the guard, briefing calls filter_access here:
    let passes = dedup.filter_access(agent_id, entry_id);
    // The slot is now consumed.

    // Now simulate context_get trying to record (passes filter_access again):
    let passes_again = dedup.filter_access(agent_id, entry_id);

    // Assert: the second filter_access returns false (slot already consumed).
    assert!(passes, "first filter_access must pass — slot was free");
    assert!(
        !passes_again,
        "NEGATIVE ARM: without D-01 guard, briefing consumes the dedup slot — \
         subsequent context_get is blocked (access_count would be 0, not 2)"
    );
    // This is the bug the D-01 guard prevents.
}
```

If `UsageDedup` is not directly testable (private type), use the following documented
assertion pattern as the negative arm:

```rust
#[test]
fn test_d01_guard_present_in_source_documentation() {
    // The D-01 guard existence is verified by AC-07 positive arm:
    // if the guard were absent, test_d01_guard_briefing_weight_zero_does_not_consume_dedup_slot
    // would fail with access_count = 0 after context_get (instead of 2).
    //
    // This function serves as the explicit documentation of the negative scenario
    // per the col-028 test plan requirement.
    //
    // Negative scenario: WITHOUT guard
    //   1. record_briefing_usage(entry_X, weight=0) calls filter_access
    //   2. filter_access inserts (agent_id, entry_X) into access_counted
    //   3. record_mcp_usage(entry_X, weight=2) calls filter_access
    //   4. filter_access finds (agent_id, entry_X) already present → returns false
    //   5. access_count += 0 (entry X never gets its weight-2 increment)
    //
    // WITH guard (post-col-028):
    //   1. record_briefing_usage(entry_X, weight=0) → early return (guard fires)
    //   2. filter_access is NOT called → access_counted is NOT modified
    //   3. record_mcp_usage(entry_X, weight=2) → filter_access passes → access_count += 2
}
```

---

### AC-06: briefing with multiple entries — none increment access_count

```rust
#[tokio::test]
async fn test_briefing_weight_zero_no_increment_for_multiple_entries() {
    // Arrange: entries X, Y, Z with access_count = 0
    // Act: record_briefing_usage with [X, Y, Z], access_weight = 0
    // Assert: access_count for X = 0, Y = 0, Z = 0
    // Assert: dedup slots for X, Y, Z are all absent (not consumed)
}
```

### AC-06: briefing twice in same session — dedup slot still absent

```rust
#[tokio::test]
async fn test_briefing_twice_same_entry_dedup_slot_remains_absent() {
    // Arrange: entry X with access_count = 0
    // Act: record_briefing_usage(entry_X, weight=0) called twice
    // Assert: access_count = 0 after both calls
    // Assert: dedup slot still absent (weight=0 never enters dedup)

    // Then: call context_get (record_mcp_usage, weight=2)
    // Assert: access_count = 2 (slot was never consumed by either briefing call)
}
```

### EC-03: briefing with empty entry list — no panic

```rust
#[tokio::test]
async fn test_briefing_empty_entry_list_no_panic() {
    // Arrange: register session
    // Act: record_briefing_usage with empty &[u64] slice, access_weight = 0
    // Assert: no panic; function returns immediately (guard fires before any loop)
}
```

---

## Guard Location Verification (AC-12 companion for usage.rs)

The D-01 guard must appear as the FIRST statement in `record_briefing_usage`, before
any call to `filter_access`. Code review must confirm:

1. `if ctx.access_weight == 0 { return; }` is the first executable statement.
2. No earlier call to `filter_access` or `access_counted.insert()` precedes the guard.
3. The guard comment matches the spec: `// D-01 guard (col-028): weight-0 is an offer-only event.`

---

## Integration Test Expectations (infra-001 — AC-07)

The infra-001 integration test for AC-07 is the end-to-end validation of the D-01 guard
through the full MCP JSON-RPC path. Location: `suites/test_lifecycle.py`.

Test: `test_briefing_then_get_does_not_consume_dedup_slot`

```python
def test_briefing_then_get_does_not_consume_dedup_slot(server):
    """
    AC-07 D-01 guard integration test.
    Validates that context_briefing (access_weight=0) does not consume the dedup slot
    for entries it returns, allowing subsequent context_get to increment access_count.
    """
    # 1. Store entry X
    store_result = server.call("context_store", {
        "title": "Test Entry", "content": "...", "topic": "test", "category": "pattern"
    })
    entry_id = store_result["id"]

    # 2. Call context_briefing (includes entry X in returned set)
    server.call("context_briefing", {"session_id": "test-session"})

    # 3. Call context_get for entry X (access_weight=2)
    get_result = server.call("context_get", {"id": entry_id})
    assert get_result["entry"]["access_count"] == 2, (
        "D-01 guard failure: context_get after briefing must show access_count=2. "
        "Got 0 means briefing consumed the dedup slot."
    )

    # 4. Second context_get — dedup prevents double increment
    get_result2 = server.call("context_get", {"id": entry_id})
    assert get_result2["entry"]["access_count"] == 2, (
        "Second context_get in same session must be deduplicated — access_count unchanged"
    )
```

**Failure diagnosis**:
- `access_count = 0`: D-01 guard absent — briefing consumed the dedup slot.
- `access_count = 1`: `context_briefing` has wrong weight (weight=1 instead of 0) and
  it incremented access_count; dedup then blocked `context_get`.
- `access_count = 4`: Dedup not working — both `context_get` calls incremented (weight=2 each).

---

## Assertions Summary

| AC | Test Function | Expected |
|----|--------------|---------|
| AC-07 positive | `test_d01_guard_briefing_weight_zero_does_not_consume_dedup_slot` | access_count = 2 after context_get following briefing |
| AC-07 negative | `test_d01_absent_guard_would_consume_dedup_slot_negative_arm` | Documents that slot IS consumed when guard is absent |
| AC-06 multi | `test_briefing_weight_zero_no_increment_for_multiple_entries` | All entries access_count = 0 |
| AC-06 twice | `test_briefing_twice_same_entry_dedup_slot_remains_absent` | Dedup slot still absent; context_get succeeds with +2 |
| EC-03 | `test_briefing_empty_entry_list_no_panic` | No panic |
| AC-07 infra | `test_briefing_then_get_does_not_consume_dedup_slot` (test_lifecycle.py) | access_count = 2 end-to-end |
