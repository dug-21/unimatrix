# FINDINGS: rmcp 0.16→1.4 Migration — API Surface and Transport Impact

**Spike**: ass-065
**Tracking**: #666
**Date**: 2026-05-29
**Approach**: investigation + evaluation
**Confidence**: validated
**Tracks**: Internal (Q1, Q4) + External (Q2, Q6) → Synthesis (Q3, Q5)

---

## Findings

### Q1: What rmcp APIs, types, traits, and features does `unimatrix-server` use?

**Answer**: 6 Cargo features, ~100+ call sites across 18 source files plus test modules. Integration surface: 3 traits/impls, 2 proc-macro attributes, 10 model types, 3 transport types, 3 service types, 1 top-level function.

**Evidence**: File-by-file audit of every `use rmcp` and `rmcp::` reference in `crates/unimatrix-server/src/`.

#### Cargo Features

```toml
rmcp = { version = "=0.16.0", features = [
    "server",
    "client",
    "transport-io",
    "macros",
    "transport-streamable-http-server",
    "transport-streamable-http-server-session",
] }
```

Note: `transport-async-rw` is used transitively (enabled by `server`) for UDS transport via `(OwnedReadHalf, OwnedWriteHalf)` tuple.

#### Complete Type/Trait/Function Inventory

| Category | Items | Call Sites | Key Files |
|----------|-------|------------|-----------|
| **Proc-macro attributes** | `#[tool_handler]`, `#[tool_router]`, `#[tool]` | 16 | `server.rs`, `tools.rs` |
| **Traits implemented** | `ServerHandler` | 1 | `server.rs` |
| **Model types** | `ErrorData`, `CallToolResult`, `Content`, `RawContent`, `ErrorCode`, `Implementation`, `ServerCapabilities`, `ServerInfo`, `InitializeRequestParams`, `InitializeResult` | ~100+ | 18 files |
| **Service types** | `RequestContext<RoleServer>`, `ServiceExt` | 17 | `tools.rs` (14), `main.rs`, `uds/mcp_listener.rs`, test |
| **Transport types** | `StreamableHttpService`, `LocalSessionManager`, `StreamableHttpServerConfig`, `stdio()` | 4 | `router.rs`, `main.rs` |
| **Handler/router types** | `ToolRouter<Self>`, `Parameters<T>` | 16 | `server.rs`, `tools.rs` |
| **Lifecycle API** | `.serve()`, `.cancellation_token()`, `.waiting()` | 6 | `main.rs`, `uds/mcp_listener.rs` |
| **Test-only types** | `ClientInfo`, `ClientCapabilities`, `ProtocolVersion`, `serve_client` | 4 | `server.rs` (test module) |

#### Error Conversion Surface

Two bridge points:
- `From<ServerError> for ErrorData` in `error.rs` — ~40+ call sites via `.map_err(rmcp::ErrorData::from)?`
- `From<ServiceError> for rmcp::ErrorData` in `services/mod.rs`
- `ErrorData::new(code, message, None)` — 30+ call sites
- `ErrorData::invalid_params(msg, None)` — 8 call sites

#### File-by-File Import Summary

| File | rmcp Imports |
|------|-------------|
| `server.rs` | `ToolRouter`, `Implementation`, `ServerCapabilities`, `ServerInfo`, `ServerHandler`, `#[tool_handler]`, `RequestContext<RoleServer>`, `InitializeRequestParams`, `InitializeResult`, `ErrorData` |
| `main.rs` | `ServiceExt`, `transport::io::stdio()` |
| `error.rs` | `ErrorCode`, `ErrorData` |
| `http/router.rs` | `LocalSessionManager`, `StreamableHttpServerConfig`, `StreamableHttpService` |
| `mcp/tools.rs` | `Parameters`, `CallToolResult`, `#[tool_router]`, `#[tool]`, `RequestContext<RoleServer>`, `ErrorData`, `Content`, `RawContent` |
| `mcp/graph_read.rs` | `CallToolResult`, `ErrorData`, `Content` |
| `mcp/graph_read_*.rs` (6 files) | `ErrorData` |
| `mcp/response/*.rs` (5 files) | `CallToolResult`, `Content`, `ErrorData` (some) |
| `services/mod.rs` | `ErrorData` (via `From<ServiceError>` impl) |
| `test_support.rs` | `RawContent` |
| `uds/mcp_listener.rs` | `ServiceExt` |

