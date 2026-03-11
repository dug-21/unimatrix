# Test Plan: handler-dispatch

Component: `crates/unimatrix-server/src/mcp/tools.rs` -- `context_retrospective` handler dispatch logic

Tests cover the routing logic that determines whether to call `format_retrospective_markdown` or `format_retrospective_report` based on the `format` parameter.

## Unit Test Expectations

### Format dispatch routing

These tests verify the conditional logic in the handler. Since the full handler requires a running server with observation data, dispatch routing tests should be structured as:

1. **Isolated dispatch function tests** (if dispatch logic is extracted into a testable helper), or
2. **End-to-end handler tests** using the existing test infrastructure for `context_retrospective`.

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_dispatch_markdown_default` | `params.format = None` | Handler calls `format_retrospective_markdown`, output starts with `# Retrospective:` (AC-01, AC-20) |
| `test_dispatch_markdown_explicit` | `params.format = Some("markdown")` | Same as above (AC-20, R-13) |
| `test_dispatch_json_explicit` | `params.format = Some("json")` | Handler calls `format_retrospective_report`, output is valid JSON (AC-02, AC-20) |
| `test_dispatch_invalid_format` | `params.format = Some("xml")` | Returns error with descriptive message, or falls back to markdown (R-13, AC-20) |

### Evidence limit interaction with format

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_json_evidence_limit_default_3` | `format = "json"`, `evidence_limit = None` | JSON path uses `unwrap_or(3)`, evidence arrays truncated to 3 (AC-02, AC-08, R-02) |
| `test_json_evidence_limit_explicit_0` | `format = "json"`, `evidence_limit = Some(0)` | No truncation, all evidence present (R-02) |
| `test_json_evidence_limit_explicit_5` | `format = "json"`, `evidence_limit = Some(5)` | Evidence truncated to 5 |
| `test_markdown_ignores_evidence_limit` | `format = None`, `evidence_limit = Some(1)` | Markdown output has k=3 examples per finding regardless of evidence_limit (R-02, IR-03) |
| `test_markdown_full_report_no_clone_truncate` | `format = None`, report with many evidence records | Clone-and-truncate step is NOT applied -- formatter receives the original report (IR-03) |

### JSON path non-regression

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_json_output_unchanged` | Known report, `format = "json"` | Output is identical to `format_retrospective_report` called directly (AC-02) |
| `test_json_path_uses_serde_json_pretty` | `format = "json"` | Output is valid pretty-printed JSON |

### Module registration

| Test | Approach | Assertion |
|------|----------|-----------|
| `test_format_retrospective_markdown_callable` | Import and call `format_retrospective_markdown` from `crate::mcp::response` | Function is accessible via `pub use` in `response/mod.rs` (IR-02) |

## Integration Harness Tests

These tests run against the compiled binary through MCP JSON-RPC and validate format routing at the system level.

| Test | Fixture | Steps | Assertion |
|------|---------|-------|-----------|
| `test_retrospective_markdown_default` | `server` | Store observation data, call `context_retrospective` with no format | Response text contains `# Retrospective:` |
| `test_retrospective_json_explicit` | `server` | Store observation data, call `context_retrospective` with `format: "json"` | Response is parseable JSON |
| `test_retrospective_format_invalid` | `server` | Call `context_retrospective` with `format: "xml"` | Error response or markdown fallback |

## Integration Risks Covered

- **IR-02**: Handler routes to correct formatter -- unit tests for each format value + integration harness tests.
- **IR-03**: Clone-and-truncate only on JSON path -- unit test confirms markdown receives full report.
- **R-02**: evidence_limit default change -- tests confirm JSON default is unchanged at 3, markdown ignores evidence_limit entirely.
- **R-13**: Invalid format string -- test confirms graceful handling.
