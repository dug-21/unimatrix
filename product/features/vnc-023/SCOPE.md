# vnc-023: rmcp 0.16 to 1.7 Migration

## Problem Statement

Unimatrix pins rmcp at `=0.16.0`, a pre-1.0 version with a known high-severity vulnerability (CVE-2026-42559, CVSS 8.8 — DNS rebinding via Host header). The pin was intentional (ADR-001, vnc-001) because rmcp was pre-1.0 with frequent breaking changes. rmcp is now at 1.7.0 with a stable API surface, and remaining on 0.16.0 means:

- **Security exposure**: DNS rebinding attack on the Streamable HTTP transport shipped in vnc-021/vnc-022. An attacker on a local network can rebind a malicious domain to localhost, bypassing same-origin protections to invoke MCP tools with the victim's bearer token.
- **Protocol non-compliance**: Advertising MCP protocol version 2024-11-05 when the current spec is 2025-11-25. Claude Desktop/Code negotiate the latest version; advertising older may trigger compatibility warnings or degraded behavior.
- **Accumulated fragility**: 11 breaking changes across 8 releases have accumulated. Deferring further increases migration cost and blocks adoption of security and robustness improvements.

Who is affected: Any deployment using HTTPS transport (vnc-021). The CVE does not affect stdio-only or UDS-only deployments, but protocol non-compliance affects all transport modes.

Why now: vnc-022 is merged. The HTTPS transport is production-ready and the CVE applies to it. The migration is patch-level effort (~4 hours per ass-065 estimate) with ~90% of call sites unaffected.

## Goals

1. Upgrade rmcp from `=0.16.0` to `1.7.0`, resolving CVE-2026-42559
2. Fix all compilation breakage from `#[non_exhaustive]` additions (B7) — 3-4 struct literal sites in `server.rs`
3. Validate all behavioral default changes (`allowed_hosts`, session `keep_alive`) against deployment topology
4. Re-validate extension propagation — `ResolvedIdentity` must survive rmcp internal processing to reach `RequestContext.extensions` post-upgrade
5. Add `Implementation` description field enrichment (Opp 20) — already rewriting the struct literal
6. Add `allowed_origins` configuration knob to `HttpConfig` (Opp 11) — already touching `router.rs`
7. Verify all 8 `ErrorData::invalid_params` call sites compile without changes
8. Verify UDS `IntoTransport` blanket impl still works for `(OwnedReadHalf, OwnedWriteHalf)` tuple

## Non-Goals

- **Runtime tool disabling (Opp 9)**: Requires ownership model changes (`Arc<Mutex<ToolRouter>>`). Separate follow-on feature.
- **`json_response` mode (Opp 7)**: Needs evaluation of `stateful_mode` interaction with `client_type_map` (keyed on `Mcp-Session-Id`). Separate evaluation.
- **Trait-based tool declaration (Opp 1)**: Strategic refactor splitting `tools.rs` (~12K lines) into per-tool structs. 1-2 week effort. Separate design session.
- **Session persistence (Opp 10)**: `SessionStore` trait implementation for container restart resilience. 3-5 day effort. Separate feature.
- **`IntoCallToolResult` adoption (Opp 3)**: Return type changes across 40+ call sites. Revisit when trait-based tools are adopted.
- **Auto-generated `get_info` (Opp 2)**: Our manual `get_info()` uses runtime-configurable instructions (dsn-001). Macro requires compile-time literals. Skip.
- **OAuth 2.0 (Opp 4)**: Enterprise repo scope (W2-3).
- **Elicitation support (Opp 17)**: Client support not verified. Watch list.

## Background Research

### Research Spike (ass-065)

Detailed findings in `product/research/ass-065/FINDINGS.md` and `product/research/ass-065/FINDINGS-FUTURE.md`.

Key results:
- **API inventory**: ~100+ call sites across 18 files. Heaviest: `tools.rs` (14 handlers, ~80 error conversions), `server.rs` (trait impl + lifecycle), `router.rs` (transport adapter).
- **Breaking changes**: 11 across 8 releases (0.17.0 through 1.4.0). Zero additional breaking changes in 1.5–1.7.
- **Impact**: 3-4 mandatory fixes (struct literals in `server.rs`), 8 verifications (`invalid_params`), 2 behavioral reviews, ~90 unaffected.
- **Transport isolation**: ADR-003 concentrates all rmcp coupling in ~100 lines across 3 files. Auth/TLS/listener untouched.
- **CVE backport**: Not advisable. No 0.16.x branch upstream, creates unmaintainable fork. Full upgrade is same effort.
- **Effort**: Patch-level, ~4 hours implementation + testing.

