# vnc-022 Specification: Remote Observation Transport

## Objective

Replace the `/observe` HTTP 501 stub with a real handler that processes hook lifecycle events through the existing intelligence pipeline, enabling full observation fidelity (behavioral signals, proactive injection, PreCompact restoration, session tracking) over HTTPS for remote deployments. The handler reuses the existing `dispatch_request` logic and `HookRequest`/`HookResponse` wire types as-is, adding no HTTP-specific envelope. This is the bridge between "MCP tools work remotely" (vnc-021) and "the product works remotely."

## Functional Requirements

### FR-01: `/observe` endpoint replaces 501 stub

The `POST /observe` route in `PathRouter` dispatches to an async handler that deserializes the request body as `HookRequest` JSON, calls the shared `dispatch_request` function, and maps the resulting `HookResponse` to an HTTP response.

### FR-02: Request body deserialization

The handler reads the full request body (subject to body size limits per FR-10), then deserializes it as `HookRequest` using `serde_json::from_slice`. Deserialization failure returns HTTP 400 with a JSON error body.

### FR-03: Response mapping — fire-and-forget events

When `dispatch_request` returns `HookResponse::Ack`, the handler returns HTTP 204 No Content with an empty body. This applies to SessionRegister, SessionClose, RecordEvent (PreToolUse, PostToolUse, PostToolUseFailure, cycle_start, cycle_stop).

### FR-04: Response mapping — synchronous events with injection

When `dispatch_request` returns `HookResponse::Entries`, the handler returns HTTP 200 with `Content-Type: application/json` and the `HookResponse::Entries` JSON body. This applies to UserPromptSubmit (ContextSearch) and SubagentStart (ContextSearch with source).

### FR-05: Response mapping — PreCompact briefing

When `dispatch_request` returns `HookResponse::BriefingContent`, the handler returns HTTP 200 with `Content-Type: application/json` and the `HookResponse::BriefingContent` JSON body. Day 1: briefing content only, no transcript block.

### FR-06: Response mapping — Ping/Pong

When `dispatch_request` returns `HookResponse::Pong`, the handler returns HTTP 200 with `Content-Type: application/json` and the `HookResponse::Pong` JSON body.

### FR-07: Response mapping — pipeline errors

When `dispatch_request` returns `HookResponse::Error`, the handler returns HTTP 400 with `Content-Type: application/json` and the `HookResponse::Error` JSON body. The HTTP status is 400 (client error) because `HookResponse::Error` signals invalid payloads or unknown request types, not server-side failures.

### FR-08: `dispatch_request` shared between UDS and HTTP

`dispatch_request` in `uds/listener.rs` becomes `pub(crate)` with no file move. A new `capabilities: &[Capability]` parameter replaces the hardcoded `uds_has_capability()` calls. Each match arm checks the provided capabilities slice instead of calling `uds_has_capability`. UDS callers pass `UDS_CAPABILITIES`; the HTTP handler passes capabilities from `ResolvedIdentity`.

### FR-09: Capability set for HTTP observe callers

The HTTP `/observe` handler extracts `ResolvedIdentity` from request extensions (injected by `StaticTokenAuth` middleware). `ResolvedIdentity` for HTTP bearer callers must include `SessionWrite` in addition to `[Read, Write, Search]`. Without `SessionWrite`, SessionRegister/SessionClose/RecordEvent arms reject the request.

### FR-10: Request body size enforcement

The handler enforces `DEFAULT_MAX_BODY_BYTES` (1 MB) using the same two-layer strategy as `McpAdapter`: (1) Content-Length header fast-path rejection, (2) `http_body_util::Limited` stream-level enforcement. Oversized bodies return HTTP 413 with JSON error body.

### FR-11: Service handle access for `/observe` handler

The `/observe` handler needs the same service handles that `dispatch_request` requires (store, embed_service, session_registry, services, etc.). These are passed to `PathRouter` at construction time. The architect must design the access pattern (direct fields on `PathRouter`, a context struct, or a reference to a service bundle).

### FR-12: CompactPayload wire type forward compatibility

Add an optional `transcript_excerpt: Option<String>` field to `HookRequest::CompactPayload` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Day 1: the field is ignored by `dispatch_request`. This is forward compatibility for #670 (server-side transcript buffer).

### FR-13: All tier-1 and tier-2 events handled

The `/observe` endpoint handles all 6 critical events and all 4 important events:

