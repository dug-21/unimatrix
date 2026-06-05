# Component 3 — /observe Content Negotiation (Deliverable 2)

> ADR-003. `Accept: text/plain` → server-formatted injection text for `Entries`/`BriefingContent`
> ONLY. HTTP-only, additive, reversible. UDS untouched. `format_injection` is the single formatting
> truth — call it, never re-implement (Constraint 4).

## Purpose

Let a future TS client (F2) receive pre-formatted injection text from the server instead of
formatting locally, by honoring `Accept: text/plain` on `/observe`. The JSON envelope is unchanged
for every other case. `Pong`/`Ack`/`Error` stay JSON regardless of `Accept`.

## Files

| File | Action | Anchor |
|------|--------|--------|
| `crates/unimatrix-server/src/uds/hook.rs` | Modify — visibility bump only | `format_injection` :1047 → `pub(crate)`; `MAX_INJECTION_BYTES` :29 → `pub(crate)` |
| `crates/unimatrix-server/src/http/router.rs` | Modify — read `Accept` → `wants_text` before `into_parts()` | ~:202, thread into call at :250 |
| `crates/unimatrix-server/src/http/router/observe.rs` | Modify — `observe_response_to_http(resp, wants_text)`; text branch | :18 |

## hook.rs — visibility bump (NO re-implementation)

```
// :29  was: const MAX_INJECTION_BYTES: usize = 1400;
pub(crate) const MAX_INJECTION_BYTES: usize = 1400;

// :1047 was: fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>
pub(crate) fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String> { ... }
```
- The text path MUST pass `MAX_INJECTION_BYTES` — the SAME constant the production UDS caller uses
  (`hook.rs:979`, `:1031`) — so AC-07 byte-identity holds (R-05, resolves ARCHITECTURE open
  question: budget = `MAX_INJECTION_BYTES = 1400`).
- No logic change to `format_injection`. Body unchanged. (Constraint 4 / SR-09.)

## router.rs — read Accept BEFORE into_parts (ORDERING is load-bearing)

The existing handler reads `CONTENT_LENGTH` at :191-196 while `request` is still whole, then consumes
it at `:203` via `into_parts()`. Insert the `Accept` read in the same window (R-07 / SR-08):

```
// AFTER the CONTENT_LENGTH check (:196), BEFORE `let (_parts, body) = request.into_parts();` (:203):
LET wants_text: bool =
    request.headers()
        .get(http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/plain"));   // predicate: substring match

// :203 (unchanged): let (_parts, body) = request.into_parts();
```
- Reading after `into_parts()` silently loses the header → wrong content-type, no error (R-07).
- `wants_text` predicate: **contains `text/plain`**. Covers `Accept: text/plain`,
  `Accept: text/plain, application/json`, but NOT `Accept: */*` and NOT absent header (→ JSON).
  (Edge cases enumerated in RISK-TEST-STRATEGY.)

Thread `wants_text` into the mapper at the existing single call site (:250):
```
// Step 7 (:250): was  Ok(observe_response_to_http(response))
Ok(observe_response_to_http(response, wants_text))
```

## observe.rs — mapper signature change + text branch

Change the signature (the only caller is router.rs:250):
```
pub(crate) fn observe_response_to_http(
    resp: HookResponse,
    wants_text: bool,
) -> Response<BoxBody<Bytes, Infallible>>
```

Add the text branch BEFORE the existing JSON match. Allowlist is exactly `{Entries, BriefingContent}`
(R-06):
```
FUNCTION observe_response_to_http(resp, wants_text):
    IF wants_text:
        MATCH resp:
            HookResponse::Entries { items, .. }:
                // call the SINGLE formatting truth with the production budget
                MATCH format_injection(&items, MAX_INJECTION_BYTES):
                    Some(text) -> RETURN http_200_text_plain(text)      // AC-07 byte-identical
                    None       -> RETURN http_204_no_content()          // empty/over-budget → 204 (ADR-003 edge)
            HookResponse::BriefingContent { content, .. }:
                RETURN http_200_text_plain(content)                     // AC-09 (emit content directly)
            _:
                // Pong / Ack / Error under text/plain → fall through to JSON (R-06)
                // (do NOT text-format these; Pong.server_version is parsed structured by the client)
                <fall through to the existing JSON match below>
    // existing JSON envelope — UNCHANGED for all non-text-negotiated cases (AC-08):
    MATCH resp:
        Ack                                      -> 204 No Content
        Entries | BriefingContent | Pong         -> 200 + application/json
        Error                                    -> 400 + application/json
```