---

### Q2: What breaking changes occurred in rmcp between 0.16.0 and 1.4.0?

**Answer**: 87 commits across 8 releases. 11 breaking changes in 5 categories.

**Evidence**: GitHub releases API, PR bodies, source code comparison between `rmcp-v0.16.0` and `rmcp-v1.4.0` tags.

#### Breaking Changes Catalog

| ID | Change | Version | Category |
|----|--------|---------|----------|
| B1 | `StreamableHttpClient` trait gained `custom_headers` parameter | 0.17.0 | API signature |
| B2 | Auth token exchange return type changed to `StandardTokenResponse` | 1.0.0-alpha | API signature |
| B3 | Builder `with_*` methods take `T` not `Option<T>` | 1.0.0 | API signature |
| B4 | `StreamableHttpServerConfig` gained `allowed_hosts` field | 1.4.0 | API signature |
| B5 | `StreamableHttpServerConfig` gained `json_response` field | 0.17.0 | API signature |
| B6 | `StreamableHttpService` lost default type parameter for `M` | 1.3.0 | API signature |
| B7 | `#[non_exhaustive]` added to ~14 model/transport types | 1.0.0-alpha | Struct construction |
| B8 | Initialized notification gate removed; `ExpectedInitializedNotification` variant removed | 1.4.0 | Behavioral |
| B9 | Default session `keep_alive` changed from `None` to 5 minutes | 1.4.0 | Behavioral |
| B10 | DNS rebinding Host header validation enabled by default | 1.4.0 | Behavioral |
| B11 | `Send+Sync` bounds cfg-gated behind `local` feature | 1.3.0 | Trait bounds |

#### Release Timeline

| Version | Date | Breaking Changes |
|---------|------|-----------------|
| 0.17.0 | 2026-02-27 | B1, B5 |
| 1.0.0-alpha | 2026-03-03 | B2, B7 |
| 1.0.0 | 2026-03-03 | B3 |
| 1.1.0 | 2026-03-04 | None |
| 1.1.1 | 2026-03-09 | None |
| 1.2.0 | 2026-03-11 | None |
| 1.3.0 | 2026-03-24 | B6, B11 |
| 1.4.0 | 2026-04-09 | B4, B8, B9, B10 |

---

### Q3: Which of our used APIs are affected by breaking changes, and how?

**Answer**: Of ~100+ call sites across 18 files, **3-4 call sites require mandatory fixes** (all struct literal constructions in `server.rs`), **8 call sites need verification** (`ErrorData::invalid_params`), **2 behavioral defaults need review**, and **~90 call sites are unaffected**.

**Evidence**: Cross-reference of Q1 inventory against Q2 catalog, validated against source code.

#### Impact Matrix

| Our API Usage | File | Sites | Breaking Change | Status | Action |
|---------------|------|-------|-----------------|--------|--------|
| `ServerInfo { server_info: Implementation { .. }, ... }` | `server.rs:274-287` | 1 | B7 (`#[non_exhaustive]`) | **BREAKS** | Switch to builder/constructor |
| `Implementation { name: ..., version: ..., ..Default::default() }` | `server.rs:275-279` | 1 | B7 | **BREAKS** | Switch to builder/constructor |
| `Implementation { ... }` (test) | `server.rs:3261-3264` | 1 | B7 | **BREAKS** | Switch to builder/constructor |
| `ClientInfo { ... }` (test) | `server.rs:3257-3266` | 1 | B7 | **BREAKS** | Use builder (test-only, lower priority) |
| `StreamableHttpServerConfig::default()` | `router.rs:268` | 1 | B4, B5 | **BEHAVIORAL** | `allowed_hosts` defaults to localhost — correct for our use. Validate deployment topology. |
| `LocalSessionManager::default()` | `router.rs:267` | 1 | B7, B9 | **VERIFY** | Confirm `Default` still derived. Sessions now auto-cleanup at 5 min. |
| `ErrorData::invalid_params(msg, None)` | `tools.rs` | 8 | Unconfirmed | **VERIFY** | Check method exists in 1.4.0 |
| `StreamableHttpService<S, LocalSessionManager>` type | `router.rs:251` | 1 | B6 | **SAFE** | Already explicit — removing default has no effect |
| `#[tool_handler]`, `#[tool_router]`, `#[tool]` | `server.rs`, `tools.rs` | 16 | None | **SAFE** | No change |
| `ServerHandler` trait impl | `server.rs` | 1 | None | **SAFE** | No change |
| `ErrorData::new(code, msg, None)` | 8+ files | 30+ | None | **SAFE** | No change |
| `ErrorData::from(ServerError)` | 18 files | 40+ | None | **SAFE** | No change (our `From` impl) |
| `CallToolResult::success/error(vec![...])` | 6+ files | ~20 | None | **SAFE** | No change |
| `Content::text(string)` | 6+ files | ~15 | None | **SAFE** | No change |
| `Parameters(params): Parameters<T>` | `tools.rs` | 14 | None | **SAFE** | No change |
| `RequestContext<RoleServer>` | `tools.rs`, `server.rs` | 15 | None | **SAFE** | No change |
| `ServiceExt::serve()` | `main.rs`, `uds/mcp_listener.rs` | 3 | None | **SAFE** | No change |
| `transport::io::stdio()` | `main.rs` | 1 | None | **SAFE** | No change |
| `RunningService` methods | `main.rs`, `uds/mcp_listener.rs` | 4 | None | **SAFE** | No change |
| `ServerCapabilities::builder()...build()` | `server.rs` | 1 | B7 | **SAFE** | Already uses builder |