| Event | Wire Type | Response |
|---|---|---|
| SessionStart | `SessionRegister` | 204 (Ack) |
| Stop | `SessionClose` | 204 (Ack) |
| TaskCompleted | `SessionClose` | 204 (Ack) |
| PreToolUse | `RecordEvent` | 204 (Ack) |
| PostToolUse | `RecordEvent` | 204 (Ack) |
| PostToolUseFailure | `RecordEvent` | 204 (Ack) |
| UserPromptSubmit | `ContextSearch` | 200 + Entries JSON |
| PreCompact | `CompactPayload` | 200 + BriefingContent JSON |
| SubagentStart | `ContextSearch` (source="SubagentStart") | 200 + Entries JSON |
| cycle_start | `RecordEvent` (via PreToolUse interception) | 204 (Ack) |
| cycle_stop | `RecordEvent` (via PreToolUse interception) | 204 (Ack) |

Nice-to-have events (SubagentStop, Ping, unrecognized) are handled by existing `dispatch_request` match arms but are not required for Day 1 acceptance.

### FR-14: Audit log integration

HTTP observe events produce audit log entries with `credential_type = "static_token"` (reuse `CREDENTIAL_TYPE_STATIC_TOKEN` from `http/auth.rs`) and `agent_id = "http-bearer"`. The audit trail structure is identical to MCP tool call audit events.

## Non-Functional Requirements

### NFR-01: No new Rust dependencies

Zero new crate dependencies. All required functionality (JSON serde, body limiting, tower service) already exists in the workspace.

### NFR-02: Body size limit

Maximum request body: 1,048,576 bytes (1 MB), matching `DEFAULT_MAX_BODY_BYTES`. Typical observation events are under 10 KB.

### NFR-03: Latency — fire-and-forget events

Fire-and-forget events (9 of 13) add no perceptible latency beyond HTTP round-trip. The server processes them asynchronously and returns 204 immediately after pipeline ingestion.

### NFR-04: Latency — synchronous events

Synchronous events (UserPromptSubmit, PreCompact, SubagentStart, Ping) block the client until the response is ready. Server-side processing should complete within the time budget established by the intelligence pipeline (context search, briefing generation). No new timeout is introduced on the server side; the client controls its own HTTP timeout (recommended: 500ms for remote, per ASS-064 RQ-1).

### NFR-05: `#![forbid(unsafe_code)]`

All new code is safe Rust. No `unsafe` blocks.

### NFR-06: rmcp 0.16.0 pinned

The `/observe` handler is a custom tower handler alongside rmcp, not an rmcp tool. No rmcp version change.

### NFR-07: No axum

Tower + hyper only, consistent with vnc-021. Body parsing is manual (no axum extractors).

### NFR-08: UDS path zero regression

The `dispatch_request` refactor (pub(crate) + capabilities parameter) must not change UDS behavior. UDS callers pass `UDS_CAPABILITIES` and produce identical responses as before the change.

### NFR-09: Wire format stability

`HookRequest`/`HookResponse` serde format is the stable wire contract. Changes to these types affect both UDS and HTTP paths simultaneously. The only Day 1 wire type change is the additive `transcript_excerpt` field on `CompactPayload` (FR-12).

### NFR-10: Endpoint path

`/observe` — unversioned, matching the existing stub. Enterprise uses a different route structure (`/v1/{team-slug}/observe`). Version only on breaking wire format change.

## Acceptance Criteria

### AC-01: SessionRegister via HTTP

**Condition**: `POST /observe` with valid bearer token and `{"type":"SessionRegister","session_id":"...","cwd":"..."}` body.
**Expected**: HTTP 204, session registered in `SessionRegistry`.
**Verification**: Integration test sends SessionRegister, then verifies session exists in registry.

### AC-02: Fire-and-forget RecordEvent via HTTP

**Condition**: `POST /observe` with valid bearer token and `{"type":"RecordEvent","event_type":"PreToolUse",...}` body.
**Expected**: HTTP 204, observation persisted to database.
**Verification**: Integration test sends RecordEvent, then verifies observation row in database.

### AC-03: UserPromptSubmit (ContextSearch) via HTTP

**Condition**: `POST /observe` with valid bearer token and `{"type":"ContextSearch","query":"...","session_id":"..."}` body.
**Expected**: HTTP 200 with `{"type":"Entries","items":[...],"total_tokens":N}` JSON body.
**Verification**: Integration test sends ContextSearch, verifies 200 status and Entries JSON structure.

