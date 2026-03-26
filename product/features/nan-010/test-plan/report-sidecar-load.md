# Test Plan: Report Sidecar Load (`eval/report/mod.rs`)

Component 7 of 7.

---

## Scope

`run_report` in `eval/report/mod.rs` gains:
1. A private helper `load_profile_meta(dir: &Path) -> Result<HashMap<String, ProfileMetaEntry>, EvalError>`.
2. A new step (Step 3.5) between aggregation and rendering: `let profile_meta = load_profile_meta(results)?;`.
3. The `profile_meta` map is passed to `render_report` as the new final parameter.

This component owns two distinct behaviors:
- **Absent sidecar** → `Ok(HashMap::new())` — backward compatibility fallback.
- **Corrupt sidecar** → `Err(EvalError::...)` — abort with non-zero exit and descriptive message.

All tests are in `eval/report/tests_distribution_gate.rs`.

---

## Unit Test Expectations

### `test_report_without_profile_meta_json` (AC-11, AC-14, R-15)

Primary backward-compatibility test. Covers two sub-scenarios.

**Sub-scenario A — Absent `profile-meta.json`** (AC-11, AC-14):
- Arrange: construct a results directory (temp dir) with pre-nan-010 ScenarioResult JSON
  files but no `profile-meta.json`.
- Act: call `load_profile_meta(&tmp_dir)` or `run_report(&tmp_dir, ...)`.
- Assert:
  - Returns `Ok(map)` where `map.is_empty()` is `true`
  - No error emitted
  - Downstream `render_report` receives empty map → renders Section 5 as "Zero-Regression Check"
  - Report output contains `"Zero-Regression Check"` and does not contain `"Distribution Gate"`

**Sub-scenario B — `ScenarioResult` field count unchanged** (R-15, dual-type constraint):
- Use the pre-nan-010 result JSON from sub-scenario A.
- Assert: deserialization of `ScenarioResult` from the old JSON succeeds without error.
  This confirms zero new fields were added to `ScenarioResult` in `report/mod.rs`.

---

### `test_distribution_gate_corrupt_sidecar_aborts` (R-07)

The abort-on-corrupt behavior. This is the most critical test for Component 7 — it guards
against the implementation regressing to WARN+fallback (the resolved WARN-3 in
ALIGNMENT-REPORT.md).

- Arrange: create a temp directory containing a `profile-meta.json` with non-JSON content
  (e.g., `"not valid json {{{{"`).
- Act: call `load_profile_meta(&tmp_dir)`.
- Assert:
  - Returns `Err(EvalError::...)`
  - Error message contains `"profile-meta.json is malformed"` (exact substring per architecture)
  - Error message contains `"re-run eval to regenerate"` (exact substring per architecture)
  - Does NOT return `Ok(HashMap::new())` — no silent fallback

If testing via process spawn (`eval report` binary):
- Assert: process exits non-zero.
- Assert: stderr contains the malformed message.

---

### `test_distribution_gate_exit_code_zero` (R-12 — shared with Component 6)

See also section5-dispatch.md. From Component 7's perspective:
- `run_report` returns `Ok(String)` when gate fails (gate failure is in the report body).
- `run_report` returns `Err(EvalError::...)` only for I/O or malformed-sidecar errors.
- The wiring from `run_report` to the exit code must propagate `Ok` as exit 0 and `Err` as
  exit non-zero — the gate outcome is not an `Err`.

---

## `load_profile_meta` Behavior Contract

| Scenario | `profile-meta.json` state | Return value |
|----------|--------------------------|-------------|
| File absent | Does not exist | `Ok(HashMap::new())` |
| File present, valid JSON | Parses successfully | `Ok(populated_map)` |
| File present, non-JSON | Truncated, invalid syntax | `Err(EvalError::...)` with "malformed" message |
| File present, valid JSON but wrong schema | e.g., missing `version` field | `Err(EvalError::...)` with "malformed" message |
| `.tmp` file present, no `.json` | Only `profile-meta.json.tmp` exists | `Ok(HashMap::new())` — `.tmp` is ignored |

The `.tmp` ignored case is tested here to confirm `load_profile_meta` reads only
`profile-meta.json`, never `profile-meta.json.tmp`.

---

## Integration Test Expectations

No infra-001 integration tests required. The sidecar load is internal to the eval binary.

The round-trip integration boundary (runner writes → report reads) is covered within the
unit test suite via `test_write_profile_meta_schema` (Component 3): that test calls
`write_profile_meta` and then deserializes the output, validating the full pipeline across
the file boundary without needing the MCP harness.

---

## Backward-Compatibility Guarantee (NFR-05)

Any result directory produced before nan-010:
- Has no `profile-meta.json`.
- Has `ScenarioResult` JSON files without `distribution_change` field.
- `load_profile_meta` returns `Ok(HashMap::new())`.
- `render_report` dispatches all profiles to zero-regression path (unchanged behavior).

This guarantee is enforced by `test_report_without_profile_meta_json`.

---

## Risks Covered

| Risk | Test |
|------|------|
| R-07 (corrupt sidecar falls back silently) | `test_distribution_gate_corrupt_sidecar_aborts` |
| R-10 (schema mismatch) | Round-trip in `test_write_profile_meta_schema` (write + load cycle) |
| R-15 (dual-type violated) | `test_report_without_profile_meta_json` pre-nan-010 JSON deserialization |
| R-12 (exit code) | `test_distribution_gate_exit_code_zero` — `run_report` Ok vs Err semantics |
| AC-11 (backward compat) | `test_report_without_profile_meta_json` absent-file sub-scenario |
| AC-14 (backward compat test) | `test_report_without_profile_meta_json` ScenarioResult sub-scenario |
| Knowledge package #3585 (absent=fallback) | Absent-file returns `Ok(HashMap::new())`, not error |
