## Agent Report: vnc-022-agent-1-architect

### Task
Design architecture for vnc-022 (Remote observation transport — /observe endpoint).

### Artifacts Produced

- `product/features/vnc-022/architecture/ARCHITECTURE.md`
- `product/features/vnc-022/architecture/ADR-001-observe-context-struct.md`
- `product/features/vnc-022/architecture/ADR-002-capability-parameter.md`
- `product/features/vnc-022/architecture/ADR-003-session-id-scoping.md`
- `product/features/vnc-022/architecture/ADR-004-response-http-mapping.md`
- `product/features/vnc-022/architecture/ADR-005-precompact-forward-compat.md`

### Key Decisions

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | ObserveContext Struct for Service Handle Passing | #4692 |
| ADR-002 | Capability Parameter on dispatch_request | #4693 |
| ADR-003 | Session ID Scoped by Transport Prefix | #4694 |
| ADR-004 | HookResponse to HTTP Status Code Mapping | #4695 |
| ADR-005 | PreCompact transcript_excerpt Forward Compatibility | #4696 |

### Scope Risks Addressed

- **SR-07 (HIGH)**: Solved by ObserveContext struct (ADR-001). PathRouter holds one ObserveContext field with all service handles.
- **SR-01 (HIGH)**: Solved by ObserveContext struct (ADR-001) + capability parameter (ADR-002). Context struct insulates layers from parameter changes.
- **SR-09 (HIGH)**: Solved by ADR-002. UDS call site passes UDS_CAPABILITIES — identical semantics, zero behavior change.
- **SR-03**: Mitigated by ADR-003. Transport prefix prevents cross-namespace session collision.
- **SR-05**: Analyzed in ARCHITECTURE.md. No critical event depends on a nice-to-have event.
- **SR-06**: Documented in ARCHITECTURE.md. Fire-and-forget data loss is acceptable and bounded.

### Items for Spec Writer Attention

1. The `observe_response_to_http` function signature and the HTTP-layer error responses (400 malformed JSON, 413 oversized, 500 body read) need precise specification with example JSON bodies.
2. Session ID prefix `http-` must be documented in the wire contract so clients understand that the session_id they send is not the session_id stored server-side.
3. The `SessionWrite` capability addition to HTTP ResolvedIdentity must be tested — verify all 9 capability check points in dispatch_request pass with the HTTP capability set `[Read, Write, Search, SessionWrite]`.
4. Integration tests must cover both UDS and HTTP paths calling dispatch_request to verify SR-09 (no UDS regression).
5. The CompactPayload transcript_excerpt field addition needs a deserialization test proving backward compat (old JSON without the field still deserializes).

### Open Questions

None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found entry #4691 (dispatch_request transport-agnostic pattern) directly applicable; confirmed approach
- Queried: mcp__unimatrix__context_search -- found vnc-021 ADRs #4666 (health bypass), #4670 (credential_type), pattern #317 (ToolContext) for reference
- Stored: entry #4692 "ADR-001 vnc-022: ObserveContext Struct for Service Handle Passing" via /uni-store-adr
- Stored: entry #4693 "ADR-002 vnc-022: Capability Parameter on dispatch_request" via /uni-store-adr
- Stored: entry #4694 "ADR-003 vnc-022: Session ID Scoped by Transport Prefix" via /uni-store-adr
- Stored: entry #4695 "ADR-004 vnc-022: HookResponse to HTTP Status Code Mapping" via /uni-store-adr
- Stored: entry #4696 "ADR-005 vnc-022: PreCompact transcript_excerpt Forward Compatibility" via /uni-store-adr
