# vnc-023: rmcp 0.16 to 1.7 Migration Architecture

## System Overview

vnc-023 is a dependency upgrade of the `rmcp` crate from `=0.16.0` to `=1.7.0` in `unimatrix-server`. It resolves CVE-2026-42559 (DNS rebinding, CVSS 8.8) and bundles two opportunistic enhancements in files already being modified: Implementation description enrichment (Opp 20) and origin header validation config (Opp 11).

The upgrade affects 3 files with code changes. ADR-003's McpAdapter isolation boundary (Unimatrix entry #4699) concentrates all rmcp transport coupling in ~100 lines. Of ~100+ rmcp call sites across 18 files, only struct literal constructions break from `#[non_exhaustive]` additions. The remaining ~90% of call sites compile without modification.

This is a dependency upgrade, not a feature build. The architecture is correspondingly lean.

## Scope Risk Resolution

### SR-01 (High): Cargo Feature Flags -- RESOLVED

All 6 required features verified present in rmcp 1.7.0 via `cargo info rmcp@1.7.0`:

| Feature | Status in 1.7.0 |
|---------|----------------|
| `server` | Present |
| `client` | Present |
| `transport-io` | Present |
| `macros` | Present |
| `transport-streamable-http-server` | Present |
| `transport-streamable-http-server-session` | Present |

Additionally, `transport-async-rw` (used transitively for UDS) remains enabled by `server`. No feature renames occurred.

### SR-02 (Med): MSRV -- RESOLVED

rmcp 1.7.0 reports `rust-version: unknown` (not specified in its Cargo.toml). Research spike confirmed rmcp 1.4+ requires Rust 1.92. Current toolchain is 1.95.0, workspace MSRV is 1.89. Since rmcp does not declare its own `rust-version` field, the workspace MSRV is unaffected. If compilation fails due to language features, the MSRV bump is a 1-line change in workspace `Cargo.toml` plus CI config.

### SR-03 (High): ServerHandler::initialize Trait Signature -- DESIGN DECISION

The current pattern uses `impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_` with `std::future::ready()`. Two scenarios:

1. **Trait signature unchanged** (return position impl trait): Our code compiles as-is.
2. **Trait changed to `async fn`**: Our `std::future::ready()` call still works -- `async fn` desugars to the same return position impl trait. The compiler accepts a `std::future::ready()` return in an `async fn` body replacement. However, the function signature must match.

**Decision**: Attempt compilation first. If the trait signature changed, the fix is mechanical: change `fn initialize(...) -> impl Future<...>` to `async fn initialize(...)` and replace `std::future::ready(Ok(self.get_info()))` with `Ok(self.get_info())`. See ADR-001-initialize-signature-strategy.md.

### SR-04 (Med): `http` crate version -- RESOLVED

Current: `http = "1"` (locked to 1.4.0). rmcp 1.7.0's `server-side-http` feature depends on `dep:http` (semver-compatible). No version mismatch expected. The `http::request::Parts` type used for extension propagation will remain compatible.

### SR-07 (Med): `schemars` version -- RESOLVED

Current: `schemars = "1"` (locked to 1.2.1). rmcp 1.7.0's `server` feature depends on `dep:schemars` (semver-compatible). No conflict.

### SR-08 (High): Extension Propagation -- INTEGRATION TEST STRATEGY

See ADR-003-extension-propagation-test.md for the test design.

## Component Breakdown

### C1: Cargo.toml Version Bump

**File**: `crates/unimatrix-server/Cargo.toml` (line 33)
**Responsibility**: Pin rmcp to `=1.7.0`
**Change**: `rmcp = { version = "=1.7.0", features = [...] }` -- features list unchanged.

### C2: ServerInfo / Implementation Construction (server.rs)