### AC-04: PreCompact via HTTP

**Condition**: `POST /observe` with valid bearer token and `{"type":"CompactPayload","session_id":"...","injected_entry_ids":[],"role":null,"feature":null,"token_limit":null}` body.
**Expected**: HTTP 200 with `{"type":"BriefingContent","content":"...","token_count":N}` JSON body.
**Verification**: Integration test sends CompactPayload, verifies 200 status and BriefingContent JSON structure.

### AC-05: SubagentStart via HTTP

**Condition**: `POST /observe` with valid bearer token and `{"type":"ContextSearch","query":"...","source":"SubagentStart"}` body.
**Expected**: HTTP 200 with `{"type":"Entries","items":[...],"total_tokens":N}` JSON body.
**Verification**: Integration test sends ContextSearch with source, verifies 200 status and Entries JSON structure.

### AC-06: Auth rejection

**Condition**: `POST /observe` without `Authorization` header or with invalid bearer token.
**Expected**: HTTP 401 with `{"error":"missing or invalid authorization"}` JSON body.
**Verification**: Integration test sends unauthenticated request, verifies 401. Existing `StaticTokenAuth` middleware handles this; no new code.

### AC-07: Malformed JSON body

**Condition**: `POST /observe` with valid bearer token and non-JSON or invalid HookRequest body.
**Expected**: HTTP 400 with JSON error body.
**Verification**: Integration test sends `{"type":"Bogus"}`, verifies 400 status.

### AC-08: Oversized body

**Condition**: `POST /observe` with body exceeding 1 MB.
**Expected**: HTTP 413 with `{"error":"request body exceeds maximum size"}` JSON body.
**Verification**: Integration test sends oversized body, verifies 413 status.

### AC-09: CompactPayload transcript_excerpt field

**Condition**: `CompactPayload` JSON with `"transcript_excerpt":"some text"` field.
**Expected**: Deserializes without error. Field value accessible on struct. When absent, defaults to `None`.
**Verification**: Unit test on wire type round-trip with and without transcript_excerpt.

### AC-10: PreCompact Day 1 — briefing only

**Condition**: PreCompact via HTTP returns briefing content only.
**Expected**: Response is `BriefingContent` with content derived from briefing pipeline. No transcript block in response.
**Verification**: Integration test verifies BriefingContent response does not contain transcript-specific markers.

### AC-14: Concurrent session isolation

**Condition**: Two requests with same bearer token but different `session_id` values.
**Expected**: Each session is independently tracked in `SessionRegistry`. Events from session A do not affect session B state.
**Verification**: Integration test registers two sessions, sends events to each, verifies independent session state.

### AC-15: All critical and important events handled

**Condition**: Each of the 10 tier-1/tier-2 events is sent via `POST /observe`.
**Expected**: Each event processes through the full `dispatch_request` pipeline and returns the correct response type per FR-13 table.
**Verification**: Integration tests cover at least one event per wire type variant (SessionRegister, SessionClose, RecordEvent, ContextSearch, ContextSearch+source, CompactPayload).

### AC-16: Wire contract documented

**Condition**: The `/observe` request/response JSON schema is documented.
**Expected**: Documentation appears in this specification (see Wire Contract section below) and in code comments on the handler.
**Verification**: Code review confirms contract documentation exists.

### AC-17: Integration test coverage

**Condition**: Integration tests exist for the HTTP observation path.
**Expected**: At minimum: one fire-and-forget event (204), one sync event with injection response (200 + JSON), one auth rejection (401), one malformed body (400).
**Verification**: `cargo test` passes with these test cases.

### AC-18: UDS path regression

**Condition**: Existing `unimatrix hook` local UDS path.
**Expected**: All existing UDS integration tests pass unchanged after `dispatch_request` refactor.
**Verification**: `cargo test` — all pre-existing UDS hook tests pass without modification.

### AC-19: dispatch_request shared, no duplication

**Condition**: `dispatch_request` is callable from both UDS and HTTP paths.
**Expected**: Single implementation in `uds/listener.rs` with `pub(crate)` visibility. No copy-paste of dispatch logic.
**Verification**: Code review confirms single call site for dispatch logic; grep for duplicated match arms returns zero results.

## Domain Models

### Event Classification

Events are classified into three tiers by intelligence pipeline criticality. Only tier-1 (Critical) and tier-2 (Important) are required for Day 1.

