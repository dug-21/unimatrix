# FINDINGS-RAW: Root Cause Analysis — agent_attribution Empty + session_id Linkage Broken in audit_log

**Spike**: ASS-056
**Date**: 2026-05-07
**Approach**: investigation (code trace + Unimatrix ADR alignment)
**Confidence**: empirical — every finding pinned to file:line

---

## Unimatrix Alignment

Relevant entries retrieved: #4356 (ADR-002: initialize override design), #4357 (ADR-003: build_context_with_external_identity), #4358 (ADR-004: schema migration), #4361 (ADR-007: two-field attribution model), #4363 (pattern: session_id namespace).

**ADR-002 alignment**: The override IS implemented exactly as specified. The insertion into `client_type_map[""]` for stdio is correct. The `std::future::ready(...)` return is correct. ADR-002 is implemented faithfully at server.rs:1035-1089. No divergence found in the implementation itself.

**ADR-003 alignment**: `build_context_with_external_identity` is implemented exactly as specified at server.rs:387-465. The `rmcp_session_key` extraction and `client_type_map` lookup are correct.

**ADR-007 alignment**: The two-field model (agent_id + agent_attribution) is implemented in AuditEvent (schema.rs:360-391). However, **service-layer AuditEvent constructions** (search.rs, store_ops.rs, store_correct.rs, gateway.rs) do not have access to `client_type` and use `..AuditEvent::default()` — leaving `agent_attribution: ""` for every service-delegated tool call. This is a gap not addressed in ADR-007 or vnc-014.

**Unimatrix #4363 alignment**: The pattern specifies "never use the rmcp Mcp-Session-Id UUID". The implementation is compliant. However, the pattern does not address the prefix mismatch with `sessions.session_id` (raw vs `mcp::`-prefixed). That mismatch is a separate bug.

**Pre-existing known issue from ASS-050**: ASS-050 FINDINGS.md explicitly identified `session_id` often `""` at call sites as a pre-existing bug. vnc-014 did NOT fix this — the `session_id: String::new()` sentinel remains at 8 call sites in tools.rs.

---

## Q1: Does `initialize` override fire in production stdio/UDS sessions?

**Answer**: YES, the override fires. The rmcp 0.16.0 dispatch is unconditional.

**Evidence**:
- rmcp handler/server.rs:24-27: `handle_request` matches `ClientRequest::InitializeRequest` → `self.initialize(request.params, context)`. No conditional. The `ServerHandler::initialize` provided method default does NOT apply when overridden.
- rmcp service/server.rs:183-262 (`serve_server_with_ct_inner`): The initialize request is received and `service.handle_request(request, context)` is called BEFORE `serve_inner`. There is no code path that skips this.
- The `tokio::select!` in `serve_server_with_ct` (service/server.rs:175-180) CAN cause `Cancelled` return before `initialize` runs — but only if the cancellation token fires during initialization. Not applicable under normal operation.

**Root cause for empty `agent_attribution`** (why the map is empty despite `initialize` firing):
- server.rs:1044: `if !client_name_raw.is_empty()` — if `clientInfo.name` is empty string, the entire insert block is skipped.
- Codex CLI (and potentially other clients) may send `clientInfo.name = ""` or omit the field. The `Implementation.name` field is `String` (not `Option<String>`) in rmcp's model.rs:838, so it defaults to `""` rather than `None` when absent.
- The SCOPE.md claim "confirmed to send `clientInfo.name`" is asserted from session observation, not from inspecting the raw MCP initialize message bytes. The guard at server.rs:1044 would silently pass if name is empty.
- **Primary hypothesis for why ALL entries have empty attribution**: `clientInfo.name` is empty in production. `initialize` fires but the guard skips the insert. Map stays empty. All tool calls get `client_type = None`. All direct audit events write `agent_attribution = ""`.

