# vnc-022: Remote Observation Transport — Architecture

## System Overview

vnc-022 replaces the `/observe` HTTP 501 stub (shipped in vnc-021) with a real handler that routes hook events through the existing intelligence pipeline. This makes the full observation pipeline — behavioral signals, proactive injection, PreCompact restoration, session lifecycle tracking — available over HTTPS, completing the personal-cloud story (`goal:personal-cloud`).

The design reuses `dispatch_request()` from `uds/listener.rs` unchanged in logic. The only structural changes are: (1) making it callable from the HTTP path via `pub(crate)` visibility and a capability parameter, (2) giving `PathRouter` access to the service handles it needs, and (3) adding `SessionWrite` to the HTTP capability set.

No new Rust dependencies. No new crates. No file moves.

## Component Breakdown

### C1: ObserveContext — Service Handle Bundle

**Location**: `crates/unimatrix-server/src/http/router.rs`

A struct that holds `Arc`-cloned references to the subset of `UnimatrixServer` fields needed by `dispatch_request()`. Constructed once in `main.rs` alongside `ProjectRouter`, stored on `PathRouter`.

**Responsibility**: Provide the `/observe` handler with the same service handles that the UDS listener holds, without exposing the full `UnimatrixServer` or breaking rmcp encapsulation.

**Why a struct, not individual fields on PathRouter**: SR-01 identified the 10-parameter signature as a maintenance hazard. `ObserveContext` bundles them into one unit. If `dispatch_request` gains or loses a parameter, only `ObserveContext` changes — `PathRouter`, `ProjectRouter`, and `main.rs` are insulated.

### C2: dispatch_request — Shared Pipeline (modified in place)

**Location**: `crates/unimatrix-server/src/uds/listener.rs` (unchanged file)

The existing `dispatch_request()` function, made `pub(crate)` with one new parameter: `capabilities: &[Capability]`. All `uds_has_capability(X)` calls inside become `capabilities.contains(&X)`.

**Responsibility**: Process any `HookRequest` and return a `HookResponse` using the intelligence pipeline. Transport-agnostic.

### C3: /observe Handler — HTTP Entry Point

**Location**: `crates/unimatrix-server/src/http/router.rs` (replaces `observe_stub_response()`)

An async function inside `PathRouter::call()` that:
1. Collects and size-limits the request body (reuses `Limited` pattern from `McpAdapter`)
2. Deserializes `HookRequest` from JSON
3. Extracts `ResolvedIdentity` from request extensions (injected by `StaticTokenAuth`)
4. Calls `dispatch_request()` with capabilities from `ResolvedIdentity`
5. Maps `HookResponse` to HTTP status codes and JSON bodies

**Responsibility**: HTTP-specific framing — body parsing, auth extraction, response mapping. Zero pipeline logic.

### C4: Capability Model Extension

**Location**: `crates/unimatrix-server/src/http/auth.rs` (StaticTokenValidator) + `crates/unimatrix-server/src/uds/mod.rs` (UDS_CAPABILITIES)

Add `SessionWrite` to the HTTP `ResolvedIdentity` capability set. The UDS path already has `SessionWrite` in `UDS_CAPABILITIES`. The HTTP path currently has `[Read, Write, Search]` — observation processing requires `SessionWrite` for session registration, event recording, and cycle management.

### C5: CompactPayload Wire Type Extension

**Location**: `crates/unimatrix-engine/src/wire.rs`

Add `transcript_excerpt: Option<String>` to `CompactPayload`. Serde `skip_serializing_if = "Option::is_none"` plus `default` ensures backward compatibility — existing UDS callers that omit the field continue to work. Forward compatibility for #670 (server-side transcript buffer).

## Component Interactions

```
Client (Claude Code / Codex / Gemini)
  |
  | POST /observe  { "type": "RecordEvent", ... }
  | Authorization: Bearer <hex>
  v
StaticTokenAuth (tower middleware)
  |  validates bearer token
  |  inserts ResolvedIdentity { capabilities: [Read, Write, Search, SessionWrite] }
  v
PathRouter::call()
  |  path match: POST /observe
  |  extracts ResolvedIdentity from request extensions
  |  collects + size-limits body via Limited
  |  deserializes HookRequest from JSON
  v
dispatch_request(request, ..., capabilities: &[Capability])
  |  same function as UDS path
  |  capability checks use capabilities.contains(&X) instead of uds_has_capability(X)
  |  processes through intelligence pipeline
  v
HookResponse
  |
  v
observe_response_to_http() mapper
  |  Ack -> 204 No Content
  |  Entries -> 200 + JSON body
  |  BriefingContent -> 200 + JSON body
  |  Pong -> 200 + JSON body
  |  Error -> 400 + JSON body
  v
HTTP Response to client
```

### UDS Path (unchanged)

