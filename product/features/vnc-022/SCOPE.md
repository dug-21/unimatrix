# vnc-022: Remote Observation Transport

## Problem Statement

Unimatrix's value is the self-learning intelligence pipeline, not the knowledge API. vnc-021 shipped HTTPS transport for MCP tools, but the observation pipeline -- behavioral signals, proactive injection, PreCompact restoration, session tracking -- remains local-only (UDS). In a remote personal cloud deployment, MCP tools work over HTTPS but:

- No observation events reach the server (behavioral signal collection is blind)
- No PreCompact events (transcript restoration disabled)
- No proactive injection (agents never receive unsolicited knowledge)
- No hook-driven phase detection (session context vector is empty)

The intelligence pipeline -- the product's core differentiator -- is offline for remote sessions. This is the gap between "MCP tools work remotely" and "the product works remotely."

Who is affected: Any developer using Unimatrix in a remote deployment (VPS, container, cloud). Without this, remote Unimatrix is a degraded product that cannot learn or inject proactively.

Why now: vnc-021 just merged with the `/observe` 501 stub and path-dispatching router specifically designed to receive this feature. ASS-064 research is complete with a clear architecture recommendation. All infrastructure is in place.

## Goals

1. Replace the `/observe` 501 stub with a real HTTP handler that processes hook events through the existing intelligence pipeline
2. Ship a `hook-remote` CLI subcommand that bridges local hook processes to the remote `/observe` endpoint
3. Enable full intelligence pipeline fidelity over HTTPS: observation recording, proactive injection, PreCompact restoration, session lifecycle tracking
4. Design session identity for remote clients that supports concurrent sessions from the same user
5. Document the `/observe` request/response contract as a stable interface for all three target clients (Claude Code, Codex CLI, Gemini CLI)

## Non-Goals

