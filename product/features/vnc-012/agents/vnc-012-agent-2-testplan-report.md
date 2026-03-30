# Agent Report: vnc-012-agent-2-testplan

## Phase
Stage 3a — Test Plan Design

## Output Files
- `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/test-plan/serde_util.md`
- `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/test-plan/tools.md`
- `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/test-plan/mod.md`
- `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/test-plan/infra_001.md`

## Risk Coverage Mapping Summary

| Risk ID | Priority | Covered By |
|---------|----------|-----------|
| R-01 | Critical | 5 absent-field tests in tools.md (AC-03-ABSENT-ID, AC-03-ABSENT-LIMIT, AC-04-ABSENT, AC-05-ABSENT, AC-06-ABSENT) |
| R-02 | Critical | AC-13 (Rust in-process, tools.md) + IT-01 + IT-02 (infra_001.md, both smoke) |
| R-03 | High | 5 null-field tests in tools.md (AC-03-NULL-ID, AC-03-NULL-LIMIT, AC-04-NULL, AC-05-NULL, AC-06-NULL) + 3 null tests in serde_util.md |
| R-04 | High | `test_deserialize_opt_usize_negative_string`, `test_deserialize_opt_usize_u64_overflow_string` in serde_util.md; AC-09 in tools.md |
| R-05 | High | AC-10 schema snapshot test in tools.md |
| R-06 | High | `test_deserialize_i64_float_number`, `test_deserialize_opt_i64_float_number`, `test_deserialize_opt_usize_float_number` in serde_util.md; AC-09-FLOAT-NUMBER in tools.md |
| R-07 | Med | `cargo build --workspace` (implicit); documented in mod.md |
| R-08 | Med | AC-08 (4 required-field tests) + AC-08-OPT (5 optional-field tests) in tools.md + serde_util.md |
| R-09 | Med | AC-10 implementation dependency on `make_server()` documented in tools.md; fallback path specified |
| R-10 | Low | AC-11 — existing test runs unmodified; tester confirms at `cargo test --workspace` |

## Integration Suite Plan

Suites to run in Stage 3c:
1. `smoke` — mandatory gate (`pytest -m smoke` including new IT-01, IT-02)
2. `tools` — 73 tests, validates all tool parameter paths
3. `protocol` — 13 tests, MCP compliance
4. `security` — 17 tests, input validation boundaries

New tests to add:
- `test_get_with_string_id` in `suites/test_tools.py` (IT-01, `@pytest.mark.smoke`, `server` fixture)
- `test_deprecate_with_string_id` in `suites/test_tools.py` (IT-02, `@pytest.mark.smoke`, `server` fixture)

AC-13 Rust in-process test: `crates/unimatrix-server/tests/mcp_coercion.rs` (preferred).
Test name must include `coercion` or `string_id`.

## Open Questions

1. **OQ-04 (R-09)**: Whether `RequestContext<RoleServer>` is constructible from rmcp's
   public API for AC-13. If not, the implementation agent must expose
   `pub(crate) async fn call_tool_for_test` on `UnimatrixServer`. The test plan documents
   both paths; the implementation agent must choose.

2. **AC-10 `make_server()` visibility**: Needs to be `pub(crate)` (or `pub`) in
   `server.rs` for the schema snapshot test. The implementation agent must verify this
   before writing AC-10.

3. **Existing `test_retrospective_params_evidence_limit` test content**: The tester must
   inspect the input used in this existing test to confirm it uses integer (not string)
   form for `evidence_limit`. If string, the test behavior changes (coercion applies) but
   the test should still pass — this is a behavior expansion, not a regression.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — returned entries #3786 (integration test
  requirement for MCP parameter deserialization fixes) and #3789 (ADR-003, mandatory
  integration test), both directly confirming the IT-01/IT-02 + AC-13 requirement. Entry
  #885 confirmed that serde-heavy types need explicit absent/null test coverage — directly
  informed the R-01/R-03 test table structure.
- Queried: `context_search("vnc-012 architectural decisions", category: "decision", topic: "vnc-012")` — returned ADRs #3787–#3790 confirming all four ADRs are recorded.
- Queried: `context_search("serde deserialization testing patterns edge cases")` — returned
  #885 and #3790 (ADR-004 mandating None-for-absent tests).
- Stored: nothing novel to store — the dual absent/null test table pattern for
  `deserialize_with` + `#[serde(default)]` optional fields is specific to this feature's
  greenfield implementation. It matches the pattern in entry #3786/#885 but does not yet
  recur across two features to warrant a new stored entry. If this pattern appears in a
  future feature, store it then.
