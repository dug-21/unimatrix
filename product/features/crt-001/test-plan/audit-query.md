# Test Plan: C7 Audit Log Query

## Risk Coverage: R-08 (write_count_since correctness)

### T-C7-01: Count write vs read events (R-08 scenario 1)
- Log 5 context_store events and 5 context_search events for same agent
- write_count_since returns 5
- Verifies: AC-12

### T-C7-02: Agent ID filtering (R-08 scenario 2)
- Log events for 3 different agents
- write_count_since for agent A returns only agent A's writes

### T-C7-03: Timestamp boundary (R-08 scenario 3)
- Log events at known timestamps
- Query with since=T returns only events with timestamp >= T

### T-C7-04: Empty audit log (R-08 scenario 4)
- No events logged
- write_count_since returns 0

### T-C7-05: Both write operations counted (R-08 scenario 5)
- Log context_store and context_correct events
- Both count as writes

### T-C7-06: Non-write operations excluded (R-08 scenario 6)
- Log context_search, context_lookup, context_get, context_briefing, context_deprecate, context_status
- None count as writes

## R-15: deserialize_audit_event visibility

### T-C7-07: deserialize_audit_event accessible as pub(crate) (R-15)
- Verify that write_count_since works (implicitly uses pub(crate) deserialize_audit_event)
- Existing audit tests still pass
