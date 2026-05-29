# Agent Report: vnc-022-agent-2-spec

## Deliverable

`/workspaces/unimatrix/product/features/vnc-022/specification/SPECIFICATION.md`

## Key Decisions

1. **Response mapping**: `HookResponse::Ack` -> 204 No Content; `Entries`/`BriefingContent`/`Pong` -> 200 with JSON; `Error` -> 400 with JSON. Deserialization failures return 400 with handler-level error JSON (distinct from pipeline errors).
2. **Capability parameter design**: Specified as `capabilities: &[Capability]` slice parameter on `dispatch_request`, replacing `uds_has_capability()` calls. UDS callers pass `UDS_CAPABILITIES` constant; HTTP callers pass capabilities from `ResolvedIdentity`.
3. **SessionWrite required**: HTTP `ResolvedIdentity` must include `SessionWrite` capability. Current vnc-021 auth returns `[Read, Write, Search]` only — this needs updating.
4. **Wire contract documented in-spec**: Full request/response JSON examples for all 6 HookRequest variants, covering all 10 Day 1 events.
5. **transcript_excerpt forward compat**: Optional field on CompactPayload with serde skip_serializing_if, ignored Day 1.

## Open Questions (for architect)

1. **SR-07 (critical path)**: How service handles reach the `/observe` handler through PathRouter. Three options enumerated; architect must choose.
2. **SR-01**: Whether dispatch_request's 11 parameters should be bundled into a context struct.
3. **SR-03**: Whether session_id should be scoped per bearer token for enterprise scenarios.

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-19)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (body size, zero new deps)
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit (11 exclusions)
- [x] Output file is in product/features/vnc-022/specification/ only
- [x] No placeholder or TBD sections
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- 14 entries returned; #4691 confirmed dispatch_request transport-agnostic pattern, #4670 confirmed credential_type audit convention from vnc-021