**File**: `crates/unimatrix-server/src/server.rs` (lines 274-287)
**Responsibility**: Fix `#[non_exhaustive]` breakage on `ServerInfo` and `Implementation` struct literals. Add description enrichment (Opp 20).
**Change**:
- `Implementation { name, version, ..Default::default() }` becomes `Implementation::new(name, version).with_description(desc)`
- `ServerInfo { server_info, capabilities, instructions, ..Default::default() }` -- verify `..Default::default()` still works with `#[non_exhaustive]` + `Default` derive. If not, use field-by-field construction with explicit defaults.

### C3: Test ClientInfo Construction (server.rs test module)

**File**: `crates/unimatrix-server/src/server.rs` (lines 3257-3266)
**Responsibility**: Fix `#[non_exhaustive]` breakage on `ClientInfo` and `Implementation` in test helper.
**Change**: Same pattern as C2 for `Implementation`. For `ClientInfo`: use constructor or builder if available; if `ClientInfo` also gained `#[non_exhaustive]`, cannot use struct literal.

### C4: McpAdapter Config Wiring (router.rs)

**File**: `crates/unimatrix-server/src/http/router.rs` (lines 384-397)
**Responsibility**: Wire `allowed_origins` from `HttpConfig` through to `StreamableHttpServerConfig`. Validate behavioral defaults.
**Change**:
- `McpAdapter::new()` gains `allowed_origins: Vec<String>` parameter
- `ProjectRouter::new()` gains `allowed_origins: Vec<String>` parameter (pass-through)
- `StreamableHttpServerConfig::default()` then set `.allowed_origins` if rmcp 1.7 exposes it as a field or builder method

### C5: HttpConfig Extension (config.rs)

**File**: `crates/unimatrix-server/src/infra/config.rs` (lines 1669-1696)
**Responsibility**: Add `allowed_origins: Vec<String>` field to `HttpConfig`. Default: empty vec (no origin restriction -- backward compatible).
**Change**: Add field with `#[serde(default)]` on the struct (already present). Default impl returns empty vec.

### C6: Main.rs Call Site Update

**File**: `crates/unimatrix-server/src/main.rs` (line 843)
**Responsibility**: Pass `config.http.allowed_origins` through `ProjectRouter::new()` to `McpAdapter::new()`.
**Change**: Add parameter to `ProjectRouter::new(server, max_body_bytes, config.http.allowed_origins.clone())`.

### C7: ServerHandler::initialize (server.rs)

**File**: `crates/unimatrix-server/src/server.rs` (lines 1038-1096)
**Responsibility**: Verify trait signature compatibility. Fix if needed.
**Change**: Compile-driven. See SR-03 above.

## Component Interactions

```
main.rs
  |-- reads config.http.allowed_origins
  |-- passes to ProjectRouter::new(server, max_body_bytes, allowed_origins)
        |
        ProjectRouter::new()
          |-- passes to McpAdapter::new(server, max_body_bytes, allowed_origins)
                |
                McpAdapter::new()
                  |-- builds StreamableHttpServerConfig::default()
                  |-- sets config.allowed_origins = allowed_origins
                  |-- constructs StreamableHttpService with config
```

Data flow for the version bump is simpler -- change Cargo.toml, fix struct literals, compile.

## Technology Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| Continue exact version pin `=1.7.0` | ADR-001 (vnc-001, entry #77) | Deliberate upgrade policy still applies post-1.0. rmcp releases biweekly. |
| Compile-first approach for initialize signature | ADR-001-initialize-signature-strategy | Two possible fixes are both mechanical. Let compiler drive. |
| Extension propagation integration test | ADR-003-extension-propagation-test | SR-08 requires explicit validation, not assumption. |
| `allowed_origins` as additive config field | ADR-002-allowed-origins-config | Backward-compatible, defense-in-depth, independent of `allowed_hosts`. |

## Integration Points