#### Breaking Changes That Do NOT Affect Us

| Change | Why |
|--------|-----|
| B1: `StreamableHttpClient` trait | We don't implement this trait (server-side only) |
| B2: Auth token exchange | We don't use the `auth` feature |
| B3: `with_*` methods take `T` | We don't call any `with_*` builders |
| B11: `Send+Sync` cfg-gated | We don't use `local` feature |

#### Summary

| Status | Call Sites | Files |
|--------|------------|-------|
| **BREAKS (must fix)** | 3-4 | `server.rs` |
| **BEHAVIORAL (review)** | 2 | `router.rs` |
| **VERIFY** | 8-9 | `tools.rs`, `router.rs` |
| **SAFE** | ~90+ | 17 files |

---

### Q4: What is the impact on vnc-021's transport layer?

**Answer**: Well-isolated by ADR-003's adapter boundary. Auth (`http/auth.rs`), listener (`http/listener.rs`), TLS (`http/tls.rs`) have **zero rmcp dependency**. All rmcp coupling concentrated in `McpAdapter` in `http/router.rs` (~40 lines).

#### Transport Architecture

```
TcpListener (http/listener.rs)          -- NO rmcp dependency
    |
TlsAcceptor (http/tls.rs)              -- NO rmcp dependency
    |
StaticTokenAuth (http/auth.rs)          -- NO rmcp dependency
    |
PathRouter (http/router.rs)             -- NO rmcp dependency (dispatches by path)
    |
    +-- GET /health -> health_response  -- NO rmcp dependency
    +-- POST /observe -> 501 stub       -- NO rmcp dependency
    +-- /* else -> McpAdapter           -- ALL rmcp dependency lives here
                    |
                    StreamableHttpService<UnimatrixServer, LocalSessionManager>
```

#### Component Risk Assessment

| Component | rmcp Dependency | Risk | Changes |
|-----------|----------------|------|---------|
| `http/auth.rs` (StaticTokenAuth) | None | LOW | None |
| `http/listener.rs` (TCP) | None | LOW | None |
| `http/tls.rs` (TLS) | None | LOW | None |
| `http/router.rs` (McpAdapter) | 4 types | MEDIUM | Validate defaults |
| `uds/mcp_listener.rs` | `ServiceExt`, `RunningService` | MEDIUM | Verify `IntoTransport` blanket impl |
| `main.rs` (stdio) | `ServiceExt`, `stdio()` | LOW | One-line fix if renamed |
| Extension propagation | `RequestContext.extensions` | MEDIUM | Re-validate after migration |

#### Bearer Token / Session Independence

Bearer token auth and rmcp sessions operate on different layers — auth runs BEFORE rmcp (HTTP layer), sessions run INSIDE rmcp (MCP protocol layer). The critical bridge is extension propagation: `ResolvedIdentity` inserted by auth must survive rmcp's internal processing to reach `RequestContext.extensions.get::<Parts>()`. This must be re-validated post-migration.

