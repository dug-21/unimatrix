# vnc-022 Implementation Brief: Remote Observation Transport

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-022/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-022/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-022/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-022/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/vnc-022/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-022/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| observe-context | pseudocode/observe-context.md | test-plan/observe-context.md |
| dispatch-request-refactor | pseudocode/dispatch-request-refactor.md | test-plan/dispatch-request-refactor.md |
| observe-handler | pseudocode/observe-handler.md | test-plan/observe-handler.md |
| capability-extension | pseudocode/capability-extension.md | test-plan/capability-extension.md |
| compact-payload-wire | pseudocode/compact-payload-wire.md | test-plan/compact-payload-wire.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Replace the `/observe` HTTP 501 stub with a real handler that routes hook lifecycle events through the existing intelligence pipeline over HTTPS. This completes the personal-cloud story: remote sessions gain full observation fidelity (behavioral signals, proactive injection, PreCompact restoration, session tracking) with zero new dependencies and zero pipeline logic duplication. The handler reuses `dispatch_request` as-is, parameterized by capability set, making the pipeline fully transport-agnostic.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Service handle access for /observe handler | ObserveContext struct bundles 10 Arc-cloned service handles; stored as single field on PathRouter; constructed in main.rs before rmcp wrapping | SR-07, SR-01 | architecture/ADR-001-observe-context-struct.md |
| Capability parameterization of dispatch_request | Add `capabilities: &[Capability]` as final parameter; replace 9 `uds_has_capability(X)` calls with `capabilities.contains(&X)`; UDS passes UDS_CAPABILITIES, HTTP passes ResolvedIdentity.capabilities | SR-09 | architecture/ADR-002-capability-parameter.md |
| Session ID transport scoping | Prefix client-supplied session_id with `http-` before passing to dispatch_request; Day 1 constant prefix; W2-3 evolves to `http-{subject_hash}-` for per-user isolation under OAuth | SR-03 | architecture/ADR-003-session-id-scoping.md |
| HookResponse to HTTP status mapping | Ack->204, Entries/BriefingContent/Pong->200+JSON, Error->400+JSON; handler-level errors: malformed JSON->400, oversized body->413, missing auth->401 | SR-04 | architecture/ADR-004-response-http-mapping.md |
| PreCompact transcript_excerpt forward compat | Add `transcript_excerpt: Option<String>` to CompactPayload with `serde(default, skip_serializing_if)`. Day 1: ignored. Forward compat for #670 (server-side transcript buffer) | SCOPE AC-09 | architecture/ADR-005-precompact-forward-compat.md |

