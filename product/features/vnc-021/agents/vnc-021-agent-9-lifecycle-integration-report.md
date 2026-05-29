# Agent Report: vnc-021-agent-9-lifecycle-integration

## Task
Implement Lifecycle Integration (C8) -- wire HTTP transport into server startup and shutdown.

## Files Modified
- `crates/unimatrix-server/src/infra/shutdown.rs` -- Added `http_acceptor_handle` and `http_listener_addr` fields to `LifecycleHandles`; inserted HTTP acceptor shutdown step (Step 0-http) between MCP acceptor (Step 0) and MCP socket guard (Step 0a); updated all test construction sites; added 3 new tests (T-LI-13, T-LI-14, HTTP abort/join pattern).
- `crates/unimatrix-server/src/main.rs` -- Added HTTP listener startup block in `tokio_main_daemon`: loads token, builds TLS acceptor, composes tower service stack (ProjectRouter -> PathRouter -> StaticTokenAuth), starts HTTP listener, stores handles. Added informational log when HTTP disabled. Updated both daemon and stdio LifecycleHandles to include new fields (None for stdio per R-16).
- `crates/unimatrix-server/src/http/mod.rs` -- Added `pub use` re-exports for `StaticTokenAuthLayer`, `StaticTokenAuth`, `PathRouter`, `ProjectRouter`, `start_http_listener`, `build_tls_acceptor`, `load_or_generate_token` to make them accessible from the binary crate.
- `crates/unimatrix-server/src/http/auth.rs` -- Changed `StaticTokenAuthLayer`, `StaticTokenAuth`, and `StaticTokenAuthLayer::new()` from `pub(crate)` to `pub` for re-export.
- `crates/unimatrix-server/src/http/router.rs` -- Changed `PathRouter`, `ProjectRouter`, and their `new()` methods from `pub(crate)` to `pub`; removed `#![allow(dead_code)]` (items now used).
- `crates/unimatrix-server/src/http/listener.rs` -- Changed `start_http_listener` from `pub(crate)` to `pub`; removed `#![allow(dead_code)]`.
- `crates/unimatrix-server/src/http/tls.rs` -- Changed `build_tls_acceptor` from `pub(crate)` to `pub`.
- `crates/unimatrix-server/src/http/token.rs` -- Changed `load_or_generate_token` from `pub(crate)` to `pub`.

## Test Results
- shutdown tests: 14 passed, 0 failed (3 new vnc-021 tests)
- HTTP module tests: 76 passed, 0 failed
- gateway (rate limiter) tests: 36 passed, 0 failed
- Total validated: 126 passed, 0 failed

## Design Decisions
- HTTP startup placed after MCP UDS acceptor and before signal handler in `tokio_main_daemon`, matching the architecture's startup wiring order.
- HTTP not started in `tokio_main_stdio` (R-16: structurally excluded).
- Service stack composition: `StaticTokenAuthLayer.layer(PathRouter::new(ProjectRouter::new(server, max_body_bytes)))` -- request flows through auth first, then path dispatch, then MCP adapter.
- Token converted to `[u8; 32]` array for `StaticTokenAuthLayer::new()` with descriptive error on size mismatch.
- `daemon_token.child_token()` passed to HTTP listener (shared shutdown infrastructure).
- Visibility broadened from `pub(crate)` to `pub` on 7 items to support `pub use` re-exports from `http/mod.rs` -- binary crate (`main.rs`) needs these through the library boundary.

## Issues
- Full `cargo test --workspace` hits OOM (SIGKILL) during test binary linking -- pre-existing environment constraint, not caused by this change. Targeted test runs all pass.
- `shutdown.rs` is 763 lines (exceeds 500-line guideline), but 525 lines are tests. Production code is 238 lines. File was already 607 lines pre-vnc-021.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found ADR #4668 (shared tokio runtime), ADR #4669 (rustls TLS), ADR #4670 (credential_type), pattern #1684 (background task panic supervisor), pattern #2057 (shutdown protocol). Applied shutdown ordering from ADR-004 and existing patterns.
- Stored: nothing novel to store -- the visibility broadening pattern (pub(crate) -> pub for binary crate access via pub use re-exports) is standard Rust. The shutdown ordering follows existing patterns exactly.
