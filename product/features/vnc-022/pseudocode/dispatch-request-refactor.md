# dispatch-request-refactor: Shared Pipeline with Capability Parameter

## Purpose

Make `dispatch_request` callable from both UDS and HTTP paths by: (1) changing visibility from `fn` to `pub(crate) fn`, (2) adding `capabilities: &[Capability]` as the final parameter, (3) replacing all 9 `uds_has_capability(X)` calls with `capabilities.contains(&X)`.

This is a mechanical refactor. Zero logic change. UDS behavior is identical before and after.

## File: `crates/unimatrix-server/src/uds/listener.rs`

### Change 1: Function Signature (line 516)

**Current**:
```
async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    _vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<Store>,
    _adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    session_registry: &SessionRegistry,
    pending_entries_analysis: &Arc<Mutex<PendingEntriesAnalysis>>,
    services: &crate::services::ServiceLayer,
) -> HookResponse {
```

**After**:
```
pub(crate) async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    _vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<Store>,
    _adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    session_registry: &SessionRegistry,
    pending_entries_analysis: &Arc<Mutex<PendingEntriesAnalysis>>,
    services: &crate::services::ServiceLayer,
    capabilities: &[Capability],
) -> HookResponse {
```

Two changes: `fn` becomes `pub(crate) async fn`, and `capabilities: &[Capability]` is appended as the last parameter.

### Change 2: Replace uds_has_capability Calls (9 sites)

Each replacement is mechanical: `uds_has_capability(Capability::X)` becomes `capabilities.contains(&Capability::X)`.

| Line | Current | After |
|------|---------|-------|
| 540 | `!uds_has_capability(Capability::SessionWrite)` | `!capabilities.contains(&Capability::SessionWrite)` |
| 625 | `!uds_has_capability(Capability::SessionWrite)` | `!capabilities.contains(&Capability::SessionWrite)` |
| 662 | `!uds_has_capability(Capability::SessionWrite)` | `!capabilities.contains(&Capability::SessionWrite)` |
| 736 | `!uds_has_capability(Capability::SessionWrite)` | `!capabilities.contains(&Capability::SessionWrite)` |
| 868 | `!uds_has_capability(Capability::SessionWrite)` | `!capabilities.contains(&Capability::SessionWrite)` |
| 1006 | `!uds_has_capability(Capability::Search)` | `!capabilities.contains(&Capability::Search)` |
| 1171 | `!uds_has_capability(Capability::Search) \|\| !uds_has_capability(Capability::Read)` | `!capabilities.contains(&Capability::Search) \|\| !capabilities.contains(&Capability::Read)` |
| 1201 | `!uds_has_capability(Capability::Search) \|\| !uds_has_capability(Capability::Read)` | `!capabilities.contains(&Capability::Search) \|\| !capabilities.contains(&Capability::Read)` |

Total: 9 call sites, 11 individual `uds_has_capability` invocations (lines 1171 and 1201 each have two).

### Change 3: Remove Import of uds_has_capability (line 51)

**Current** (line 51):
```
use crate::uds::uds_has_capability;
```

**After**: Remove this import line. The function is no longer called inside `dispatch_request`. If `uds_has_capability` is used elsewhere in the file, keep the import; otherwise remove.

Verification: grep for `uds_has_capability` usage in listener.rs outside of dispatch_request. If none, remove the import.

### Change 4: UDS Call Site (line 478)

**Current**:
```
let response = dispatch_request(
    request,
    &store,
    &embed_service,
    &vector_store,
    &entry_store,
    &adapt_service,
    &server_version,
    &session_registry,
    &pending_entries_analysis,
    &services,
)
.await;
```

**After**:
```
let response = dispatch_request(
    request,
    &store,
    &embed_service,
    &vector_store,
    &entry_store,
    &adapt_service,
    &server_version,
    &session_registry,
    &pending_entries_analysis,
    &services,
    crate::uds::UDS_CAPABILITIES,
)
.await;
```

Append `crate::uds::UDS_CAPABILITIES` as the final argument. `UDS_CAPABILITIES` is `&[Capability]` which matches the parameter type.

### Change 5: CompactPayload Destructure (line 1164)

Add `transcript_excerpt: _` to the pattern to accommodate the new wire type field from compact-payload-wire:

**Current**:
```
HookRequest::CompactPayload {
    session_id,
    injected_entry_ids: _,
    role,
    feature,
    token_limit,
} => {
```

**After**:
```
HookRequest::CompactPayload {
    session_id,
    injected_entry_ids: _,
    role,
    feature,
    token_limit,
    transcript_excerpt: _,
} => {
```

### What Does NOT Change

- `uds_has_capability` function in `uds/mod.rs`: remains (still used as the capability source)
- `UDS_CAPABILITIES` constant in `uds/mod.rs`: remains (passed at UDS call site)
- All logic inside match arms: unchanged (only the capability check predicate changes)
- Return types: unchanged
- Other functions in listener.rs (`handle_connection`, `write_response`, etc.): unchanged

## Error Handling

No new error paths. The `capabilities.contains()` check returns the same boolean as `uds_has_capability()` for the UDS path, so the existing Error responses are unchanged.

For the HTTP path, the Error response with code -32003 is still returned if the caller lacks the required capability. The HTTP handler maps this to 400 per ADR-004.

## Key Test Scenarios

1. **All existing UDS tests pass unchanged** (AC-18): The refactor is zero-behavior-change for UDS. Every existing test must pass without modification.
2. **Grep audit**: After refactor, `grep -c "uds_has_capability" dispatch_request_body` returns 0. All references replaced.
3. **UDS_CAPABILITIES includes SessionWrite**: Unit test in `uds/mod.rs` already verifies this (test_uds_has_capability_session_write, line 53-55).
4. **Capability denial**: Unit test calls dispatch_request with empty capabilities slice `&[]`, sends SessionRegister. Must return HookResponse::Error with code -32003.
5. **Capability grant**: Unit test calls dispatch_request with `&[Capability::SessionWrite]`, sends SessionRegister. Must NOT return capability error (may return other errors due to missing services, but not -32003).
