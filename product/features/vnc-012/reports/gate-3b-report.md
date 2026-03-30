# Gate 3b Report: vnc-012

> Gate: 3b (Code Review)
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All three helpers and all nine annotations match pseudocode exactly; one documented deviation in path syntax is correct and benign |
| Architecture compliance | PASS | Component boundaries, ADR decisions, and integration points all honored |
| Interface implementation | PASS | All nine fields annotated with correct deserializer + schemars pairs; optional fields have `#[serde(default)]` |
| Test case alignment | PASS | 43 new unit tests (vnc012_coercion_tests) + AC-10 schema snapshot in server.rs + IT-01/IT-02 in test_tools.py — all AC mappings covered |
| Code quality | WARN | tools.rs is 5412 lines (pre-existing; documented in OVERVIEW.md as out-of-scope for this feature); serde_util.rs is 451 lines (within limit); server.rs is 3031 lines (pre-existing); no stubs or placeholders; all `.unwrap()` calls confined to `#[cfg(test)]` blocks |
| Security | PASS | No hardcoded secrets; no path traversal; no command injection; deserialization rejects malformed input with typed errors; `usize::try_from` (never `as usize`) per C-06; no new crate dependencies |
| Knowledge stewardship compliance | PASS | All three rust-dev agent reports contain `## Knowledge Stewardship`; `Queried:` and `Stored:` entries present in all reports |

---

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Evidence**:

`serde_util.rs` implements exactly the three Visitor structures and functions specified in `pseudocode/serde_util.md`:

- `I64OrStringVisitor` — implements `visit_i64`, `visit_u64`, `visit_str`, `visit_string`, `visit_f64` (reject), `visit_f32` (reject); calls `d.deserialize_any(I64OrStringVisitor)`.
- `OptI64OrStringVisitor` — implements `visit_none`, `visit_unit`, `visit_some` (delegating to `deserialize_i64_or_string`); calls `d.deserialize_option(OptI64OrStringVisitor)`.
- `UsizeOrStringVisitor` + `OptUsizeOrStringVisitor` — inner visitor with `visit_u64`, `visit_i64` (rejects negative), `visit_str` (parses via `u64` first), `visit_f64`/`visit_f32` (reject); outer visitor uses `deserialize_option`.
- `usize::try_from` is used throughout (never `as usize`), honoring C-06.
- `visit_f64` and `visit_f32` return `de::Error::invalid_type(Unexpected::Float(...), &self)` per FR-13.

**Documented deviation**: The `deserialize_with` path string uses the crate-absolute form `"crate::mcp::serde_util::deserialize_..."` rather than the pseudocode-suggested relative form `"serde_util::..."`. The agent report (vnc-012-agent-4-tools) documents this: serde resolves `deserialize_with` path strings from the crate root, not from the current module. The absolute path is functionally equivalent and compilation confirms it resolves correctly. ADR-001 mentions "or by path in the attribute macro" without mandating which form. This is not a defect.

`mod.rs` has exactly `mod serde_util;` (private, between `response` and `tools`) as specified in `pseudocode/mod.md`.

All nine field annotations in `tools.rs` match `pseudocode/tools.md` exactly:
- `GetParams.id`, `DeprecateParams.id`, `QuarantineParams.id`, `CorrectParams.original_id` — `deserialize_i64_or_string` + `#[schemars(with = "i64")]`
- `LookupParams.id`, `LookupParams.limit`, `SearchParams.k`, `BriefingParams.max_tokens` — `default` + `deserialize_opt_i64_or_string` + `#[schemars(with = "Option<i64>")]`
- `RetrospectiveParams.evidence_limit` — `default` + `deserialize_opt_usize_or_string` + `#[schemars(with = "Option<u64>")]`

### Check 2: Architecture Compliance

**Status**: PASS

**Evidence**:

