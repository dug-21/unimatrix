# capability-extension: Add SessionWrite to HTTP Capabilities

## Purpose

Add `Capability::SessionWrite` to the capability set returned by `StaticTokenValidator::validate_sync` for HTTP bearer callers. Without this, all session-mutating dispatch arms (SessionRegister, SessionClose, RecordEvent, RecordEvents, cycle_start/stop) return Error with code -32003 when called via HTTP.

## File: `crates/unimatrix-server/src/http/auth.rs`

### Modified Function: StaticTokenValidator::validate_sync

**Current** (line 119-123):
```
Ok(ResolvedIdentity {
    agent_id: "http-bearer".to_string(),
    trust_level: TrustLevel::Restricted,
    capabilities: vec![Capability::Read, Capability::Write, Capability::Search],
})
```

**After**:
```
Ok(ResolvedIdentity {
    agent_id: "http-bearer".to_string(),
    trust_level: TrustLevel::Restricted,
    capabilities: vec![
        Capability::Read,
        Capability::Write,
        Capability::Search,
        Capability::SessionWrite,
    ],
})
```

### Rationale

The dispatch_request capability checks (after the refactor in dispatch-request-refactor.md) use `capabilities.contains(&Capability::SessionWrite)` for:
- SessionRegister (line 540)
- SessionClose (line 625)
- RecordEvent (rework candidate, line 662)
- RecordEvent (general, line 736)
- RecordEvents (line 868)

And `capabilities.contains(&Capability::Search)` for:
- ContextSearch (line 1006)
- CompactPayload (line 1171, also needs Read)
- Briefing (line 1201, also needs Read)

The HTTP path needs SessionWrite, Search, and Read -- all three. Write is already present for MCP tool operations. Adding SessionWrite completes the set needed for full observation pipeline access.

### Why Not Admin?

Admin capability is not granted to HTTP bearer callers. Admin operations (agent registry management, config changes) are restricted to direct UDS/MCP access. The /observe endpoint only needs observation pipeline access, not administrative operations.

### No Other Changes in auth.rs

- `BearerValidator` trait: unchanged
- `StaticTokenAuth` middleware: unchanged
- `unauthorized_response`: unchanged
- `AUTH_BYPASS_PATHS`: unchanged (health bypass already exists; /observe requires auth)

## Error Handling

None. This is a value change to a Vec literal. No new error paths.

## Key Test Scenarios

1. **Unit test**: `StaticTokenValidator::validate_sync` with valid token returns `ResolvedIdentity` with capabilities containing `Capability::SessionWrite`.
2. **Unit test**: Capabilities vec contains exactly `[Read, Write, Search, SessionWrite]` -- no more, no less.
3. **Integration test** (in observe-handler): SessionRegister via HTTP returns 204 (not 400 with capability error) -- proves SessionWrite is present end-to-end.
