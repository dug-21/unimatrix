# FINDINGS: Root Cause Analysis — agent_attribution Empty + session_id Linkage Broken in audit_log

**Spike**: ASS-056
**Date**: 2026-05-07
**Approach**: investigation (code trace + Unimatrix ADR alignment)
**Confidence**: empirical — every finding pinned to file:line

---

## Unimatrix Alignment

Relevant entries retrieved: #4356 (ADR-002: initialize override design), #4357 (ADR-003: build_context_with_external_identity), #4358 (ADR-004: schema migration), #4361 (ADR-007: two-field attribution model), #4363 (pattern: session_id namespace).

**ADR-002**: The `initialize` override is implemented faithfully at server.rs:1035-1089. Insertion into `client_type_map[""]` for stdio is correct. `std::future::ready(...)` return is correct. No implementation divergence.

**ADR-003**: `build_context_with_external_identity` is implemented as specified at server.rs:387-465. `rmcp_session_key` extraction and `client_type_map` lookup are correct.

**ADR-007**: The two-field model (`agent_id` + `agent_attribution`) is correctly defined in `AuditEvent` (schema.rs:360-391). However, service-layer callers — search.rs, store_ops.rs, store_correct.rs, gateway.rs — construct `AuditEvent` with `..AuditEvent::default()`. They receive `&AuditContext` but not `client_type`. This leaves `agent_attribution = ""` for every service-delegated audit event. This gap is not addressed in ADR-007 or vnc-014.

**Entry #4363**: The restriction against using the rmcp `Mcp-Session-Id` UUID in `audit_log.session_id` is correctly implemented. The entry does not address the `mcp::` prefix mismatch with `sessions.session_id` — that is a separate defect.

**Pre-existing known issue**: ASS-050 FINDINGS.md explicitly identified `session_id` often `""` at call sites as a pre-existing bug. vnc-014 did not fix it. The `session_id: String::new()` sentinel remains at 8 call sites in tools.rs.

---

## Findings

### Q1: Is the `initialize` override on `UnimatrixServer` actually invoked in production stdio sessions?

**Answer**: Yes, the override fires unconditionally. However, the map insert is skipped when `clientInfo.name` is an empty string — which is the probable root cause of all 3,759 entries having `agent_attribution = ""`.

**Evidence**:
- rmcp handler/server.rs:24-27: `handle_request` matches `ClientRequest::InitializeRequest` unconditionally and calls `self.initialize(request.params, context)`. No conditional guards.
- rmcp service/server.rs:183-262 (`serve_server_with_ct_inner`): The initialize request is received and `service.handle_request(request, context)` is called before `serve_inner`. There is no skip path under normal operation.
- server.rs:1044: `if !client_name_raw.is_empty()` — if `clientInfo.name` is `""` or absent, the insert block is bypassed entirely. Map stays empty. All subsequent tool calls get `client_type = None` and write `agent_attribution = ""`.
- rmcp model.rs:838: `Implementation.name` is `String` (not `Option<String>`). An absent `clientInfo.name` field deserializes to `""`, not `None`.
- The SCOPE.md claim that the codex session "confirmed to send `clientInfo.name`" was not confirmed by raw message bytes — the guard at server.rs:1044 would silently pass with an empty name.

**Recommendation**: Add `tracing::warn!("clientInfo.name empty on initialize — agent_attribution will be blank for this session")` in the else branch at server.rs:1044. Verify via daemon-level MCP session logs what value Codex CLI / Claude Code actually sends in the `initialize` message. If `clientInfo.name` is consistently empty, introduce a fallback (e.g., `"unknown"` or transport type string) as a default.

---

### Q2: Does `http::request::Parts` appear in stdio/UDS `RequestContext.extensions`?

**Answer**: No, never — for either the `initialize` context or per-tool-call contexts on stdio/UDS transports.

