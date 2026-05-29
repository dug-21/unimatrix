# Test Plan: dispatch-request-refactor

Component: `crates/unimatrix-server/src/uds/listener.rs` — dispatch_request visibility and capability parameterization

Covers: AC-18, AC-19, R-02

## Primary Gate: Zero Regression

The most critical assertion for this component is that ALL existing tests pass unchanged after the refactor. The refactor is mechanical:
1. `fn dispatch_request` -> `pub(crate) fn dispatch_request`
2. Add `capabilities: &[Capability]` as final parameter
3. Replace 9 `uds_has_capability(X)` calls with `capabilities.contains(&X)`
4. UDS call site passes `UDS_CAPABILITIES`

## Regression Tests

### All existing cargo tests pass

Run: `cargo test --workspace 2>&1 | tail -30`
Assert: Zero test failures. Zero test modifications required for UDS path.

### Grep audit: zero stale uds_has_capability in dispatch_request

Run: `grep -c "uds_has_capability" crates/unimatrix-server/src/uds/listener.rs` within the dispatch_request function body.
Assert: Only the `use` import line remains; zero calls inside dispatch_request body. The 9 call sites (lines 540, 625, 662, 736, 868, 1006, 1171x2, 1201) must all be replaced.

### Grep audit: dispatch_request is pub(crate)

Run: `grep "pub(crate) async fn dispatch_request" crates/unimatrix-server/src/uds/listener.rs`
Assert: Exactly one match.

## Unit Tests

Location: `crates/unimatrix-server/src/uds/listener.rs` or a new `#[cfg(test)]` section

### test_dispatch_request_respects_capabilities_session_write_denied

Arrange: Construct a `SessionRegister` HookRequest. Call `dispatch_request(...)` with `capabilities: &[Capability::Read, Capability::Search]` (no SessionWrite).
Act: Await the response.
Assert:
- Returns `HookResponse::Error` with code `-32003`
- Message contains "insufficient capability"

Note: This test requires access to the full service handle set (store, embed_service, etc.). If constructing these is impractical at unit level, this can be an integration test.

### test_dispatch_request_respects_capabilities_session_write_granted

Arrange: Construct a `SessionRegister` HookRequest. Call `dispatch_request(...)` with `capabilities: &[Capability::Read, Capability::Search, Capability::SessionWrite]`.
Act: Await the response.
Assert:
- Does NOT return `HookResponse::Error` with code `-32003`
- Returns `HookResponse::Ack` (session registered)

### test_dispatch_request_respects_capabilities_search_denied

Arrange: Construct a `ContextSearch` HookRequest. Call `dispatch_request(...)` with `capabilities: &[Capability::SessionWrite]` (no Search).
Act: Await the response.
Assert:
- Returns `HookResponse::Error` with code `-32003`
- Message contains "insufficient capability"

### test_uds_call_site_passes_uds_capabilities

Verify by code review and grep: The UDS call site (currently ~line 478) passes `crate::uds::UDS_CAPABILITIES` to the new `capabilities` parameter. This is a structural assertion, not a runtime test.

## Integration Surface

### Capability parameter at all 9 check sites

The 9 `uds_has_capability` calls check these capabilities:
- Lines 540, 625, 662, 736, 868: `Capability::SessionWrite` (5 sites)
- Line 1006: `Capability::Search` (1 site)
- Lines 1171, 1201: `Capability::Search` AND `Capability::Read` (2 sites, 4 checks)

After refactor, each becomes `capabilities.contains(&X)`. The capability parameter test above covers SessionWrite and Search denial. Read denial is covered implicitly by the Search+Read combined check in ContextSearch/CompactPayload arms.

## Risk Trace

| Risk | Scenario | Test |
|------|----------|------|
| R-02 | Missed uds_has_capability replacement | Grep audit + existing test regression |
| R-02 | Wrong capabilities slice passed at UDS call site | All existing UDS integration tests |
| R-02 | Visibility change breaks compilation | `cargo build --workspace` gate |
| AC-18 | UDS path zero regression | Full `cargo test --workspace` |
| AC-19 | Single implementation, no duplication | Grep: exactly one `pub(crate) async fn dispatch_request` |