## Files to Create/Modify

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/uds/listener.rs` | `dispatch_request`: `fn` -> `pub(crate) fn`, add `capabilities: &[Capability]` param, replace 9 `uds_has_capability(X)` with `capabilities.contains(&X)` |
| `crates/unimatrix-server/src/http/router.rs` | Add `ObserveContext` struct, add `observe_ctx` field to `PathRouter`, replace `observe_stub_response()` with real async handler, add `observe_response_to_http()` mapping function |
| `crates/unimatrix-server/src/http/auth.rs` | Add `Capability::SessionWrite` to `StaticTokenValidator` returned `ResolvedIdentity.capabilities` |
| `crates/unimatrix-server/src/main.rs` | Construct `ObserveContext` from `UnimatrixServer` fields (Arc::clone), pass to `PathRouter::new()` |
| `crates/unimatrix-engine/src/wire.rs` | Add `transcript_excerpt: Option<String>` to `CompactPayload` with serde annotations |

### No New Files

Zero new source files. All changes are modifications to existing files. Integration tests are added to existing test modules.

## Data Structures

### ObserveContext (new, `http/router.rs`)

```rust
#[derive(Clone)]
pub(crate) struct ObserveContext {
    pub(crate) store: Arc<Store>,
    pub(crate) embed_service: Arc<EmbedServiceHandle>,
    pub(crate) vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    pub(crate) entry_store: Arc<Store>,
    pub(crate) adapt_service: Arc<AdaptationService>,
    pub(crate) server_version: String,
    pub(crate) session_registry: Arc<SessionRegistry>,
    pub(crate) pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    pub(crate) services: ServiceLayer,
}
```

### CompactPayload (modified, `wire.rs`)

```rust
CompactPayload {
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transcript_excerpt: Option<String>,  // NEW — forward compat for #670
}
```

### HTTP Response Mapping Table

| HookResponse Variant | HTTP Status | Content-Type | Body |
|---------------------|-------------|--------------|------|
| `Ack` | 204 No Content | (none) | (empty) |
| `Entries { items, total_tokens }` | 200 OK | application/json | serde_json::to_vec(&response) |
| `BriefingContent { content, token_count }` | 200 OK | application/json | serde_json::to_vec(&response) |
| `Pong { server_version }` | 200 OK | application/json | serde_json::to_vec(&response) |
| `Error { code, message }` | 400 Bad Request | application/json | serde_json::to_vec(&response) |

## Function Signatures

### dispatch_request (modified)

```rust
// crates/unimatrix-server/src/uds/listener.rs
pub(crate) async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<Store>,
    adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    session_registry: &SessionRegistry,
    pending_entries_analysis: &Arc<Mutex<PendingEntriesAnalysis>>,
    services: &ServiceLayer,
    capabilities: &[Capability],  // NEW
) -> HookResponse
```

### observe_response_to_http (new)

```rust
// crates/unimatrix-server/src/http/router.rs
fn observe_response_to_http(resp: HookResponse) -> Response<BoxBody<Bytes, Infallible>>
```

### PathRouter::new (modified)

```rust
// crates/unimatrix-server/src/http/router.rs
pub fn new(project_router: ProjectRouter<ReqBody>, observe_ctx: ObserveContext) -> Self
```

### Session ID Prefixing (in /observe handler)

```rust
// Applied before calling dispatch_request
let prefixed_session_id = format!("http-{}", client_session_id);
```

## Constraints

1. **rmcp 0.16.0 pinned** -- `/observe` is a custom tower handler alongside rmcp, not an rmcp tool
2. **No axum** -- tower + hyper only, body parsing is manual (no extractors)
3. **`#![forbid(unsafe_code)]`** -- all new code must be safe Rust
4. **No new Rust dependencies** -- zero new crates in Cargo.toml
5. **dispatch_request stays in place** -- `pub(crate)` in `uds/listener.rs`, no file move, preserves git history
6. **Shared wire types** -- `HookRequest`/`HookResponse` JSON serde format IS the wire contract, no HTTP-specific envelope
7. **Body size limit** -- `DEFAULT_MAX_BODY_BYTES` (1 MB), two-layer enforcement: Content-Length header fast-path + `http_body_util::Limited` stream-level
8. **Unversioned endpoint** -- `/observe` matches existing stub; enterprise uses different route structure
9. **Audit consistency** -- `credential_type = "static_token"`, `agent_id = "http-bearer"`, same structure as MCP audit events

## Dependencies

### Workspace Crates (existing, no changes to Cargo.toml)

| Crate | Used For |
|-------|----------|
| `unimatrix-engine` | `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload` wire types |
| `unimatrix-store` | `Capability`, `TrustLevel` enums, `SqlxStore` |
| `unimatrix-server` | `dispatch_request`, `PathRouter`, `StaticTokenAuth`, `ResolvedIdentity`, `SessionRegistry`, `ServiceLayer` |

### Existing Infrastructure Reused

| Component | Location | Reuse |
|-----------|----------|-------|
| `StaticTokenAuth` middleware | `http/auth.rs` | Provides `ResolvedIdentity` in request extensions |
| `PathRouter` 501 stub | `http/router.rs` | Replaced with real handler |
| `dispatch_request` | `uds/listener.rs` | Made `pub(crate)`, capability parameter added |
| `SessionRegistry` | `infra/session.rs` | Session state management (unchanged) |
| `ServiceLayer` | `services/mod.rs` | Search, briefing, index services (unchanged) |
| Body size limiting | `http_body_util::Limited` | Same `DEFAULT_MAX_BODY_BYTES` pattern as MCP path |
| `sanitize_session_id` | `infra/session.rs` | Format validation of prefixed session_id (unchanged) |

