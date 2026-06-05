# Agent Report — vnc-024-agent-5-observe-content-negotiation (Stage 3b, Wave 1, Component 3)

> Reconstructed by the Delivery Leader: the primary agent's connection dropped before returning
> (temp-filesystem exhaustion). Its production edits landed; the existing-test fixups + new
> content-negotiation tests were completed by recovery agent `vnc-024-agent-5b`. End-state verified
> against committed HEAD (`0096c58e`/`8aa2d5ce`) + test runs.

## Scope
Component 3 — `/observe` HTTP content negotiation (Deliverable 2, ADR-003).

## Files modified
- `crates/unimatrix-server/src/http/router.rs` — capture `Accept` → `wants_text: bool` BEFORE `request.into_parts()` (~:206); pass to the mapper at the `observe_response_to_http` call (~:260).
- `crates/unimatrix-server/src/http/router/observe.rs` — signature `observe_response_to_http(resp, wants_text)`; `text/plain` branch for `Entries` (body == `format_injection(items, MAX_INJECTION_BYTES)`) and `BriefingContent` only; `Pong`/`Ack`/`Error` stay JSON; non-text path unchanged.
- `crates/unimatrix-server/src/uds/hook.rs` — `format_injection` (:1047) bumped to `pub(crate)`; visibility-only, no re-implementation.
- `crates/unimatrix-server/src/http/router/tests.rs` — 7 existing call sites updated with `wants_text: false` (preserve JSON intent); 8 new content-negotiation unit tests (AC-07 byte-identity incl. over-budget truncation + empty→204; AC-08 all variants JSON; AC-09 BriefingContent text + Pong/Ack/Error stay JSON).

## Tests
54 router tests pass (incl. 8 new). `format_injection`/`MAX_INJECTION_BYTES` imported from `crate::uds::hook` — single formatting truth (Constraint 4 / AC-07).

## Confirmed
Production injection budget reused for the text path: `MAX_INJECTION_BYTES = 1400` (`hook.rs:29`). `Accept` read strictly before `into_parts()` (Constraint 2). UDS path untouched (AC-10). HTTP-boundary ordering + UDS golden parity are Stage 3c integration scope.

## Knowledge Stewardship
- Queried: `context_search` for HTTP content-negotiation / Accept-header tower patterns + vnc-024 ADRs (ADR-003). Recovery agent re-confirmed the mapper surface.
- Stored: nothing novel — the change is an additive, reversible mapper branch reusing an existing formatter; the only trap (read `Accept` before `into_parts()`) is already captured in ADR-003 / the brief. The lone compile-time gotcha found by the recovery agent (`EntryPayload.id` is `u64`, not `i64`) is compiler-caught, not a runtime-invisible pattern.