| Dependency | Current | After | Risk |
|------------|---------|-------|------|
| `rmcp` | `=0.16.0` | `=1.7.0` | Primary change. 11 breaking changes, 3-4 affect us. |
| `http` | `"1"` (1.4.0) | `"1"` (unchanged) | rmcp 1.7 uses `http ^1`. Compatible. |
| `schemars` | `"1"` (1.2.1) | `"1"` (unchanged) | rmcp 1.7 uses `schemars ^1`. Compatible. |
| Rust toolchain | 1.95.0 | 1.95.0 (unchanged) | rmcp 1.7 MSRV unknown but <= 1.92. |

## Integration Surface

| Integration Point | Type/Signature | Source | Change |
|-------------------|---------------|--------|--------|
| `Implementation::new(name, version)` | `fn new(String, String) -> Self` | rmcp 1.0.0+ | Replaces struct literal |
| `Implementation::with_description(desc)` | `fn with_description(String) -> Self` | rmcp 1.0.0+ | New: Opp 20 enrichment |
| `ServerInfo { server_info, capabilities, instructions, ..Default::default() }` | struct construction | rmcp | Verify `Default` + `#[non_exhaustive]` coexistence |
| `StreamableHttpServerConfig.allowed_origins` | `Vec<String>` field | rmcp 1.6.0 | New: Opp 11 config wiring |
| `StreamableHttpServerConfig.allowed_hosts` | `Vec<String>` field (default: localhost) | rmcp 1.4.0 | Behavioral: DNS rebinding fix (CVE) |
| `LocalSessionManager::default()` | `fn default() -> Self` | rmcp | Gains `keep_alive: 5min` default |
| `HttpConfig.allowed_origins` | `Vec<String>` field (default: empty) | unimatrix-server config.rs | New field for Opp 11 |
| `McpAdapter::new(server, max_body_bytes, allowed_origins)` | constructor | router.rs | Gains `allowed_origins` param |
| `ProjectRouter::new(server, max_body_bytes, allowed_origins)` | constructor | router.rs | Gains `allowed_origins` param |
| `ServerHandler::initialize` | `fn initialize(&self, InitializeRequestParams, RequestContext<RoleServer>) -> impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_` | rmcp trait | Verify unchanged or adapt |
| `ErrorData::invalid_params(msg, None)` | `fn invalid_params(String, Option<Value>) -> Self` | rmcp | 8 call sites -- verify exists in 1.7 |
| `rmcp::serve_client(client_info, transport)` | test helper | rmcp | Verify exists in 1.7 |

## Implementation Ordering

Single wave, sequential within:

1. **Cargo.toml**: bump version (C1)
2. **Compile**: identify all breakage
3. **server.rs production code**: fix `Implementation`/`ServerInfo` construction + add description (C2, C7)
4. **server.rs test code**: fix `ClientInfo`/`Implementation` construction (C3)
5. **config.rs**: add `allowed_origins` field to `HttpConfig` (C5)
6. **router.rs**: wire `allowed_origins` through `McpAdapter::new()` (C4)
7. **main.rs**: pass `allowed_origins` from config (C6)
8. **Verify**: `cargo build`, `cargo clippy`, `cargo test`
9. **Extension propagation test**: validate ResolvedIdentity survives (SR-08)

## Open Questions

1. **`ServerInfo` construction with `..Default::default()`**: Does `#[non_exhaustive]` + `Default` derive permit the `..Default::default()` pattern within the same crate? It does NOT for external crates -- the implementer must verify whether rmcp provides a builder or if all fields must be set explicitly. If `..Default::default()` fails, the fix is to use whatever builder/constructor rmcp provides.
2. **`allowed_origins` vs `allowed_hosts` interaction semantics**: Are these independent checks (both must pass) or alternative (either suffices)? The implementer should check rmcp source. For config documentation purposes, describe them as independent layers: `allowed_hosts` validates the Host header (DNS rebinding), `allowed_origins` validates the Origin header (CSRF). Both default to permissive when empty/localhost.
3. **`serve_client` location in 1.7**: Test helper `rmcp::serve_client(...)` may have been renamed or moved. Compilation will reveal this. Fix is mechanical (update import path).
