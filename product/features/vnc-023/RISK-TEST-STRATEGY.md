# Risk-Based Test Strategy: vnc-023

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Extension propagation regression — ResolvedIdentity silently lost through rmcp 1.7 internals | High | Med | Critical |
| R-02 | ServerHandler::initialize trait signature incompatibility breaks compilation or alters behavior | High | Med | Critical |
| R-03 | #[non_exhaustive] struct literal migration introduces logic error in ServerInfo/Implementation construction | Med | Med | High |
| R-04 | allowed_origins config wiring disconnected — value set in config.toml but not propagated to StreamableHttpServerConfig | Med | Med | High |
| R-05 | CVE-2026-42559 not fully resolved — allowed_hosts default does not activate or is overridden | High | Low | High |
| R-06 | Behavioral default regression — keep_alive 5min or init_timeout 60s causes unexpected session drops under load | Med | Low | Medium |
| R-07 | UDS IntoTransport blanket impl fails — transport-async-rw not transitively enabled in 1.7 | Med | Low | Medium |
| R-08 | serve_client test helper renamed or moved — test infrastructure silently broken | Low | Med | Medium |
| R-09 | Backward-incompatible config deserialization — existing config.toml without allowed_origins fails to parse | Med | Low | Medium |
| R-10 | http crate version mismatch — Parts type from different http versions causes silent extension extraction failure | High | Low | High |
| R-11 | ErrorData::invalid_params signature changed — 8 call sites in tools.rs fail to compile | Med | Low | Medium |
| R-12 | Description string in Implementation not returned in initialize response | Low | Low | Low |
| R-13 | allowed_origins vs allowed_hosts interaction — both configured but semantics unclear, operator locks themselves out | Med | Med | High |

## Risk-to-Scenario Mapping

