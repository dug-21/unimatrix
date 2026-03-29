# Scope Risk Assessment: vnc-012

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `#[schemars(with = "T")]` produces schema identical to the native type schema — not verified against schemars 1.2.1 compiled output. If schema differs (e.g., adds `minimum: 0` unexpectedly for `Option<u64>`), AC-10 fails and callers see a changed schema. | Med | Low | Architect should confirm via a `cargo test` schema snapshot test that the advertised JSON Schema for each affected field is unchanged from the baseline before the feature ships. |
| SR-02 | `serde_util.rs` is greenfield — zero prior uses of `deserialize_with` in the codebase (confirmed by scope research). Any subtlety in the serde `Visitor` pattern (null vs. absent for `Option`, overflow on 32-bit `usize`) risks silent correctness bugs. | Med | Med | Spec writer should require explicit test cases for null, absent, and overflow paths in addition to the happy-path cases listed in AC-06 and AC-09. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Open Question 4 in SCOPE.md (§Open Questions) is unresolved: no integration test confirms the MCP dispatch path accepts a string-encoded id end-to-end. Unit tests in `tools.rs` only exercise serde deserialization; a bug in the rmcp → serde_json → struct dispatch layer would not be caught. | High | Med | Spec writer must require at least one infra-001 or equivalent integration test that sends a string-encoded integer over the UDS/stdio transport and asserts the tool handler returns a valid response (not an MCP error). Entry #3526 confirms infra-001 is the right vehicle for MCP boundary risks. |
| SR-04 | Scope explicitly excludes updating agent definitions and protocol files (§Non-Goals). If agents continue emitting string-encoded integers for non-numeric fields (e.g., `format`, `category`), those failures will recur and erode the perceived value of this fix. | Low | Low | Architect should note the hard boundary: this fix covers only numeric fields. Document the remaining failure surface in ARCHITECTURE.md so follow-on work (GH #448 follow-ups) is scoped correctly. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | The fix touches 5 param structs used by 8+ tools. A regression in `deserialize_opt_i64_or_string` (e.g., treating absent field as `Some(0)` instead of `None`) would silently corrupt optional query parameters like `limit`, `k`, and `max_tokens` across multiple tools. | High | Low | Architect must ensure the `None`-for-absent path is covered by a mandatory non-negotiable test. Entry #3579 shows gate failures where mandatory test modules were entirely absent — flag these tests as required in the test plan. |
| SR-06 | `context_get` id parameter is typed `i64` but the MCP server is itself affected by this bug (confirmed by search session: calling `context_get` with string id fails with the exact error this feature fixes). Any integration test written during delivery that uses `context_get` via MCP may fail if the test harness emits string ids. | Low | Low | Test author should use integer ids in all test fixtures to avoid self-inflicted failures during the feature delivery window. |

## Assumptions

- **§Background Research / rmcp deserialization path**: Assumes `Parameters<T>` transparent serde means `#[serde(deserialize_with)]` on `T`'s fields is respected without rmcp changes. If rmcp 0.16.0 wraps or pre-processes tool arguments before delegating to serde (e.g., via a custom `Deserialize` impl on `Parameters`), the approach breaks. Scope research confirms this from rmcp source but the architect should add a compile-time or test assertion.
- **§Constraints / schemars 1.2.1**: Assumes `#[schemars(with = "i64")]` produces `{"type": "integer"}` with no additional constraints. If schemars 1.2.1 infers `minimum`/`maximum` bounds from the Rust type, AC-10 (schema unchanged) may fail.
- **§Non-Goals**: Assumes validation in `infra/validation.rs` (`validated_id`, `validated_k`, etc.) receives the already-coerced `i64` value and therefore needs no changes. This is correct if coercion occurs at deserialization time before the handler is called — which the rmcp path guarantees — but should be verified for `evidence_limit`, which has no `validated_*` function.

## Design Recommendations

- **SR-03 (critical)**: Require an integration test over the actual MCP transport as a mandatory acceptance gate. Unit-only coverage is insufficient for a deserialization fix in the transport layer.
- **SR-01 + SR-02**: Add a schema snapshot test (serialize the tool list, assert `type: integer` for all 9 affected fields) as a non-negotiable test. Prevents schema regressions from going undetected.
- **SR-05**: Enumerate all `None`-for-absent test cases explicitly in the spec acceptance criteria, one per affected optional field. Do not rely on a single generic "optional fields default to None" test.