- **`hook-remote` CLI subcommand**: Cut from vnc-022. No local binary required. Claude Code uses `"type": "http"` hook handlers (direct POST). Codex/Gemini use curl-based shell scripts. Reference patterns documented in the contract. No new Rust dependencies.
- **`context_observe` MCP tool**: In-session telemetry via MCP is an additive follow-on (ASS-064 option b). The `/observe` endpoint's internal pipeline does not preclude it, but implementing the tool is out of scope.
- **SSE server-push notifications**: Blocked on client support (Claude Code #36665). Design the pipeline to be notification-ready but do not implement SSE injection.
- **Client hook configuration/installation automation**: Per-client settings.json/config.toml generation is a separate feature that references this contract.
- **Enterprise OAuth on `/observe`**: Private repo scope. Uses same endpoint + different `BearerValidator` implementation (W2-3). The `BearerValidator` trait from vnc-021 already provides the extensibility seam.
- **Nice-to-have event tier (SubagentStop, unrecognized events)**: ASS-064 classified 3 events as nice-to-have. These can be deferred to a follow-on if they add scope risk.
- **Event queue / offline buffering for remote**: The local UDS `EventQueue` handles server-unavailable scenarios for local deployments. Implementing a remote equivalent (queue events to disk when `/observe` is unreachable) is out of scope for this feature.
- **Full PreCompact transcript restoration**: Day 1 PreCompact returns briefing content only (slightly degraded vs local). `CompactPayload` wire type gets an optional `transcript_excerpt: Option<String>` field for forward compatibility. #670 (session transcript buffer from accumulated observation events) is the real solution — improves both remote and local paths.

## Background Research

### Codebase Findings

**Pipeline factoring (key finding)**: The core hook processing logic lives in `dispatch_request()` in `uds/listener.rs` (line 516). This function is `async fn` (private), takes 10 parameters (all `Arc`-wrapped service handles), and returns `HookResponse`. It is fully transport-agnostic -- it operates on `HookRequest`/`HookResponse` wire types with no UDS-specific code. The only UDS-specific aspect is the `uds_has_capability()` check within each arm (hardcoded to return `true` for all capabilities on the UDS path).

To reuse this from the HTTP path, the function needs:
- Visibility change: `pub(crate)` or extracted to a shared module
- Capability source parameterization: HTTP callers resolve capabilities from `ResolvedIdentity` (injected by `StaticTokenAuth` middleware into request extensions), not from UDS peer credentials

**`dispatch_request` dependencies** (all available on `UnimatrixServer`):
- `store: Arc<Store>` -- `server.store`
- `embed_service: Arc<EmbedServiceHandle>` -- `server.embed_service`
- `vector_store: Arc<AsyncVectorStore<VectorAdapter>>` -- `server.vector_store`
- `entry_store: Arc<Store>` -- `server.entry_store`
- `adapt_service: Arc<AdaptationService>` -- `server.adapt_service`
- `session_registry: Arc<SessionRegistry>` -- `server.session_registry`
- `pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>` -- `server.pending_entries_analysis`
- `services: ServiceLayer` -- `server.services`
- `server_version: &str` -- derivable from `env!("CARGO_PKG_VERSION")`

All 10 parameters are already `Arc`-wrapped fields on `UnimatrixServer`. The observe handler can access them from the server instance that `ProjectRouter` already holds.

**Router architecture** (vnc-021): `PathRouter` dispatches `POST /observe` to the 501 stub in `router.rs` (line 115-118). The stub is a simple static response. Replacing it requires:
1. The `PathRouter` to hold a reference to `UnimatrixServer` (or the necessary service handles)
2. An async handler that deserializes the request body, calls `dispatch_request`, and serializes the response

Currently `PathRouter` only holds a `ProjectRouter`, which holds an `McpAdapter`, which holds a `StreamableHttpService<UnimatrixServer>`. The `UnimatrixServer` is inside rmcp's service wrapper and not directly accessible. The `/observe` handler needs its own reference to the service handles.

**Wire types**: `HookRequest` and `HookResponse` are already `Serialize`/`Deserialize` with `#[serde(tag = "type")]` JSON discrimination. The HTTP wire format can be identical to the UDS wire format -- the only difference is framing (4-byte length prefix on UDS, HTTP Content-Length on HTTPS).

**Auth model**: `StaticTokenAuth` middleware in `http/auth.rs` validates `Authorization: Bearer <hex>` and inserts `ResolvedIdentity` into request extensions. The `/observe` handler receives requests already authenticated. The `ResolvedIdentity` carries `TrustLevel::Restricted` and capabilities `[Read, Write, Search]` -- this is the same capability set needed for observation processing.

**Session identity**: UDS hook events carry `session_id` from Claude Code (a UUID string). The `SessionRegistry` in `infra/session.rs` indexes by this string. Remote events must include the same `session_id` field. The `sanitize_session_id()` function (max 128 chars, alphanumeric + `-_`) already validates format. No server-assigned session IDs are needed -- the client-generated session ID that Claude Code/Codex/Gemini provide is sufficient. `SessionStart` registers the session; subsequent events reference it.

**Client-side hook processing**: `uds/hook.rs` runs as a synchronous subprocess (no tokio). It reads stdin, normalizes event names, builds a `HookRequest`, connects to UDS, sends, receives, writes stdout. The `hook-remote` subcommand follows the same pattern but POSTs to HTTP instead of connecting to UDS. Key reusable logic:
- `parse_hook_input()` -- stdin parsing
- `normalize_event_name()` -- event name canonicalization
- `build_request()` -- `HookInput` to `HookRequest` conversion
- `write_stdout()` / `write_stdout_subagent_inject_response()` -- response formatting
- `extract_transcript_block()` -- PreCompact transcript extraction (runs client-side)

**Transport trait**: `unimatrix-engine/src/transport.rs` defines `trait Transport` with `connect()`, `request()`, `fire_and_forget()`. An `HttpTransport` implementation would be the natural extension, but the hook subprocess runs without tokio, so HTTP requests would use blocking `ureq` or `minreq`, not async reqwest.

### ASS-064 Architecture (Validated)

- **Hybrid architecture**: `/observe` HTTP endpoint + future `context_observe` MCP tool
- **Single TCP listener**: Same port, same TLS, same auth layer, path-based routing
- **Response-based injection**: Sync events (UserPromptSubmit, PreCompact, SubagentStart) return injection content in the HTTP response body. Hook handler writes to stdout locally. Zero client changes required.
- **13 events, 3 tiers**: Critical (6), Important (4), Nice-to-have (3). Fire-and-forget (9) port trivially; synchronous (4) need the 500ms timeout.
- **`hook-remote` CLI**: Reads stdin, POSTs to `/observe`, writes stdout. Config via `UNIMATRIX_URL` + `UNIMATRIX_TOKEN` env vars.

### Technical Landscape

- **rmcp 0.16.0 pinned** -- no upgrade. `/observe` is a custom tower handler, not an rmcp tool.
- **No axum** -- tower + hyper only. Consistent with vnc-021.
- **`#![forbid(unsafe_code)]`** -- all deps must be safe Rust or already audited.
- **HTTP client for hook-remote**: Must be synchronous (no tokio in hook process per ADR-002). Options: `ureq` (pure Rust, TLS via rustls, already-compatible), `minreq` (minimal, but less mature TLS). `ureq` is preferred -- rustls already transitive.

## Proposed Approach

### 1. Make dispatch_request pub(crate) in place

Make `dispatch_request()` `pub(crate)` in `uds/listener.rs`. Add a capabilities parameter so HTTP callers pass capabilities from `ResolvedIdentity` instead of hardcoded UDS capabilities. Minimal diff, preserves git history. Add a doc comment noting it's shared across UDS and HTTP transports. Do not move ~500 lines for a visibility change.

### 2. Implement `/observe` handler in `http/router.rs`

Replace the `observe_stub_response()` call in `PathRouter::call()` with an async handler that:
1. Reads and size-limits the request body (reuse the `Limited` body collection pattern from `McpAdapter`)
2. Deserializes `HookRequest` from JSON
3. Calls the shared `dispatch_request()` with capabilities from `ResolvedIdentity`
4. Maps `HookResponse` to HTTP responses:
   - `Ack` -> 204 No Content
   - `Entries` / `BriefingContent` -> 200 with JSON body
   - `Pong` -> 200 with JSON body
   - `Error` -> 400 with JSON error body

The `PathRouter` needs access to the server's service handles. Options:
- Store `UnimatrixServer` (or an `ObserveContext` subset) directly on `PathRouter`
- Pass service handles through `ProjectRouter` which already holds the server

### 3. Client integration patterns (documentation, no binary)

No `hook-remote` CLI binary. Clients connect directly:
- **Claude Code**: `"type": "http"` hook handler POSTs directly to `/observe`. Document the hook configuration in settings.json format.
- **Codex CLI / Gemini CLI**: curl-based shell script reference pattern. `curl -s -X POST -H "Authorization: Bearer $UNIMATRIX_TOKEN" -H "Content-Type: application/json" -d @- "$UNIMATRIX_URL/observe"` reading from stdin.
- **TLS trust**: curl handles natively (`--cacert` for self-signed, system trust store for CA-signed). Claude Code's HTTP client uses system trust store or environment-configured CA.

The `/observe` request/response contract is the stable interface. Client installation automation is a separate feature.

### 4. Session identity via explicit `session_id`

Remote hook events carry `session_id` in the `HookRequest` payload (same as UDS). The client (Claude Code/Codex/Gemini) generates the session ID. `SessionStart` registers it in `SessionRegistry`. No server-assigned sessions needed.

The bearer token authenticates the user. The `session_id` correlates events within a session. Multiple concurrent sessions from the same user (same token) are distinguished by `session_id`.

### 5. Audit log integration

HTTP observe events use `credential_type = "static_token"` (existing const from vnc-021). The `ResolvedIdentity.agent_id` is `"http-bearer"`. Audit events from `/observe` are indistinguishable in structure from MCP audit events -- same credential type, same agent attribution model.

## Acceptance Criteria

- AC-01: `POST /observe` with valid bearer token and a `SessionRegister` HookRequest JSON body returns 204 and registers the session in `SessionRegistry`
- AC-02: `POST /observe` with a `RecordEvent` (fire-and-forget type) returns 204 and persists the observation to the database
- AC-03: `POST /observe` with a `UserPromptSubmit` (ContextSearch) returns 200 with `HookResponse::Entries` JSON containing ranked entries identical in structure to the UDS path
- AC-04: `POST /observe` with a `CompactPayload` (PreCompact) returns 200 with `HookResponse::BriefingContent` JSON
- AC-05: `POST /observe` with a `SubagentStart` (ContextSearch with source) returns 200 with entries JSON
- AC-06: `POST /observe` without a valid bearer token returns 401 (existing auth middleware, no new work)
- AC-07: `POST /observe` with malformed JSON body returns 400 with error JSON
- AC-08: `POST /observe` with oversized body returns 413 (reuse body limit pattern from McpAdapter)
- AC-09: `CompactPayload` wire type accepts optional `transcript_excerpt: Option<String>` field (forward compatibility for #670)
- AC-10: PreCompact returns briefing content only (no transcript block) — Day 1 degradation documented
- AC-14: Multiple concurrent sessions from the same bearer token with different `session_id` values are correctly isolated in `SessionRegistry`
- AC-15: All 6 critical event types (SessionStart, Stop/TaskCompleted, PreToolUse, PostToolUse, UserPromptSubmit, PreCompact) and all 4 important event types (PostToolUseFailure, SubagentStart, cycle_start, cycle_stop) are handled by the `/observe` endpoint
- AC-16: The `/observe` request/response JSON schema is documented (inline code comments or a contract doc) specifying the `HookRequest` envelope, per-event payloads, and response shapes
- AC-17: Integration tests cover the HTTP observation path: at minimum one fire-and-forget event, one sync event with injection response, one auth rejection, one malformed body
- AC-18: Existing UDS hook path (local `unimatrix hook`) continues to work unchanged (zero regression)
- AC-19: `dispatch_request` (or its extracted equivalent) is shared between UDS and HTTP paths with no logic duplication

## Constraints

1. **rmcp 0.16.0 pinned** -- `/observe` handler is a custom tower handler alongside rmcp, not an rmcp tool. No rmcp upgrade.
2. **No axum** -- tower + hyper only, consistent with vnc-021. Body parsing is manual (no axum extractors).
3. **`#![forbid(unsafe_code)]`** -- all code paths must be safe Rust.
4. **No new Rust dependencies** -- `hook-remote` CLI cut from scope. No HTTP client crate needed. Zero new deps for vnc-022.
5. **`dispatch_request` stays in place** -- `pub(crate)` visibility in `uds/listener.rs`, capability parameter added. No file move. Preserves git history.
6. **Shared wire types** -- `HookRequest`/`HookResponse` in `unimatrix-engine/src/wire.rs` are the contract. No HTTP-specific envelope types. The JSON serde format IS the wire format.
7. **Audit log consistency** -- HTTP observe events must use `credential_type = "static_token"` and produce the same audit trail structure as MCP tool calls.
8. **Body size limit** -- Same `DEFAULT_MAX_BODY_BYTES` (1 MB) as the MCP path. Observation events are small (typically <10KB), but the limit prevents abuse.
9. **Unversioned endpoint** -- `/observe` matches the existing stub. Enterprise uses a different route structure (`/v1/{team-slug}/observe`). Version only if a breaking wire format change occurs.

## Resolved Decisions

1. **`dispatch_request`**: `pub(crate)` in place in `uds/listener.rs`. Capability parameter added. No file move.
2. **Capability model**: Add `SessionWrite` to HTTP `ResolvedIdentity` capability set. `/observe` is a session write path.
3. **No `hook-remote` binary**: Claude Code uses `"type": "http"` hooks (direct POST). Codex/Gemini use curl scripts. No new Rust dependencies.
4. **PreCompact**: Briefing-only Day 1. Optional `transcript_excerpt: Option<String>` field on `CompactPayload` for forward compat. #670 (server-side transcript buffer from accumulated observations) is the real solution.
5. **Versioning**: `/observe` unversioned, matching stub. Enterprise uses different route structure.
6. **TLS**: Curl's problem (`--cacert` for self-signed). Claude Code uses system trust store.

## Open Questions

None — all design questions resolved. Proceed to architecture and specification.

## Tracking

GitHub Issue: #669
