# Component: server-test-migration

## Purpose

Replace `ClientInfo` and `Implementation` struct literal construction in the test helper `run_initialize_handshake()` (server.rs, lines 3257-3266) with constructor/builder calls compatible with rmcp 1.7's `#[non_exhaustive]` attributes.

## Current Code (lines 3257-3266)

```rust
let client_info = rmcp::model::ClientInfo {
    meta: None,
    protocol_version: ProtocolVersion::LATEST,
    capabilities: ClientCapabilities::default(),
    client_info: Implementation {
        name: client_name.to_string(),
        version: "0.0.1".to_string(),
        ..Default::default()
    },
};
```

## New/Modified Functions

### run_initialize_handshake() -- client_info construction

```
// Implementation: use constructor (same pattern as production)
let implementation = Implementation::new(client_name, "0.0.1")

// ClientInfo: compile-driven, same #[non_exhaustive] challenge as ServerInfo
// Strategies in order of preference:

// Strategy A: Constructor if available
let client_info = ClientInfo::new(implementation)

// Strategy B: Builder pattern if available
let client_info = ClientInfo::builder()
    .protocol_version(ProtocolVersion::LATEST)
    .capabilities(ClientCapabilities::default())
    .client_info(implementation)
    .build()

// Strategy C: Default + field mutation if fields are pub
let mut client_info = ClientInfo::default()
client_info.protocol_version = ProtocolVersion::LATEST
client_info.capabilities = ClientCapabilities::default()
client_info.client_info = implementation

// Strategy D: If ..Default::default() works within test context
let client_info = ClientInfo {
    protocol_version: ProtocolVersion::LATEST,
    capabilities: ClientCapabilities::default(),
    client_info: implementation,
    ..Default::default()
}
```

**Decision**: Compile-driven. The implementer must check which construction API rmcp 1.7 provides for `ClientInfo`. The `meta: None` field from the current code may be handled by the constructor default.

### serve_client reference (line 3273)

```
// Current:
let _ = rmcp::serve_client(client_info, client_transport).await;

// If rmcp 1.7 renamed or moved serve_client:
// Check import path. May now be rmcp::client::serve_client or similar.
// Compile-driven: the compiler will point to the correct location.
```

## Data Flow

- Input: `client_name: &str` (test parameter)
- Output: `ClientInfo` passed to `rmcp::serve_client()` for test handshake
- Consumed by: `run_initialize_handshake()` which tests client_type_map population

## Error Handling

Test-only code. `.await` results are discarded with `let _ = ...` (existing pattern). No change to error handling.

## Key Test Scenarios

1. **Compile gate** (R-08): `cargo test -p unimatrix-server --no-run` succeeds
2. **Handshake test** (R-02): `run_initialize_handshake("test-client")` completes and populates client_type_map with "test-client"
3. **serve_client availability** (R-08): `rmcp::serve_client` (or equivalent) resolves and compiles
