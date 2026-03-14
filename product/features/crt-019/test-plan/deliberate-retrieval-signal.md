# Test Plan: deliberate-retrieval-signal
## Component: `crates/unimatrix-server/src/mcp/tools.rs` (context_get and context_lookup handlers)

### Risk Coverage

| Risk | Severity | Test(s) |
|------|----------|---------|
| R-07 | High | UsageDedup fires before access_weight — same agent second lookup produces 0 (not 2) |
| R-08 | High | context_get implicit vote must not spawn a second task |
| R-11 | High | Store-layer duplicate ID test — access_count += 2 (not +1) |
| SEC-02 | Medium | access_weight field not exposed in MCP parameter schema |
| EC-04 | Medium | access_weight default is 1 at non-lookup construction sites |
| EC-05 | Low | context_lookup returning zero results: no panic, no increment |
| C-04 | High | Zero new spawn_blocking calls in context_get handler |
| C-05 | High | Dedup order verified via same-agent-second-call scenario |

---

## Unit Tests (`services/usage.rs`)

### AC-08a: context_get Implicit Helpful Vote

**Three scenarios**:

#### Scenario 1: params.helpful.is_none() → helpful_count increments

```rust
#[test]
fn test_context_get_implicit_helpful_vote_when_none() {
    // Arrange: entry with helpful_count = 0
    // Act: call record_mcp_usage with UsageContext { helpful: Some(true), ... }
    //      (this is what the context_get handler now constructs via params.helpful.or(Some(true)))
    // Assert: entry.helpful_count == 1 after spawn_blocking completion
    //
    // Note: This tests the UsageService path, not the handler directly.
    // The handler transformation (params.helpful.or(Some(true))) is verified by code review.
    let (entry, _store) = make_test_entry_and_store(/* ... */);
    let ctx = UsageContext {
        helpful: Some(true), // what context_get now passes when params.helpful.is_none()
        access_weight: 1,
        ..test_usage_context()
    };
    record_mcp_usage(entry.id, ctx, &store, now).await;
    let updated = store.get(entry.id).unwrap();
    assert_eq!(updated.helpful_count, 1, "implicit helpful vote must increment helpful_count");
}
```

#### Scenario 2: params.helpful = Some(false) → helpful_count does NOT increment

```rust
#[test]
fn test_context_get_explicit_false_does_not_increment() {
    // UsageContext { helpful: Some(false), ... }
    // helpful_count must remain 0 after recording
    let (entry, store) = make_test_entry_and_store(/* ... */);
    let ctx = UsageContext {
        helpful: Some(false),
        access_weight: 1,
        ..test_usage_context()
    };
    record_mcp_usage(entry.id, ctx, &store, now).await;
    let updated = store.get(entry.id).unwrap();
    assert_eq!(updated.helpful_count, 0, "explicit false must not increment helpful_count");
    assert_eq!(updated.unhelpful_count, 1, "explicit false must increment unhelpful_count");
}
```

#### Scenario 3: NFR-04 fire-and-forget latency must not regress

The existing test `test_record_access_fire_and_forget_returns_quickly` (50ms bound) must
continue to pass after the `helpful: Some(true)` injection. The injection happens in-process
before spawn, adding zero I/O on the calling thread.

**Assert**: `test_record_access_fire_and_forget_returns_quickly` passes unchanged. If it fails
after the context_get changes, this is a C-04 violation.

---

### AC-08b: context_lookup Doubled access_count (R-07, R-11)

Three scenarios from RISK-TEST-STRATEGY.md R-07 coverage requirement.

#### Scenario 1: New agent, new entry → access_count == 2

