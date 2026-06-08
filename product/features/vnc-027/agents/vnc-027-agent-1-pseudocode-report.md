# Agent Report — vnc-027-agent-1-pseudocode

Stage 3a pseudocode for vnc-027 (TS UDS hook client + hook-set reduction, F4a, #680).

## Deliverables

`product/features/vnc-027/pseudocode/`:
- OVERVIEW.md — component map, shared types, data flow, merge sequencing, cross-cutting contracts
- size-gate.md (1)
- wire-accept-text.md (2)
- listener-preformatted.md (3)
- transport-uds.md (4)
- config-transport-selection.md (5)
- index-dispatch.md (6)
- build-request-sentinel.md (7)
- merge-settings-reduction.md (8)
- state-offset-rekey.md (9)
- parity-corpus-uds.md (10)

All 10 Component-Map components covered + OVERVIEW. No deviation from the architecture's decomposition.

## Grounding

Every signature traced to existing source (read in full): transport-http.js (oracle),
config.js, index.js, build-request-tools.js, state.js, merge-settings.js,
transform.js, the existing size gate, and the Rust seams (wire.rs ContextSearch/
CompactPayload/HookResponse, observe.rs::observe_response_to_http,
listener.rs::handle_connection, hook.rs::format_injection). No invented interfaces.

## Pinned ADR specifics (override looser spec wording)

- ADR-006 age-prune-only: SessionClose delete removed; `pruneOffsets` is the sole
  effective mechanism; `TaskCompleted` branch keyed by canonical event name (never
  frame type), unreachable-but-unit-tested, NOT in HOOK_EVENTS. The keying change
  lives in index.js (caller); state.js functions are behaviorally unchanged.
- ADR-001 Text↔accept coupling: `Text` returned ONLY to callers that sent `accept`;
  hard allowlist {Entries, BriefingContent}; `wants_text` extracted pre-dispatch,
  converted post-dispatch; mechanical `accept: None` at hook.rs construction sites is
  the only permitted hook.rs edit.
- ADR-005 size gate is the literal first commit; stripper in test/, does not count
  against budget; self-test fails closed.
- Frozen F1 wire contract additive-only (skip_serializing_if optionals + one new
  variant); merge sequencing (size-gate FIRST) reflected in OVERVIEW.

## Open questions / gaps for delivery

1. **merge-settings opt-OUT removal (merge-settings-reduction.md)**: ADR-004 §2 says
   SubagentStop is "registered only when the opt-in key is set" but does not
   explicitly require stripping a previously-registered Unimatrix SubagentStop entry
   on opt-out. AC-08's "on/off matrix" implies bidirectional behavior, so the
   pseudocode proposes pruning stale Unimatrix-owned entries for any HOOK_EVENTS event
   absent from the resolved set (reusing isUnimatrixHook). Confirm this is intended vs
   "additive-only, never remove."
2. **wire.rs field placement**: `accept` is added at the END of ContextSearch (after
   `source`) and CompactPayload (after `transcript_excerpt`). serde JSON object key
   presence is order-independent and `skip_serializing_if` keeps absent-when-None, so
   AC-11 byte-parity holds; flagged only so the ts-rs binding diff is expected to be
   additive (new optional field), not a reorder.
3. **canonicalEvent source in index.js**: the FR-16 keying uses `canonical` from
   `normalize.normalizeEventName(rawEvent)` (not `effectiveEvent`, not
   `request.type`). RISK-TEST R-04 confirms normalize.js recognizes `TaskCompleted`;
   delivery should assert the canonical string is exactly `"TaskCompleted"` so the
   pinning unit test keys correctly.
4. **R-12 server-independence test** is owned by the parity/integration layer
   (no-SubagentStop full lifecycle); flagged in merge-settings-reduction.md and
   parity-corpus-uds.md as a contract assertion, not a code change.

No blocking gaps. No TODO/placeholder content in any file.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) and grounded against
  ADR entries #4806/#4802/#4803 and patterns #4798 (transport-asymmetry, drove
  ADR-001), #2582 (hook IPC uses 4-byte BE length prefix — confirms framing),
  #300 (UDS capability/auth boundary), #4743 (vnc-025 shared-core parity-by-
  construction — supports the ADR-001 single-formatting-truth). All consistent with
  the ADRs; no contradictions found.
- Deviations from established patterns: none. The UDS framing (4-byte BE length
  prefix), shared-core formatting, and transport-seam adapter all follow existing
  Unimatrix conventions.