### Codebase Patterns

**Struct literal construction (3-4 sites that break)**:
- `server.rs:274-287`: `ServerInfo { server_info: Implementation { ... }, ... }` — must switch to builder or `Implementation::new()` constructor due to B7 `#[non_exhaustive]`.
- `server.rs:3257-3266` (test): `ClientInfo { meta: None, ... }` — same issue, test-only.

**McpAdapter isolation boundary** (`router.rs:368-397`):
- `StreamableHttpServerConfig::default()` gains `allowed_hosts` (defaults to localhost — correct) and `json_response` (defaults to false — no behavioral change).
- `LocalSessionManager::default()` gains `keep_alive` default of 5 minutes (previously None). Acceptable for our use.
- `allowed_origins` (Opp 11, v1.6.0) is a new field on config — can be wired to `HttpConfig` during migration.

**ServerHandler::initialize override** (`server.rs:1038-1096`):
- Captures `clientInfo.name` from handshake, stores in `client_type_map` keyed on `Mcp-Session-Id`.
- Return type uses `impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_` with `std::future::ready()` — must verify this trait signature is unchanged in 1.7.
- Pattern #4367 documents 4 implementation traps from vnc-014. Traps 1 (Peer constructor) and 3 (initialize return type) need re-verification post-upgrade.

**Extension propagation** (`router.rs` McpAdapter):
- `ResolvedIdentity` inserted into request extensions by `StaticTokenAuth` middleware must survive rmcp's internal processing to be available in `RequestContext.extensions.get::<Parts>()`.
- vnc-021 R-01 spike confirmed this works in 0.16.0. Must be re-validated in 1.7.0.

**Automatic improvements from version bump** (zero code changes):
- Protocol version 2025-11-25 (v1.5.0)
- Stdio parse resilience: -32700 reply instead of connection close (v1.7.0)
- SSE connection reuse via stream drain (v1.5.0)
- Idle timeout logged at DEBUG instead of ERROR (v1.7.0)
- Init timeout protection: 60s default (v1.6.0)
- Error type constructors for `#[non_exhaustive]` types (v1.5.0)

### rmcp 0.16 Traps to Re-Verify (Pattern #4367)

1. **Peer constructor**: No public constructor — tests use `tokio::io::duplex` + `serve_client`. Verify this pattern still works in 1.7.
2. **`http` crate dependency**: Must remain explicit in Cargo.toml for `http::request::Parts` extraction from extensions.
3. **Initialize return type**: `impl Future + Send + '_` with `std::future::ready()`. Verify trait signature unchanged.
4. **Private error module**: `rmcp::ErrorData` (re-export) not `rmcp::error::ErrorData`. Verify re-export path unchanged.

## Proposed Approach

### Single-wave migration

The migration is patch-level with concentrated changes in 3 files. A single implementation wave is appropriate.

**Phase 1: Version bump + compilation fixes**
1. Update `Cargo.toml`: `rmcp = { version = "=1.7.0", features = [...] }`
2. Fix `Implementation` struct literals → `Implementation::new(name, version)` + `.with_description()`
3. Fix `ServerInfo` struct literal → use `..Default::default()` or builder (verify `Default` still derived alongside `#[non_exhaustive]`)
4. Fix `ClientInfo` test literal → builder or constructor
5. Compile, fix any remaining `#[non_exhaustive]` misses

**Phase 2: Behavioral validation + opportunistic enhancements**
1. Verify `ErrorData::invalid_params` exists (8 sites)
2. Verify `LocalSessionManager::default()` compiles
3. Verify UDS `(OwnedReadHalf, OwnedWriteHalf)` IntoTransport blanket impl
4. Add `allowed_origins: Vec<String>` to `HttpConfig`, wire to `StreamableHttpServerConfig`
5. Validate `allowed_hosts` default (localhost) against deployment topology

**Phase 3: Integration testing**
1. Extension propagation: `ResolvedIdentity` survives rmcp processing
2. Full test suite pass
3. Manual smoke test: stdio, UDS, HTTPS transports

### Rationale for bundling Opp 11 and Opp 20

