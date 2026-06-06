# Agent Report — vnc-026-agent-1-pseudocode (Stage 3a)

## Deliverables

All under `product/features/vnc-026/pseudocode/`:

| File | Component |
|---|---|
| OVERVIEW.md | Component interaction, shared types, wire-serialization rules (null-vs-omit table), pipeline, build order, hard limits |
| index.md | Entry/dispatch; readStdin fd-0; parseHookInput serde-parity (incl. unknown-field flatten preservation, ass-071) |
| config.md | ADR-006 resolution; root walk; project hash; state dir; timeout-override key shape |
| normalize.md | mapToCanonical / normalizeEventName table ports |
| build-request.md | Full build_request port (hook.rs:440-951) + topic-signal chain (attribution.rs) + validate_cycle_params port |
| transcript.md | transcript_block.rs port; byte-budget + BufRead::lines() UTF-8-line-drop parity |
| transport-http.md | POST /observe; timeouts; failure classification; strict pingForInit |
| transform.md | ADR-002 literal templates; R-15 content-type defense |
| delta.md | Offset tracking; end-anchored elision (ADR-008) with effectiveEnd boundary handling; uniform advance; never-queued (ADR-004) |
| queue.md | ADR-003 mini-spec; O_EXCL; bounds; bounded replay; poison-pill |
| state.md | Atomic writes; sanitized keys; breadcrumb aggregation rule (pinned) |
| init-remote.md | init --remote branch; mergeSettings commandSource generalization; **WARN 2 spaced-path regex fix** |
| parity-corpus.md | Rust generator design (private-fn access via cfg(test) module), MANIFEST, CI drift wiring (R-20), Layer 1/2 suites |

## Components Covered
index, config, normalize, build-request, transcript, transport-http, transform, delta,
queue, state, init-remote, parity-corpus — all 12 from the brief's Component Map.

## Design Decisions Made at Pseudocode Level (delivery-visible)

1. **WARN 2 fix (the open gate note)**: command builder quotes spaced paths
   (`buildHookClientCommand`); ONE new ownership regex with quoted/unquoted/backslash
   alternations replaces the defective `\S*` form; positive/negative test table included.
   require.resolve output shapes documented per platform (POSIX forward-slash, Windows
   drive+backslash/UNC, both may contain spaces).
2. **Elision anchor uses `effectiveEnd`** = fileLen backed off ≤3 bytes when the file
   ends mid-UTF-8-char (normally == fileLen). All ADR-008 pinned assertions hold with
   fileLen ≡ effectiveEnd; uniform ADR-004 advance preserved. This closes a gap the ADR
   leaves implicit (oversized span + file ending mid-char).
3. **Breadcrumb aggregation rule pinned** (no ADR named it): all-attempted-success →
   success/reset; any failure → failure, carrying-event class wins over delta class;
   config-miss increments consecutive_failures.
4. **Timeout override key names pinned**: `unimatrix.remote.timeouts.{connect_ms,sync_ms,fnf_ms}`
   (no ADR names them — flag for leader sign-off).
5. **Stdout-golden generation**: write_stdout* writes to real stdout and cannot be
   captured in-process without refactoring hook.rs (forbidden, C-07). Generator
   reconstructs the envelope/println bytes with the SAME serde_json expressions
   (format_injection is pub(crate) and called directly). One accepted oracle-indirection,
   documented in parity-corpus.md.
6. **Parse-failure parity**: wrong-typed named stdin field collapses the WHOLE input to
   the empty HookInput (serde semantics), `extra = null` vs `{}` distinction preserved.
7. **Replay does not gate the carrying send** (Rust run() parity — best-effort replay,
   then send regardless); out-of-order delivery accepted, server order-tolerant.
8. **Generator runs as `#[ignore]`d same-crate cfg(test) module** (private-fn access);
   CI non-vacuity via "1 passed" grep + MANIFEST mtime + zero-diff (R-20).

## Open Questions / Gaps for the Leader

1. Timeout-override key names (decision 4 above) — accept or rename before delivery.
2. Short (<5-word) UserPromptSubmit builds a RecordEvent → FNF path (hook.rs:244-251
   classification by request type). Over HTTP the host still awaits process exit, so a
   degraded server can hold a short prompt up to the 3,000 ms FNF deadline (+ concurrent
   delta, same deadline). Faithful to the architecture; flagging the latency profile.
3. Remote `Ping` hook event prints nothing (Pong is JSON; sync path drops
   non-text/plain 200 — R-15). Rust prints Pong JSON locally. Documented deviation in
   OVERVIEW; confirm no consumer depends on remote Ping stdout.
4. JS object key ordering reorders integer-like keys (e.g. stdin extra key `"1"`)
   relative to serde_json preserve_order. AC-01 comparison is structural so requests are
   unaffected; only a hypothetical byte-compare of payloads would see it. No action
   needed; recorded so nobody "discovers" it mid-delivery.
5. `extract_key_param` fallback iterates object fields in insertion order — same caveat
   as 4 for integer-like input keys; corpus should avoid/include one such case
   deliberately (tester's call).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4714 (text/plain negotiation),
  #4720 (transcript_delta event_type), #4743/#4740/#4739 (vnc-025 buffer + shared
  transcript core), #4758 (ADR-008 end-anchored), #4751 (ADR-001 corpus), #4306
  (provider field); all incorporated. context_search (pattern) returned #4703 — folded
  into transport/transform header-assertion scenarios; decision search confirmed the
  vnc-026 ADR set matches the on-disk ADR files.
- Deviations from established patterns: none — pseudocode follows the ADRs; the two
  documented behavioral deviations from hook.rs::run() (PreCompact client-side prepend
  removed; remote Ping stdout dropped) are architecture-mandated, not pattern deviations.

## Self-Check
- [x] Architecture + all 8 ADRs + spec + risk strategy read before writing
- [x] No invented interface names — all traced to architecture Integration Surface,
      bindings fixtures, hook.rs/transcript_block.rs/wire.rs/attribution.rs/validation.rs,
      or existing init.js/merge-settings.js
- [x] Per-component output (OVERVIEW + 12 files), no monolith
- [x] Every file: signatures, error handling, test scenarios
- [x] No TODO/TBD — gaps flagged explicitly above
- [x] Shared types/limits defined once in OVERVIEW and referenced
- [x] All outputs within product/features/vnc-026/
