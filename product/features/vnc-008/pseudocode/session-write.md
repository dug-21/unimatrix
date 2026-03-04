# Pseudocode: session-write

## Purpose
Add `SessionWrite` capability variant to the Capability enum, define UDS_CAPABILITIES constant, add UDS capability enforcement at dispatch.

## Files Modified
- `src/infra/registry.rs`
- `src/uds/mod.rs`
- `src/uds/listener.rs`

## Pseudocode

### src/infra/registry.rs — Capability enum update

```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Read,
    Write,
    Search,
    Admin,
    /// Session-scoped writes: injection logs, session records, signals, co-access pairs.
    /// Distinct from Write (knowledge writes). UDS connections get this instead of Write.
    SessionWrite,
}
```

Serde compatibility: bincode v2 with serde path serializes enum variants by index. Adding a new variant at the end is backward-compatible for deserialization of existing data (existing data only contains indices 0-3; new variant is index 4). However, verify this with a round-trip test.

Also update the `capabilities_for_trust_level` or similar function if one exists. Check if there is a default capability set that needs updating. The existing auto-enrollment gives Restricted agents `[Read, Search]` — this does not change. SessionWrite is only assigned to UDS connections.

### registry.rs — require_capability update

The existing `require_capability` method checks if an agent's capabilities vec contains the required capability. No changes needed — SessionWrite is just another variant that can be checked. However, UDS connections are not enrolled agents. The UDS dispatch must check capabilities differently.

UDS capability checking approach: UDS connections use a fixed capability set (`UDS_CAPABILITIES`), not the agent registry. The capability check in UDS dispatch is:

```
fn uds_has_capability(cap: Capability) -> bool {
    UDS_CAPABILITIES.contains(&cap)
}
```

This is a local check, not a registry lookup.

### src/uds/mod.rs — UDS_CAPABILITIES constant

```
use crate::infra::registry::Capability;

pub mod hook;
pub mod listener;

/// Fixed capabilities for UDS connections. Not configurable at runtime.
pub(crate) const UDS_CAPABILITIES: &[Capability] = &[
    Capability::Read,
    Capability::Search,
    Capability::SessionWrite,
];

/// Check if UDS connections have a specific capability.
pub(crate) fn uds_has_capability(cap: Capability) -> bool {
    UDS_CAPABILITIES.contains(&cap)
}
```

### src/uds/listener.rs — Capability enforcement at dispatch

In the main dispatch match, add capability checks before each operation:

```
// In the dispatch function/match block:

HookRequest::Ping => {
    // No capability check — Ping is always allowed
    HookResponse::Pong
}

HookRequest::SessionRegister { session_id, ... } => {
    if !uds_has_capability(Capability::SessionWrite) {
        return HookResponse::Error {
            code: -32003,
            message: "insufficient capability: SessionWrite required".to_string(),
        };
    }
    // ... existing logic
}

HookRequest::SessionClose { session_id } => {
    // Same SessionWrite check
}

HookRequest::RecordEvent { .. } | HookRequest::RecordEvents { .. } => {
    // Same SessionWrite check
}

HookRequest::ContextSearch { .. } => {
    if !uds_has_capability(Capability::Search) {
        return HookResponse::Error { ... };
    }
    // ... existing logic
}

HookRequest::CompactPayload { .. } => {
    // Requires Search + Read
    if !uds_has_capability(Capability::Search) || !uds_has_capability(Capability::Read) {
        return HookResponse::Error { ... };
    }
    // ... existing logic
}

HookRequest::Briefing { .. } => {
    // Requires Search + Read
    if !uds_has_capability(Capability::Search) || !uds_has_capability(Capability::Read) {
        return HookResponse::Error { ... };
    }
    // ... existing logic
}
```

Note: Since UDS_CAPABILITIES is `[Read, Search, SessionWrite]`, the capability checks for Read and Search will always pass. The enforcement is for formal documentation and future-proofing — if UDS_CAPABILITIES ever changes, the checks will correctly reject operations.

The key behavioral change: if someone adds a new HookRequest variant that requires Write or Admin, it will be correctly rejected by UDS dispatch.

### Fire-and-forget operations

Operations that are fire-and-forget (injection log writes, signal queue writes, co-access pair writes) happen inside existing handler logic. They do not need separate capability checks — they are consequences of SessionRegister/SessionClose/RecordEvent which are already gated by SessionWrite.

## Compilation Gate

After this step: `cargo check --workspace` must succeed. All existing UDS tests pass. New tests verify capability enforcement.
