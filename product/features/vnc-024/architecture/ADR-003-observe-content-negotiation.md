## ADR-003: HTTP /observe Content Negotiation — text/plain for Entries and BriefingContent Only

### Context
ass-068 Q4 moves response formatting server-side so the TS client's transform surface shrinks to
host-envelope serialization. Today `/observe` returns JSON `HookResponse` envelopes; a non-Rust
client would re-implement `format_injection` (~40 parity-critical lines, `hook.rs:1047`). The server
already has `format_injection` and the `Entries` data. Two integration hazards: (1) the `Accept`
header must be read before `request.into_parts()` (`router.rs:203`) consumes the request — a late
read silently yields JSON when text was requested (SR-08); (2) any text path that re-implements
`format_injection` breaks the byte-identical gate now or later (Constraint 4, SR-09). Not every
response is safe to emit as text: `Pong` carries a structured `server_version` the client parses
during handshake — emitting it as text would break the handshake (OQ-06).

### Decision
Add HTTP-only content negotiation to `/observe`:

1. In `http/router.rs`, read `request.headers().get(http::header::ACCEPT)` and compute
   `wants_text: bool` (true iff the value contains `text/plain`) **before** `request.into_parts()`,
   mirroring the existing CONTENT_LENGTH read at `router.rs:191-196`.
2. Thread `wants_text` into the mapper: change `observe_response_to_http(resp)` →
   `observe_response_to_http(resp, wants_text)` (`http/router/observe.rs:18`).
3. In the mapper, text applies **only** to the two injection-bearing responses:
   - `Entries { items, .. }` + `wants_text` → call `format_injection(&items, max_bytes)` →
     200 `Content-Type: text/plain` with the byte-identical body (AC-07). On `None` (empty/over-budget)
     return 204, matching the no-content semantics.
   - `BriefingContent { content, .. }` + `wants_text` → 200 `text/plain` body = `content` (AC-09).
   - All others — `Ack` (204), `Error` (400 JSON), `Pong` (200 JSON), and any response when the
     `Accept` header is absent or `application/json` — return the current JSON envelope, unchanged
     (AC-08/AC-09).
4. `format_injection` (`crates/unimatrix-server/src/uds/hook.rs:1047`) is promoted to `pub(crate)`
   and called by the text path. **No re-implementation** — it is the single source of formatting
   truth (Constraint 4); the byte-identical gate (AC-07) holds because the same function produces the
   text on both the UDS hook path and the server text path.
5. The UDS hook path is untouched — UDS clients continue to receive JSON and format locally; output
   is identical before and after (AC-10, Constraint 2).

### Consequences
**Easier**: The future TS client receives pre-formatted injection text and only wraps it in a
host-specific envelope (~40 lines), shrinking its parity-risk surface. The change is purely additive
— a new branch in the mapper — and fully reversible. One formatting function, no duplication.

**Harder**: The mapper signature changes, touching the single existing caller (`router.rs:250`).
`format_injection` gains a crate-visibility bump. The text path needs a `max_bytes` budget argument;
it must reuse the same injection budget the production UDS caller uses or AC-07 byte-identity fails
(flagged as a delivery detail). The two-response allowlist (`Entries`/`BriefingContent`) is a hard
contract: adding a future text-eligible response requires revisiting this ADR.

Cross-references: vnc-022 (#669) `observe_response_to_http` / `prefix_session_id`; ADR-004 (the
`transcript_delta` guard shares the same dispatch path but is a fire-and-forget `Ack`, never text).
