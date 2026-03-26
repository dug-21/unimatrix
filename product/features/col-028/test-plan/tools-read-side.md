# Test Plan: Phase Helper + Four Read-Side Call Sites + query_log Write Site (Component 2/3/6)
# Files: crates/unimatrix-server/src/mcp/tools.rs

## Risks Addressed

| Risk | AC | Priority |
|------|-----|----------|
| R-01 D-01 dedup collision | AC-07 | Critical |
| R-03 Phase snapshot race | AC-12 | Critical |
| R-04 Dual get_state at context_search | AC-16 | High |
| R-07 context_get weight not corrected | AC-05 | High |
| R-08 context_briefing weight not corrected | AC-06 | High |
| R-10 Phase not written to query_log | AC-16 | Medium |
| R-13 confirmed_entries cardinality | AC-10 | Medium |
| R-14 context_lookup weight drifted | AC-11 | Low |

---

## Part A: current_phase_for_session Free Function (Component 2)

Location: `crates/unimatrix-server/src/mcp/tools.rs`, module scope.
Visibility: `pub(crate)` (ADR-001 — testable without handler construction).

### Unit Test: Returns Some(phase) when session has active phase

```rust
#[test]
fn test_current_phase_for_session_returns_phase_when_set() {
    let registry = SessionRegistry::new();
    registry.register_session("sess-delivery", None, None);
    // Simulate phase set by context_cycle(start)
    registry.set_current_phase("sess-delivery", Some("delivery".to_string()));

    let result = current_phase_for_session(&registry, Some("sess-delivery"));
    assert_eq!(result, Some("delivery".to_string()));
}
```

### Unit Test: Returns None when session has no phase

```rust
#[test]
fn test_current_phase_for_session_returns_none_when_no_phase() {
    let registry = SessionRegistry::new();
    registry.register_session("sess-no-phase", None, None);
    // No phase set — current_phase is None by default

    let result = current_phase_for_session(&registry, Some("sess-no-phase"));
    assert!(result.is_none());
}
```

### Unit Test: Returns None when session_id is None (EC-02)

```rust
#[test]
fn test_current_phase_for_session_returns_none_for_no_session_id() {
    let registry = SessionRegistry::new();
    registry.register_session("sess-exists", None, None);
    registry.set_current_phase("sess-exists", Some("design".to_string()));

    // session_id parameter is None — must short-circuit without lookup
    let result = current_phase_for_session(&registry, None);
    assert!(result.is_none(), "None session_id must return None");
}
```

### Unit Test: Returns None when session_id is not registered

```rust
#[test]
fn test_current_phase_for_session_returns_none_for_unknown_session() {
    let registry = SessionRegistry::new();
    // Registry is empty

    let result = current_phase_for_session(&registry, Some("nonexistent-session"));
    assert!(result.is_none());
}
```

---

## Part B: Read-Side Handler Phase Capture (AC-01 through AC-04)

All four handlers must call `current_phase_for_session` as their first statement before
any `.await`. The tests below verify the observable result: `UsageContext.current_phase`
carries the correct value.

Implementation note: these tests require either (a) handler-level unit tests that
construct a `UnimatrixHandler` with a test `SessionRegistry`, or (b) inspection of
`UsageContext` via a test spy/channel. Follow the existing handler test patterns in the
codebase — the `UsageService` in tests typically accepts a channel receiver for inspection.

### AC-01: context_search passes current_phase in UsageContext

```
Arrange: SessionRegistry with session "sess-A" having phase "scope"
Act:     Call context_search handler for session "sess-A"
Assert:  UsageContext received by UsageService has current_phase = Some("scope")
         (inspect via test channel or spy)

Negative arm:
Act:     Call context_search with no session_id (None)
Assert:  UsageContext.current_phase = None
```

### AC-02: context_lookup passes current_phase in UsageContext

```
Arrange: SessionRegistry with session "sess-B" having phase "delivery"
Act:     Call context_lookup handler for session "sess-B" with target_ids=[42]
Assert:  UsageContext.current_phase = Some("delivery")

Negative arm:
Act:     Call context_lookup with no session
Assert:  UsageContext.current_phase = None
```

### AC-03: context_get passes current_phase in UsageContext

```
Arrange: SessionRegistry with session "sess-C" having phase "bugfix"
         Entry with id=99 exists in the store
Act:     Call context_get handler for session "sess-C", entry_id=99
Assert:  UsageContext.current_phase = Some("bugfix")

Negative arm:
Act:     Call context_get with no session
Assert:  UsageContext.current_phase = None
```

### AC-04: context_briefing passes current_phase in UsageContext

