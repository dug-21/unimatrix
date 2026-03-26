## ADR-002: Round-Trip Integration Test as Dual-Type Guard

### Context

The eval harness maintains two independent copies of `ScenarioResult`: one in
`eval/runner/output.rs` (writer side) and one in `eval/report/mod.rs` (reader side). This
is an established architectural pattern (pattern #3550, confirmed across nan-007 and
nan-008) that avoids a compile-time dependency between the runner and report modules.

The consequence is a non-obvious sync requirement: adding `phase: Option<String>` to
`ScenarioResult` requires updating both files. A partial update — adding the field only to
`runner/output.rs` — compiles successfully. The report module silently defaults `phase` to
`None` for every result via `#[serde(default)]`. No compile error. No test failure unless
there is a test that specifically checks for a non-null phase in the report output.

SR-03 from the scope risk assessment identifies this as a high-severity risk. Three
approaches exist:

**Option A — Shared type**: Introduce a single `EvalScenarioResult` type in a common
location (a new `eval/types.rs` or via a re-export from `runner/output.rs`) that both
modules use. This provides compile-time enforcement: a missing field fails everywhere.
Cost: it requires restructuring the deliberate compile-time isolation between runner and
report, changing the module boundary contract established in nan-007.

**Option B — Newtypes or type aliases**: Introduce an `EvalPhase` newtype (`type EvalPhase
= Option<String>`) in a shared module. This makes the sync risk visible at compile time for
the type itself but does not enforce that both `ScenarioResult` copies carry the field.
Partial mitigation only.

**Option C — Round-trip integration test**: Follow the precedent from ADR-003 nan-008,
which used a single round-trip integration test to guard the same risk for `cc_at_k` and
`icd`. The test serializes a `ScenarioResult` (runner side) with a non-null phase, writes
it to a temp directory, calls `run_report`, and asserts that the rendered report contains
the phase value. A partial update that omits `phase` from `report/mod.rs` will cause the
report to silently zero/default the field, and the assertion `content.contains("delivery")`
will fail.

The deliberate compile-time isolation between runner and report is an architectural
decision from nan-007 that reduces coupling at the cost of a manual sync burden. ADR-003
nan-008 established the round-trip test as the sufficient enforcement mechanism for this
pattern. Changing the module boundary now (Option A) would require touching both modules
and the module visibility structure, producing changes beyond the scope of nan-009.

The scope risk assessment's recommendation (SR-03) is to use a round-trip integration
test, consistent with ADR-003 nan-008.

### Decision

Use a round-trip integration test (`test_report_round_trip_phase_section_7_distribution`
in `eval/report/tests.rs`) as the enforcement mechanism for the dual-type sync requirement.
Do not introduce a shared type or newtype for `phase` in nan-009.

The test must:
1. Construct a `ScenarioResult` (using the report module's local type) with
   `phase: Some("delivery".to_string())` and non-trivial metric values.
2. Write it as JSON to a `TempDir`.
3. Call `run_report`.
4. Assert that the rendered report output contains:
   - `"## 6. Phase-Stratified Metrics"` (new section present).
   - `"## 7. Distribution Analysis"` (renumbered section present — SR-02 guard).
   - `"delivery"` (phase value appears in section 6 — SR-03 guard).
   - Correct section order: `pos("## 6.")` < `pos("## 7.")`.
   - `!content.contains("## 6. Distribution Analysis")` (old heading absent).
5. Update the existing `test_report_contains_all_five_sections` to assert the new heading
   count and updated section numbers.
6. Update the existing `test_report_round_trip_cc_at_k_icd_fields_and_section_6` to
   assert `"## 7. Distribution Analysis"` (not `"## 6."`).

The `make_scenario_result` helper in `tests.rs` must be updated to include the `phase`
field so that construction compiles after the type change.

### Consequences

- The module boundary between runner and report is preserved as designed in nan-007.
- A single integration test catches both SR-02 (section renumbering) and SR-03 (dual-type
  sync) in one pass, consistent with the approach proven effective in nan-008.
- Delivery agents must not skip or stub the round-trip test. It is a mandatory acceptance
  criterion, not an optional enhancement.
- If a future feature adds another field to both `ScenarioResult` copies, the round-trip
  test must be updated to include non-trivial values for that field too — zero-valued
  fields do not catch partial updates.
- The pattern is now documented twice in Unimatrix (nan-008 ADR-003 and this ADR) and
  should be referenced in any future nan-* eval harness feature that adds fields to the
  dual-type structure.
