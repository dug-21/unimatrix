# Test Plan: params-extension

Component: `crates/unimatrix-server/src/mcp/tools.rs` -- `RetrospectiveParams` struct modification

All tests are unit tests in the existing `#[cfg(test)]` module in `tools.rs`, extending the current `RetrospectiveParams` test section.

## Unit Test Expectations

### RetrospectiveParams deserialization

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_retrospective_params_format_markdown` | `{"feature_cycle": "test", "format": "markdown"}` | `params.format == Some("markdown".to_string())` (AC-19) |
| `test_retrospective_params_format_json` | `{"feature_cycle": "test", "format": "json"}` | `params.format == Some("json".to_string())` (AC-19) |
| `test_retrospective_params_format_absent` | `{"feature_cycle": "test"}` | `params.format.is_none()` -- None means default to markdown (AC-19) |
| `test_retrospective_params_format_unknown` | `{"feature_cycle": "test", "format": "xml"}` | Deserializes successfully (it is `Option<String>`, any string is valid serde). Validation happens in handler dispatch. (R-13) |
| `test_retrospective_params_all_fields` | All fields populated | All fields deserialize correctly including `format` |
| `test_retrospective_params_backward_compat` | Existing test payloads (no `format`) | All existing tests still pass -- `format` is `Option`, backward-compatible |

### evidence_limit behavior (unchanged, verify non-regression)

| Test | Existing? | Assertion |
|------|-----------|-----------|
| `test_evidence_limit_default` | Yes (existing) | `params.evidence_limit.unwrap_or(3) == 3` -- JSON path default unchanged per human override; doc comment should read `(default: 3, JSON path only)` (AC-08) |
| `test_retrospective_params_evidence_limit` | Yes (existing) | Explicit value deserializes correctly |
| `test_retrospective_params_evidence_limit_zero` | Yes (existing) | `Some(0)` deserializes |

### Feature gate (IR-04)

| Test | Approach | Assertion |
|------|----------|-----------|
| `test_module_gated_behind_feature` | Compile check | `cargo test` without `mcp-briefing` feature must not fail to compile. The `retrospective` module must be conditionally compiled. This is verified by CI or a manual `cargo check --no-default-features` if `mcp-briefing` is a default feature. |

## Integration Risks Covered

- **IR-04**: Feature gate on `retrospective.rs` -- compilation test with feature disabled.
- **R-13**: Invalid format strings -- deserialization accepts any string; validation is handler-dispatch's responsibility. Param tests confirm the boundary.