### External Dependencies

None. Zero new external dependencies.

## NOT in Scope

- `hook-remote` CLI subcommand (cut -- clients POST directly or use curl)
- `context_observe` MCP tool (follow-on, ASS-064 option b)
- SSE server-push notifications (blocked on client support)
- Client hook configuration/installation automation
- Enterprise OAuth on `/observe` (W2-3, uses existing `BearerValidator` trait seam)
- Nice-to-have event tier (SubagentStop, Ping, unrecognized) -- handled by existing dispatch_request arms but no acceptance criteria
- Event queue / offline buffering for remote
- Full PreCompact transcript restoration (Day 1 briefing-only; #670 is the real solution)
- `dispatch_request` file move (stays in `uds/listener.rs`)
- Session ID per-token scoping (Day 1 uses constant `http-` prefix; per-token deferred to OAuth)
- PreCompact degradation signaling (no response field indicating degraded mode)

## Alignment Status

**Overall: PASS with 1 WARN**

The feature directly advances `goal:personal-cloud` and enables `goal:self-learning` + `goal:proactive-delivery` for remote sessions. All SCOPE.md requirements are addressed. All 9 scope risks are traced to architecture decisions and test scenarios.

### Variance: Session ID Prefix Scheme (WARN -- scope addition, low risk)

Architecture ADR-003 introduces session_id prefixing (`http-{client_session_id}`) not explicitly requested in SCOPE.md. SCOPE says "No server-assigned session IDs needed." The architect added this in response to SR-03 (session hijacking risk) from the scope risk assessment. Day 1 implementation is minimal: one `format!("http-{session_id}")` call. The forward-looking per-token hash design is documentation only, not Day 1 implementation scope.

**Recommendation**: ACCEPT. Defensive addition, directly responds to identified scope risk, minimal Day 1 overhead.

## Risk Summary

14 risks identified across 38 test scenarios. Key risks:

| Risk | Priority | Mitigation |
|------|----------|-----------|
| R-01: ObserveContext field divergence from dispatch_request params | High | End-to-end integration tests exercise all 10 service handles |
| R-02: UDS regression from dispatch_request refactor | High | All existing UDS tests must pass unchanged; grep audit for stale uds_has_capability calls |
| R-03: Session ID prefix not applied/applied incorrectly | High | Integration tests verify "http-" prefix in SessionRegistry |
| R-06: ResolvedIdentity missing SessionWrite | High | Integration test confirms session-mutating operations succeed via HTTP |
| R-10: Warn+continue failure paths lack coverage | High | At least one warn+continue path tested per dispatch arm category |

Full risk register and test scenarios: `product/features/vnc-022/RISK-TEST-STRATEGY.md`

## Event Coverage

10 events required for Day 1 (6 critical + 4 important):

| Event | Wire Type | Response |
|-------|-----------|----------|
| SessionStart | SessionRegister | 204 (Ack) |
| Stop, TaskCompleted | SessionClose | 204 (Ack) |
| PreToolUse | RecordEvent | 204 (Ack) |
| PostToolUse | RecordEvent | 204 (Ack) |
| PostToolUseFailure | RecordEvent | 204 (Ack) |
| UserPromptSubmit | ContextSearch | 200 + Entries JSON |
| PreCompact | CompactPayload | 200 + BriefingContent JSON |
| SubagentStart | ContextSearch (source="SubagentStart") | 200 + Entries JSON |
| cycle_start | RecordEvent (via PreToolUse interception) | 204 (Ack) |
| cycle_stop | RecordEvent (via PreToolUse interception) | 204 (Ack) |