- **Fire-and-forget event**: An observation event where the client does not block on the response content. The server returns an acknowledgment (204) immediately after ingesting the event. 9 of 13 events are fire-and-forget.
- **Synchronous event**: An observation event where the client blocks on the response because the response carries injection content (ranked entries, briefing text) that the hook process writes to stdout. 4 of 13 events are synchronous: UserPromptSubmit, PreCompact, SubagentStart, Ping.

### Wire Types (Ubiquitous Language)

- **HookRequest**: The request envelope sent to the server. Tagged union discriminated by `"type"` field. Variants: `Ping`, `SessionRegister`, `SessionClose`, `RecordEvent`, `RecordEvents`, `ContextSearch`, `CompactPayload`, `Briefing`. Defined in `unimatrix-engine/src/wire.rs`.
- **HookResponse**: The response envelope returned by the server. Tagged union discriminated by `"type"` field. Variants: `Pong`, `Ack`, `Error`, `Entries`, `BriefingContent`. Defined in `unimatrix-engine/src/wire.rs`.
- **ImplantEvent**: A single observation event within a `RecordEvent` or `RecordEvents` request. Contains `event_type`, `session_id`, `timestamp`, `payload`, optional `topic_signal`, optional `provider`.
- **EntryPayload**: A knowledge entry in search results. Contains `id`, `title`, `content`, `confidence`, `similarity`, `category`.
- **ResolvedIdentity**: Authenticated caller identity injected by `StaticTokenAuth` middleware. Contains `agent_id`, `trust_level`, `capabilities: Vec<Capability>`.
- **Capability**: Atomic permission unit. Enum: `Read`, `Write`, `Search`, `Admin`, `SessionWrite`. The `/observe` handler requires `SessionWrite`, `Read`, `Search` at minimum.
- **SessionRegistry**: In-memory session state store indexed by `session_id` string. Manages injection history, co-access dedup, rework tracking, topic tallies per session.

### Response Mapping

| dispatch_request result | HTTP Status | Body | Content-Type |
|---|---|---|---|
| `HookResponse::Ack` | 204 No Content | (empty) | (none) |
| `HookResponse::Entries { items, total_tokens }` | 200 OK | `{"type":"Entries","items":[...],"total_tokens":N}` | application/json |
| `HookResponse::BriefingContent { content, token_count }` | 200 OK | `{"type":"BriefingContent","content":"...","token_count":N}` | application/json |
| `HookResponse::Pong { server_version }` | 200 OK | `{"type":"Pong","server_version":"..."}` | application/json |
| `HookResponse::Error { code, message }` | 400 Bad Request | `{"type":"Error","code":N,"message":"..."}` | application/json |
| Deserialization failure | 400 Bad Request | `{"error":"invalid request body: <detail>"}` | application/json |
| Body too large | 413 Payload Too Large | `{"error":"request body exceeds maximum size"}` | application/json |
| Missing/invalid auth | 401 Unauthorized | `{"error":"missing or invalid authorization"}` | application/json |

## Wire Contract: `/observe` Request/Response JSON

This section defines the stable interface for the `/observe` endpoint. All clients (Claude Code, Codex CLI, Gemini CLI) target this contract.

### Request

```
POST /observe HTTP/1.1
Host: <server>
Authorization: Bearer <64-hex-char-token>
Content-Type: application/json
Content-Length: <N>

<HookRequest JSON>
```

### HookRequest Variants

**Ping** (health check):
```json
{"type": "Ping"}
```
Response: `{"type":"Pong","server_version":"0.x.y"}`

**SessionRegister** (session start):
```json
{
  "type": "SessionRegister",
  "session_id": "uuid-or-client-generated-id",
  "cwd": "/path/to/workspace",
  "agent_role": "developer",
  "feature": "vnc-022"
}
```
`agent_role` and `feature` are optional (may be `null` or absent).
Response: HTTP 204 (empty body).

**SessionClose** (session end):
```json
{
  "type": "SessionClose",
  "session_id": "uuid-or-client-generated-id",
  "outcome": "success",
  "duration_secs": 3600
}
```
`outcome` is optional. `duration_secs` is required (u64).
Response: HTTP 204 (empty body).

**RecordEvent** (fire-and-forget observation):
```json
{
  "type": "RecordEvent",
  "event_type": "PreToolUse",
  "session_id": "uuid-or-client-generated-id",
  "timestamp": 1717000000,
  "payload": { "tool_name": "Read", "input": "..." },
  "topic_signal": "vnc-022",
  "provider": "claude-code"
}
```
`topic_signal` and `provider` are optional (omitted when null).
Response: HTTP 204 (empty body).