### R-01: Extension propagation regression (Critical)
**Severity**: High
**Likelihood**: Med — rmcp internals are not API-stable; HTTP-to-MCP bridge refactoring between 0.16 and 1.7 could drop Parts from extensions (pattern #4699 flags this as a key verification point)
**Impact**: All capability-gated tool calls silently downgrade to anonymous. Audit attribution is blank. No compile-time signal — failure is purely runtime.

**Test Scenarios**:
1. Integration test: HTTP request with bearer token -> StaticTokenAuth inserts ResolvedIdentity -> rmcp processes request -> tool handler extracts identity via `RequestContext.extensions.get::<Parts>()` -> assert identity is non-None and matches expected value
2. Negative scenario: tool handler that requires identity returns permission error when identity is absent — validates the test would catch a regression (per pattern #4452: test must fail against unfixed code)

**Coverage Requirement**: At least one integration test exercises the full HTTP->rmcp->tool chain with a non-privileged agent whose capability depends on ResolvedIdentity presence. Test must fail if propagation breaks.

### R-02: ServerHandler::initialize trait signature incompatibility (Critical)
**Severity**: High
**Likelihood**: Med — pattern #4367 trap 3 documents that rmcp 0.16 uses `impl Future` not `async fn`; 1.7 may have switched
**Impact**: Compilation failure blocks entire migration. If silently accepted with wrong return type semantics, client_type_map population and session key extraction could break.

**Test Scenarios**:
1. Compile gate: `cargo build -p unimatrix-server` succeeds
2. Integration test: MCP initialize handshake completes, server responds with correct ServerInfo including capabilities and instructions
3. Behavioral test: after initialize, client_type_map contains the connecting client's name keyed on Mcp-Session-Id

**Coverage Requirement**: Compilation plus at least one test that exercises the full initialize handshake and verifies client_type_map population.

### R-03: #[non_exhaustive] struct literal migration logic error (High)
**Severity**: Med
**Likelihood**: Med — manual rewrite of struct construction could omit a field, use wrong default, or swap argument order in constructor
**Impact**: ServerInfo returned to clients is incomplete or incorrect. Capabilities may be missing. Instructions may be empty.

**Test Scenarios**:
1. Unit/integration test: `get_info()` returns ServerInfo with correct name, version, description, capabilities, and instructions
2. Verify capabilities list matches pre-migration behavior (tools, resources, prompts capabilities present)
3. Verify instructions string is non-empty and matches expected value

**Coverage Requirement**: Test that asserts all fields of the returned ServerInfo against expected values. Not just compilation — field correctness.

### R-04: allowed_origins config wiring disconnected (High)
**Severity**: Med
**Likelihood**: Med — 4-hop config chain (HttpConfig -> main.rs -> ProjectRouter -> McpAdapter -> StreamableHttpServerConfig) has multiple points where the value could be dropped
**Impact**: Operator configures allowed_origins but rmcp never receives them. CSRF defense-in-depth silently inactive.

**Test Scenarios**:
1. Config deserialization test: config.toml with `allowed_origins = ["https://example.com"]` deserializes into HttpConfig with correct Vec
2. Wiring test: McpAdapter constructed with non-empty allowed_origins propagates them to StreamableHttpServerConfig
3. Empty default test: HttpConfig without allowed_origins field deserializes to empty Vec

**Coverage Requirement**: At minimum, config deserialization round-trip and one test that traces the value from HttpConfig to StreamableHttpServerConfig.

### R-05: CVE-2026-42559 not fully resolved (High)
**Severity**: High
**Likelihood**: Low — rmcp 1.4+ adds allowed_hosts defaulting to localhost, which is the fix
**Impact**: DNS rebinding attack remains viable on HTTPS transport. CVSS 8.8.

**Test Scenarios**:
1. Version verification: Cargo.lock shows rmcp 1.7.0 (not 0.16.x)
2. Behavioral verification: StreamableHttpServerConfig::default().allowed_hosts is non-empty and contains localhost
3. No override: McpAdapter does not clear or override allowed_hosts from the rmcp default

**Coverage Requirement**: Cargo.toml version pin verified. No code path overrides allowed_hosts to empty.

### R-06: Behavioral default regression (Medium)
**Severity**: Med
**Likelihood**: Low — defaults are documented and acceptable per scope analysis
**Impact**: Sessions expire after 5 minutes idle (previously never). Init handshake times out at 60 seconds (previously never). Could affect long-running operations or slow clients.

**Test Scenarios**:
1. Existing integration tests that exercise session lifecycle pass without timeout-related failures
2. Verify LocalSessionManager::default() compiles and existing session tests pass

**Coverage Requirement**: Existing test suite passes. No new test required unless existing tests show timeout failures.

### R-07: UDS IntoTransport blanket impl failure (Medium)
**Severity**: Med
**Likelihood**: Low — transport-async-rw is transitively enabled by server feature
**Impact**: UDS transport mode completely broken. Blocks stdio-alternative deployments.

**Test Scenarios**:
1. Compile gate: `cargo build -p unimatrix-server` with UDS code paths included
2. If UDS integration tests exist, they pass

**Coverage Requirement**: Compilation of UDS transport path. Explicit `transport-async-rw` addition only if transitive enablement breaks.

### R-08: serve_client test helper renamed or moved (Medium)
**Severity**: Low
**Likelihood**: Med — rmcp public API surface has been refactored across 11 breaking changes
**Impact**: Test infrastructure fails to compile. Blocks test suite execution.

**Test Scenarios**:
1. Compile gate: `cargo test -p unimatrix-server --no-run` succeeds
2. All tests using `rmcp::serve_client` compile and execute

**Coverage Requirement**: Test compilation gate.

### R-09: Backward-incompatible config deserialization (Medium)
**Severity**: Med
**Likelihood**: Low — HttpConfig uses `#[serde(default)]` (pattern #646 confirms this approach is reliable)
**Impact**: Existing deployments fail to start after upgrade. config.toml parse error on missing allowed_origins field.

**Test Scenarios**:
1. Deserialization test: TOML string without `allowed_origins` key deserializes into HttpConfig with allowed_origins = empty vec
2. Deserialization test: TOML string WITH `allowed_origins` key deserializes correctly

**Coverage Requirement**: Config deserialization round-trip test for both present and absent field.

### R-10: http crate version mismatch (High)
**Severity**: High
**Likelihood**: Low — architecture resolved SR-04 as compatible
**Impact**: `RequestContext.extensions.get::<Parts>()` silently returns None because Parts from http 1.x != Parts from http 2.x. Same symptom as R-01 but different root cause.

**Test Scenarios**:
1. Cargo.lock inspection: both unimatrix-server and rmcp depend on the same http major version
2. R-01 integration test implicitly covers this — if Parts extraction works, versions match

**Coverage Requirement**: Covered by R-01 integration test. Additional: `cargo tree -i http` shows single version.

### R-11: ErrorData::invalid_params signature changed (Medium)
**Severity**: Med
**Likelihood**: Low — architecture notes these as stable across 0.16-1.7 range
**Impact**: 8 compilation failures in tools.rs. Mechanical fixes but blocks build.

**Test Scenarios**:
1. Compile gate: `cargo build -p unimatrix-server` succeeds with zero changes to tools.rs ErrorData call sites
2. Diff verification: tools.rs has zero modifications

**Coverage Requirement**: Compile gate plus diff review confirming no changes to ErrorData sites.

### R-12: Description string not returned in initialize response (Low)
**Severity**: Low
**Likelihood**: Low — straightforward builder chain
**Impact**: MCP clients don't display server description. Cosmetic.

**Test Scenarios**:
1. Unit test: `get_info()` result contains expected description string

**Coverage Requirement**: One assertion on description field presence and value.

### R-13: allowed_origins vs allowed_hosts interaction confusion (High)
**Severity**: Med
**Likelihood**: Med — ADR-002 states they are independent checks but rmcp source confirmation is needed
**Impact**: Operator configures both, expects either-or semantics, gets and-semantics — legitimate requests rejected. Or vice versa.

**Test Scenarios**:
1. Code review: McpAdapter does not modify allowed_hosts when setting allowed_origins
2. Documentation: in-code comment on HttpConfig.allowed_origins clarifies independent-check semantics
3. If rmcp source confirms and-semantics: test with both configured verifies requests pass both checks

**Coverage Requirement**: Code review verification. In-code documentation of interaction semantics.

## Integration Risks

1. **Config chain integrity** (R-04): The 4-hop config propagation (config.toml -> HttpConfig -> ProjectRouter::new -> McpAdapter::new -> StreamableHttpServerConfig) is the longest data flow in this migration. A dropped parameter at any hop silently disables origin validation with no compile-time signal.

2. **Extension type identity across crate boundaries** (R-10 + R-01): `http::request::Parts` must be the exact same type (same crate version) in both unimatrix-server and rmcp. If Cargo resolves two different `http` versions, `extensions.get::<Parts>()` returns None because the TypeId differs. This is a well-known Rust footgun with no compile-time diagnostic.

3. **Constructor chain fidelity** (R-03): The ServerInfo/Implementation construction must produce identical capabilities, instructions, and metadata as the pre-migration struct literals. The `..Default::default()` rest pattern with `#[non_exhaustive]` is allowed only within the defining crate — our code CANNOT use it. Every field must be explicitly provided or obtained via constructor defaults.

4. **Test infrastructure compatibility** (R-08): Integration tests depend on `rmcp::serve_client`, `ClientInfo` construction, and `Peer<RoleServer>` patterns documented in #4367. Any rename in the test support API cascades to all integration tests.

## Edge Cases

1. **Empty allowed_origins + populated allowed_hosts**: rmcp should enforce hosts but skip origin check. Verify this is the default behavior — not reject-all.
2. **allowed_origins with trailing slashes or port numbers**: `"https://example.com"` vs `"https://example.com/"` vs `"https://example.com:443"` — which format does rmcp expect? Misconfiguration silently rejects legitimate requests.
3. **ServerInfo with Default::default() fields**: If the migration uses a constructor that doesn't set capabilities or instructions, the server advertises no capabilities and clients may refuse to send tool calls.
4. **keep_alive timeout during long tool execution**: A tool call that takes >5 minutes — does the session expire mid-call, or does active request processing extend the keep-alive? If sessions expire mid-call, long-running tools break.
5. **Concurrent initialize calls**: rmcp 1.6+ adds init_timeout (60s). If two clients race to initialize, does each get independent timeout tracking? Relevant for multi-client deployments.
6. **ClientInfo with #[non_exhaustive] in tests**: If rmcp 1.7 added new required fields to ClientInfo that have no default, test construction fails even with builders. The test helper must use whatever constructor rmcp provides.

## Security Risks

**Untrusted input surfaces affected by this migration**:

1. **Host header** (CVE-2026-42559): rmcp 1.4+ validates Host header against allowed_hosts. This is the primary security improvement. Blast radius if bypassed: attacker can invoke any MCP tool with victim's bearer token via DNS rebinding. Mitigated by rmcp default (localhost only).

2. **Origin header** (Opp 11): rmcp 1.6+ validates Origin header against allowed_origins. Defense-in-depth for CSRF. Blast radius if misconfigured: cross-origin web page can invoke MCP tools. Mitigated by empty default (all origins accepted — same as pre-migration behavior).

3. **Bearer token in extensions**: ResolvedIdentity propagation (R-01). If extensions are silently dropped, tool calls execute as anonymous — bypassing capability gates. This is an authorization bypass, not a data leak. Blast radius: anonymous access to all tools that assume identity is always present.

4. **Config injection**: allowed_origins is deserialized from config.toml via serde. No path traversal or injection risk — values are opaque strings compared against the Origin header by rmcp. No user-controlled input reaches this config at runtime.

## Failure Modes

| Failure | Expected Behavior | Detection |
|---------|-------------------|-----------|
| Extension propagation breaks (R-01) | Tool calls should fail with explicit permission-denied error, not silently succeed as anonymous | Integration test asserting identity presence |
| initialize signature mismatch (R-02) | Compilation failure with clear trait bound error | Compile gate |
| ServerInfo missing capabilities (R-03) | Client receives empty capabilities — should still connect but tools unavailable | Integration test asserting capabilities in initialize response |
| allowed_origins not wired (R-04) | Origin validation silently inactive — no error, just no protection | Config propagation test |
| Config parse failure (R-09) | Server fails to start with serde error on missing field | Config deserialization test with legacy TOML |
| UDS transport broken (R-07) | UDS connections fail at transport setup — error logged, client cannot connect | Compile gate + UDS test if present |
| Session timeout under load (R-06) | Idle sessions cleaned up at 5min — active sessions should NOT be affected | Existing session tests pass |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (High): Cargo feature flags renamed/removed | — | RESOLVED pre-architecture. All 6 features verified present in rmcp 1.7.0 via cargo info. |
| SR-02 (Med): MSRV exceeds 1.89 | — | RESOLVED. rmcp 1.7.0 does not declare rust-version. Workspace MSRV unaffected. |
| SR-03 (High): ServerHandler::initialize signature | R-02 | ADR-001 compile-first strategy. Mechanical fix if changed. Covered by compile gate + initialize integration test. |
| SR-04 (Med): http crate version mismatch | R-10 | RESOLVED in architecture. Both use http "1". Covered by R-01 integration test (extension extraction). |
| SR-05 (Low): Bundled enhancements expand review | R-04, R-12 | Accepted. Enhancements are strictly additive. Tested independently. |
| SR-06 (Med): allowed_origins vs allowed_hosts interaction | R-13 | ADR-002 documents independent checks. Implementer must confirm from rmcp source. |
| SR-07 (Med): schemars version drift | — | RESOLVED. schemars "1" compatible with rmcp 1.7. |
| SR-08 (High): Extension propagation regression | R-01 | ADR-003 specifies integration test strategy. Highest-priority test scenario. |
| SR-09 (Med): UDS IntoTransport blanket impl | R-07 | Compile gate covers this. Add explicit feature only if transitive enablement breaks. |
| SR-10 (Med): Behavioral default changes | R-06 | Defaults documented and acceptable. Existing test suite validates no regression. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 5 scenarios |
| High | 4 (R-03, R-04, R-05, R-10) | 9 scenarios |
| Medium | 5 (R-06, R-07, R-08, R-09, R-11) | 8 scenarios |
| Low | 2 (R-12, R-13) | 3 scenarios |
| **Total** | **13** | **25 scenarios** |
