# Agent Report: vnc-022-researcher

## Task
Research problem space for vnc-022 (Remote observation transport). Explore codebase, validate ASS-064 architecture, identify constraints, produce SCOPE.md.

## Output
- SCOPE.md written to `product/features/vnc-022/SCOPE.md`

## Key Findings

1. **dispatch_request is transport-agnostic**: The ~500-line function in `uds/listener.rs` (line 516) takes 10 Arc-wrapped parameters (all on UnimatrixServer) and returns HookResponse. Only transport-specific aspect: hardcoded `uds_has_capability()` checks. Parameterize the capability source and it works for HTTP.

2. **PathRouter needs server access**: Currently `PathRouter` holds `ProjectRouter` which holds `McpAdapter` which wraps rmcp's `StreamableHttpService<UnimatrixServer>`. The UnimatrixServer is inside rmcp and not directly accessible. The `/observe` handler needs its own reference to service handles. Store a separate `UnimatrixServer` (or subset) on `PathRouter` or `ProjectRouter`.

3. **Wire types are ready**: `HookRequest`/`HookResponse` are `Serialize`/`Deserialize` with `#[serde(tag = "type")]`. HTTP wire format = UDS wire format minus the 4-byte length prefix framing.

4. **Capability gap**: HTTP `ResolvedIdentity` has `[Read, Write, Search]`. `dispatch_request` arms check `SessionWrite` capability (for SessionRegister, SessionClose, RecordEvent). Need to add `SessionWrite` to the HTTP capability set or adjust the capability model.

5. **No tokio in hook-remote**: Matching ADR-002 constraint from the existing `hook` subcommand. HTTP client must be synchronous blocking. `ureq` (pure Rust, rustls) is the natural choice.

6. **Transcript handling for remote PreCompact**: Transcript file is local to the client machine. `hook-remote` must extract the transcript block client-side and prepend to the server's briefing response, matching the existing `prepend_transcript()` pattern.

## Open Questions for Human
See SCOPE.md Open Questions section (6 items). Most critical:
- dispatch_request extraction strategy (new module vs pub(crate) in place)
- HTTP client dependency choice (ureq vs minreq)
- `/observe` versioning strategy (unversioned vs `/v1/observe`)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 11 entries returned; vnc-021 ADRs (#4668, #4669, #4670), alc-003 session identity deferral (#2267), and scope-risk pattern (#4489) were most relevant
- Stored: entry #4691 "dispatch_request is transport-agnostic -- parameterize capabilities to reuse from HTTP" via /uni-store-pattern
