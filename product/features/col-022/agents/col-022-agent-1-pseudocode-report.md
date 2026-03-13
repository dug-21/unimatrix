# Agent Report: col-022-agent-1-pseudocode

## Task
Produce per-component pseudocode files for col-022 (Explicit Feature Cycle Lifecycle).

## Components Covered

1. **shared-validation** -- `validate_cycle_params()`, `CycleType`, `ValidatedCycleParams`, event-type constants
2. **schema-migration** -- v11->v12 keywords column, `SessionRecord.keywords`, `session_from_row`, `insert_session`, `update_session`
3. **mcp-tool** -- `context_cycle` tool handler, `CycleParams` struct, acknowledgment-only response
4. **hook-handler** -- PreToolUse interception for `context_cycle`, tool_name matching, parameter extraction, `RecordEvent` construction
5. **uds-listener** -- `handle_cycle_start`, `set_feature_force`, `SetFeatureResult`, `update_session_keywords`

## Output Files

- `product/features/col-022/pseudocode/OVERVIEW.md`
- `product/features/col-022/pseudocode/shared-validation.md`
- `product/features/col-022/pseudocode/schema-migration.md`
- `product/features/col-022/pseudocode/mcp-tool.md`
- `product/features/col-022/pseudocode/hook-handler.md`
- `product/features/col-022/pseudocode/uds-listener.md`

## Open Questions

1. **Event type naming**: The specification (FR-14) uses "cycle_begin"/"cycle_end" while the architecture (ADR-001) uses "cycle_start"/"cycle_stop". I used "cycle_start"/"cycle_stop" from ADR-001 since the architecture is authoritative over the specification for implementation details. The implementer should verify this is intentional.

2. **set_feature_force on unregistered session**: The architecture says `set_feature_force` returns `SetFeatureResult`, but the brief does not define behavior when the session is not in the registry. I defaulted to returning `Set` with a debug log. The implementer may prefer a fourth variant like `SessionNotFound`.

3. **MCP tool capability enforcement**: The architecture says "Requires SessionWrite capability" but the MCP server tools use a different capability model than UDS connections (Write vs SessionWrite). The implementer needs to verify how `context_store` enforces Write capability and follow the same pattern.

4. **Keywords serialization format**: The hook serializes keywords as a JSON string in the payload (`payload["keywords"] = serde_json::Value::String(json_string)`). The listener reads this as a string and stores it directly. This means the keywords column contains a JSON string like `"[\"kw1\",\"kw2\"]"`. The future injection pipeline must `serde_json::from_str` this. An alternative would be storing the JSON Value directly, but the String approach avoids nested JSON handling.

## Knowledge Stewardship

- Queried: existing codebase patterns for MCP tool handlers, hook build_request, listener dispatch_request, schema migration, session registry, validation module
- Deviations from established patterns: none. All pseudocode follows existing conventions:
  - Tool handler follows 6-step pipeline (#318)
  - Hook handler follows the PostToolUse pattern (extract from `input.extra`, validate, build RecordEvent)
  - Listener follows the #198 pattern (extract from payload, fire-and-forget persistence)
  - Migration follows the v8/v10 pattern (pragma_table_info guard, ALTER TABLE)
  - Validation follows the existing `validate_*_params` pattern (returns Result with descriptive error)

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names -- every name traced to architecture Integration Surface
- [x] Output is per-component (OVERVIEW.md + 5 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections -- gaps flagged as open questions
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/col-022/pseudocode/`
- [x] Knowledge Stewardship report block included
