## ADR-003: Defer CallerId::ApiKey Variant

### Context

The research architecture document (`server-refactoring-architecture.md`) defines `CallerId` with three variants: `Agent(String)`, `UdsSession(String)`, `ApiKey(String)`. The `ApiKey` variant is for a future HTTP transport that does not yet exist.

Including `ApiKey` now means:
- Every `match` on `CallerId` must handle the `ApiKey` arm
- The variant is unused, requiring `#[allow(dead_code)]`
- Rate limiting exemption logic must decide whether ApiKey callers are exempt (unknown without HTTP transport design)
- It signals a design commitment to API-key-based HTTP auth before that decision is made

Not including it means:
- Adding `ApiKey` later is a minor, non-breaking change within `crates/unimatrix-server/` (crate-internal enum)
- Match arms are simpler (two variants)
- No dead code
- HTTP transport design can freely choose its auth model without being constrained by a pre-declared variant

### Decision

Defer `CallerId::ApiKey` until HTTP transport ships. `CallerId` has two variants for vnc-009:

```rust
pub(crate) enum CallerId {
    Agent(String),
    UdsSession(String),
}
```

Adding `ApiKey(String)` (or `HttpBearer(String)`, or whatever the HTTP transport needs) is a one-line addition to the enum plus match arm updates — all within the server crate.

### Consequences

**Easier**:
- Simpler match arms, no dead code
- HTTP transport design is unconstrained
- Less code to maintain and test

**Harder**:
- When HTTP transport ships, all `match` on `CallerId` must be updated (grep + mechanical update, bounded effort)