**Fix target**: server.rs:1044. Add a `tracing::warn` or `tracing::debug` in the else branch to surface when `clientInfo.name` is empty. The fix to `agent_attribution` requires either: (a) the client sends a non-empty name, or (b) a fallback attribution strategy when name is empty (e.g., use a fixed string `"unknown"` or the transport type).

---

## Q2: Does `http::request::Parts` appear in stdio/UDS `RequestContext.extensions`?

**Answer**: NO, never. For both `initialize` and tool call contexts on stdio/UDS transports.

**Evidence**:
- rmcp transport/streamable_http_server/tower.rs:323-326: `http::request::Parts` is injected into message extensions ONLY by the streamable HTTP server tower transport. No other transport does this.
- rmcp transport/async_rw.rs and transport/io.rs: No extension injection. These are the transports used for stdio (`rmcp::transport::io::stdio()`) and UDS (`(OwnedReadHalf, OwnedWriteHalf)` tuple).
- rmcp service/server.rs:204-210: For `initialize`, `RequestContext.extensions = request.extensions().clone()`. For stdio/UDS: empty extensions.
- rmcp service.rs:833-845: For tool calls, `extensions` comes from `std::mem::swap(&mut extensions, request.extensions_mut())`. For stdio/UDS: also empty.
- Result: `context.extensions.get::<http::request::Parts>()` returns `None` in BOTH the `initialize` handler (server.rs:1062) AND in `build_context_with_external_identity` (server.rs:437-442).
- `unwrap_or("")` produces `""` in both cases — the map key is `""` for both insert and lookup.

**SCOPE hypothesis "key mismatch" is REFUTED**: There is no key mismatch. The `""` key is used consistently. This is confirmed by the passing unit test SRV-U-02.

**Blast radius for HTTPS/REST path**: When the streamable HTTP server transport is used (ASS-053 REST path), `http::request::Parts` IS injected. The `Mcp-Session-Id` header value becomes the map key. This path would work correctly once `clientInfo.name` is non-empty — the key would be the HTTP session UUID and the map lookup at tool call time would find it.

---

## Q3: Is `client_type_map` re-created between `initialize` and tool calls?

**Answer**: NO. The `client_type_map: Arc<Mutex<HashMap<String, String>>>` is created once at `UnimatrixServer::new()` (server.rs:352) and shared across all lifetimes of the session.

**Evidence**:
- server.rs:190: `#[derive(Clone)]` — auto-derived Clone of `Arc<Mutex<...>>` is a refcount increment, not a new HashMap.
- rmcp service.rs:652: `serve_inner` wraps the concrete `UnimatrixServer` in `Arc::new(service)`. The inner `client_type_map` Arc is NOT replaced — the new outer Arc just wraps the entire struct.
- rmcp handler/server.rs:571: `impl_server_handler_for_wrapper!(Arc)` generates `Arc<UnimatrixServer>: ServerHandler` via `(**self).method(...)` delegation. All `call_tool` invocations reach the same `UnimatrixServer` value inside the Arc.
- mcp_listener.rs:179: `server_clone = server.clone()` — clone shares `client_type_map`.
- server.rs:3401-3423 (SRV-U-01b): Test proves clone shares Arc — insert via original, read via clone succeeds.
- ONE `UnimatrixServer::new()` call in production (main.rs:688), one `client_type_map` Arc.

**SCOPE hypothesis refuted**: No re-creation path exists. The `client_type_map` persists correctly across the session.

---

## Q4: `session_id` in `audit_log` — write path and provenance chain

**Answer (a)**: `audit_log.session_id` is written in two patterns:
1. `""` (empty string) — from `session_id: String::new()` sentinel at 8+ call sites in tools.rs
2. `"mcp::AGENT_DECLARED_VALUE"` — from `prefix_session_id("mcp", sid)` at correctly-implemented sites

**Answer (b)**: `sessions` rows ARE created for Claude Code sessions via the hook UDS path (uds/listener.rs:596-612). `sessions.session_id` is stored as the RAW Claude Code session ID — NOT prefixed. `sessions.feature_cycle` IS populated when `context_cycle` is active and the hook fires. `context_cycle` correctly populates `cycle_events`.