```rust
#[test]
fn test_context_lookup_doubled_access_new_agent_new_entry() {
    // Arrange: entry with access_count = 0
    // Act: call record_mcp_usage with UsageContext { access_weight: 2, helpful: None, ... }
    //      with a fresh agent_id not in UsageDedup
    // Assert: entry.access_count == 2 after spawn_blocking completion
    // Also: entry.helpful_count == 0 (no vote injected for lookup)
    let (entry, store) = make_test_entry_and_store(/* ... */);
    let ctx = UsageContext {
        helpful: None,
        access_weight: 2, // context_lookup sets this
        agent_id: Some("fresh-agent-id".to_string()),
        ..test_usage_context()
    };
    record_mcp_usage_with_access_weight(entry.id, ctx, &store, now).await;
    let updated = store.get(entry.id).unwrap();
    assert_eq!(updated.access_count, 2, "lookup must produce access_count += 2");
    assert_eq!(updated.helpful_count, 0, "lookup must not inject helpful vote");
}
```

#### Scenario 2: Same agent, same entry, second call → access_count remains 2 (R-07)

This is the critical dedup-before-multiply verification:

```rust
#[test]
fn test_context_lookup_doubled_access_second_call_same_agent_zero() {
    // Arrange: same agent_id as scenario 1, entry now has access_count = 2
    // Act: call record_mcp_usage again with access_weight: 2, same agent_id
    // Assert: access_count stays 2 (UsageDedup filtered the entry; multiplier applied to empty set)
    //
    // R-07: If dedup fires AFTER multiply, the doubled ID list gets deduplicated,
    // producing access_count += 1 on second call instead of 0. This test fails that case.
    let (entry, store, dedup) = make_test_entry_store_and_dedup(/* ... */);
    let agent_id = "same-agent-id";

    // First call
    let ctx1 = UsageContext { access_weight: 2, agent_id: Some(agent_id.to_string()), .. };
    record_mcp_usage_with_dedup(entry.id, ctx1, &store, &dedup, now).await;
    assert_eq!(store.get(entry.id).unwrap().access_count, 2, "first call: access_count = 2");

    // Second call — same agent, dedup should suppress entirely
    let ctx2 = UsageContext { access_weight: 2, agent_id: Some(agent_id.to_string()), .. };
    record_mcp_usage_with_dedup(entry.id, ctx2, &store, &dedup, now).await;
    assert_eq!(store.get(entry.id).unwrap().access_count, 2,
        "second call same agent: access_count must remain 2 (dedup before multiply)");
}
```

#### Scenario 3: Two different agents → access_count == 4

```rust
#[test]
fn test_context_lookup_doubled_access_two_agents() {
    // Agent A and Agent B each look up the same entry once
    // access_count should be 4 (2 per agent × 2 agents)
    let (entry, store, dedup) = make_test_entry_store_and_dedup(/* ... */);

    let ctx_a = UsageContext { access_weight: 2, agent_id: Some("agent-a".to_string()), .. };
    record_mcp_usage_with_dedup(entry.id, ctx_a, &store, &dedup, now).await;
    assert_eq!(store.get(entry.id).unwrap().access_count, 2);

    let ctx_b = UsageContext { access_weight: 2, agent_id: Some("agent-b".to_string()), .. };
    record_mcp_usage_with_dedup(entry.id, ctx_b, &store, &dedup, now).await;
    assert_eq!(store.get(entry.id).unwrap().access_count, 4,
        "two agents × 2 per lookup = access_count 4");
}
```

---

### R-11: Store-Layer Duplicate ID Test (Blocking Prerequisite)

This test must pass before the `flat_map` repeat approach is committed. It tests the store
layer directly, bypassing `UsageService`:

```rust
#[test]
fn test_store_record_usage_duplicate_ids_increments_twice() {
    // Call record_usage_with_confidence with entry_ids = [42, 42]
    // Assert: entry 42's access_count increases by 2 (not 1)
    //
    // If the store deduplicates IDs in its UPDATE loop, this test will FAIL.
    // In that case, the fallback strategy (explicit (id, increment) pairs or
    // update_access_count(id, 2)) must be used instead of flat_map repeat.
    let store = make_test_store_with_entry(42, /* access_count: 0 */);
    let now = current_timestamp();

    store.record_usage_with_confidence(
        &[42u64, 42u64], // duplicate ID
        now,
        None, // no confidence fn needed for this test
    ).unwrap();

    let entry = store.get(42).unwrap();
    assert_eq!(entry.access_count, 2,
        "store must NOT deduplicate IDs; duplicate ID must produce access_count += 2");
    // If this assertion fails: the store deduplicates IDs.
    // Fallback: use update_access_count(id, increment) with multiplier,
    // or pass explicit (id, weight) pairs.
}
```