```
Arrange: SessionRegistry with session "sess-D" having phase "design"
         Store has entries to return in briefing
Act:     Call context_briefing handler for session "sess-D"
Assert:  UsageContext.current_phase = Some("design")

Negative arm:
Act:     Call context_briefing with no session
Assert:  UsageContext.current_phase = None
```

---

## Part C: Access Weight Corrections

### AC-05: context_get access_weight = 2 (changed from 1)

```rust
// Test must use real UsageService (not a mock) and inspect access_count.
#[tokio::test]
async fn test_context_get_access_weight_is_2() {
    // Arrange: insert entry X with access_count = 0 in a fresh store
    // Register session, no prior dedup entry for X
    // Act: call context_get for entry X
    // Assert: access_count for X = 2 (not 1)
}
```

Specific assertion: `entry.access_count == 2`. This distinguishes post-feature
(weight=2) from pre-feature (weight=1). The test must be an integration test that
actually reads access_count from the database after the UsageService has processed
the event.

### AC-05 second call: dedup prevents second increment

```rust
// Same session — call context_get for entry X a second time.
// Assert: access_count remains 2 (dedup filter blocked the second increment).
// This verifies dedup still works correctly with the new weight=2.
```

### AC-06: context_briefing access_weight = 0 — no access_count increment

```rust
#[tokio::test]
async fn test_context_briefing_does_not_increment_access_count() {
    // Arrange: insert entries [X, Y] with access_count = 0
    // Register session
    // Act: call context_briefing; briefing returns [X, Y]
    // Assert: access_count for X = 0 AND access_count for Y = 0
    // (Neither entry is incremented regardless of dedup state)
}
```

The failure mode if access_weight is still 1: access_count increments to 1 for
each returned entry after briefing. The test must read access_count from the
database after the UsageService has processed the event.

---

## Part D: confirmed_entries Handler-Level Tests

### AC-09: context_get populates confirmed_entries

```rust
#[tokio::test]
async fn test_context_get_populates_confirmed_entries() {
    // Arrange: register session "sess-E"; entry X exists in store
    // Act: call context_get handler for entry X in session "sess-E"
    // Assert: registry.get_state("sess-E").confirmed_entries.contains(&X)
}
```

### AC-09: context_get does NOT populate confirmed_entries on not-found (EC-05)

```rust
#[tokio::test]
async fn test_context_get_not_found_does_not_populate_confirmed_entries() {
    // Arrange: register session "sess-F"; entry 9999 does NOT exist
    // Act: call context_get for entry 9999 — handler returns not-found response
    // Assert: registry.get_state("sess-F").confirmed_entries is empty
    // (record_confirmed_entry called only on successful retrieval per FR-08)
}
```

### AC-10 (positive): single-target context_lookup populates confirmed_entries

```rust
#[tokio::test]
async fn test_context_lookup_single_target_populates_confirmed_entries() {
    // Arrange: register session "sess-G"; entry X exists in store
    // Act: call context_lookup with target_ids=[X] (len==1)
    // Assert: registry.get_state("sess-G").confirmed_entries.contains(&X)
}
```

### AC-10 (negative — REQUIRED): multi-target context_lookup does NOT populate confirmed_entries

```rust
#[tokio::test]
async fn test_context_lookup_multi_target_does_not_populate_confirmed_entries() {
    // Arrange: register session "sess-H"; entries X, Y both exist
    // Act: call context_lookup with target_ids=[X, Y] (len==2)
    // Assert: registry.get_state("sess-H").confirmed_entries is EMPTY
    // (ADR-004: request-side cardinality — only single-ID triggers confirmed_entries)
}
```

The negative arm is as important as the positive. Thompson Sampling inherits
`confirmed_entries` data cold; incorrect population with multi-target entries would
inflate the explicit-fetch signal and corrupt sampling (R-13, FM-07).

### AC-10 boundary: empty target_ids does not populate confirmed_entries (EC-04)

```rust
#[tokio::test]
async fn test_context_lookup_empty_target_ids_does_not_populate_confirmed_entries() {
    // target_ids.len() == 0 is NOT the same as len() == 1
    // Arrange: register session; call context_lookup with target_ids=[]
    // Assert: confirmed_entries is empty
}
```

---

## Part E: Phase Snapshot Placement (AC-12 — Code Review Gate)

AC-12 is a manual verification step, not an automated test. The test plan must
document the exact verification procedure:

**Manual verification procedure for AC-12**:

1. Open `crates/unimatrix-server/src/mcp/tools.rs` in the diff viewer.
2. Navigate to each of the four handler bodies:
   - `handle_context_search`
   - `handle_context_lookup`
   - `handle_context_get`
   - `handle_context_briefing`
3. Confirm that `current_phase_for_session(...)` appears as the **first statement**
   in the handler body, before the first `await` point.
