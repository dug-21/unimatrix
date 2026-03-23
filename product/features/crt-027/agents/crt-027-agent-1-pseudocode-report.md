# Agent Report: crt-027-agent-1-pseudocode

## Summary

Produced pseudocode for all 8 components of crt-027 (WA-4 Proactive Knowledge Delivery).
All source documents and ADR files were read before any pseudocode was written.

## Files Produced

| File | Component |
|------|-----------|
| `product/features/crt-027/pseudocode/OVERVIEW.md` | Component interaction, data flow, shared types, sequencing |
| `product/features/crt-027/pseudocode/wire-source-field.md` | `unimatrix-engine/src/wire.rs` |
| `product/features/crt-027/pseudocode/hook-routing.md` | `uds/hook.rs` |
| `product/features/crt-027/pseudocode/listener-dispatch.md` | `uds/listener.rs` |
| `product/features/crt-027/pseudocode/index-briefing-service.md` | `services/index_briefing.rs` (new file) |
| `product/features/crt-027/pseudocode/service-layer-wiring.md` | `services/mod.rs` |
| `product/features/crt-027/pseudocode/context-briefing-handler.md` | `mcp/tools.rs` |
| `product/features/crt-027/pseudocode/index-entry-formatter.md` | `mcp/response/briefing.rs` |
| `product/features/crt-027/pseudocode/protocol-update.md` | `.claude/protocols/uni/uni-delivery-protocol.md` |

## Open Questions / Implementation Gaps Flagged

### OQ-1: `IndexBriefingService::index` and `category_histogram` resolution

The `IndexBriefingService` does not hold a `SessionRegistry` reference. The `ServiceSearchParams`
struct has a `category_histogram: Option<HashMap<String, u32>>` field (confirmed from the
existing `handle_context_search` code). Therefore, callers must pre-resolve the category
histogram before calling `IndexBriefingService::index()`.

**Resolution**: `IndexBriefingParams` must include a `category_histogram: Option<HashMap<String, u32>>`
field so callers can pass the pre-resolved histogram. The pseudocode in `index-briefing-service.md`
already documents this. The implementation agent must add this field to `IndexBriefingParams`.

### OQ-2: `context_briefing` MCP handler and `self.session_registry`

The `context_briefing` handler needs `session_registry.get_state(session_id)` and
`session_registry.get_category_histogram(session_id)` for steps 2 and 3 of query derivation.
The pseudocode assumes `self.session_registry` is available on the `UnimatrixServer` struct.

**Action for implementation agent**: Verify whether `session_registry` is currently a field
on `UnimatrixServer`. If not, it must be added. The UDS listener receives it as a function
parameter (`handle_compact_payload` gets it passed in). The MCP handler struct may or may
not hold it. Check `server.rs` or the struct definition.

If `session_registry` is not available on the MCP server struct, the fallback is graceful:
`session_state = None` causes query derivation to fall to step 3 (topic fallback). This is
correct behavior but loses WA-2 boost. The implementation agent should resolve this and flag
if a server struct change is needed.

### OQ-3: `derive_briefing_query` step 2 — feature_cycle absent

The pseudocode specifies: if `feature_cycle` is absent from session state but `topic_signals`
are present, fall to step 3. This is a conservative choice. An alternative would be to
synthesize using signals only (without feature_cycle). The architect documented this as
"fall to step 3 when feature_cycle is absent" per FR-11 ("feature_cycle + top 3 topic_signals").

### OQ-4: `format_index_table` exact column widths

The implementation agent must finalize exact column widths and write the R-05 scenario 2
test AFTER implementing the function (not before). The test must match the actual output.
The pseudocode provides approximate widths that should be adjusted for readability.

### OQ-5: Status filtering in SearchService

The pseudocode shows a post-filter for `status=Active` after SearchService returns. If
`ServiceSearchParams.filters` supports a status filter (via `QueryFilter`), use it instead
of post-filtering — it is more efficient. Implementation agent must inspect `QueryFilter` type.

### OQ-6: `write_stdout_subagent_inject_from_response` vs direct extraction

The `run()` function in `hook.rs` needs to route stdout writing based on `source`. The
pseudocode proposes a `write_stdout_subagent_inject_from_response` wrapper. An alternative
is to extract the formatted text directly and call `write_stdout_subagent_inject`. Either
approach is acceptable. Choose whichever produces less code duplication with `write_stdout`.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "hook routing injection patterns conventions" — found
  entries #3230 (SubagentStart routing pattern), #281 (caller-parameterized service pattern),
  #314 (hybrid gateway injection), #321 (access-source-discriminated entry point). Entry #3230
  is the directly relevant pre-established crt-027 scoping artifact and confirms the routing design.
- Queried: `/uni-query-patterns` for "crt-027 architectural decisions" — found entries #3244
  (ADR-003), #3245 (ADR-004), #3243 (ADR-002), #3242 (ADR-001), #3246 (ADR-005). All ADRs
  found in Unimatrix, consistent with the ADR files.
- Deviations from established patterns: none. The pseudocode follows the BriefingService
  caller-parameterized service pattern (#281) and extends it for IndexBriefingService. The
  EffectivenessStateHandle non-optional constructor pattern (ADR-004 crt-018b, entry #1546)
  is preserved.
