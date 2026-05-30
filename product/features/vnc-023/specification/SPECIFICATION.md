# Specification: vnc-023 — rmcp 0.16 to 1.7 Migration

**Feature**: vnc-023
**GitHub Issue**: #673
**Date**: 2026-05-30
**Source**: product/features/vnc-023/SCOPE.md
**Research**: product/research/ass-065/FINDINGS.md, FINDINGS-FUTURE.md

---

## Objective

Upgrade the rmcp dependency from `=0.16.0` to `=1.7.0` to resolve CVE-2026-42559 (CVSS 8.8, DNS rebinding via Host header on Streamable HTTP transport), restore MCP protocol compliance (2025-11-25), and adopt two defense-in-depth enhancements (`Implementation` description enrichment, `allowed_origins` configuration) whose code paths overlap with mandatory migration changes.

---

## Functional Requirements

### FR-01: Cargo dependency version bump

Update `rmcp` in the workspace `Cargo.toml` from `version = "=0.16.0"` to `version = "=1.7.0"`, retaining the exact-pin strategy per ADR-001. All six Cargo features (`server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session`) must be preserved and verified to exist in rmcp 1.7.0.

**Verification**: `cargo build --workspace` succeeds with zero errors.

### FR-02: Non-exhaustive struct literal migration (production)

Replace direct struct literal construction of `ServerInfo` and `Implementation` in `server.rs` (production `get_info()`) with constructor/builder calls compatible with `#[non_exhaustive]`. Specifically:
- `Implementation` must use `Implementation::new(name, version)` or equivalent constructor.
- `ServerInfo` must use constructor, builder, or `..Default::default()` rest syntax if `Default` is derived alongside `#[non_exhaustive]`.

**Verification**: `server.rs` contains no struct literal construction of `#[non_exhaustive]` rmcp types in production code paths. Code compiles.

### FR-03: Non-exhaustive struct literal migration (test)

Replace direct struct literal construction of `ClientInfo` and any other `#[non_exhaustive]` types in the `server.rs` test module with constructor or builder calls.

**Verification**: `cargo test -p unimatrix-server` compiles all test modules.

### FR-04: ErrorData::invalid_params call site compatibility

Verify that all 8 `ErrorData::invalid_params(msg, None)` call sites in `tools.rs` compile without modification against rmcp 1.7.0.

**Verification**: `cargo build -p unimatrix-server` succeeds. No changes required at these 8 sites (if changes are required, they must be made).

### FR-05: LocalSessionManager default compilation

Verify that `LocalSessionManager::default()` in `router.rs` compiles. The new default `keep_alive` of 5 minutes is acceptable and must not be overridden.

**Verification**: `cargo build -p unimatrix-server` succeeds. `LocalSessionManager::default()` call site unchanged.

### FR-06: UDS IntoTransport blanket impl

Verify that the `(OwnedReadHalf, OwnedWriteHalf)` tuple used for UDS transport in `uds/mcp_listener.rs` still resolves the `IntoTransport` blanket impl via the transitively-enabled `transport-async-rw` feature.

**Verification**: `cargo build -p unimatrix-server` succeeds. UDS transport path compiles without explicit `transport-async-rw` feature addition.

### FR-07: Extension propagation integrity

Validate that `ResolvedIdentity` inserted into request extensions by `StaticTokenAuth` middleware survives rmcp 1.7.0 internal processing and is accessible at tool call time via `RequestContext.extensions.get::<http::request::Parts>()`.

**Verification**: Integration test exercises the full chain: HTTP request with bearer token -> `StaticTokenAuth` inserts `ResolvedIdentity` -> rmcp processes request -> tool handler extracts identity from `RequestContext.extensions`. Test asserts non-None identity.

### FR-08: Implementation description enrichment (Opp 20)

Enrich the production `Implementation` construction in `server.rs` with a description field: `.with_description("Self-learning knowledge engine for agentic workflows")` (or equivalent builder method).

**Verification**: MCP `initialize` response contains `serverInfo.implementation.description` with the specified string. Unit or integration test asserts description presence.

### FR-09: Origin validation configuration (Opp 11)

Add `allowed_origins: Vec<String>` field to `HttpConfig` with `#[serde(default)]` (default: empty vec = no origin restriction). Wire the value through to `StreamableHttpServerConfig.allowed_origins` in `McpAdapter::new()`.