---

### Q5: Migration effort estimate

**Answer: Patch-level fix. ~4 hours implementation, ~1 day including testing.**

| Task | Scope | Estimate | Risk |
|------|-------|----------|------|
| Fix `ServerInfo` struct literal | `server.rs:274-287` | 30 min | Low |
| Fix `Implementation` struct literals | `server.rs:275-279`, test | 30 min | Low |
| Fix `ClientInfo` struct literal (test) | `server.rs:3257-3266` | 15 min | Low |
| Verify `LocalSessionManager::default()` | `router.rs:267` | 10 min | Low |
| Validate `StreamableHttpServerConfig` defaults | `router.rs:268` | 30 min | Medium |
| Verify `ErrorData::invalid_params` in 1.4.0 | `tools.rs` (8 sites) | 15 min | Low |
| Verify `IntoTransport` blanket impl for UDS | `uds/mcp_listener.rs` | 10 min | Low |
| Re-validate extension propagation | Integration test | 1 hour | Medium |
| Update Cargo.toml version | 1 line | 5 min | None |
| Full test suite + fix any `#[non_exhaustive]` misses | Whole crate | 1 hour | Low |
| **Total** | | **~4 hours** | |

#### Why Patch-Level

1. No trait signature changes affect us
2. ADR-003 isolates transport changes to ~40 lines
3. `#[non_exhaustive]` affects only 3-4 struct literals
4. ~90% of call sites require zero changes
5. Behavioral changes (DNS rebinding, session timeout) are sensible defaults we want

---

### Q6: CVE backport feasibility

**Answer: Do not backport. Upgrade to 1.4.0+.**

- **CVE**: CVE-2026-42559 (GHSA-89vp-x53w-74fx), CVSS 8.8
- **Fix**: Host header validation, 3 files, ~279 lines added
- **Technically feasible**: Fix is self-contained, no complex dependencies on intervening changes
- **Practically inadvisable**: No 0.16.x branch upstream, creates unmaintainable fork, no future CVE coverage

| Factor | Backport | Full Upgrade |
|--------|----------|-------------|
| CVE coverage | This one only | All current + future |
| Regression risk | Low | Low (per Q3 analysis) |
| Testing confidence | Low (custom fork) | High (upstream CI) |
| Maintenance | High ongoing | One-time |
| Time | ~1 day | ~1 day (patch-level) |

**Interim mitigation**: If migration can't happen immediately, deploy Host header validation at reverse proxy layer.

---

## Future Opportunities

Features in rmcp 1.4+ that Unimatrix could leverage:

### Trait-Based Tool Declaration (v0.17.0)

**Enables**: Tools as standalone structs implementing a `Tool` trait instead of a monolithic `#[tool_router]` impl block.
**Value**: Current `tools.rs` bundles all 14 handlers in one impl block. Splitting into `tools/search.rs`, `tools/store.rs`, `tools/graph.rs` supports the 500-line file limit convention.
**Effort beyond migration**: Significant refactor — each tool becomes its own struct. Follow-on feature.

### Auto-Generated `get_info` (v1.4.0)

**Enables**: `#[tool_handler]` and `#[tool_router]` macros auto-generate `get_info()`, eliminating manual `ServerInfo` construction.
**Value**: Removes boilerplate, keeps `ServerInfo` in sync with actual tool declarations.
**Effort beyond migration**: Minimal — remove manual `get_info()` and let macro generate it. **Worth adopting during base migration** since the `ServerInfo` struct literal must be rewritten anyway.

### `IntoCallToolResult` (v1.4.0)

**Enables**: Tool handlers can return wider error types directly without explicit `ErrorData` conversion.
**Value**: Could simplify 40+ `.map_err(rmcp::ErrorData::from)?` call sites.
**Effort beyond migration**: Moderate refactor — changes tool handler return types. High-value ergonomic improvement.

### OAuth 2.0 Client Credentials (v1.1.0)

**Enables**: Machine-to-machine auth via OAuth 2.0 behind `auth` feature.
**Value**: Enables identity provider integration for enterprise deployments, replacing static bearer tokens.
**Effort beyond migration**: New feature — `auth` feature flag, OAuth config, new auth middleware.

### UDS Client Transport (v1.3.0)

