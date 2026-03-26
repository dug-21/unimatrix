# Test Plan: Result Passthrough (`eval/runner/`)

Component files: `output.rs`, `replay.rs`

---

## Risk Coverage

| Risk | Tests in this component |
|------|------------------------|
| R-03 (High) | `test_scenario_result_phase_round_trip_serde` |
| R-05 (High) | `test_scenario_result_phase_null_serialized_as_null` |
| R-06 (High) | `test_replay_scenario_phase_not_in_search_params` + code review |
| IR-02 (Med) | `test_replay_scenario_phase_not_in_search_params` |

---

## Unit Tests

### `test_scenario_result_phase_null_serialized_as_null` (AC-03, R-05)

**Location**: `eval/runner/` tests (inline or dedicated `tests.rs`, sync `#[test]`)

**Arrange**:
Build a `ScenarioResult` (runner-side type from `eval/runner/output.rs`) with
`phase: None` and minimal required fields (e.g., `scenario_id`, `query`, `profiles`
as empty map, and `comparison` with zero values).

**Act**: `let json = serde_json::to_string(&result).expect("serialize");`

**Assert**:
- `json.contains("\"phase\":null")` — the key IS present with a null value.
- `!json.contains("\"phase\":\"")` — not a non-null string value.

**Rationale**: Runner copy carries `#[serde(default)]` only — no `skip_serializing_if`.
An `Option<String>` with `serde(default)` alone does NOT suppress null on serialization
(pattern #3255). If a delivery agent mistakenly adds `skip_serializing_if` to the runner
copy, this test fails. This is the first direction of the R-05 two-direction guard.

---

### `test_scenario_result_phase_round_trip_serde` (R-03)

**Location**: `eval/runner/` tests (sync `#[test]`)

**Arrange**:
Build `ScenarioResult { phase: Some("design".to_string()), ... }` using the runner-side
type.

**Act**:
1. Serialize: `let json = serde_json::to_string(&result).expect("serialize")`.
2. Deserialize using the **report module's local type**:
   `let deserialized: eval::report::ScenarioResult = serde_json::from_str(&json).expect("deserialize")`.

   Note: because `report::ScenarioResult` is a module-private type, this test may need
   to live in `eval/report/tests.rs` using the locally visible type, or `pub(super)` must
   be added to the report-side type for test visibility. Either approach is acceptable;
   the key requirement is that deserialization uses the **report copy's type**.

**Assert**:
- `assert_eq!(deserialized.phase, Some("design".to_string()))`.

**Rationale**: Catches a partial update where `phase` is added to the runner copy but
omitted from the report copy. If the report copy lacks `phase`, deserialization defaults
it to `None` (via `#[serde(default)]`) and the assertion fails. Non-trivial value
`"design"` ensures the test cannot pass by accident with a zero/null default.
This addresses R-03 (ADR-002 constraint).

---

### `test_replay_scenario_phase_not_in_search_params` (R-06, IR-02)

**Location**: `eval/runner/` tests (may require `#[tokio::test]` if `replay_scenario`
is async)

**Arrange**:
Construct a `ScenarioRecord` where `context.phase = Some("design".to_string())` and
minimal other fields. Provide a mock or real `unimatrix_server` search service
(depending on the existing test infrastructure for the runner module).

**Act**: Call `replay_scenario(&record, ...)` or inspect its internals.

**Assert**:
- The `ServiceSearchParams` constructed inside `replay_scenario` does not contain a
  phase field, OR if `ServiceSearchParams` has a `phase` field, it remains at its
  default value (zero weight, None, or equivalent).
- `result.phase == Some("design".to_string())` — phase IS carried to the result.
  This confirms phase is set on the output struct only, not forwarded to the search
  invocation.

**Rationale**: This test guards the measurement purity constraint (FR-06, Constraint 3).
Phase must never influence the search execution path. The most important correctness
risk in this feature. If no existing test infrastructure exists for inspecting
`ServiceSearchParams`, a code review checkpoint is the fallback (see code review note
below).

**Code review checkpoint (R-06 fallback)**:
If `replay_scenario` cannot be unit-tested against `ServiceSearchParams` without
significant mock infrastructure, the delivery agent must confirm by code review that
`phase` is only assigned to `result.phase` and is never passed to `ServiceSearchParams`,
`AuditContext`, or any retrieval weight parameter. This must be documented in the
RISK-COVERAGE-REPORT.md as "verified by code review."

---

## Integration Notes

The runner module has a deliberate compile-time isolation from `report/mod.rs`. The
`ScenarioResult` type in `runner/output.rs` is the producing side; the report-side
copy deserializes it from JSON files. The round-trip test
`test_scenario_result_phase_round_trip_serde` (above) bridges the two types explicitly
to catch a partial update.

The full end-to-end round-trip test `test_report_round_trip_phase_section_7_distribution`
(in `report-entrypoint.md`) also covers R-03 by asserting `"delivery"` appears in
rendered section 6 — a partial update causes `phase` to be `None` on the report side,
so "delivery" never appears.
