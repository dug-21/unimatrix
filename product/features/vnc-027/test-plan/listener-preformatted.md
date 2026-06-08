# Test Plan — listener-preformatted (`listener.rs` + `observe.rs` shared injection core)

Component 5 / ADR-001 §5 / FR-17 / **AC-03, AC-11** / Risks R-08 (Med), R-09 (Med).
`handle_connection` extracts `wants_text` (presence of `accept`) BEFORE `dispatch_request` (dispatch unchanged),
converts the response AFTER via ONE `pub(crate)` shared injection-text core consumed by both `observe_response_to_http`
and the new UDS branch. Allowlist is a HARD contract. Rust `cargo test -p unimatrix-server`.

## Unit expectations — wants_text extraction & allowlist (R-08, the coupling contract)

- `test_no_accept_yields_typed_json_never_text` — `ContextSearch`/`CompactPayload` WITHOUT `accept` → response is typed `Entries`/`Ack`/`BriefingContent` JSON, NEVER `Text` (ADR-001 §6; frozen-hook safety).
- `test_accept_text_plain_entries_yields_text` — `accept: "text/plain"` + `Entries` result → `HookResponse::Text { body }`.
- `test_accept_text_plain_briefing_yields_text` — `accept: "text/plain"` + `BriefingContent` → `Text { body: content }` verbatim.
- `test_allowlist_ack_error_pong_always_json` — even WITH `accept`, `Ack`/`Error`/`Pong` stay JSON (hard allowlist; a `Pong` carrying `server_version` must remain parseable JSON — vnc-024 OQ-06).
- `test_unknown_accept_value_treated_as_absent` — `accept: "application/xml"` (or any non-`text/plain`) behaves as no-accept → typed JSON (security: unknown negotiation values are inert).
- `test_empty_injection_yields_ack_not_text` — `format_injection` → `None` (empty `Entries`) → `Ack` (204-equivalent), NOT a headerless `Text` (ADR-001 §4).
- `test_wants_text_extracted_pre_dispatch` — dispatch path is unchanged; `wants_text` is read from the deserialized request before `dispatch_request` is called (assert dispatch receives the same request shape as today).

## Shared-core / injection-header expectations (R-09 — the #4778 bug class)

- `test_entries_text_body_starts_with_injection_header` — `Text` body for `Entries` begins byte-exactly with `--- Unimatrix Context ---\n` (the load-bearing wire contract, vnc-024 ADR-003 amendment). An unrenderable `Entries` yields 204/`Ack`, never a headerless 200.
- `test_briefing_text_body_has_no_injection_header` — `BriefingContent` `Text` body is `content` verbatim (starts with the fixed `CONTEXT_GET_INSTRUCTION` constant), no header prepended — the wire-distinguishable difference transform.js dispatches on.
- `test_shared_core_single_implementation` — `format_injection(&[EntryPayload], MAX_INJECTION_BYTES=1400)` is the single formatting truth; the UDS branch and `observe_response_to_http` call the SAME `pub(crate)` core (no duplicated formatting). Asserted structurally (one function) + behaviorally below.
- `test_http_text_plain_and_uds_text_body_byte_identical` — for the same request, the HTTP `Accept: text/plain` body and the UDS `Text` body are byte-identical (parity-by-construction, vnc-025 ADR-005). (Cross-checked end-to-end in parity-corpus-uds.md.)

## FNF / SubagentStop independence (cross-references)

- `test_subagentstop_all_none_fallthrough` (R-12) — `"SubagentStop" | _ => (None, None, None, None)` at `listener.rs:2919`: no server lifecycle (session close, buffer finalization) awaits SubagentStop. Pinned here as a Rust unit; the full-lifecycle proof is the node:test in parity-corpus-uds.md.
- EPIPE on Ack write to a FIN'd FNF socket stays DEBUG-classified (#3448) — assert no WARN-level noise from a normal TS FNF (also asserted at the live-listener integration layer).

## Edge cases
- 1 MiB `Entries` result truncated by `format_injection` (MAX_INJECTION_BYTES=1400) → `Text` body is the truncated formatted output, header intact.
- `CompactPayload` with `accept` but empty buffer → server-built empty block path; allowlist still routes correctly.