**Verification**: (a) Existing `config.toml` files without `allowed_origins` deserialize successfully (backward compatible). (b) When `allowed_origins` is populated, the values are propagated to `StreamableHttpServerConfig`. (c) When empty, no origin restriction is applied (rmcp default behavior).

### FR-10: ServerHandler::initialize override compilation

The `ServerHandler::initialize` override in `server.rs` (which captures `clientInfo.name`, stores in `client_type_map`, extracts session key from `Mcp-Session-Id`) must compile with unchanged logic against rmcp 1.7.0.

If the trait signature has changed (e.g., `async fn` instead of `impl Future`), adapt the method signature while preserving identical behavior.

**Verification**: `cargo build -p unimatrix-server` succeeds. `client_type_map` population logic unchanged. Session key extraction logic unchanged.

### FR-11: Clippy compliance

All code changes must pass `cargo clippy --workspace -- -D warnings` with zero warnings.

**Verification**: Clippy exits with code 0.

### FR-12: Full test suite pass

The complete test suite must pass after migration.

**Verification**: `cargo test --workspace` exits with code 0.

---

## Non-Functional Requirements

### NFR-01: Zero behavioral regression on existing transports

Stdio, UDS, and HTTPS transports must exhibit identical external behavior post-migration. The following behavioral defaults from rmcp 1.4+ are acceptable and expected:
- `allowed_hosts` defaults to localhost (correct for all deployment topologies — reverse proxy connects to localhost).
- Session `keep_alive` defaults to 5 minutes (previously None). Sessions auto-cleanup. No regression for our use.
- Init timeout defaults to 60 seconds (new in 1.6.0). Prevents zombie sessions. No regression.

### NFR-02: Backward-compatible configuration

Existing `config.toml` files must deserialize without error after the `HttpConfig` struct gains `allowed_origins`. The `#[serde(default)]` attribute ensures this.

### NFR-03: Compilation performance

Build time impact of the version bump must be negligible. rmcp is already a dependency; changing version does not add new transitive dependency trees (verify via `cargo tree` diff).

### NFR-04: Dependency version compatibility

The following transitive dependencies must remain compatible with the workspace:
- `http` crate: must remain `http = "1"` or be bumped in lockstep.
- `schemars` crate: must remain `schemars = "1"` or be bumped in lockstep.
- `tokio`, `tower`, `hyper`: no version conflicts.

### NFR-05: MSRV compatibility

If rmcp 1.7.0 requires Rust >1.89 (workspace MSRV), the workspace `rust-version` field must be bumped accordingly. The current toolchain (1.95.0) satisfies rmcp 1.4+'s Rust 1.92 requirement. If MSRV bump is needed, it is a scope-internal change, not a scope expansion.

### NFR-06: Security posture

CVE-2026-42559 must be fully resolved by the upgrade. DNS rebinding via Host header on Streamable HTTP transport must be blocked by rmcp's default `allowed_hosts` validation (localhost). No additional application-level mitigation required.

---

## Acceptance Criteria

All AC-IDs traced from SCOPE.md. Verification methods specified for each.

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `Cargo.toml` specifies `rmcp = { version = "=1.7.0", ... }` and `cargo build --workspace` succeeds with zero errors | Compile gate: `cargo build --workspace` exit code 0. Inspect `Cargo.toml` for exact version string. Verify all 6 feature flags present. |
| AC-02 | `ServerInfo` and `Implementation` construction in `server.rs` uses constructors or builders (no struct literal construction of `#[non_exhaustive]` types) | Code review: grep for `ServerInfo {` and `Implementation {` in production code — zero matches. Compile gate. |
| AC-03 | Test `ClientInfo` construction in `server.rs` test module compiles using builder or constructor | Compile gate: `cargo test -p unimatrix-server --no-run`. Code review: no struct literal construction of `ClientInfo`. |
| AC-04 | All 8 `ErrorData::invalid_params` call sites in `tools.rs` compile without modification (verified, not assumed) | Compile gate: `cargo build -p unimatrix-server`. Diff shows zero changes to `ErrorData::invalid_params` call sites. |
| AC-05 | `LocalSessionManager::default()` compiles and session `keep_alive` defaults to 5 minutes (no regression in session behavior) | Compile gate. Behavioral: existing integration tests pass without session timeout failures. |
| AC-06 | UDS transport via `(OwnedReadHalf, OwnedWriteHalf)` tuple compiles and `IntoTransport` blanket impl resolves | Compile gate: `cargo build -p unimatrix-server`. UDS code path compiles without adding explicit `transport-async-rw` feature. |
| AC-07 | Extension propagation validated — `ResolvedIdentity` inserted by `StaticTokenAuth` is accessible in `RequestContext.extensions.get::<http::request::Parts>()` at tool call time | Integration test: test sends authenticated HTTP request, tool handler extracts and asserts `ResolvedIdentity` presence. Test must fail if propagation breaks. |
| AC-08 | `Implementation` includes `.with_description("Self-learning knowledge engine for agentic workflows")` in production `ServerInfo` construction | Code review: description string present in `server.rs`. Integration or unit test: `get_info()` result contains expected description. |
| AC-09 | `HttpConfig` gains `allowed_origins: Vec<String>` field (default empty = no origin restriction), wired through to `StreamableHttpServerConfig.allowed_origins` in `McpAdapter::new()` | Code review: field present in `HttpConfig` with `#[serde(default)]`. Unit test: config without `allowed_origins` deserializes. Unit test: config with `allowed_origins` propagates to `StreamableHttpServerConfig`. |
| AC-10 | `cargo clippy --workspace -- -D warnings` passes with zero warnings | CI gate: clippy exit code 0. |
| AC-11 | Full test suite (`cargo test --workspace`) passes | CI gate: test exit code 0. |
| AC-12 | `ServerHandler::initialize` override in `server.rs` compiles with unchanged logic (client_type_map population, session key extraction) | Compile gate. Code review: diff shows no logic changes to initialize body (signature adaptation is permitted if trait changed). |