```
Hook process (unimatrix hook)
  |  stdin -> HookRequest
  |  4-byte length prefix framing over UDS
  v
uds/listener.rs handle_connection()
  |  peer credential auth
  |  deserializes HookRequest from length-prefixed JSON
  v
dispatch_request(request, ..., capabilities: UDS_CAPABILITIES)
  |  UDS_CAPABILITIES = [Read, Search, SessionWrite]
  v
HookResponse
  |  4-byte length prefix framing back over UDS
  v
Hook process stdout
```

## Technology Decisions

| Decision | ADR | Summary |
|----------|-----|---------|
| ObserveContext struct for handle passing | ADR-001 | Bundles 10 Arc params into one struct; solves SR-07 (PathRouter reach) and SR-01 (parameter sprawl) |
| Capability parameter on dispatch_request | ADR-002 | Replace hardcoded uds_has_capability with &[Capability] slice; enables HTTP path reuse |
| Session ID scoped per bearer token | ADR-003 | Prefix session_id with token identity hash to prevent cross-token collision |
| HookResponse to HTTP status code mapping | ADR-004 | Ack->204, content responses->200, Error->400; consistent with REST conventions |
| PreCompact transcript_excerpt forward compat | ADR-005 | Optional field on CompactPayload; Day 1 briefing-only; #670 provides full solution |

## Integration Points

### Existing Code Touched

1. **`uds/listener.rs`**: `dispatch_request` visibility `fn` -> `pub(crate) fn`, new `capabilities: &[Capability]` parameter. All internal `uds_has_capability(X)` calls become `capabilities.contains(&X)`. The UDS call site at line 478 passes `crate::uds::UDS_CAPABILITIES`.

2. **`http/router.rs`**: `PathRouter` gains `observe_ctx: ObserveContext` field. `observe_stub_response()` function removed. `POST /observe` match arm replaced with async handler calling `dispatch_request`. New `ObserveContext` struct defined. New `observe_response_to_http()` mapping function.

3. **`http/auth.rs`**: `StaticTokenValidator::validate_sync` adds `Capability::SessionWrite` to the returned `ResolvedIdentity.capabilities` vec.

4. **`main.rs`**: `ObserveContext` constructed from `server` fields, passed to `PathRouter::new()`.

5. **`wire.rs`**: `CompactPayload` gains `transcript_excerpt: Option<String>`.

### Dependencies (unchanged)

All dependencies are existing workspace crates and already-imported external crates. No new `Cargo.toml` entries.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `dispatch_request` | `pub(crate) async fn dispatch_request(request: HookRequest, store: &Arc<Store>, embed_service: &Arc<EmbedServiceHandle>, vector_store: &Arc<AsyncVectorStore<VectorAdapter>>, entry_store: &Arc<Store>, adapt_service: &Arc<AdaptationService>, server_version: &str, session_registry: &SessionRegistry, pending_entries_analysis: &Arc<Mutex<PendingEntriesAnalysis>>, services: &ServiceLayer, capabilities: &[Capability]) -> HookResponse` | `uds/listener.rs` |
| `ObserveContext` | `pub(crate) struct ObserveContext { store, embed_service, vector_store, entry_store, adapt_service, server_version, session_registry, pending_entries_analysis, services }` | `http/router.rs` |
| `PathRouter::new` | `pub fn new(project_router: ProjectRouter<ReqBody>, observe_ctx: ObserveContext) -> Self` | `http/router.rs` |
| `ResolvedIdentity.capabilities` | `vec![Capability::Read, Capability::Write, Capability::Search, Capability::SessionWrite]` | `http/auth.rs` |
| `CompactPayload.transcript_excerpt` | `#[serde(default, skip_serializing_if = "Option::is_none")] pub transcript_excerpt: Option<String>` | `wire.rs` |
| `HookRequest` (HTTP wire format) | `#[serde(tag = "type")] pub enum HookRequest` — JSON, Content-Type: application/json | `wire.rs` |
| `HookResponse` (HTTP wire format) | `#[serde(tag = "type")] pub enum HookResponse` — JSON response body | `wire.rs` |
| `/observe` HTTP contract | `POST /observe`, `Authorization: Bearer <hex>`, JSON body = `HookRequest`, response = mapped `HookResponse` | `http/router.rs` |
| Body size limit | `DEFAULT_MAX_BODY_BYTES = 1_048_576` (1 MB) | `http/router.rs` |
| `observe_response_to_http` | `fn observe_response_to_http(resp: HookResponse) -> Response<BoxBody<Bytes, Infallible>>` | `http/router.rs` |
| `UDS_CAPABILITIES` | `&[Capability::Read, Capability::Search, Capability::SessionWrite]` | `uds/mod.rs` |
| Audit: credential_type | `CREDENTIAL_TYPE_STATIC_TOKEN = "static_token"` | `http/auth.rs` (vnc-021, unchanged) |
| Audit: AuditSource | `AuditSource::Http { session_id, agent_id: "http-bearer" }` — new variant or reuse existing pattern | `services/mod.rs` |

