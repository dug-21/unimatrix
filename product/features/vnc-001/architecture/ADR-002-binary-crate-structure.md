## ADR-002: Binary Crate in Workspace

### Context

vnc-001 needs to produce an executable that Claude Code invokes. Options: (a) binary crate `unimatrix-server` in `crates/`, (b) split into library + binary (lib crate with `src/main.rs`), (c) workspace-root binary with `src/main.rs`.

The existing workspace has 4 library crates in `crates/`. The server has business logic (registry, audit, identity) that tests need to exercise.

### Decision

Create `crates/unimatrix-server/` as a binary crate with `src/main.rs` as entry point and `src/lib.rs` exposing modules for integration testing.

Structure:
```
crates/unimatrix-server/
  Cargo.toml
  src/
    main.rs          -- #[tokio::main], arg parsing, serve
    lib.rs           -- module declarations, pub exports for tests
    server.rs        -- UnimatrixServer, ServerHandler impl
    tools.rs         -- #[tool_router] impl, tool param types
    project.rs       -- project root detection, data dir management
    registry.rs      -- AgentRegistry, TrustLevel, Capability
    audit.rs         -- AuditLog, AuditEvent
    identity.rs      -- agent identity resolution
    embed_handle.rs  -- lazy-loading embed service wrapper
    shutdown.rs      -- graceful shutdown coordination
    error.rs         -- ServerError, MCP error mapping
  tests/
    integration/     -- integration tests via lib.rs exports
```

### Consequences

- **Easier:** Consistent with workspace conventions (`crates/unimatrix-*`). Integration tests can import from `lib.rs`.
- **Easier:** Clear separation: `main.rs` is just wiring; `lib.rs` exposes testable modules.
- **Harder:** Binary crate adds to workspace compile time. Mitigated by incremental compilation.