---

## Domain Models

### Key Entities

| Term | Definition |
|------|-----------|
| **rmcp** | The Rust MCP SDK crate (`rmcp` on crates.io). Provides `ServerHandler` trait, model types, transport implementations, and proc macros for MCP protocol compliance. |
| **Implementation** | rmcp model type representing server identity in MCP handshake (`name`, `version`, `description`). Returned in `initialize` response as `serverInfo.implementation`. Marked `#[non_exhaustive]` since v1.0.0-alpha. |
| **ServerInfo** | rmcp model type wrapping `Implementation` plus `ServerCapabilities`. Returned by `ServerHandler::get_info()`. Marked `#[non_exhaustive]`. |
| **ServerHandler** | rmcp trait that `UnimatrixServer` implements. Defines `get_info()`, `initialize()`, and tool routing. The primary integration surface between Unimatrix and rmcp. |
| **StreamableHttpServerConfig** | rmcp config type for the Streamable HTTP transport. Controls `allowed_hosts`, `allowed_origins`, `json_response`, `init_timeout`. Constructed in `McpAdapter::new()`. |
| **LocalSessionManager** | rmcp session manager for single-instance deployments. Manages MCP session lifecycle with configurable `keep_alive` timeout. |
| **McpAdapter** | Unimatrix's adapter in `router.rs` that bridges the HTTP listener stack to rmcp's `StreamableHttpService`. All rmcp transport coupling is concentrated here (~40 lines). |
| **ResolvedIdentity** | Unimatrix type inserted into HTTP request extensions by `StaticTokenAuth`. Must survive rmcp internal processing to reach tool handlers via `RequestContext.extensions`. |
| **HttpConfig** | Unimatrix's configuration struct for HTTPS transport settings (port, TLS, auth, and now `allowed_origins`). Deserialized from `config.toml`. |
| **CVE-2026-42559** | DNS rebinding vulnerability (CVSS 8.8) in rmcp <1.4.0. Attacker on local network can rebind a malicious domain to localhost, bypassing same-origin protections to invoke MCP tools with victim's bearer token. Fixed by Host header validation in rmcp 1.4.0+. |
| **#[non_exhaustive]** | Rust attribute preventing struct literal construction outside the defining crate. Applied to ~14 rmcp types in v1.0.0-alpha. Forces use of constructors/builders. |

### Relationships

- `UnimatrixServer` **implements** `ServerHandler` (rmcp trait)
- `McpAdapter` **wraps** `StreamableHttpService<UnimatrixServer, LocalSessionManager>`
- `HttpConfig` **configures** `StreamableHttpServerConfig` (via `McpAdapter::new()`)
- `StaticTokenAuth` **inserts** `ResolvedIdentity` **into** HTTP extensions **consumed by** `RequestContext.extensions`
- `ServerHandler::get_info()` **returns** `ServerInfo` **containing** `Implementation`

---

## User Workflows

### Workflow 1: Developer upgrades rmcp (this feature)