## HTTP Response Mapping

| HookResponse Variant | HTTP Status | Content-Type | Body |
|---------------------|-------------|--------------|------|
| `Ack` | 204 No Content | (none) | (empty) |
| `Entries { items, total_tokens }` | 200 OK | application/json | `{"type":"Entries","items":[...],"total_tokens":N}` |
| `BriefingContent { content, token_count }` | 200 OK | application/json | `{"type":"BriefingContent","content":"...","token_count":N}` |
| `Pong { server_version }` | 200 OK | application/json | `{"type":"Pong","server_version":"..."}` |
| `Error { code, message }` | 400 Bad Request | application/json | `{"type":"Error","code":N,"message":"..."}` |

## Event Tier Coverage

All 10 Critical + Important events are handled. 3 Nice-to-have events deferred.

| Tier | Events | Wire Type | Response | Handled |
|------|--------|-----------|----------|---------|
| Critical | SessionStart | SessionRegister | Ack (204) | Yes |
| Critical | Stop, TaskCompleted | SessionClose | Ack (204) | Yes |
| Critical | PreToolUse | RecordEvent | Ack (204) | Yes |
| Critical | PostToolUse | RecordEvent | Ack (204) | Yes |
| Critical | UserPromptSubmit | ContextSearch | Entries (200) | Yes |
| Critical | PreCompact | CompactPayload | BriefingContent (200) | Yes |
| Important | PostToolUseFailure | RecordEvent | Ack (204) | Yes |
| Important | SubagentStart | ContextSearch (source=SubagentStart) | Entries or BriefingContent (200) | Yes |
| Important | cycle_start | RecordEvent (intercepted) | Ack (204) | Yes |
| Important | cycle_stop | RecordEvent (intercepted) | Ack (204) | Yes |
| Nice-to-have | SubagentStop | RecordEvent | Ack (204) | Deferred |
| Nice-to-have | Ping | Ping | Pong (200) | Deferred |
| Nice-to-have | unrecognized | RecordEvent | Ack (204) | Deferred |

Note: "Deferred" events are not blocked — `dispatch_request` already handles all `HookRequest` variants. "Deferred" means the spec does not require test coverage or contract documentation for these events. They will work if sent.

## Session Identity Security Model (SR-03)

**Threat**: Client-generated `session_id` could collide with or hijack another user's session when multiple users share a bearer token (team deployment) or when separate tokens happen to produce the same client-generated session ID.

**Mitigation**: The `/observe` handler prefixes the client-supplied `session_id` with a truncated hash of the bearer token identity before passing to `dispatch_request`. Format: `http:{identity_hash_prefix}:{client_session_id}`. The UDS path does not prefix (UDS is single-user by nature).

This scoping is transparent to `dispatch_request` — it sees a string session_id and operates on it. The prefix ensures:
- Two users sharing a token but different client session IDs are isolated (already true)
- Two different tokens generating the same client session ID are isolated (the hash prefix distinguishes them)
- UDS sessions never collide with HTTP sessions (different prefix namespace)

The identity hash is derived from `ResolvedIdentity.agent_id` — currently `"http-bearer"` for all static-token users. For the single-user personal cloud deployment (Day 1), all HTTP sessions share the same prefix, and client-generated UUIDs provide uniqueness. When OAuth lands (W2-3), `agent_id` will carry the OAuth subject, providing per-user session isolation.

**Day 1 simplification**: Since vnc-022 targets single-user personal cloud (one bearer token = one user), the prefix is `http:` (constant). This is sufficient. Per-token isolation becomes load-bearing only in multi-user deployments (W2-3 OAuth).

## Fire-and-Forget Semantics (SR-06)

9 of 13 events are fire-and-forget. The client does not block on the response. Network failures silently drop these events.

**Acceptable because**:
- Fire-and-forget events (RecordEvent variants) are observational — they improve the intelligence pipeline but are not required for correctness
- The local UDS path has the same semantics (broken pipe on fire-and-forget is logged at DEBUG)
- Event queue / offline buffering is explicitly out of scope (Non-Goals)
- The data loss window is bounded: one event per network failure, not cumulative

## Event Dependency Analysis (SR-05)

No critical-path event depends on a nice-to-have event:
- **SubagentStop**: Only updates session tracking counters. SubagentStart (Important tier) is independent.
- **Ping**: Health check only. No pipeline consumer depends on Ping responses.
- **Unrecognized events**: Passed through to generic RecordEvent. No handler depends on them.

## Open Questions

None. All design questions resolved in SCOPE.md and this architecture document.
