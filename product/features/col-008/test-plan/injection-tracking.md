# Test Plan: injection-tracking

## Component Scope

Changes to `crates/unimatrix-server/src/uds_listener.rs`: ContextSearch injection recording, SessionRegister/Close lifecycle integration, CoAccessDedup replacement with SessionRegistry.

## Risk Coverage

| Risk | Test |
|------|------|
| R-05 (injection tracking fails silently) | ContextSearch with/without session_id |
| R-06 (session_id mismatch) | End-to-end lifecycle test |
| R-12 (no SessionRegister before ContextSearch) | ContextSearch without prior register |

## Unit Tests (Dispatch Tests)

### SessionRegister Integration

#### test_dispatch_session_register_registers_session
```
Arrange: SessionRegistry, dispatch SessionRegister { session_id: "s1", agent_role: Some("dev"), feature: Some("col-008"), ... }
Act: dispatch_request(...)
Assert: response is Ack, session_registry.get_state("s1") returns Some with role="dev", feature="col-008"
```

### SessionClose Integration

#### test_dispatch_session_close_clears_session
```
Arrange: SessionRegistry with session "s1" registered
Act: dispatch_request(SessionClose { session_id: "s1", ... })
Assert: response is Ack, session_registry.get_state("s1") returns None
```

### ContextSearch Injection Tracking

Note: Full ContextSearch tests require embed service. These tests use the embed-not-ready path to verify the session_id plumbing.

#### test_dispatch_context_search_with_session_id
```
Arrange: SessionRegistry with session "s1" registered, embed not ready
Act: dispatch_request(ContextSearch { query: "test", session_id: Some("s1"), ... })
Assert: response is Entries (empty -- embed not ready), session_registry state unchanged (no entries to record)
```

#### test_dispatch_context_search_without_session_id
```
Arrange: SessionRegistry with session "s1" registered, embed not ready
Act: dispatch_request(ContextSearch { query: "test", session_id: None, ... })
Assert: response is Entries (empty), no injection tracking attempted
```

### CoAccessDedup Replacement

#### test_dispatch_session_close_clears_coaccess_via_registry
```
Arrange: SessionRegistry with session "s1", insert coaccess set [1,2,3]
Act: dispatch_request(SessionClose { session_id: "s1", ... })
Assert: after clear, check_and_insert_coaccess("s1", [1,2,3]) returns true (fresh)
```

### Signature Compatibility

#### test_dispatch_ping_with_session_registry
```
Arrange: SessionRegistry passed to dispatch_request
Act: dispatch_request(Ping, ...)
Assert: response is Pong (existing behavior unchanged)
```

#### test_dispatch_record_event_with_session_registry
```
Arrange: SessionRegistry passed to dispatch_request
Act: dispatch_request(RecordEvent { ... })
Assert: response is Ack (existing behavior unchanged)
```

## Integration-Level Verification

The full lifecycle (SessionRegister -> ContextSearch with entries -> CompactPayload) requires a running embed service and populated vector index. This is verified at Stage 3c test execution, not at the unit test level.
