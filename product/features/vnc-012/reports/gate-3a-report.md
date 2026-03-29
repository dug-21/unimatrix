# Gate 3a Report: vnc-012

> Gate: 3a (Design Review)
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All four components match approved architecture; data flow, function signatures, and component boundaries are exact |
| Specification coverage | PASS | All 13 FRs and 27 ACs addressed by pseudocode and test plans |
| Risk coverage | PASS | All 10 risks (R-01 through R-10) have mapped test scenarios in the test plans |
| Interface consistency | WARN | Minor tool-name discrepancy between test-plan/tools.md (`context_retrospective`) and pseudocode/tools.md (`context_cycle_review`) for `RetrospectiveParams` |
| Knowledge stewardship compliance | WARN | Architect report lacks the required `## Knowledge Stewardship` section heading; content is present but under different heading |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:
- `pseudocode/serde_util.md` defines exactly three `pub(crate)` functions matching the architecture's Component 1 specification: `deserialize_i64_or_string`, `deserialize_opt_i64_or_string`, `deserialize_opt_usize_or_string`. Signatures are identical to those in ARCHITECTURE.md.
- `pseudocode/tools.md` specifies nine field annotations matching the architecture's Component 2 table exactly (GetParams.id, DeprecateParams.id, QuarantineParams.id, CorrectParams.original_id, LookupParams.id, LookupParams.limit, SearchParams.k, BriefingParams.max_tokens, RetrospectiveParams.evidence_limit).
- `pseudocode/mod.rs` specifies the single `mod serde_util;` line matching Component 3.
- `pseudocode/infra_001.md` specifies IT-01 (`test_get_with_string_id`) and IT-02 (`test_deprecate_with_string_id`) matching Component 4.
- Data flow diagram in `pseudocode/OVERVIEW.md` is consistent with the architecture's component interaction diagram.
- ADR decisions are reflected: ADR-001 (submodule placement), ADR-002 (schemars override), ADR-003 (mandatory integration tests), ADR-004 (#[serde(default)] requirement) are all honored in the pseudocode.
- No new crate dependencies introduced (C-02, FR-10 honored).
- No handler logic changes specified (FR-09 honored).
- No rmcp version changes (NFR-06, C-01 honored).

### Check 2: Specification Coverage

**Status**: PASS

**Evidence — Functional Requirements**:

- FR-01: `pseudocode/serde_util.md` specifies exactly three `pub(crate)` helper functions. No additional helpers are added.
- FR-02: `I64OrStringVisitor` handles `visit_i64`, `visit_u64`, `visit_str`, `visit_string` with correct semantics; non-numeric strings produce `de::Error::custom`.
- FR-03: `OptI64OrStringVisitor` handles null via `visit_none`/`visit_unit` returning `Ok(None)`; present values via `visit_some` delegating to `deserialize_i64_or_string`.
- FR-04: Absent-field behavior documented explicitly — `#[serde(default)]` handles the absent case before the Visitor is called; the OVERVIEW.md and serde_util.md both document this distinction.
- FR-05: `OptUsizeOrStringVisitor` parses via `u64` first (`str::parse::<u64>()`), then `usize::try_from(val_u64)` — never `as usize`. Negative strings fail at the `u64` parse stage.
- FR-06: `pseudocode/mod.md` specifies `mod serde_util;` with no `pub` modifier.
- FR-07: All nine field annotations with correct struct/field/type/deserialize_with/schemars values are specified in `pseudocode/tools.md`.
- FR-08: All five optional fields have `#[serde(default)]` in the pseudocode attribute patterns.
- FR-09: No changes to `infra/validation.rs` are specified anywhere in the pseudocode.
- FR-10: No new dependencies introduced.
- FR-11: Non-numeric strings produce `de::Error::custom` — documented in error table in `serde_util.md`.
- FR-12: Float strings rejected by `str::parse::<i64>()` — documented.
- FR-13: `visit_f64` and `visit_f32` explicitly return `de::Error::invalid_type(Unexpected::Float(v), &self)` — specified in all three Visitor implementations.

**Evidence — Acceptance Criteria**:

All 27 ACs (AC-01 through AC-13 including subcriteria -ABSENT, -NULL, -ZERO, -FLOAT, -FLOAT-NUMBER, -OPT) are represented in named test functions in `pseudocode/tools.md` and `pseudocode/serde_util.md`. The test plan files enumerate each AC explicitly with test name, input, and expected output.

**OQ-04 resolution**: The pseudocode resolves OQ-04 by using `serde_json::from_value::<GetParams>(Value::Object(args))` directly in the `#[cfg(test)]` block of `tools.rs`. The pseudocode correctly argues this is the exact serde call executed by `Parameters<T>: FromContextPart` in rmcp (line ~173 of `rmcp/src/handler/server/tool.rs`). The test names include "coercion" satisfying the findability requirement. The infra-001 IT-01/IT-02 tests cover the full stdio transport layer.

**NFR coverage**:
- NFR-01 (zero-allocation happy path): Visitor pattern is stateless; integer inputs pass through without allocation.
- NFR-02 (additive tests): Pseudocode explicitly notes the existing `test_wrong_type_doesnt_panic` and `test_retrospective_params_evidence_limit` tests remain unmodified.
- NFR-03 (clippy): Not explicitly addressed in pseudocode, but the Visitor pattern with `#[allow]` is a standard pattern; no obvious clippy issues in the pseudocode.
- NFR-04 (cargo fmt): Not explicitly addressed; implementation responsibility.
- NFR-05 (schema type: integer): AC-10 schema snapshot test covers all nine fields; `evidence_limit` minimum:0 handling is documented.
- NFR-06 (rmcp pinned): Honored throughout.

### Check 3: Risk Coverage

**Status**: PASS

**Evidence — Risk-to-Test Mapping**:

| Risk ID | Priority | Test Coverage |
|---------|----------|--------------|
| R-01 | Critical | 5 absent-field tests: AC-03-ABSENT-ID, AC-03-ABSENT-LIMIT, AC-04-ABSENT, AC-05-ABSENT, AC-06-ABSENT — one per optional field — in `test-plan/tools.md` |
| R-02 | Critical | AC-13 (Rust serde_json::from_value path) + IT-01/IT-02 (infra-001 smoke, full stdio transport) in `test-plan/infra_001.md` |
| R-03 | High | 5 null-field tests: AC-03-NULL-ID, AC-03-NULL-LIMIT, AC-04-NULL, AC-05-NULL, AC-06-NULL in `test-plan/tools.md` + 3 null tests in `test-plan/serde_util.md` |
| R-04 | High | `test_deserialize_opt_usize_negative_string`, `test_deserialize_opt_usize_u64_overflow_string` in `serde_util.md`; AC-09 and AC-06-ZERO in `tools.md` |
| R-05 | High | AC-10 schema snapshot test in `test-plan/tools.md` — all nine fields asserted |
| R-06 | High | `test_deserialize_i64_float_number`, `test_deserialize_opt_i64_float_number`, `test_deserialize_opt_usize_float_number` in `serde_util.md`; AC-09-FLOAT-NUMBER tests in `tools.md` |
| R-07 | Med | `cargo build --workspace` (implicit); documented in `test-plan/mod.md` with rename-trap warning |
| R-08 | Med | AC-08 (4 required-field tests) + AC-08-OPT (5 optional-field tests) in `test-plan/tools.md` + `serde_util.md` |
| R-09 | Med | AC-10 placement documented in `pseudocode/tools.md` — `tool_router_for_test()` accessor pattern specified for `server.rs` test block |
| R-10 | Low | AC-11 — existing `test_retrospective_params_evidence_limit` runs unmodified; confirmed integer input unchanged |

All 10 risks have at least one corresponding test scenario. Critical and High priority risks have multiple covering tests. The integration risk for `Parameters<T>` transparent delegation is addressed by both the in-process AC-13 test and the stdio transport IT-01/IT-02 tests.

### Check 4: Interface Consistency

**Status**: WARN

**Evidence**:

The shared types defined in `pseudocode/OVERVIEW.md` (three Visitor structs as private zero-size types) are consistent across all per-component pseudocode files. The nine field annotations in `pseudocode/tools.md` match exactly the integration surface table in ARCHITECTURE.md.

**Minor inconsistency**: The tool name for `RetrospectiveParams` is specified differently across two test-plan files:
- `test-plan/tools.md` line 161: `context_retrospective`
- `pseudocode/tools.md` line 255: `context_cycle_review`

Both documents acknowledge the tool name must be confirmed against the actual `#[tool(name = "...")]` annotation in `tools.rs` at implementation time. This is a documentation inconsistency in design artifacts, not a logic gap. The implementation agent has been instructed to search for `feature_cycle` in `tools.rs` to find the correct name. This does not block implementation.

All other interface contracts are internally consistent:
- Function signatures in `serde_util.md` match the architecture's Integration Surface table exactly.
- Attribute patterns in `tools.md` are consistent with the function signatures.
- The `mod serde_util;` declaration in `mod.md` correctly makes the helpers accessible via `serde_util::` path from `tools.rs`.
- The infra-001 test harness API usage (`context_store`, `call_tool`, `extract_entry_id`, `assert_tool_success`, `get_result_text`) is consistent across `pseudocode/infra_001.md` and `test-plan/infra_001.md`.

### Check 5: Knowledge Stewardship Compliance

**Status**: WARN

**Evidence**:

| Agent | Report | Stewardship Block | Assessment |
|-------|--------|------------------|------------|
| vnc-012-agent-1-architect | `agents/vnc-012-agent-1-architect-report.md` | No `## Knowledge Stewardship` section. Content is present under "Prior Knowledge Applied" (queries to #3784, #3786) and ADR storage documented (IDs #3787–#3790). | WARN — heading mismatch, not a missing section |
| vnc-012-agent-3-risk | `agents/vnc-012-agent-3-risk-report.md` | Present with `## Knowledge Stewardship`; `Queried:` entries (#885, #3786, #3526, #3548) and `Stored: nothing novel to store -- {reason}` present. | PASS |
| vnc-012-agent-1-pseudocode | `agents/vnc-012-agent-1-pseudocode-report.md` | Present with `## Knowledge Stewardship`; `Queried:` entries listed; `Stored: nothing novel to store` with reason present. | PASS (read-only agent, Queried entries present) |
| vnc-012-agent-2-testplan | `agents/vnc-012-agent-2-testplan-report.md` | Present with `## Knowledge Stewardship`; `Queried:` entries listed; `Stored: nothing novel to store` with reason present. | PASS (read-only agent, Queried entries present) |

The architect agent is an active-storage agent (stored ADRs #3787–#3790). It has the required Stored entries documented in the report body, but under "Prior Knowledge Applied" rather than the required `## Knowledge Stewardship` section. The content satisfies the intent; the formatting is non-compliant. This is a WARN, not a blocking FAIL.

---

## Rework Required

None — gate result is PASS. The two WARNs are advisory:

1. **Tool name inconsistency** (interface consistency): The implementation agent must confirm the actual tool name for `RetrospectiveParams` by searching for `feature_cycle` or `evidence_limit` in `crates/unimatrix-server/src/mcp/tools.rs` before writing the AC-10 schema snapshot test. Use whichever name is found in the `#[tool(name = "...")]` annotation.

2. **Architect stewardship heading**: The architect agent report documents all required content (queries and ADR storage) but under a non-standard heading. No rework required — the gate is PASS. Future architect agents should use the `## Knowledge Stewardship` heading.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate-3a results are feature-specific and belong in this report, not in Unimatrix. No recurring validation failure pattern was observed across this review.