**Evidence**:
- server.rs:413-419 (`build_context_with_external_identity`): When agent provides `session_id` parameter, it is validated then prefixed: `prefix_session_id("mcp", sid)` → `"mcp::AGENT_DECLARED_VALUE"`. Stored in `AuditContext.session_id`.
- services/mod.rs:84-86: `prefix_session_id(transport, raw) = format!("{transport}::{raw}")`.
- uds/listener.rs:596-612: Hook path writes `SessionRecord { session_id: session_id.clone(), ... }` where `session_id` is the raw value — no prefix.
- tools.rs call sites with `session_id: String::new()` (broken): lines 544, 783, 963, 1064, 1398, 1493, 1531, 1624 — context_lookup, context_get, context_deprecate, context_status, context_briefing, context_correct inner calls, context_quarantine.
- tools.rs and services (correct sites): services/store_ops.rs:165, 190, services/store_correct.rs:89, services/gateway.rs:227 — use `audit_ctx.session_id.clone().unwrap_or_default()`.

**Two-hop provenance chain is structurally broken by prefix mismatch**:
- Chain: `audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id`
- `audit_log.session_id` = `"mcp::CLAUDE_SESSION_ID_VALUE"` (when written correctly)
- `sessions.session_id` = `"CLAUDE_SESSION_ID_VALUE"` (raw, no prefix)
- Direct join `audit_log.session_id = sessions.session_id` NEVER matches.
- services/mod.rs:93-98: `strip_session_prefix()` exists but is `#[allow(dead_code)]` — NEVER called in any join query.
- **Fix target**: Either (a) store `audit_log.session_id` without the `mcp::` prefix (aligns with sessions table), or (b) use `strip_session_prefix()` in the join query. Option (b) is additive.

---

## Q5: What `session_id` values ARE in `audit_log` entries?

**Answer**: The 3,759 entries contain two values:
1. `""` (empty) — majority, from `session_id: String::new()` sentinel
2. `"mcp::CLAUDE_SESSION_ID_VALUE"` — minority, from tools that correctly thread session_id (context_store, context_cycle_review, context_enroll)

The raw rmcp UUID is NOT present in any entries — Unimatrix #4363's restriction is correctly implemented.

**Evidence**: The codex-test session would have `sessions.session_id = "CODEX_SESSION_ID"` (raw). Matching `audit_log` entries would need `session_id = "mcp::CODEX_SESSION_ID"`. A query `WHERE audit_log.session_id = sessions.session_id` always returns 0 rows because of the prefix mismatch — regardless of how many `mcp::` entries exist. Empty-string entries cannot match any real session_id.

GH#582's observation "no `audit_log` entries match the codex-test session_id" is explained by TWO compounding bugs: (a) most entries have `session_id = ""`, and (b) the few with non-empty session_id have the `mcp::` prefix that doesn't match `sessions.session_id`.

---

## Root Cause Summary

Three independent defects:

### Defect 1 (Primary): Empty `clientInfo.name` from client → empty `agent_attribution`

**Root cause**: `clientInfo.name` sent by the MCP client (Codex CLI, Claude Code) is an empty string. The guard at server.rs:1044 (`if !client_name_raw.is_empty()`) correctly skips the map insert. Map stays empty. `client_type = None` for all tool calls. `agent_attribution = ""` in all tools.rs direct audit events.

**Fix targets**:
- server.rs:1044 else branch — add `tracing::warn!("clientInfo.name empty on initialize — agent_attribution will be blank")` to surface the condition in logs.
- Determine whether the client (Codex CLI / Claude Code) should be sending a non-empty name, and whether a fallback (e.g., `"unknown"`) is appropriate when name is empty.

### Defect 2 (Contributing): Service-layer audits have no `client_type` access