**Evidence**:
- rmcp transport/streamable_http_server/tower.rs:323-326: `http::request::Parts` is injected into message extensions only by the streamable HTTP tower transport. No other transport does this.
- rmcp transport/async_rw.rs and transport/io.rs: No extension injection. These are the transports used for stdio (`rmcp::transport::io::stdio()`) and UDS (`(OwnedReadHalf, OwnedWriteHalf)` tuple).
- rmcp service/server.rs:204-210: For `initialize`, `RequestContext.extensions = request.extensions().clone()`. For stdio/UDS: empty extensions.
- rmcp service.rs:833-845: For tool calls, `extensions` comes from `std::mem::swap(&mut extensions, request.extensions_mut())`. For stdio/UDS: also empty.
- `context.extensions.get::<http::request::Parts>()` returns `None` in both the `initialize` handler (server.rs:1062) and in `build_context_with_external_identity` (server.rs:437-442). `unwrap_or("")` produces `""` in both paths.

**SCOPE hypothesis "key mismatch between initialize and tool call" is REFUTED.** The `""` key is used consistently in both paths. Confirmed by passing unit test SRV-U-02. No fix is needed for key consistency.

**Blast radius (HTTPS/REST)**: When the streamable HTTP server transport is used (the ASS-053 REST path), `http::request::Parts` IS injected and the `Mcp-Session-Id` header value becomes the map key. That path would work correctly once `clientInfo.name` is non-empty — the HTTP session UUID key would be inserted at `initialize` and found at each tool call.

---

### Q3: Is `client_type_map` re-created between `initialize` and tool calls?

**Answer**: No. The `Arc<Mutex<HashMap<String, String>>>` is created once at `UnimatrixServer::new()` and shared across the entire session lifetime.

**Evidence**:
- server.rs:352: `client_type_map: Arc::new(Mutex::new(HashMap::new()))` — single allocation.
- server.rs:190: `#[derive(Clone)]` — auto-derived Clone on `Arc<Mutex<...>>` is a refcount increment, not a new HashMap.
- rmcp service.rs:652: `serve_inner` wraps `UnimatrixServer` in `Arc::new(service)`. The inner `client_type_map` Arc is not replaced.
- rmcp handler/server.rs:571: `impl_server_handler_for_wrapper!(Arc)` generates delegation via `(**self).method(...)`. All `call_tool` invocations reach the same `UnimatrixServer` value.
- mcp_listener.rs:179: `server_clone = server.clone()` — shares the same `client_type_map` Arc.
- server.rs:3401-3423 (SRV-U-01b): Test confirms clone shares the Arc.

**SCOPE hypothesis "re-creation after initialize" is REFUTED.** No re-creation path exists. No fix is needed.

---

### Q4: `session_id` write path and provenance chain integrity

**Answer (a)**: `audit_log.session_id` is written in two patterns:
1. `""` (empty string) — from `session_id: String::new()` sentinel at 8 call sites in tools.rs (lines 544, 783, 963, 1064, 1398, 1493, 1531, 1624 — context_lookup, context_get, context_deprecate, context_status, context_briefing, context_correct inner calls, context_quarantine).
2. `"mcp::AGENT_DECLARED_VALUE"` — from `prefix_session_id("mcp", sid)` at correctly-implemented sites (services/store_ops.rs:165, 190; services/store_correct.rs:89; services/gateway.rs:227).

**Answer (b)**: `sessions` rows ARE created for Claude Code sessions via the hook UDS path (uds/listener.rs:596-612). `sessions.session_id` stores the raw Claude Code session ID — no prefix. `sessions.feature_cycle` is populated when `context_cycle` is active and the hook fires. `context_cycle` correctly populates `cycle_events`.

**Two-hop provenance chain is structurally broken by a prefix mismatch**:
- Designed chain: `audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id`
- `audit_log.session_id` = `"mcp::CLAUDE_SESSION_ID_VALUE"` (at correct call sites)
- `sessions.session_id` = `"CLAUDE_SESSION_ID_VALUE"` (raw, no prefix)
- A direct join `WHERE audit_log.session_id = sessions.session_id` never matches.
- `strip_session_prefix()` exists at services/mod.rs:93-98 but is `#[allow(dead_code)]` and is never called in any join query.

**Evidence**:
- server.rs:413-419: Agent-declared `session_id` is prefixed: `prefix_session_id("mcp", sid)` → `"mcp::AGENT_DECLARED_VALUE"`. Stored in `AuditContext.session_id`.
- services/mod.rs:84-86: `prefix_session_id(transport, raw) = format!("{transport}::{raw}")`.
- uds/listener.rs:596-612: Hook writes `SessionRecord { session_id: session_id.clone(), ... }` with raw value.

