# Component: listener-preformatted (`uds/listener.rs` + `http/router/observe.rs` + mechanical `uds/hook.rs`)

ADR-001 §2,§4,§5,§6. FR-17, FR-19(rejected — preformatted chosen), AC-03, AC-11.
Risks R-07, R-08, R-09. Merge step 2 (with wire-accept-text).

## Purpose

Make the UDS listener return server-side preformatted sync responses for callers
that send `accept`, using ONE shared injection-text core also consumed by the HTTP
`/observe` path — so UDS `Text` bodies, HTTP text/plain bodies, and Rust-hook stdout
are byte-identical by construction (vnc-025 ADR-005 lesson). The mechanical
`accept: None` edits at hook.rs construction sites are an approved variance, nothing
else in hook.rs changes.

## New: shared injection-text core (one `pub(crate)` fn)

Factor the response→text mapping currently inline in
`observe_response_to_http` (observe.rs:25-46) into ONE function consumed by both the
HTTP path and the new UDS branch (ADR-001 §5). Exact module placement is a delivery
detail; single-implementation is binding. Suggested home: alongside
`observe_response_to_http` or in `uds/hook.rs` next to `format_injection`.

```
/// Single formatting truth shared by HTTP /observe and the UDS Text branch.
/// Returns Some(text) for the two injection-bearing variants, None otherwise
/// (caller maps None → 204 / Ack). Allowlist is exactly {Entries, BriefingContent}.
pub(crate) fn response_injection_text(resp: &HookResponse) -> Option<String> {
    match resp {
        HookResponse::Entries { items, .. } =>
            format_injection(items, MAX_INJECTION_BYTES),   // includes header; None when empty/over-budget
        HookResponse::BriefingContent { content, .. } =>
            Some(content.clone()),                          // verbatim, NO header
        _ => None,                                          // Pong/Ack/Error never convert
    }
}
```

`format_injection(&[EntryPayload], MAX_INJECTION_BYTES=1400) -> Option<String>`
(hook.rs:1034) stays the single formatting truth — unchanged.

## Modified: `observe_response_to_http(resp, wants_text)` (observe.rs:25)

Refactor only — externally byte-identical (AC-12 / AC-03 HTTP path). Replace the
inline `match` in the `wants_text` branch with the shared core:

```
if wants_text {
    if let Some(text) = response_injection_text(&resp) {
        return http_200_text_plain(text);
    }
    // Entries that formatted to None → 204 (unchanged); Pong/Ack/Error fall through to JSON.
    if matches!(resp, HookResponse::Entries { .. }) { return http_204_no_content(); }
}
// ... unchanged JSON envelope below
```

Behavior MUST stay identical to today: `Entries→Some` →200 text; `Entries→None`
→204; `BriefingContent`→200 text; `Pong`/`Ack`/`Error`→JSON. The HTTP
`prefix_session_id` `"http-"` rewrite on ingest is UNCHANGED (SR-10 — UDS does not
prefix).

## Modified: `handle_connection` (listener.rs:377) — the UDS Text branch (ADR-001 §5)

The current flow reads header→length→payload→deserialize→`dispatch_request`→
`write_response`. Insert two seams: extract `wants_text` BEFORE dispatch (dispatch
is unchanged and consumes the request), convert the response AFTER dispatch.

