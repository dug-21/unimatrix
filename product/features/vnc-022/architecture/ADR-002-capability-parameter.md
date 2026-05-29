## ADR-002: Capability Parameter on dispatch_request

### Context

`dispatch_request()` in `uds/listener.rs` calls `uds_has_capability(Capability::X)` at 9 points across its match arms. This function is hardcoded to check against `UDS_CAPABILITIES` — a compile-time constant for the UDS transport. The HTTP path needs the same dispatch logic but with capabilities from `ResolvedIdentity` (resolved by `StaticTokenAuth` middleware).

Options:
- **(A) Trait object `dyn CapabilitySource`**: Over-engineered for a `&[Capability].contains()` check. Adds a trait, a box, and dynamic dispatch for a static slice lookup.
- **(B) Generic `F: Fn(Capability) -> bool`**: Functional but obscures what's happening. Callers must construct a closure.
- **(C) `capabilities: &[Capability]` slice parameter**: Direct, zero-cost, obvious. Callers pass a slice reference. Internal checks become `capabilities.contains(&X)`.

SR-09 (HIGH): The refactor must not regress the UDS path. The UDS call site passes `crate::uds::UDS_CAPABILITIES` (an existing `&[Capability]` constant). The check semantics are identical — `uds_has_capability` already delegates to `UDS_CAPABILITIES.contains()`.

### Decision

Option (C): Add `capabilities: &[Capability]` as the final parameter to `dispatch_request`. Replace all 9 `uds_has_capability(X)` calls with `capabilities.contains(&X)`.

UDS call site (line 478 of `listener.rs`):
```rust
dispatch_request(request, ..., crate::uds::UDS_CAPABILITIES).await
```

HTTP call site (new, in `router.rs`):
```rust
dispatch_request(request, ..., &identity.capabilities).await
```

The function signature change is mechanical: one new parameter, same return type, same async behavior.

### Consequences

- `dispatch_request` becomes transport-agnostic. Any future transport (WebSocket, MCP tool tunneling) can call it with its own capability set.
- The UDS path is a zero-behavior-change refactor: `capabilities.contains(&Capability::SessionWrite)` returns the same boolean as `uds_has_capability(Capability::SessionWrite)` because both delegate to the same `UDS_CAPABILITIES` slice.
- The `uds_has_capability` function and `UDS_CAPABILITIES` constant remain — they are still used as the UDS caller's capability source. No dead code.
- Capability checks are visible at every call site (what capabilities does this transport grant?) rather than hidden inside a global function.
- Adding a new capability variant to `Capability` enum does not require changes to `dispatch_request` — it only requires updating the capability slices at each transport's call site.