- Component 1 (`mcp/serde_util.rs`): Created at the specified path. Three `pub(crate)` helpers only. No crate-level export. Consistent with ADR-001.
- Component 2 (`mcp/tools.rs`): Nine field annotations applied, no handler logic changed, no new imports, no new types. Consistent with "struct field annotations only" specification.
- Component 3 (`mcp/mod.rs`): `mod serde_util;` (private) added exactly as specified. Consistent with ADR-001.
- Component 4 (infra-001): IT-01 and IT-02 added to `test_tools.py` as specified. Consistent with ADR-003.
- No new crate dependencies introduced (FR-10, C-02 honored).
- `infra/validation.rs` is unchanged (FR-09, C-07 honored).
- rmcp version unchanged (NFR-06, C-01 honored).
- AC-10 schema snapshot test placed in `server.rs` test block, accessing `tool_router` directly (same module = private field access). The `tool_router_for_test()` accessor from pseudocode/tools.md was not needed; direct access is simpler and correct.

### Check 3: Interface Implementation

**Status**: PASS

**Evidence**:

All nine fields verified in tools.rs (lines 52-57, 81-95, 137-141, 158-161, 183-186, 198-201, 237-242, 277-282):

| Field | `#[serde(default)]` | `deserialize_with` | `schemars(with)` |
|-------|---------------------|-------------------|-----------------|
| GetParams.id | absent (required field, correct) | `deserialize_i64_or_string` | `"i64"` |
| DeprecateParams.id | absent (correct) | `deserialize_i64_or_string` | `"i64"` |
| QuarantineParams.id | absent (correct) | `deserialize_i64_or_string` | `"i64"` |
| CorrectParams.original_id | absent (correct) | `deserialize_i64_or_string` | `"i64"` |
| LookupParams.id | present (correct) | `deserialize_opt_i64_or_string` | `"Option<i64>"` |
| LookupParams.limit | present (correct) | `deserialize_opt_i64_or_string` | `"Option<i64>"` |
| SearchParams.k | present (correct) | `deserialize_opt_i64_or_string` | `"Option<i64>"` |
| BriefingParams.max_tokens | present (correct) | `deserialize_opt_i64_or_string` | `"Option<i64>"` |
| RetrospectiveParams.evidence_limit | present (correct) | `deserialize_opt_usize_or_string` | `"Option<u64>"` |

All five optional fields correctly carry `#[serde(default)]` alongside `deserialize_with`, satisfying FR-08 and ADR-004 (the highest-severity trap R-01).

### Check 4: Test Case Alignment

**Status**: PASS

**Evidence**:

**Rust unit tests (serde_util.rs `#[cfg(test)]` — 33 tests)**:
Covers all pseudocode scenarios: integer input, string input, negative string (i64), zero string, MAX/MIN boundary strings, overflow string rejection, non-numeric string rejection, empty string rejection, float string rejection, whitespace-padded rejection, float JSON Number rejection (visit_f64), boolean rejection, array rejection, null input (opt variants), absent field (opt variants), negative string rejection (usize), u64 overflow string rejection.

**Rust unit tests (tools.rs `vnc012_coercion_tests` module — 43 tests)**:
All ACs covered:
- AC-01: `test_get/deprecate/quarantine_params_string_id` — GetParams, DeprecateParams, QuarantineParams with string id
- AC-02: `test_correct_params_string_original_id`
- AC-03: `test_lookup_params_string_id/limit`, absent-id/limit, null-id/limit
- AC-04: `test_search_params_string_k`, absent, null
- AC-05: `test_briefing_params_string_max_tokens`, absent, null
- AC-06: `test_retro_params_string/zero/absent/null_evidence_limit`
- AC-07: regression tests for integer inputs on all four required-i64 fields
- AC-08/AC-08-OPT: non-numeric string rejection for all 9 fields
- AC-09: negative string rejection for usize
- AC-09-FLOAT: float string rejection
- AC-09-FLOAT-NUMBER: float JSON Number rejection (3 tests, one per field type)
- AC-13: `test_get_params_string_id_coercion` + `test_deprecate_params_string_id_coercion` — from_value path (rmcp Parameters<T> equivalent)

**server.rs AC-10 schema snapshot test**:
`test_schema_integer_type_preserved_for_all_nine_fields` — uses `make_server()` + `tool_router.list_all()`, asserts `"type": "integer"` for all 9 fields, verifies `evidence_limit` minimum: 0 if present. Covers R-05.

