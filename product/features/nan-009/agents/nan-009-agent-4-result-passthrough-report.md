# Agent Report: nan-009 Result Passthrough

**Agent ID**: nan-009-agent-4-result-passthrough
**Component**: Result Passthrough (`eval/runner/`)
**Feature**: nan-009 Phase-Stratified Eval Scenarios (GH #400)

---

## Files Modified

- `crates/unimatrix-server/src/eval/runner/output.rs` — added `phase: Option<String>` with `#[serde(default)]` only to `ScenarioResult`; added `make_scenario_result` helper and two new unit tests
- `crates/unimatrix-server/src/eval/runner/replay.rs` — set `phase: record.context.phase.clone()` on constructed `ScenarioResult` in `replay_scenario`
- `crates/unimatrix-server/src/eval/runner/tests.rs` — added `phase: None` to two `ScenarioResult` struct literal constructions
- `crates/unimatrix-server/src/eval/runner/tests_metrics.rs` — added `phase: None` to `ScenarioContext` struct literal construction

Note: `eval/scenarios/types.rs` (`ScenarioContext.phase`) was already added by the Scenario Extraction agent running concurrently — no conflict, no duplicate edit needed.

---

## Changes Summary

### output.rs

Added `phase: Option<String>` as the last field on `ScenarioResult`, annotated with `#[serde(default)]` only — no `skip_serializing_if`. This ensures the runner always emits `"phase":null` or `"phase":"delivery"` in result JSON (ADR-001 consistent key presence on the writer side). Added `make_scenario_result` helper and two tests:

- `test_scenario_result_phase_null_serialized_as_null` — asserts `"phase":null` is emitted for `phase: None` (R-05 guard)
- `test_scenario_result_phase_non_null_serialized` — asserts `"phase":"delivery"` for `phase: Some("delivery")`

### replay.rs

Added `phase: record.context.phase.clone()` to the `ScenarioResult` struct literal in `replay_scenario`. Phase is assigned only to the result struct — it is not forwarded to `ServiceSearchParams` or `AuditContext` (R-06, measurement purity). Constraint is automatically satisfied since neither struct has a `phase` field; confirmed by code review.

---

## Tests

```
running 49 tests
... all pass ...
test result: ok. 49 passed; 0 failed
```

Pre-existing failures unrelated to this component: `uds::listener::tests::col018_*` (embedding model not initialized in test environment — pre-existing, confirmed by stash-and-retest).

---

## Issues / Blockers

None. All constraints from the spawn prompt are satisfied:
- Runner `ScenarioResult.phase` uses `#[serde(default)]` only — no `skip_serializing_if` (ADR-001 V-3 resolved)
- Phase does not reach `ServiceSearchParams` or `AuditContext` (R-06)
- `test_scenario_result_phase_null_serialized_as_null` added and passing

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "eval harness serde dual-type ScenarioResult" — found patterns #3526, #3550, #3555, #3557 (all directly relevant, applied)
- Queried: `context_lookup(topic: "nan-009", category: "decision")` — found ADR-001, ADR-002, ADR-003 (all applied)
- Stored: nothing — `context_store` returned `MCP error -32003: Agent 'anonymous' lacks Write capability`. Pattern to store: "When adding a field to ScenarioContext or runner-side ScenarioResult in the eval harness, struct literal constructions exist in at least 4 test files (runner/tests.rs, runner/tests_metrics.rs, scenarios/tests.rs, scenarios/extract.rs). Grep for all sites before attempting a build — the compiler surfaces one at a time." Would store as pattern in topic `unimatrix-server`.