**ContextSearch** (proactive injection — UserPromptSubmit, SubagentStart):
```json
{
  "type": "ContextSearch",
  "query": "user prompt text or subagent task",
  "session_id": "uuid-or-client-generated-id",
  "role": "developer",
  "task": "implement /observe handler",
  "feature": "vnc-022",
  "k": 10,
  "max_tokens": 3000,
  "source": "SubagentStart"
}
```
All fields except `query` are optional. `source` distinguishes SubagentStart from UserPromptSubmit (absent or null = UserPromptSubmit).
Response:
```json
{
  "type": "Entries",
  "items": [
    {
      "id": 42,
      "title": "Entry Title",
      "content": "Entry content...",
      "confidence": 0.85,
      "similarity": 0.92,
      "category": "decision"
    }
  ],
  "total_tokens": 150
}
```

**CompactPayload** (PreCompact — transcript restoration):
```json
{
  "type": "CompactPayload",
  "session_id": "uuid-or-client-generated-id",
  "injected_entry_ids": [1, 2, 3],
  "role": "developer",
  "feature": "vnc-022",
  "token_limit": 500,
  "transcript_excerpt": null
}
```
`role`, `feature`, `token_limit` are optional. `transcript_excerpt` is optional (Day 1: ignored, forward compat for #670).
Response:
```json
{
  "type": "BriefingContent",
  "content": "# Context Briefing\n\n...",
  "token_count": 250
}
```

### Session ID Rules

- Format: `[a-zA-Z0-9_-]`, 1-128 characters.
- Generated by the client. The server validates format only.
- Multiple concurrent sessions from the same bearer token with different `session_id` values are independently tracked.
- `SessionRegister` must precede other events for a given `session_id`. Events referencing an unregistered session are processed per existing `dispatch_request` behavior (warn and continue for most arms).

### Error Responses

Pipeline error (from dispatch_request):
```json
{"type": "Error", "code": -32004, "message": "description"}
```

Deserialization error (handler-level):
```json
{"error": "invalid request body: <serde error detail>"}
```

## User Workflows

### Workflow 1: Claude Code HTTP Hook

1. Developer configures Claude Code with `"type": "http"` hook handler pointing to `https://<server>/observe` with bearer token header.
2. Claude Code fires a hook event (e.g., UserPromptSubmit). The HTTP hook handler POSTs the event JSON to `/observe`.
3. Server authenticates via bearer token, deserializes `HookRequest`, runs `dispatch_request`.
4. For sync events: server returns injection content (entries/briefing) as JSON. Claude Code hook handler writes response to stdout.
5. For fire-and-forget events: server returns 204. Hook handler exits.

### Workflow 2: Codex/Gemini CLI curl Script

1. Developer configures shell-command hooks in Codex `config.toml` or Gemini `.gemini/settings.json`.
2. Hook script reads stdin (event JSON from CLI), POSTs to `/observe` via curl: `curl -s -X POST -H "Authorization: Bearer $UNIMATRIX_TOKEN" -H "Content-Type: application/json" -d @- "$UNIMATRIX_URL/observe"`.
3. Curl writes HTTP response body to stdout.
4. For sync events: stdout content is the injection payload for the CLI.

### Workflow 3: Session Lifecycle

1. Client sends `SessionRegister` on session start. Server registers session in `SessionRegistry`, resumes goal from prior cycle if applicable.
2. Client sends observation events (`RecordEvent`, `ContextSearch`, `CompactPayload`) during session.
3. Client sends `SessionClose` on session end. Server drains session state, records outcome, generates implicit signals.

## Constraints

1. **C-01**: `dispatch_request` stays in `uds/listener.rs` as `pub(crate)`. No file move. (Resolved Decision 1, SR-01 mitigation: capability parameter instead of growing the 10-param list further.)
2. **C-02**: No axum. Tower + hyper only. (Consistent with vnc-021.)
3. **C-03**: `#![forbid(unsafe_code)]`. All new code safe Rust.
4. **C-04**: No new Rust dependencies. Zero new crates in Cargo.toml.
5. **C-05**: rmcp 0.16.0 pinned. `/observe` is a custom tower handler.
6. **C-06**: Wire format is `HookRequest`/`HookResponse` JSON. No HTTP-specific envelope.
7. **C-07**: Body size limit: `DEFAULT_MAX_BODY_BYTES` (1 MB). Same constant as MCP path.
8. **C-08**: `/observe` unversioned. Enterprise uses different route structure.
9. **C-09**: Audit log: `credential_type = "static_token"`, `agent_id = "http-bearer"`.
10. **C-10**: `dispatch_request` parameter growth — add `capabilities: &[Capability]` parameter. Do not add a separate capability parameter per call; pass the full slice. Architect may introduce a context struct to address SR-01 if the parameter count becomes unwieldy.

## Dependencies

### Existing Crates (workspace)

- `unimatrix-engine` — `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload` wire types
- `unimatrix-store` — `Capability`, `TrustLevel` enums, `SqlxStore`
- `unimatrix-server` — `dispatch_request`, `PathRouter`, `StaticTokenAuth`, `ResolvedIdentity`, `SessionRegistry`, `ServiceLayer`

### Existing Infrastructure

- `StaticTokenAuth` middleware (auth.rs) — provides `ResolvedIdentity` in request extensions
- `PathRouter` (router.rs) — dispatches `POST /observe`; currently returns 501 stub
- `dispatch_request` (uds/listener.rs) — complete hook processing pipeline, all 10 service handle parameters
- `SessionRegistry` (infra/session.rs) — session state management
- `ServiceLayer` (services/mod.rs) — search, briefing, index services
- Body size limiting pattern — `Limited` from `http_body_util`, `DEFAULT_MAX_BODY_BYTES`

### External

- No new external dependencies.

## NOT in Scope

- **`hook-remote` CLI subcommand**: Cut. Clients connect via HTTP directly or curl. No new Rust binary or HTTP client crate.
- **`context_observe` MCP tool**: In-session telemetry tool is a follow-on feature (ASS-064 option b).
- **SSE server-push notifications**: Blocked on client support (Claude Code #36665). Pipeline designed to be notification-ready but no SSE implementation.
- **Client hook configuration automation**: Per-client settings.json/config.toml generation is a separate feature.
- **Enterprise OAuth on `/observe`**: Uses same endpoint + different `BearerValidator` implementation. The `BearerValidator` trait already provides the extensibility seam.
- **Nice-to-have event tier**: SubagentStop, Ping health check, unrecognized events are handled by existing dispatch_request arms but are not acceptance criteria.
- **Event queue / offline buffering for remote**: No retry or disk queue for failed remote observations. Fire-and-forget semantics mean transient network failures drop events silently. Documented data-loss window.
- **Full PreCompact transcript restoration**: Day 1 returns briefing only. `transcript_excerpt` field is forward compat for #670.
- **`dispatch_request` file move**: The function stays in `uds/listener.rs`. No extraction to a shared module.
- **Session ID scoping to bearer token**: Session IDs are globally unique by client convention. No server-side enforcement of per-token session namespacing (SR-03 assumption documented).
- **PreCompact degradation signaling**: No explicit response field or header indicating degraded mode (SR-02 acknowledged; follow-on if needed).

## Open Questions for Architect

1. **Service handle access pattern (SR-07)**: `PathRouter` currently cannot reach `UnimatrixServer` service handles because the server is wrapped inside rmcp's `StreamableHttpService`. The architect must design how service handles reach the `/observe` handler. Options: (a) store an `ObserveContext` struct (subset of handles) directly on `PathRouter`, (b) pass handles through `ProjectRouter` which already receives `UnimatrixServer` at construction, (c) clone the handles before rmcp wrapping. This is the primary structural decision.

2. **dispatch_request context struct vs. parameter list (SR-01)**: The function currently takes 10 parameters. Adding `capabilities: &[Capability]` makes 11. The architect should decide whether to introduce a context struct bundling these parameters or keep the flat parameter list. A struct improves call-site clarity and makes future parameter additions safer.

3. **Session ID per-token scoping (SR-03)**: Currently, session IDs are globally scoped. A malicious or buggy client with a valid token could reference another user's session_id if tokens are shared (e.g., team deployment). The architect should document whether this is acceptable for Day 1 personal cloud scope and what the mitigation path is for enterprise.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- Returned 14 entries. Key relevant entries: #4691 (dispatch_request operates on wire types, transport-agnostic), #4670 (vnc-021 credential_type audit decision), #83 (ADR-007 enforcement point architecture). Confirmed dispatch_request is fully transport-agnostic except for uds_has_capability check.