**Python infra-001 tests (IT-01, IT-02)**:
Both tests marked `@pytest.mark.smoke`. IT-01 stores an entry, converts id to string via `str(entry_id)`, calls `context_get` via `server.call_tool`, asserts success + non-empty content + content match. IT-02 does the same for `context_deprecate`. Uses `extract_entry_id` + `assert_tool_success` + `get_result_text` per pseudocode specification. Covers R-02 (rmcp dispatch path).

### Check 5: Code Quality

**Status**: WARN (pre-existing file size only)

**Evidence**:

- `cargo check -p unimatrix-server` — PASS. No errors. 14 warnings, all pre-existing (unused imports/fields in non-vnc-012 code).
- `cargo test -p unimatrix-server` — 2455 unit tests pass. 0 failures. 0 regressions.
- No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` in `serde_util.rs` or in the new test/annotation sections of `tools.rs` and `server.rs`.
- All `.unwrap()` calls in new code are inside `#[cfg(test)]` blocks (acceptable per project rules).
- `serde_util.rs`: 451 lines (within 500-line limit).
- `tools.rs`: 5412 lines — exceeds the 500-line limit. This is a pre-existing condition documented in `pseudocode/OVERVIEW.md` (line 167): "tools.rs currently contains 5020 lines" at design time. The feature added ~18 annotation lines + ~392 test lines. A refactor is explicitly out of scope for vnc-012. This WARN is inherited from pre-existing state, not introduced by this feature.
- `server.rs`: 3031 lines — pre-existing condition, not modified beyond adding the AC-10 test.

### Check 6: Security

**Status**: PASS

**Evidence**:

- No hardcoded secrets, API keys, or credentials in any modified file.
- No path traversal: the feature touches only serde deserialization of integer fields. No file system operations introduced.
- No command injection: no shell/process invocations added.
- Input validation: non-numeric strings, float strings, float JSON Numbers, booleans, arrays, and objects are all rejected with typed serde errors (not panics, not silent coercions). Verified via 33+ unit tests.
- `usize::try_from` (not `as usize`) prevents silent truncation on 32-bit targets per C-06.
- Malformed data handling: invalid input produces `serde::de::Error` which rmcp wraps as `ErrorData::invalid_params(...)` — no panics, no state corruption.
- No new crate dependencies (C-02) — no new CVE surface.

### Check 7: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

| Agent | Report | Stewardship Block | Assessment |
|-------|--------|------------------|------------|
| vnc-012-agent-3-serde_util | `agents/vnc-012-agent-3-serde_util-report.md` | `## Knowledge Stewardship` present; `Queried:` entry (context_briefing); `Stored:` entry #3791 "Use deserialize_option (not deserialize_any) for Option<T> serde visitor helpers" | PASS |
| vnc-012-agent-4-tools | `agents/vnc-012-agent-4-tools-report.md` | `## Knowledge Stewardship` present; `Queried:` entry (context_search for vnc-012 decisions); `Stored:` entry #3792 "serde deserialize_with path must be crate-absolute, not module-relative" | PASS |
| vnc-012-agent-5-infra_001 | `agents/vnc-012-agent-5-infra_001-report.md` | `## Knowledge Stewardship` present; `Queried:` noted (skipped as not needed given fully specified pseudocode); `Stored:` "nothing novel to store -- the pattern is already implied by the test plan spec" | WARN (Queried entry says "skipped" but reason is given; pattern is documented) |

The agent-5 stewardship is a minor WARN: the `Queried:` entry says it was skipped rather than showing evidence of a query. The pseudocode was fully specified and the infra-001 tests are an exact mechanical translation, so the skip is reasonable and a reason is provided. Per gate 3b rules: "Present but no reason after 'nothing novel' = WARN" — the reason is present. The WARN does not block the gate.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate-3b results are feature-specific (all 7 checks passed with minor pre-existing WARNs); no recurring validation failure pattern observed. The `deserialize_with` path absolute-vs-relative deviation was already captured by agent-4 as entry #3792.
