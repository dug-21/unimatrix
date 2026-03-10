# Agent Report: nxs-010-agent-2-spec

## Status: COMPLETE

## Artifact Produced

`product/features/nxs-010/specification/SPECIFICATION.md`

## Key Decisions

1. **`total_tool_calls` not backfilled** — Set to 0 during migration. The sessions table has no tool_call count column. Joining with `observation_metrics` adds complexity for data that col-020 will recompute anyway. Resolves SCOPE.md open question #1.

2. **No foreign keys** — Neither `topic_deliveries` nor `query_log` have FK constraints to `sessions`. Consistent with existing patterns (injection_log, signal_queue, observations all lack FKs). Resolves SCOPE.md open question #3.

3. **Shared QueryLogRecord constructor required** — Per SR-07, both UDS and MCP paths must use a shared builder/constructor to prevent field divergence. Specified as FR-08.

4. **Fire-and-forget failure semantics explicitly defined** — Per SR-05, query_log write failures: log warning, no retry, no caller impact. Matches injection_log precedent exactly. Specified as C-03.

5. **AUTOINCREMENT decision boundary documented** — Per SR-03, append-only logs use AUTOINCREMENT, entity tables use named counters. Noted that an ADR should be created. Specified as C-04.

6. **Init sequence safety verified** — Per SR-01, existing `Store::open()` already runs migrate before create_tables. No code change needed, but a test should verify v10-to-v11 upgrade path. Specified as C-02.

## Open Questions for Architect

1. **Where does the MCP query_log write live?** The SearchService itself is transport-agnostic. The fire-and-forget write should go in the tool handler that calls `SearchService::search`, not in the service itself. The architect should determine the exact insertion point (likely in the `context_search` tool handler in `tools.rs` or equivalent).

2. **ADR for AUTOINCREMENT vs counter pattern** — SR-03 recommends documenting the decision boundary. The architect should create this ADR during delivery.

3. **Integration test strategy for AC-12/AC-13** — The existing injection_log integration tests in `listener.rs` use the full search pipeline (with embedding model). The architect should confirm whether query_log integration tests follow the same pattern or if a lighter-weight approach is preferred given that the write is structurally identical.