**Recommendation**: Wire `strip_session_prefix()` (services/mod.rs:93-98) into any query joining `audit_log.session_id` to `sessions.session_id`. This is the minimal additive fix — no new logic required. Also replace `session_id: String::new()` at tools.rs:544, 783, 963, 1064, 1398, 1493, 1531, 1624 with `ctx.audit_ctx.session_id.clone().unwrap_or_default()`.

---

### Q5: What `session_id` values ARE being written to `audit_log`?

**Answer**: The 3,759 entries contain exactly two values:
1. `""` (empty string) — majority, from the `String::new()` sentinel at 8 call sites.
2. `"mcp::CLAUDE_SESSION_ID_VALUE"` — minority, from tools that correctly thread session_id (context_store, context_cycle_review, context_enroll).

The raw rmcp UUID is absent — entry #4363's restriction is correctly implemented.

GH#582's observation ("no audit_log entries match the codex-test session_id") is explained by two compounding bugs:
- **Bug A**: Most entries have `session_id = ""` and cannot match any real session.
- **Bug B**: The few entries with non-empty `session_id` carry the `mcp::` prefix that doesn't match `sessions.session_id` in the join, so the provenance chain fails even for the minority of correctly-written entries.

**Evidence**: The codex-test session would produce `sessions.session_id = "CODEX_SESSION_ID"` (raw). Matching audit entries would need `session_id = "mcp::CODEX_SESSION_ID"`. The direct-equality join returns 0 rows because of the prefix mismatch.

---

## Unanswered Questions

**What value does Codex CLI / Claude Code actually send for `clientInfo.name` in production?** This cannot be confirmed from code read alone. The circumstantial evidence (all 3,759 entries empty, unit tests pass) is consistent with the empty-name hypothesis but is not proof. Confirmation requires adding a tracing log at server.rs:1044's else branch and observing a live session, or inspecting raw MCP initialization message bytes at the daemon level.

---

## Out-of-Scope Discoveries

1. **Service interface lacks `client_type` parameter** — `ServiceSearchParams`, `StoreOps`, and other service types do not carry `client_type`. Fixing the service-layer attribution gap requires adding `Option<String>` to service params — a broader refactor across the service layer. Warrants its own delivery feature separate from the targeted sentinel fix.

2. **`strip_session_prefix()` is dead code** — services/mod.rs:93-98 is marked `#[allow(dead_code)]`. Written for exactly the join use case but never wired up. Wiring it is the minimal fix for the prefix mismatch; no new logic required.

3. **GH#302 soft cap race condition** — mcp_listener.rs:156-161 documents a known race: `active_count` increment happens inside the spawned task, so the acceptor may allow more than 32 concurrent sessions. Comment: "Fix: move fetch_add before tokio::spawn." Out of scope for this spike.

4. **Codex CLI `clientInfo.name` behavior is unknown** — Whether Codex CLI sends a meaningful name value is not determinable from Unimatrix code. This is a product-level question about client behavior, not a Unimatrix defect.

---

## Recommendations Summary

| Question | Finding | Action |
|----------|---------|--------|
| Q1 — `initialize` fires? | YES — unconditionally; but `clientInfo.name` likely `""`, guard skips insert | Add `tracing::warn!` at server.rs:1044 else branch; verify client's actual initialize payload |
| Q2 — key mismatch? | REFUTED — `""` key consistent for stdio/UDS in both paths | No fix needed |
| Q3 — map re-created? | REFUTED — Arc shared correctly across session lifetime | No fix needed |
| Q4(a) — `session_id` sentinel | 8 tools.rs sites use `String::new()` | Replace with `ctx.audit_ctx.session_id.clone().unwrap_or_default()` at lines 544, 783, 963, 1064, 1398, 1493, 1531, 1624 |
| Q4(b) — prefix mismatch | `mcp::` prefix breaks join to `sessions.session_id` | Wire existing `strip_session_prefix()` (services/mod.rs:93-98) into join queries |
| Q5 — values in `audit_log` | `""` (majority) or `"mcp::<value>"` (minority); raw UUID absent | Both Q4 bugs must be fixed together to restore provenance chain integrity |
| Service-layer attribution gap | No `client_type` in service API boundary | Separate delivery feature — out of scope for targeted bugfix |