Both are in files already being modified:
- **Opp 20** (Implementation enrichment): The `Implementation` struct literal must be rewritten anyway. Adding `.with_description()` is one chained method call.
- **Opp 11** (Origin validation): `router.rs` McpAdapter::new() is already under review for `StreamableHttpServerConfig` behavioral changes. Adding `allowed_origins` config propagation is ~15 min marginal effort.

Both are defense-in-depth security improvements that complement the CVE fix.

## Acceptance Criteria

- AC-01: `Cargo.toml` specifies `rmcp = { version = "=1.7.0", ... }` and `cargo build --workspace` succeeds with zero errors
- AC-02: `ServerInfo` and `Implementation` construction in `server.rs` uses constructors or builders (no struct literal construction of `#[non_exhaustive]` types)
- AC-03: Test `ClientInfo` construction in `server.rs` test module compiles using builder or constructor
- AC-04: All 8 `ErrorData::invalid_params` call sites in `tools.rs` compile without modification (verified, not assumed)
- AC-05: `LocalSessionManager::default()` compiles and session `keep_alive` defaults to 5 minutes (no regression in session behavior)
- AC-06: UDS transport via `(OwnedReadHalf, OwnedWriteHalf)` tuple compiles and `IntoTransport` blanket impl resolves
- AC-07: Extension propagation validated — `ResolvedIdentity` inserted by `StaticTokenAuth` is accessible in `RequestContext.extensions.get::<http::request::Parts>()` at tool call time (integration test)
- AC-08: `Implementation` includes `.with_description("Self-learning knowledge engine for agentic workflows")` in production `ServerInfo` construction
- AC-09: `HttpConfig` gains `allowed_origins: Vec<String>` field (default empty = no origin restriction), wired through to `StreamableHttpServerConfig.allowed_origins` in `McpAdapter::new()`
- AC-10: `cargo clippy --workspace -- -D warnings` passes with zero warnings
- AC-11: Full test suite (`cargo test --workspace`) passes
- AC-12: `ServerHandler::initialize` override in `server.rs` compiles with unchanged logic (client_type_map population, session key extraction)

## Constraints

- **Exact version pin**: Continue pinning `=1.7.0` (not `^1.7`). ADR-001 rationale (deliberate upgrades) still applies even post-1.0. rmcp's release cadence is ~biweekly.
- **Rust toolchain**: rmcp 1.4+ requires Rust 1.92. Current toolchain is 1.95.0, workspace MSRV is 1.89. If rmcp 1.7 requires >1.89, the workspace `rust-version` must be bumped.
- **Feature flags**: Verify all 6 Cargo features (`server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session`) still exist in 1.7.0. Feature renames would be a compilation blocker.
- **`http` crate version**: rmcp 1.7 may bump its `http` dependency. Current: `http = "1"`. Must remain compatible or be bumped in lockstep.
- **`schemars` version**: rmcp proc macros generate JSON Schema via schemars. Current: `schemars = "1"`. Must remain compatible.
- **No `transport-async-rw` explicit dep**: UDS transport relies on this transitively via `server` feature. Verify transitive enablement unchanged.
- **`allowed_origins` config wiring**: The `HttpConfig` struct uses `#[serde(default)]`. Adding `allowed_origins: Vec<String>` with `Default` = empty vec is backward-compatible with existing `config.toml` files.

## Open Questions

1. **`ServerHandler::initialize` trait signature in 1.7**: Has the return type changed from `impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_`? If it's now `async fn`, the `std::future::ready()` pattern may need adjustment. The research spike did not catalog trait signature changes in the `ServerHandler` trait itself.
2. **rmcp 1.7 MSRV**: The spike confirmed 1.4+ requires Rust 1.92. Does 1.7 raise this further? If it exceeds 1.89 (workspace MSRV), we need a workspace MSRV bump.
3. **`Implementation::new()` constructor availability**: The research spike references `Implementation::new(name, version)` from v1.0.0+. Does it return `Self` directly or a builder? Does `..Default::default()` still work with `#[non_exhaustive]` + `Default` derive?
4. **`allowed_origins` interaction with `allowed_hosts`**: Are these independent checks (both must pass) or alternative (either can pass)? This affects config documentation.
5. **`serve_client` function location**: Test helper uses `rmcp::serve_client`. Has this been renamed or moved in 1.7?

## Tracking

GitHub Issue: #673
