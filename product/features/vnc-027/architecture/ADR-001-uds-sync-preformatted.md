## ADR-001: UDS Sync Responses Are Server-Side Preformatted via Additive `accept` Field + `HookResponse::Text` Variant

### Context

Sync-injection formatting is transport-asymmetric (Unimatrix #4798): HTTP /observe
formats server-side via `Accept: text/plain` content negotiation (vnc-024 ADR-003);
UDS returns typed `HookResponse` frames the client must format
(`format_injection`, hook.rs:1034, ~40 parity-critical lines). The TS UDS client
(vnc-027) needs sync responses it can print. A JS `format_injection` port is the
single largest size-budget risk (SR-02; the client sits at 99,997/100,000 bytes),
duplicates the formatting truth (vnc-025 ADR-005 lesson: shared core = parity by
construction), and becomes dead weight at F6. uni-zero recommended server-side
preformatted; this ADR confirms the wire mechanism (SR-03: must be additive
against the frozen F1 contract).

### Decision

1. **Request side (additive)**: add `accept: Option<String>` with
   `#[serde(default, skip_serializing_if = "Option::is_none")]` to
   `HookRequest::ContextSearch` and `HookRequest::CompactPayload` in `wire.rs`.
   Value `"text/plain"` mirrors the HTTP Accept header. Not added to `Ping`/
   `Briefing` (Pong stays JSON — handshake parses `server_version`, vnc-024 OQ-06).
2. **Set by the transport, not the builder**: `transport-uds.js` injects
   `accept: "text/plain"` at serialization time for sync injection-bearing frames
   (`type === "ContextSearch" || "CompactPayload"`), exactly as `transport-http.js`
   sets the Accept header. `build-request*.js` and queued frames never carry it —
   the queue stays transport-agnostic and HTTP frame goldens stay byte-unchanged.
3. **Response side (additive)**: new variant `HookResponse::Text { body: String }`
   (serde tag `"type"`). `body` is the exact bytes the HTTP text/plain path would
   return: `Entries` → `format_injection(items, MAX_INJECTION_BYTES=1400)` output
   (including the load-bearing `--- Unimatrix Context ---\n` header — vnc-024
   ADR-003 amendment); `BriefingContent` → `content` verbatim.
4. **Empty injection**: `format_injection` → `None` (the HTTP 204 case) → server
   returns existing `Ack`; the client maps it to a 204-equivalent SendResult
   (no stdout). No new variant needed.
5. **One formatting truth**: factor the response→text mapping shared by
   `observe_response_to_http` (http/router/observe.rs:25) and the new UDS branch
   into one `pub(crate)` core in unimatrix-server (exact module placement is a
   delivery detail; the single-implementation requirement is binding). The UDS
   branch lives in `listener.rs::handle_connection`: extract `wants_text` from the
   deserialized request BEFORE `dispatch_request` (dispatch unchanged), convert the
   response after. Allowlist is a hard contract: only `Entries` and
   `BriefingContent` convert; `Ack`/`Error`/`Pong` always stay JSON.
6. **Freeze compliance**: hook.rs constructs `ContextSearch` (step 5b) and
   `CompactPayload` (build_request); adding the field forces mechanical
   `accept: None` additions at those construction sites (plus tests and the parity
   corpus generator). This is within the "Rust hook untouched" constraint:
   `skip_serializing_if` keeps serialized bytes identical — proven by the existing
   parity goldens passing byte-unchanged (explicit AC required). No other hook.rs
   change is permitted.

### Consequences

Easier: no `format_injection` JS port — the size-budget driver disappears (SR-02);
stdout parity is by construction (same fn produces UDS Text bodies, HTTP text
bodies, and Rust-hook stdout); the TS client reuses its entire HTTP sync path
downstream of the transport (transform.js untouched); F6 retires hook.rs without
relocating formatting truth.

Harder: `wire.rs` gains a variant old Rust hooks cannot deserialize — safe only
because a `Text` response is returned exclusively to callers that sent `accept`,
which the frozen Rust hook never does (this coupling is the contract); ts-rs
bindings regenerate (additive — drift check must confirm); mechanical hook.rs
edits require the byte-unchanged golden proof in CI; a future text-eligible
response type must revisit this ADR (same allowlist rule as vnc-024 ADR-003).

Cross-references: vnc-024 ADR-003 (+ amendment: injection header is a wire
contract), vnc-025 ADR-005 (shared-core parity), ADR-002 (SendResult mapping),
ADR-003 (socket lifecycle).