### Helper (new, observe.rs)
```
FUNCTION http_200_text_plain(body: String) -> Response:
    Response::builder()
        .status(200)
        .header("content-type", "text/plain")     // NOT application/json
        .body(Full::new(Bytes::from(body)).map_err(never).boxed())
```
Reuse the existing 204 builder shape for `http_204_no_content`.

## Data flow

```
HookResponse + wants_text
  ├ Entries + text          ─► format_injection(items, MAX_INJECTION_BYTES)
  │                              ├ Some(text) ─► 200 text/plain   (AC-07)
  │                              └ None       ─► 204              (empty/over-budget edge)
  ├ BriefingContent + text  ─► 200 text/plain body=content       (AC-09)
  ├ Pong/Ack/Error + text   ─► JSON (text ignored)               (AC-09 / R-06)
  └ any response + !text     ─► JSON envelope UNCHANGED           (AC-08)
```

## Error handling

- `format_injection` returning `None` (empty `Entries`, or over-budget with <100 bytes room) → 204,
  NOT 500 (failure-mode table). Matches ADR-003.
- JSON serialization failure in the unchanged branch → existing `internal_error_response()` path,
  untouched.
- Malformed `Accept` (non-UTF-8) → `.to_str().ok()` yields `None` → `wants_text = false` → JSON.
  Bounded, no panic.

## Boundaries / constraints honored

- **HTTP-only**: UDS hook path (`hook.rs` stdout injection at :979/:1031) is untouched (AC-10,
  Constraint 2). Only the visibility of `format_injection`/`MAX_INJECTION_BYTES` changes.
- **Additive**: the JSON branch is byte-for-byte unchanged; the text branch is new code in front of
  it (NFR-02 / NFR-05 reversibility).
- **Single formatting truth**: text path CALLS `format_injection`; no re-implementation (Constraint 4).
- **Payload ceiling unchanged**: `MAX_PAYLOAD_SIZE` and the `/observe` body limit (router.rs:42)
  authoritative; no per-delta cap added (NFR-06).

## Key test scenarios (hints — full plan in test-plan/observe-content-negotiation.md)

- **AC-07 (R-05)**: `POST /observe` `Accept: text/plain` for an `Entries` response → `Content-Type:
  text/plain`, body **byte-identical** to a direct `format_injection(&items, MAX_INJECTION_BYTES)`
  call — including an over-budget/truncation entry set, asserted at the HTTP boundary (not the
  mapper in isolation, which hides the ordering bug — R-07).
- **AC-08**: `application/json` or no header → JSON envelope unchanged for every response type.
- **AC-09 / R-06**: `BriefingContent` under `text/plain` → text; `Pong`/`Ack`/`Error` under
  `text/plain` → JSON (`Pong.server_version` still parseable).
- **R-07**: `text/plain` Entries actually returns `text/plain` (proves header survived `into_parts`);
  no header → JSON. Assert the negotiated content-type on both branches.
- **Edge**: empty `Entries` under `text/plain` → 204. `Accept: */*` → JSON. Multi-value
  `text/plain, application/json` → text.
- **AC-10**: UDS hook output identical before/after (golden comparison).

## Open questions / gaps

- **RESOLVED here**: the text path budget is `MAX_INJECTION_BYTES` (=1400), the constant the
  production UDS caller already passes (`hook.rs:979`/`:1031`). This closes the ARCHITECTURE
  §Open-Questions item and R-05's budget-parity concern: both paths reference one constant.