1. Update `Cargo.toml` version pin from `=0.16.0` to `=1.7.0`
2. Run `cargo build --workspace` — observe compilation failures at struct literal sites
3. Fix `Implementation` and `ServerInfo` construction using constructors/builders
4. Fix `ClientInfo` construction in test module
5. Add `.with_description()` to `Implementation` construction (FR-08)
6. Add `allowed_origins` to `HttpConfig` and wire to `StreamableHttpServerConfig` (FR-09)
7. Run `cargo test --workspace` — verify all tests pass
8. Run `cargo clippy --workspace -- -D warnings` — verify zero warnings
9. Validate extension propagation via integration test (FR-07)

### Workflow 2: Operator configures origin validation (post-upgrade)

1. Edit `config.toml`, add `allowed_origins = ["https://claude.ai", "vscode-webview://..."]` under `[http]`
2. Restart Unimatrix server
3. Requests from listed origins succeed; requests from unlisted origins are rejected by rmcp

### Workflow 3: MCP client observes enriched server metadata (post-upgrade)

1. Client sends `initialize` request
2. Server responds with `serverInfo.implementation` containing `name`, `version`, and `description`
3. Client displays or logs the description ("Self-learning knowledge engine for agentic workflows")

---

## Constraints

### C-01: Exact version pin

Continue pinning `=1.7.0` (not `^1.7`). ADR-001 rationale (deliberate upgrades for a rapidly-releasing dependency) still applies. rmcp releases approximately biweekly.

### C-02: Feature flag existence (SR-01)

All 6 Cargo features must exist in rmcp 1.7.0. A renamed or removed feature is a compilation blocker. Verify before implementation begins. If any feature is missing, escalate — scope estimate is invalid.

### C-03: ServerHandler trait compatibility (SR-03)

If `ServerHandler::initialize` trait signature changed (e.g., `async fn` vs `impl Future`), adapt the method signature. The internal logic (client_type_map population, session key extraction) must not change. Signature adaptation is in-scope; logic changes are out-of-scope.

### C-04: http crate version lockstep (SR-04)

`http::request::Parts` extraction from `RequestContext.extensions` depends on Unimatrix and rmcp using the same `http` crate major version. If rmcp 1.7 bumps `http` beyond `"1"`, bump in lockstep.

### C-05: schemars compatibility (SR-07)

rmcp proc macros depend on `schemars` for JSON Schema generation. Verify no version conflict with the workspace's `schemars = "1"`.

### C-06: MSRV floor (SR-02, SR-06)

rmcp 1.4+ requires Rust 1.92. Workspace MSRV is 1.89. If rmcp 1.7 requires >1.89, bump workspace `rust-version`. Current toolchain (1.95.0) is sufficient. Document the MSRV bump in the PR if triggered.

### C-07: Backward-compatible config deserialization

The `allowed_origins` field on `HttpConfig` must use `#[serde(default)]` so existing `config.toml` files without this field deserialize without error.

### C-08: Extension propagation regression guard (SR-08)

Extension propagation (ResolvedIdentity through rmcp internals) is the highest integration risk. An integration test must be added or verified to cover this path. The test must fail if rmcp changes break extension propagation.

### C-09: Three-file change boundary

Per ass-065 analysis, the migration primarily affects 3 files: `server.rs`, `router.rs`, `Cargo.toml`. Changes outside these files should be limited to: (a) transitive compilation fixes in test modules, (b) `HttpConfig` struct definition (likely in a config module). If changes spread beyond this boundary, reassess effort estimate.

### C-10: allowed_origins vs allowed_hosts interaction (SR-05, SR-06)

`allowed_origins` (Origin header) and `allowed_hosts` (Host header) are independent checks in rmcp — both are validated when configured. The architect must confirm this from rmcp source. Config documentation must clarify that both checks apply independently.

---

## Dependencies

### Crate Dependencies

| Dependency | Current | Post-Migration | Notes |
|------------|---------|----------------|-------|
| `rmcp` | `=0.16.0` | `=1.7.0` | Primary change |
| `http` | `"1"` | `"1"` (verify) | Must match rmcp's `http` version |
| `schemars` | `"1"` | `"1"` (verify) | Must match rmcp's `schemars` version |
| `tokio` | existing | unchanged | No version interaction |

### Existing Components

