# vnc-023: Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-023/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-023/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-023/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-023/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/vnc-023/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-023/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| cargo-version-bump | pseudocode/cargo-version-bump.md | test-plan/cargo-version-bump.md |
| server-struct-migration | pseudocode/server-struct-migration.md | test-plan/server-struct-migration.md |
| server-test-migration | pseudocode/server-test-migration.md | test-plan/server-test-migration.md |
| config-allowed-origins | pseudocode/config-allowed-origins.md | test-plan/config-allowed-origins.md |
| router-origin-wiring | pseudocode/router-origin-wiring.md | test-plan/router-origin-wiring.md |
| main-call-site | pseudocode/main-call-site.md | test-plan/main-call-site.md |
| initialize-signature | pseudocode/initialize-signature.md | test-plan/initialize-signature.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Upgrade rmcp from `=0.16.0` to `=1.7.0` to resolve CVE-2026-42559 (CVSS 8.8, DNS rebinding on Streamable HTTP transport) and restore MCP protocol compliance (2025-11-25). Bundle two defense-in-depth enhancements in files already being modified: `Implementation` description enrichment and `allowed_origins` configuration for Origin header validation.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Continue exact version pin `=1.7.0` | Deliberate upgrade policy still applies post-1.0; rmcp releases biweekly | ADR-001 (vnc-001, Unimatrix #77) | N/A (existing policy) |
| Compile-first for ServerHandler::initialize signature | Attempt cargo build; if trait changed to `async fn`, fix is mechanical 2-line edit preserving all logic | Architecture SR-03 resolution | architecture/ADR-001-initialize-signature-strategy.md |
| allowed_origins as additive HttpConfig field | Single field, `#[serde(default)]`, empty vec = no restriction, wired through 4-hop config chain | Architecture C5 + ADR-002 | architecture/ADR-002-allowed-origins-config.md |
| Extension propagation integration test | Validate ResolvedIdentity survives rmcp 1.7 processing via existing or new integration test through full HTTP->rmcp->tool chain | Architecture SR-08 resolution | architecture/ADR-003-extension-propagation-test.md |
| All 6 Cargo features verified present in rmcp 1.7 | `server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session` confirmed via `cargo info` | Architecture SR-01 resolution | N/A (verification, not decision) |
| MSRV unaffected | rmcp 1.7.0 does not declare `rust-version`; workspace MSRV 1.89 unchanged | Architecture SR-02 resolution | N/A (verification, not decision) |

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/Cargo.toml` | Modify | Change rmcp version from `=0.16.0` to `=1.7.0`; feature list unchanged |
| `crates/unimatrix-server/src/server.rs` (production) | Modify | Replace `Implementation`/`ServerInfo` struct literals with constructors; add `.with_description()` |
| `crates/unimatrix-server/src/server.rs` (test module) | Modify | Replace `ClientInfo`/`Implementation` struct literals with constructors in test helpers |
| `crates/unimatrix-server/src/server.rs` (initialize) | Modify | Adapt `ServerHandler::initialize` signature if trait changed (compile-driven) |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add `allowed_origins: Vec<String>` field to `HttpConfig` |
| `crates/unimatrix-server/src/http/router.rs` | Modify | Wire `allowed_origins` through `McpAdapter::new()` and `ProjectRouter::new()` to `StreamableHttpServerConfig` |
| `crates/unimatrix-server/src/main.rs` | Modify | Pass `config.http.allowed_origins` to `ProjectRouter::new()` |

## Data Structures

### HttpConfig (modified)

```rust
// crates/unimatrix-server/src/infra/config.rs
#[derive(Deserialize, Default)]
pub struct HttpConfig {
    // ... existing fields ...

    /// Allowed Origin headers for CSRF defense-in-depth.
    /// Empty vec = no origin restriction (backward-compatible default).
    /// Independent of allowed_hosts (Host header / DNS rebinding defense).
    /// Both checks apply independently when configured.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}
```

### Implementation construction (modified pattern)

```rust
// Before (0.16.0 — struct literal):
Implementation {
    name: "unimatrix".to_string(),
    version: env!("CARGO_PKG_VERSION").to_string(),
}

// After (1.7.0 — constructor + builder):
Implementation::new("unimatrix", env!("CARGO_PKG_VERSION"))
    .with_description("Self-learning knowledge engine for agentic workflows")
```

### ServerInfo construction (modified pattern)

```rust
// Before (0.16.0 — struct literal with all fields):
ServerInfo {
    server_info: implementation,
    capabilities: capabilities,
    instructions: Some(instructions),
}

// After (1.7.0 — verify Default + #[non_exhaustive] coexistence):
// Option A: ..Default::default() if permitted externally (unlikely — #[non_exhaustive] blocks this)
// Option B: Use whatever constructor/builder rmcp 1.7 provides
// Compile-driven: let compiler guide the correct pattern
```

## Function Signatures

### McpAdapter::new (modified)

```rust
// Before:
pub fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self

// After:
pub fn new(server: UnimatrixServer, max_body_bytes: usize, allowed_origins: Vec<String>) -> Self
```

### ProjectRouter::new (modified)

```rust
// Before:
pub fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self

// After:
pub fn new(server: UnimatrixServer, max_body_bytes: usize, allowed_origins: Vec<String>) -> Self
```

### ServerHandler::initialize (potentially modified)

```rust
// Current (0.16.0):
fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_

// If changed to async fn in 1.7:
async fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> Result<InitializeResult, ErrorData>
```

## Constraints

1. **Exact version pin `=1.7.0`** — no semver range. ADR-001 deliberate upgrade policy.
2. **6 Cargo features must be preserved** — verified present in 1.7.0.
3. **`http` crate must remain `"1"`** — rmcp 1.7 uses `http ^1`. Compatible.
4. **`schemars` must remain `"1"`** — rmcp 1.7 uses `schemars ^1`. Compatible.
5. **Three-file change boundary** — primary changes in `server.rs`, `router.rs`, `Cargo.toml`. Additional: `config.rs`, `main.rs` for allowed_origins wiring.
6. **Backward-compatible config** — `#[serde(default)]` on `allowed_origins` so existing `config.toml` files parse without error.
7. **No logic changes in initialize** — signature adaptation permitted; internal logic (client_type_map population, session key extraction) must not change.
8. **No changes to tools.rs handler logic** — only compile verification at `ErrorData::invalid_params` sites.
9. **No changes to auth, TLS, or listener infrastructure** — zero rmcp dependency per ADR-003.
10. **`transport-async-rw` not added explicitly** — UDS relies on transitive enablement. Add only if broken.

## Dependencies

### Crate Dependencies

| Dependency | Current | Post-Migration | Notes |
|------------|---------|----------------|-------|
| `rmcp` | `=0.16.0` | `=1.7.0` | Primary change; resolves CVE-2026-42559 |
| `http` | `"1"` (1.4.0) | `"1"` (unchanged) | Must match rmcp's http version for Parts extraction |
| `schemars` | `"1"` (1.2.1) | `"1"` (unchanged) | Must match rmcp's schemars for proc macros |
| `tokio` | existing | unchanged | No version interaction |

### Research Dependencies

| Artifact | Path | Status |
|----------|------|--------|
| ass-065 Findings | product/research/ass-065/FINDINGS.md | Complete |
| ass-065 Future Findings | product/research/ass-065/FINDINGS-FUTURE.md | Complete |

## NOT in Scope

1. **Runtime tool disabling (Opp 9)** — requires `Arc<Mutex<ToolRouter>>` ownership changes
2. **`json_response` mode (Opp 7)** — needs `stateful_mode` interaction evaluation
3. **Trait-based tool declaration (Opp 1)** — 1-2 week refactor of `tools.rs` (~12K lines)
4. **Session persistence (Opp 10)** — `SessionStore` trait, 3-5 day effort
5. **`IntoCallToolResult` adoption (Opp 3)** — 40+ call site return type changes
6. **Auto-generated `get_info` (Opp 2)** — runtime-configurable instructions incompatible with compile-time macro
7. **OAuth 2.0 (Opp 4)** — enterprise repo scope
8. **Elicitation support (Opp 17)** — client support unverified
9. **Any changes to `tools.rs` handler logic** — compile verification only
10. **Any changes to auth, TLS, or listener infrastructure** — zero rmcp coupling
11. **Explicit `transport-async-rw` feature addition** — transitive enablement expected
12. **Config documentation or user-facing changelog** — limited to PR description and in-code comments

## Alignment Status

All 6 alignment checks PASS. No variances detected.

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — CVE resolution and protocol compliance advance the personal-cloud security goal |
| Milestone Fit | PASS — all changes within Vinculum phase scope |
| Scope Gaps | PASS — all 12 ACs fully addressed in specification and architecture |
| Scope Additions | PASS — no material additions beyond SCOPE.md |
| Architecture Consistency | PASS — lean architecture appropriate for dependency upgrade |
| Risk Completeness | PASS — 13 risks mapped, all scope risks traced, 25 test scenarios |

## Implementation Ordering

Single wave, sequential:

1. **C1**: Cargo.toml — bump rmcp to `=1.7.0`
2. **Compile**: `cargo build` to identify all breakage
3. **C2 + C7**: server.rs production — fix Implementation/ServerInfo construction, add description, adapt initialize signature if needed
4. **C3**: server.rs test module — fix ClientInfo/Implementation construction
5. **C5**: config.rs — add `allowed_origins` to HttpConfig
6. **C4**: router.rs — wire `allowed_origins` through McpAdapter::new() and ProjectRouter::new()
7. **C6**: main.rs — pass `config.http.allowed_origins` to ProjectRouter::new()
8. **Verify**: `cargo build`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
9. **Extension propagation**: validate ResolvedIdentity survives rmcp 1.7 processing

## Key Risks

| Risk | Priority | Mitigation |
|------|----------|------------|
| R-01: Extension propagation regression (ResolvedIdentity silently lost) | Critical | Integration test through full HTTP->rmcp->tool chain; test must fail if propagation breaks |
| R-02: ServerHandler::initialize trait signature incompatibility | Critical | Compile-first strategy; mechanical fix if changed (ADR-001) |
| R-03: #[non_exhaustive] struct literal migration logic error | High | Test asserting all ServerInfo fields (capabilities, instructions, description) |
| R-04: allowed_origins config wiring disconnected (4-hop chain) | High | Config deserialization + propagation test |
| R-05: CVE not fully resolved | High | Cargo.lock verification; no code overrides allowed_hosts |
| R-10: http crate version mismatch (TypeId footgun) | High | Covered by R-01 test; `cargo tree -i http` single version |

## PR Notes for Reviewer

1. **keep_alive during long tool execution**: rmcp 1.7 defaults session `keep_alive` to 5 minutes. If a tool call takes longer than 5 minutes (e.g., large embedding batch), the session may be cleaned up mid-execution. Add a test case if feasible — a slow tool mock with >5min simulated duration that verifies session survival. At minimum, document this edge case in the PR description so reviewers are aware of the behavioral change.

2. **allowed_origins vs allowed_hosts**: These are independent checks per ADR-002. Both must pass when both are configured. Document this interaction in the PR description.

## Automatic Improvements (zero code changes)

These improvements come free from the version bump:

- Protocol version 2025-11-25 (v1.5.0)
- Stdio parse resilience: -32700 reply instead of connection close (v1.7.0)
- SSE connection reuse via stream drain (v1.5.0)
- Idle timeout logged at DEBUG instead of ERROR (v1.7.0)
- Init timeout protection: 60s default (v1.6.0)
- Error type constructors for `#[non_exhaustive]` types (v1.5.0)
