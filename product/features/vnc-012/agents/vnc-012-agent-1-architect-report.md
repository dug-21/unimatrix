# Agent Report: vnc-012-agent-1-architect

## Task

Design the architecture for vnc-012: server-side coercion of string-encoded integers for all numeric MCP parameters in unimatrix-server. Address SR-03 (missing integration test) and SR-05 (optional field silent corruption).

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/architecture/ARCHITECTURE.md`

### ADR Files

| File | Unimatrix ID |
|------|-------------|
| `architecture/ADR-001-serde-util-submodule.md` | #3787 |
| `architecture/ADR-002-schemars-with-override.md` | #3788 |
| `architecture/ADR-003-integration-test-requirement.md` | #3789 |
| `architecture/ADR-004-optional-field-none-guarantee.md` | #3790 |

## Key Decisions

1. **serde_util submodule** (ADR-001): Three `pub(crate)` helpers in a new
   `mcp/serde_util.rs` file, consistent with the existing `response/` submodule.
   Zero new crate dependencies.

2. **`#[schemars(with = "T")]`** (ADR-002): Preserves `type: integer` in
   published JSON Schema for all nine fields without any schema-generation
   functions. `evidence_limit` gains `minimum: 0` (semantically correct, not
   a regression).

3. **Mandatory integration test** (ADR-003, SR-03): Two infra-001 Python
   integration tests (`test_get_with_string_id`, `test_deprecate_with_string_id`)
   marked `smoke`, using `call_tool` raw method to exercise the full rmcp dispatch
   path over stdio.

4. **`#[serde(default)]` requirement** (ADR-004, SR-05): All five optional
   fields using `deserialize_opt_*` helpers must carry `#[serde(default)]`.
   20+ mandatory unit tests cover absent-field and null paths explicitly per
   field.

## Prior Knowledge Applied

- Entry #3784 (briefing result): Confirmed the `#[serde(with = "T")]` +
  `#[schemars(with = "T")]` pairing pattern is already documented as a
  Unimatrix pattern.
- Entry #3786 (briefing result): Confirmed infra-001 integration test
  requirement for MCP deserialization fixes is an established expectation.

## Open Questions

None. All four SCOPE.md open questions are resolved.