| Component | Interaction | Risk |
|-----------|------------|------|
| `server.rs` (UnimatrixServer) | `ServerHandler` impl, `get_info()`, `initialize()` | High — struct literal fixes, trait compatibility |
| `router.rs` (McpAdapter) | `StreamableHttpServerConfig`, `LocalSessionManager` | Medium — behavioral defaults, origin config wiring |
| `Cargo.toml` | Version pin, feature flags | Low — single line change |
| `tools.rs` | `ErrorData::invalid_params` (8 sites) | Low — verify-only, no changes expected |
| `uds/mcp_listener.rs` | `IntoTransport` blanket impl | Low — verify-only |
| `http/auth.rs` (StaticTokenAuth) | Extension propagation source | None — no rmcp dependency |
| `HttpConfig` (config module) | New `allowed_origins` field | Low — additive change |

### Research Dependencies

| Artifact | Status | Usage |
|----------|--------|-------|
| ass-065 FINDINGS.md | Complete | API inventory, breaking change catalog, impact matrix |
| ass-065 FINDINGS-FUTURE.md | Complete | Opp 11 and Opp 20 details, 1.5-1.7 changelog |

---

## NOT in Scope

These items are explicitly excluded to prevent scope creep:

1. **Runtime tool disabling (Opp 9)**: Requires `Arc<Mutex<ToolRouter>>` ownership changes. Separate follow-on feature.
2. **`json_response` mode (Opp 7)**: Needs evaluation of `stateful_mode` interaction with `client_type_map`. Separate evaluation.
3. **Trait-based tool declaration (Opp 1)**: 1-2 week refactor splitting `tools.rs` (~12K lines). Separate design session.
4. **Session persistence (Opp 10)**: `SessionStore` trait implementation. 3-5 day effort. Separate feature.
5. **`IntoCallToolResult` adoption (Opp 3)**: Return type changes across 40+ call sites. Revisit with trait-based tools.
6. **Auto-generated `get_info` (Opp 2)**: Our manual `get_info()` uses runtime-configurable instructions. Macro requires compile-time literals.
7. **OAuth 2.0 (Opp 4)**: Enterprise repo scope (W2-3).
8. **Elicitation support (Opp 17)**: Client support not verified.
9. **Any changes to `tools.rs` handler logic**: Only compile verification at `ErrorData::invalid_params` sites. No refactoring.
10. **Any changes to auth, TLS, or listener infrastructure**: These have zero rmcp dependency per ADR-003.
11. **`transport-async-rw` explicit feature addition**: UDS relies on transitive enablement. If it breaks, add explicitly — but do not add preemptively.
12. **Config documentation or user-facing changelog**: This is a dependency upgrade. Documentation is limited to PR description and in-code comments.

---

## Open Questions

1. **`ServerHandler::initialize` trait signature in 1.7**: Has the return type changed from `impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_` to `async fn`? If so, the `std::future::ready()` pattern needs mechanical adaptation. The architect must verify this from rmcp 1.7 source before implementation begins.

2. **`Implementation::new()` constructor shape**: Does `Implementation::new(name, version)` return `Self` directly, or does it return a builder? Does `..Default::default()` work with `#[non_exhaustive]` + `Default` derive? The architect must verify constructor availability.

3. ~~**`allowed_origins` vs `allowed_hosts` interaction semantics**~~: **Resolved by ADR-002** — both are checked independently by rmcp. A request must pass both Host header validation (`allowed_hosts`) and Origin header validation (`allowed_origins`) when both are configured. See `architecture/ADR-002-allowed-origins-config.md`.

4. **`serve_client` test helper location**: Integration tests use `rmcp::serve_client`. Has this been renamed or moved in 1.7? If so, test infrastructure needs updating (in-scope as a transitive fix).

5. **rmcp 1.7 MSRV**: Does 1.7 raise the minimum Rust version beyond 1.92 (required by 1.4)? If it exceeds workspace MSRV 1.89, a workspace MSRV bump is needed (in-scope per C-06).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- 20 entries returned; relevant: #77 (ADR-001 rmcp 0.16 with stdio), #4356 (vnc-014 ServerHandler initialize), #4355 (vnc-014 clientInfo capture), #1913 (vnc-005 UnimatrixServer sharing), #317 (MCP handler pipeline pattern). These informed domain model definitions and constraint articulation.
- Queried: `mcp__unimatrix__context_search` (pattern, convention) -- relevant: #4452 (gate-fix test pattern informed AC-07 verification method), #1369 (MCP tool handler pipeline informed FR-07 extension chain).
