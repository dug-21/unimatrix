# Agent Report — vnc-027-agent-3-listener-preformatted

Feature: vnc-027 (F4a, #680). Stage 3b, Component 5 (listener-preformatted).
ADR-001 §5,§6 / AC-03, AC-11 / R-07, R-08, R-09.

## Summary

Factored the response→injection-text mapping into ONE `pub(crate)` shared core
(`response_injection_text`) in `http/router/observe.rs`, consumed by BOTH the HTTP
text/plain path and the new UDS `Text` branch. Added the two `handle_connection`
seams in `uds/listener.rs` as pure, unit-testable functions: `request_wants_text`
(seam 1, pre-dispatch `accept` extraction) and `negotiate_text_response` (seam 2,
post-dispatch conversion). `format_injection(items, MAX_INJECTION_BYTES=1400)`
remains the single formatting truth — unchanged.

## Files modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/http/router/observe.rs` — added shared core `response_injection_text`; refactored `observe_response_to_http` text branch to use it (externally byte-identical).
- `/workspaces/unimatrix/crates/unimatrix-server/src/http/router.rs` — `observe` module made `pub(crate)` so the UDS listener can reach the shared core.
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener.rs` — `handle_connection` extracts `wants_text` pre-dispatch (`request_wants_text`) and converts post-dispatch (`negotiate_text_response`); dispatch path unchanged; registered `preformatted` test submodule.
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener/tests/preformatted.rs` — NEW: 12 component unit tests.

`uds/hook.rs` required no change: the mechanical `accept: None` construction-site
edits already landed with the wire component (commit 910de4b0). `wire.rs` already
carried `HookResponse::Text` and the `accept` fields.

## Contract enforcement

- **Text-only-to-accept-callers allowlist ENFORCED**: `negotiate_text_response`
  returns the untouched typed frame when `wants_text == false` (every frozen Rust
  hook frame). Only `Entries`/`BriefingContent` convert; `Ack`/`Error`/`Pong`
  always stay JSON regardless of `accept`. Unknown `accept` values (e.g.
  `application/xml`, empty) are inert — only exact `"text/plain"` qualifies.
- **Empty injection → Ack** (204-equivalent), never a headerless `Text`.
- **HTTP-vs-UDS body byte-equivalence HOLDS**: both paths call the same
  `response_injection_text` core. Proven directly by
  `test_http_text_plain_and_uds_text_body_byte_identical` (collects the HTTP
  text/plain body and compares bytes to the UDS `Text` body) and structurally by
  `test_shared_core_single_implementation`.

## Tests

`cargo test -p unimatrix-server --lib`: 3620 passed, 0 failed, 1 ignored
(pre-existing). The 12 new `preformatted` tests all pass. `cargo build --workspace`
clean; `cargo fmt` applied; no new clippy warnings on touched files;
`unimatrix-engine` wire AC-11 byte-unchanged fixtures still pass (101 passed).

Integration tests (Stage 3c) NOT run, per scope.

## Issues / blockers

None. `listener.rs` is pre-existing large (~8.9k lines); tests added as a focused
submodule under `uds/listener/tests/` per the established split pattern (500-line
rule honored — no monolithic growth). The `test_subagentstop_all_none_fallthrough`
cross-reference is owned by the hook-set-reduction component (ADR-004), not this one.

`mcp/tools.rs` showed as modified in this shared checkout from concurrent agent
work — NOT staged in this commit.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced #4802 (ADR-001 vnc-027), #4795 (vnc-024 ADR-003 content negotiation), #4743 (vnc-025 ADR-005 shared-core parity-by-construction), #3455 (handle_connection EPIPE/EOF DEBUG classification), #4798 (transport-asymmetric formatting pattern this feature retires). All applied.
- Stored: nothing novel to store — the implementation is a textbook application of the already-stored vnc-025 ADR-005 shared-core parity pattern (#4743) and the vnc-024 content-negotiation allowlist (#4795). No new gotcha discovered beyond what the ADRs already capture.