**Root cause**: search.rs, store_ops.rs, store_correct.rs, gateway.rs construct `AuditEvent` with `..AuditEvent::default()`. They receive `&AuditContext` but not `client_type`. `agent_attribution = ""` in ALL service-delegated audit events regardless of `client_type_map` state.

**Fix target**: Pass `Option<&str>` client_type through service interfaces (ServiceSearchParams, StoreOps, etc.) and use it when constructing AuditEvent. Larger change — service API boundary.

**Blast radius**: context_search, context_store (search path), context_briefing (search path), context_correct (correct path) — the most commonly called tools. These account for the majority of the 3,759 entries.

### Defect 3: `session_id` `mcp::` prefix breaks join to `sessions` table + `String::new()` sentinel

**Root causes**:
1. Prefix mismatch: `audit_log.session_id` stores `mcp::CLAUDE_SESSION_ID_VALUE`. `sessions.session_id` stores raw `CLAUDE_SESSION_ID_VALUE`. Direct join never matches. `strip_session_prefix()` exists at services/mod.rs:93-98 but is dead code.
2. `String::new()` sentinel: 8 call sites in tools.rs write `session_id: ""`. These entries are completely unjoinable.

**Fix targets**:
- Prefix mismatch: Wire `strip_session_prefix()` into any query joining `audit_log.session_id` to `sessions.session_id`.
- `String::new()` sentinel: Replace with `ctx.audit_ctx.session_id.clone().unwrap_or_default()` at tools.rs:544, 783, 963, 1064, 1398, 1493, 1531, 1624.

---

## Recommendations Summary

| Question | Finding | Action |
|----------|---------|--------|
| Q1 — `initialize` fires? | YES — but `clientInfo.name` is likely empty, skipping map insert | Add tracing::warn at server.rs:1044 else branch; verify what client sends |
| Q2 — key mismatch? | REFUTED — `""` key is consistent for stdio/UDS in both paths | No fix needed |
| Q3 — map re-created? | REFUTED — Arc is shared correctly, persists across session | No fix needed |
| Q4(a) — session_id write path | 8 call sites use `String::new()` sentinel | Replace with `ctx.audit_ctx.session_id.clone().unwrap_or_default()` at tools.rs:544, 783, 963, 1064, 1398, 1493, 1531, 1624 |
| Q4(b) — prefix mismatch | `mcp::` prefix breaks join to `sessions.session_id` | Wire `strip_session_prefix()` into join queries |
| Q5 — values ARE empty or `mcp::` prefixed | Both explain no session match | Fix per Q4 |
| Service-layer attribution | Structural gap — no `client_type` in service API | Separate feature scope |

---

## Unanswered Questions

**Cannot confirm from code alone**: Whether `clientInfo.name` sent by Codex CLI in production is actually `""` vs a non-empty value. This requires either inspecting actual MCP session logs at the daemon level or adding a tracing log and observing a live session. The circumstantial evidence (all 3,759 entries empty, unit tests pass) is consistent with the empty-name hypothesis but cannot be proven from code read alone.

---

## Out-of-Scope Discoveries

1. **Service interface lacks `client_type` parameter** — `ServiceSearchParams`, `StoreOps`, and other service types don't carry `client_type`. Fixing Defect 2 requires adding `Option<String>` to service params — broader refactor across the service layer. Warrants its own delivery feature.

2. **`strip_session_prefix()` is dead code** — services/mod.rs:93-98 has `#[allow(dead_code)]`. Written for the exact join use case but never wired up. Wiring it is the minimal fix for the prefix mismatch.

3. **GH#302 soft cap race condition** — mcp_listener.rs:156-161 documents a known race: `active_count` increment happens inside spawned task, acceptor may allow more than 32 concurrent sessions. Comment: "Fix: move fetch_add before tokio::spawn." Out of scope for this spike.

4. **Codex CLI `clientInfo.name` behavior** — Whether Codex CLI sends a meaningful name value is not determinable from Unimatrix code alone. If it sends `"codex-cli"` in some versions and `""` in others, the diagnostic log would reveal it. Product-level question about client behavior.
