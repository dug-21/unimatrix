# Test Plan — observe-content-negotiation (Deliverable 2)

> Covers AC-07, AC-08, AC-09, AC-10. Risks R-05 (byte-identity / budget drift), R-06 (non-injection
> response text-formatted), R-07 (Accept read after `into_parts`). Pseudocode:
> `pseudocode/observe-content-negotiation.md`. Files: `http/router.rs` (Accept read), `http/router/observe.rs`
> (`observe_response_to_http(resp, wants_text)`), `uds/hook.rs` (`format_injection` → `pub(crate)`).
> **All assertions are at the HTTP boundary** — a unit test on the mapper in isolation hides the
> ordering bug (R-07) and the budget-coupling bug (R-05). Integration tests = `unimatrix-server`
> `#[tokio::test]` against the `POST /observe` tower handler (NOT the MCP stdio harness — see OVERVIEW
> integration plan).

## Scope of this component

HTTP-only content negotiation: `Accept: text/plain` → server-formatted injection text for `Entries`
and `BriefingContent` **only**; `Pong`/`Ack`/`Error` and absent/`application/json` Accept stay JSON;
UDS path untouched. The text path **calls** `format_injection` (`hook.rs:1047`), never re-implements it.

## R-05 — byte-identity & budget parity (AC-07)

- **test_observe_text_entries_byte_identical**: `POST /observe` with `Accept: text/plain` producing an
  `Entries` response → assert `Content-Type: text/plain` **and** body bytes **==** a direct
  `format_injection(&items, max_bytes)` call for the same entries.
- **Budget parity (critical):** the `max_bytes` the text path passes **equals the constant the
  production UDS caller uses** (OQ-1). Test with an entry set **large enough to cross the truncation
  boundary**, so a wrong budget produces a detectable length difference — a small happy-path set would
  let a wrong-but-self-consistent budget pass.
- **Over-budget / truncation case** (Edge): an `Entries` set exceeding `max_bytes` → body matches
  `format_injection`'s truncated output exactly (not a server re-truncation).
- **Empty Entries** (Edge): `format_injection` returns `None` → **204 no-content** (not 200 empty, not
  500), matching ADR-003.
- **Structure/reviewer check (Constraint 4):** the text path calls `hook.rs:1047`, not a
  re-implementation; `format_injection` is `pub(crate)` and there is exactly one formatting source.

## R-06 — allowlist is exactly {Entries, BriefingContent} (AC-09)

Under `Accept: text/plain`:
- **test_observe_text_briefingcontent_returns_text**: `BriefingContent` → 200 `text/plain`, body =
  formatted content (positive control proving the allowlist includes BriefingContent, AC-09).
- **test_observe_text_pong_stays_json**: `Pong` → 200 **JSON** envelope, `server_version` parseable
  (text would break the F2 handshake).
- **test_observe_text_ack_stays_json**: `Ack` → **204** (unchanged).
- **test_observe_text_error_stays_json**: `Error` → **400 JSON** (unchanged).

All three non-injection responses asserted to stay JSON under `text/plain`; both injection responses
asserted to honor it. The allowlist is a hard contract.

## R-07 — Accept read before `into_parts` (AC-07, AC-08)

The header must be read **before** `request.into_parts()` (`router.rs:~202`, mirroring the
CONTENT_LENGTH read at `:191-196`), or it is silently lost.

- **test_observe_text_entries_content_type_text**: `Accept: text/plain` Entries → assert the response
  `Content-Type` is **actually** `text/plain` (proves the header survived `into_parts`). Asserting on
  the negotiated content-type at the HTTP boundary — not status alone — is what catches the ordering bug.
- **test_observe_no_accept_returns_json**: no `Accept` header → JSON; assert content-type is JSON.
- **wants_text predicate** (Edge / OQ-3): `Accept: text/plain, application/json` and `Accept: */*` →
  define and assert the predicate ("contains `text/plain`" ⇒ text). At least one multi-value and one
  wildcard case.

## AC-08 — JSON envelope unchanged with/without header

Over each response type, with `Accept: application/json` and with **no** header, assert status + JSON
body **identical to pre-change behavior**: `Ack`→204, `Entries`/`BriefingContent`/`Pong`→200 JSON,
`Error`→400 JSON. Negotiated content-type asserted at the HTTP boundary (R-07), not just status.

## R-10 (mapper)/AC-10 — UDS path unchanged

- **test_uds_hook_output_unchanged**: UDS hook round-trip / golden comparison — assert the UDS
  transport output is **identical before and after** this change (additive HTTP-only change; UDS clients
  continue to receive JSON and format locally). `format_injection`'s visibility bump must not alter its
  behavior or any UDS caller's result.

## Integration risk notes (from RISK strategy)

- **Mapper signature change** `observe_response_to_http(resp, wants_text)` touches the single existing
  caller (`router.rs:~250`). The `wants_text` bool must be computed **before** `into_parts` and threaded
  correctly — testing the mapper in isolation hides the ordering bug, so assert at the HTTP boundary.
- **Budget coupling (R-05):** the text path's `max_bytes` is implicitly coupled to the UDS hook's
  injection constant; if that constant changes later, byte-identity silently breaks unless both reference
  one source. The AC-07 test uses the production constant; OVERVIEW OQ-1 flags confirming a single source.

## Edge cases

- Empty Entries → 204 (None from `format_injection`).
- Over-budget Entries → truncation boundary matches `format_injection` exactly.
- `Accept: text/plain, application/json` (multi-value) and `Accept: */*` → predicate behavior.
- `format_injection` returning `None` must be **204, not 500** (Failure Modes table).

## Out of scope for this plan

- `transcript_delta` accept-and-drop / zero-rows (AC-12) → `transcript-delta-guard.md` (shares the
  `/observe` HTTP entry but is a fire-and-forget `Ack`, never text — ADR-003/ADR-004).

## Self-check
- [ ] AC-07 byte-identity vs real `format_injection` with PRODUCTION budget, incl. truncation + empty(204).
- [ ] AC-09 allowlist: Entries+BriefingContent honor text; Pong/Ack/Error stay JSON; Pong.server_version parseable.
- [ ] AC-07/08 negotiated content-type asserted at HTTP boundary (catches R-07 ordering bug); no-Accept → JSON.
- [ ] wants_text predicate tested for multi-value + wildcard Accept.
- [ ] AC-10 UDS output identical before/after (golden compare).
- [ ] Reviewer: text path CALLS format_injection (no re-impl); single formatting source (Constraint 4).