4. Confirm that `current_phase_for_session` is called exactly **once** per handler
   (NFR-01: no duplicate `get_state` calls).
5. For `context_search` specifically: confirm the return value of
   `current_phase_for_session` is bound to a single variable and that variable is
   used for both `UsageContext.current_phase` and `QueryLogRecord::new`'s phase
   parameter (C-04, FR-18).

If any handler calls `current_phase_for_session` after an `.await`, mark AC-12 FAIL.
This is a delivery gate checklist item (R-03 risk materialization path).

---

## Part F: query_log Phase Write (Component 6, AC-16)

### AC-16: context_search writes phase to query_log (integration test, real drain)

This test MUST use the real analytics drain (pattern #3004). No stubs or mocks for
the drain path.

```
Arrange:
  - Open real SqlxStore (fresh DB at v17)
  - Create analytics drain channel (real AnalyticsService / enqueue_analytics)
  - Register session with phase "delivery"

Act:
  - Call context_search handler for that session (or call insert_query_log directly
    with phase = Some("delivery") via the updated QueryLogRecord::new signature)
  - Flush analytics drain / wait for drain task to process the event

Assert:
  - Call scan_query_log_by_session for the session_id
  - Assert returned QueryLogRecord.phase == Some("delivery")
```

### AC-16 (no session): context_search with no session writes phase=NULL

```
Act:   Call context_search with no session_id
Flush drain
Assert: query_log row has phase = None (not empty string, not panic)
```

### AC-16: phase value encoding — non-trivial phase string (EC-06)

```
Act:   context_search in a session with phase = "design/v2" (contains slash)
Assert: query_log.phase == Some("design/v2")
```

This verifies parameterized binding handles non-trivial characters correctly
(SQLx bind, not string interpolation — injection safety already assured, but
round-trip fidelity must be confirmed).

---

## Part G: context_lookup Weight Unchanged (AC-11)

AC-11 is primarily a regression guard. No new test is required if existing
context_lookup tests pass after the col-028 changes. The specific check:

- Existing tests that verify `access_count` after `context_lookup` must still pass.
- Code review confirms `access_weight: 2` is present in the `context_lookup`
  `UsageContext` literal (unchanged from pre-feature state).

---

## Integration Test Expectations (infra-001)

### New Test: test_briefing_then_get_does_not_consume_dedup_slot

Suite: `suites/test_lifecycle.py`
Fixture: `server`

This test exercises the full end-to-end MCP path:
1. `context_store` → create entry X.
2. `context_briefing` → briefing returns entry X (verifies briefing does not
   increment access_count and does not consume dedup slot).
3. `context_get` (entry X) → must increment access_count by 2.
4. `context_lookup` (entry X) → read access_count; assert = 2.

If the D-01 guard is absent: access_count after step 3 = 0 (dedup slot consumed
by briefing), and step 4 would show access_count = 0.
If briefing has wrong weight (weight=1 instead of 0): access_count = 1 after step 2,
then dedup blocks step 3 → access_count stays at 1.

### New Test: test_context_search_phase_persisted_to_query_log

Suite: `suites/test_lifecycle.py`
Fixture: `server`

1. `context_cycle` start with phase "delivery".
2. `context_search` for any query.
3. Drain analytics (or use a settle wait).
4. Inspect query_log phase (via a status query or direct inspection).
5. Assert phase = "delivery".

---

## Assertions Summary

| AC | Assertion | Expected |
|----|-----------|---------|
| AC-01 | UsageContext.current_phase for context_search | Some("scope") or None |
| AC-02 | UsageContext.current_phase for context_lookup | Some("delivery") or None |
| AC-03 | UsageContext.current_phase for context_get | Some("bugfix") or None |
| AC-04 | UsageContext.current_phase for context_briefing | Some("design") or None |
| AC-05 | access_count after context_get (first call) | 2 (not 1) |
| AC-05 dedup | access_count after context_get (second call) | 2 (no change) |
| AC-06 | access_count after context_briefing | 0 for all returned entries |
| AC-09 | confirmed_entries after context_get | Contains entry_id |
| AC-09 not-found | confirmed_entries after failed context_get | Empty |
| AC-10 positive | confirmed_entries after single-target lookup | Contains X |
| AC-10 negative | confirmed_entries after multi-target lookup | Empty (does NOT contain X or Y) |
| AC-10 boundary | confirmed_entries after empty target_ids lookup | Empty |
| AC-11 | context_lookup access_count increment | Same as pre-feature (weight=2) |
| AC-12 | Phase snapshot placement | CODE REVIEW GATE — first statement before await |
| AC-16 | query_log.phase after context_search with phase | "delivery" |
| AC-16 null | query_log.phase after context_search with no session | None |