**Note**: If this test fails, it is not a test bug — it reveals the store behavior. In that
case, document the fallback strategy and update the implementation plan accordingly.

---

### EC-05: context_lookup Zero Results

```rust
#[test]
fn test_context_lookup_zero_results_no_side_effects() {
    // When context_lookup returns no entries (all IDs invalid or not found),
    // the access recording receives an empty list.
    // Assert: no panic, no access_count increment for any entry
    let (store, dedup) = make_test_store_and_dedup();
    let ctx = UsageContext { access_weight: 2, agent_id: Some("agent-a".to_string()), .. };

    // Record access for empty list
    record_mcp_usage_list(&[], ctx, &store, &dedup, now).await;
    // No assertions on specific entries since none were accessed;
    // the test passes if no panic occurs.
}
```

---

### C-04: Zero New spawn_blocking Calls (Code Review)

**Requirement**: The `context_get` handler diff must show zero new `spawn_blocking`,
`tokio::spawn`, `spawn_blocking_with_mandate`, or `task::spawn*` calls.

**Verification method**: Code review at Stage 3c. Grep the diff:
```bash
git diff HEAD~1 crates/unimatrix-server/src/mcp/tools.rs | grep "spawn_blocking\|tokio::spawn"
# Must show NO new + lines with spawn calls in the context_get handler area
```

**Secondary coverage**: The existing `test_record_access_fire_and_forget_returns_quickly` test
(50ms bound) provides behavioral evidence that no second blocking task was added.

---

## Integration Expectations

### New tests in `suites/test_tools.py`

#### `test_context_get_implicit_helpful_vote` (AC-08a)

```python
def test_context_get_implicit_helpful_vote(server):
    """AC-08a: context_get with helpful=null increments helpful_count."""
    # Store entry
    store_result = call_tool(server, "context_store", { "title": "test", "content": "test content",
        "topic": "test", "category": "decision", "trust_source": "agent" })
    entry_id = extract_id(store_result)

    # First get — helpful not specified (null)
    call_tool(server, "context_get", { "id": entry_id })
    time.sleep(0.1)  # wait for spawn_blocking

    # Read back — helpful_count should be 1
    status_result = call_tool(server, "context_status", {})
    # Verify via context_get response metadata or a separate status query
    # (exact assertion depends on what context_get returns about the entry)
    assert extract_helpful_count(store_result, entry_id) == 1

    # Second get — explicit helpful=false
    call_tool(server, "context_get", { "id": entry_id, "helpful": False })
    time.sleep(0.1)
    # helpful_count must remain 1 (false → unhelpful_count increment, not helpful_count)
    assert extract_helpful_count(store_result, entry_id) == 1
```

#### `test_context_lookup_doubled_access_count` (AC-08b, R-11)

```python
def test_context_lookup_doubled_access_count(server):
    """AC-08b: context_lookup produces access_count += 2; dedup suppresses second call."""
    store_result = call_tool(server, "context_store", { ... })
    entry_id = extract_id(store_result)

    # First lookup with fresh agent
    call_tool(server, "context_lookup", { "ids": [entry_id] })
    time.sleep(0.1)

    # Verify access_count == 2 (not 1)
    assert get_access_count(server, entry_id) == 2

    # Second lookup — same session/agent
    call_tool(server, "context_lookup", { "ids": [entry_id] })
    time.sleep(0.1)

    # UsageDedup suppresses: access_count remains 2
    assert get_access_count(server, entry_id) == 2
```