**Enables**: MCP client connections over Unix domain sockets.
**Value**: Currently UDS is server-only. Enables Unimatrix-to-Unimatrix communication for multi-instance deployments.
**Effort beyond migration**: New feature, does not affect base migration.

### Transparent Session Re-Init (v1.3.0)

**Enables**: Client-side `enable_reinit_on_expired_session` auto re-initializes expired sessions.
**Value**: Prevents session expiry errors during long-running operations when Unimatrix acts as MCP client.
**Effort beyond migration**: Client-side configuration only.

### `json_response` Mode (v0.17.0)

**Enables**: `StreamableHttpServerConfig { json_response: true }` returns JSON instead of SSE for non-streaming requests.
**Value**: Most of our 14 tools are request-response. Reduces SSE overhead, improves HTTP client compatibility.
**Effort beyond migration**: One-line config change. **Worth evaluating during migration** since we're already touching `router.rs`.

### `local` Feature for `!Send` Handlers (v1.3.0)

**Enables**: Relaxes `Send + Sync` bounds for thread-local data.
**Value**: Potentially useful for future ML model integrations with thread-local inference sessions.
**Effort beyond migration**: Feature flag addition + refactoring shared state. Not immediately needed.

---

## Unanswered Questions

1. **`ErrorData::invalid_params` in 1.4.0** — Used at 8 call sites. Not listed as removed but not confirmed present. Must verify before migration.
2. **`Implementation` derives `Default` alongside `#[non_exhaustive]`** — The `..Default::default()` pattern in `server.rs:278` depends on this.
3. **`ServerInitializeError` enum variants** — `ExpectedInitializedNotification` removed; full variant list not enumerated. We don't pattern-match this type.
4. **rmcp 1.5.0–1.7.0 changes** — Three additional releases not cataloged. Consider targeting 1.7.0 instead of 1.4.0.
5. **Rust toolchain 1.92 requirement** — rmcp v1.4.0 requires Rust 1.92. Current project toolchain not verified.

---

## Out-of-Scope Discoveries

1. **`transport-async-rw` implicit dependency** — UDS transport relies on blanket `IntoTransport` impl from `transport-async-rw`, transitively enabled by `server`. Not in Cargo.toml. Consider adding explicitly.
2. **Test infrastructure client types** — `run_initialize_handshake` helper uses `ClientInfo`, `ClientCapabilities`, `ProtocolVersion::LATEST`, `serve_client`. Must compile after migration.
3. **rmcp is at 1.7.0** — Upgrading to 1.7.0 instead of 1.4.0 picks up 3 more releases. Likely minimal additional effort; avoids second near-term upgrade.
4. **MCP protocol compliance** — Staying on 0.16.0 means non-conformance with current MCP protocol spec.
5. **jsonwebtoken 9→10 in v1.2.0** — Potential version conflicts if used elsewhere in workspace.

---

## Recommendations Summary

1. **Q1 (API inventory)**: ~100+ call sites across 18 files in 5 categories. Heaviest: `tools.rs` (14 handlers, ~80 error conversions), `server.rs` (trait impl + lifecycle), `router.rs` (transport adapter).

2. **Q2 (breaking changes)**: 11 breaking changes across 8 releases. Only 3 affect our code: `#[non_exhaustive]` (B7), behavioral defaults (B4/B9/B10), and `StreamableHttpService` type param (B6, no-op for us).

3. **Q3 (cross-reference)**: 3-4 mandatory fixes (struct literals in `server.rs`), 8 verifications (`invalid_params`), 2 behavioral reviews, ~90 unaffected. **The migration is narrowly scoped.**

4. **Q4 (transport impact)**: ADR-003 pays off. All rmcp coupling in ~100 lines across 3 files. Auth/TLS/listener untouched. Re-validate extension propagation post-migration.

5. **Q5 (effort)**: **Patch-level fix, ~4 hours + testing. Do not defer.**

6. **Q6 (backport)**: **Do not backport. Upgrade to 1.4.0+ (recommend 1.7.0).** Reverse proxy Host validation as interim mitigation.

7. **Future opportunities**: Auto-generated `get_info` and `json_response` mode are low-hanging fruit adoptable during the base migration. Trait-based tool declaration and `IntoCallToolResult` are high-value follow-on refactors.
