# Agent Report: vnc-022-agent-1-pseudocode

## Task
Produce per-component pseudocode for vnc-022 Remote Observation Transport (5 components).

## Output Files

| File | Lines | Component |
|------|-------|-----------|
| `product/features/vnc-022/pseudocode/OVERVIEW.md` | ~80 | Component interaction, data flow, wave ordering |
| `product/features/vnc-022/pseudocode/compact-payload-wire.md` | ~60 | CompactPayload transcript_excerpt field |
| `product/features/vnc-022/pseudocode/capability-extension.md` | ~50 | SessionWrite added to HTTP capabilities |
| `product/features/vnc-022/pseudocode/dispatch-request-refactor.md` | ~100 | pub(crate) + capabilities parameter |
| `product/features/vnc-022/pseudocode/observe-context.md` | ~130 | ObserveContext struct + PathRouter + main.rs |
| `product/features/vnc-022/pseudocode/observe-handler.md` | ~250 | Full /observe handler + response mapper |

## Components Covered

1. **compact-payload-wire** -- Single field addition to CompactPayload in wire.rs
2. **capability-extension** -- One line change in auth.rs (add SessionWrite to vec)
3. **dispatch-request-refactor** -- Mechanical: pub(crate), 9 call sites, 1 new param
4. **observe-context** -- ObserveContext struct (router.rs), PathRouter field, main.rs construction, http/mod.rs re-export
5. **observe-handler** -- 501 stub replacement with full handler: body parsing, deser, prefix, dispatch, response mapping. Three new functions: observe_response_to_http, prefix_session_id, json_error_response

## Open Questions

1. **ObserveContext visibility**: Architecture says `pub(crate)` but main.rs is a binary target that accesses the library via `use unimatrix_server::http::...`. Binary targets in the same Cargo.toml see the library as external. Therefore ObserveContext and its fields must be `pub`, not `pub(crate)`. Pseudocode uses `pub`. Implementation agent should verify this compiles.

2. **sanitize_session_id visibility**: Currently `fn sanitize_session_id` (private in listener.rs). The observe handler does NOT call it directly -- `dispatch_request` calls it internally for each arm. The `prefix_session_id` function runs BEFORE dispatch, so the prefixed value passes through sanitize inside dispatch. No visibility change needed, but the implementation agent should verify that "http-{uuid}" passes sanitize (allowed chars: alphanumeric, hyphen, underscore; max 128 chars).

3. **RecordEvent serde(flatten)**: The RecordEvent variant uses `#[serde(flatten)]` on the `event` field. When deserializing from HTTP JSON, the client sends the ImplantEvent fields at the top level alongside `"type":"RecordEvent"`. The `prefix_session_id` function accesses `event.session_id` after deserialization. Implementation agent should verify this works with the flatten annotation.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- Returned 13 entries. Key: #4692 (ADR-001 ObserveContext), #4693 (ADR-002 capability parameter), #4695 (ADR-004 response mapping), #4691 (dispatch_request transport-agnostic pattern), #763 (server-side observation intercept pattern).
- Queried: mcp__unimatrix__context_search (pattern: 'observe hook dispatch transport') -- Returned #763, #3471, #1266, #3324, #1611. Confirmed dispatch pattern and hook-side formatting conventions.
- Queried: mcp__unimatrix__context_search (decision: vnc-022) -- Returned ADR-002, ADR-003, ADR-004 entries confirming all 5 ADRs are stored.
- Deviations from established patterns: none. ObserveContext follows the Arc-clone bundle pattern used by UDS listener. Body size enforcement reuses the two-layer Limited pattern from McpAdapter. Response mapping follows tower Service conventions.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names -- every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections -- gaps flagged as open questions
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within product/features/vnc-022/pseudocode/
- [x] Knowledge Stewardship report block included