```
// ... existing: auth, read_exact(4)=header, length validation (0 / >MAX rejected
//     with Error before allocating), read_exact(length)=buffer, deserialize request.

// NEW seam 1 — extract wants_text BEFORE dispatch (request is moved into dispatch).
wants_text = match &request {
    HookRequest::ContextSearch { accept, .. } => accept.as_deref() == Some("text/plain"),
    HookRequest::CompactPayload  { accept, .. } => accept.as_deref() == Some("text/plain"),
    _ => false,                                    // accept exists on no other variant
};

response = dispatch_request(request, ...).await;   // UNCHANGED

// NEW seam 2 — convert AFTER dispatch, ONLY when the caller asked (coupling, ADR-001 §6).
wire_response =
    if wants_text {
        match response_injection_text(&response) {
            Some(text) => HookResponse::Text { body: text },   // Entries(header) / BriefingContent(verbatim)
            None => match response {
                HookResponse::Entries { .. } => HookResponse::Ack,  // empty injection → 204-equiv (ADR-001 §4)
                other => other,                                     // Pong/Ack/Error/BriefingContent(None n/a) stay
            },
        }
    } else {
        response                                   // no accept → typed frame unchanged (R-07 s2)
    };

write_response(&mut writer, &wire_response).await   // EPIPE on FIN'd FNF socket stays DEBUG (#3448)
```

**Hard allowlist (ADR-001 §5, R-08 s3)**: only `Entries` and `BriefingContent` ever
become `Text`. `Ack`/`Error`/`Pong` always stay JSON, regardless of `accept`.

**Coupling (ADR-001 §6, R-08)**: when `wants_text == false` (no `accept`, e.g. every
frozen Rust hook frame), the response is the untouched typed frame — `Text` is never
sent to a caller that didn't ask, so a frozen hook never receives an undeserializable
variant.

## Modified (mechanical only): `uds/hook.rs` — approved variance (ADR-001 §6, brief)

Adding `accept` to `ContextSearch`/`CompactPayload` forces the compiler to demand the
field at construction sites in hook.rs (the `ContextSearch` build at step 5b and the
`CompactPayload` build in `build_request`), plus parity tests and the corpus
generator. The ONLY permitted change: add `accept: None` at each construction site.

```
// at every ContextSearch { .. } and CompactPayload { .. } construction in hook.rs:
ContextSearch { /* existing fields */, accept: None }
CompactPayload  { /* existing fields */, accept: None }
```

No behavioral change: `skip_serializing_if` keeps serialized bytes identical (AC-11
proves byte-unchanged). No other hook.rs edit is permitted (R-08 s4 runs the real
frozen binary end-to-end against the updated daemon).

## State / sequencing

`handle_connection` per-connection lifecycle is otherwise unchanged: one frame per
connection, FIN-after-flush from the client is harmless (listener reads exactly one
frame then writes). Truncation cannot be silently ingested — `read_exact(len)`
rejects short frames (ADR-003 §6).

## Error handling

- Declared length 0 or > MAX_PAYLOAD_SIZE → `Error{ERR_INVALID_PAYLOAD}` before
  allocating (existing behavior, retained).
- Deserialize failure → `Error{ERR_INVALID_PAYLOAD}` (existing).
- EPIPE/BrokenPipe writing the response to a FIN'd FNF socket → DEBUG, treated as
  success (existing #3448 handling, retained).

## Key test scenarios (hints for tester)

1. `ContextSearch`/`CompactPayload` WITHOUT `accept` → typed `Entries`/`Ack` JSON,
   never `Text` (coupling) — R-07 s2.
2. WITH `accept:"text/plain"` → `Text` only for `Entries`/`BriefingContent`;
   `Ack`/`Error`/`Pong` stay JSON regardless — R-08 s3.
3. Empty injection (`format_injection`→None) with `accept` → `Ack` (204-equiv), no
   stdout downstream — ADR-001 §4 edge case.
4. `Text{body}` for Entries starts with `--- Unimatrix Context ---\n` byte-exactly;
   BriefingContent body is `content` verbatim (no header) — R-09 s2.
5. Shared-core equivalence: HTTP text/plain body and UDS `Text` body for the same
   request are byte-identical — R-09 s3 (the parity backbone).
6. Compiled frozen Rust hook end-to-end against the updated daemon: full sync trio
   unchanged — R-08 s4 (strongest coupling proof).
7. AC-11: pre-existing Rust fixtures + ts-rs bindings pass byte-unchanged including
   the mechanical `accept: None` edits — R-07 s1.
