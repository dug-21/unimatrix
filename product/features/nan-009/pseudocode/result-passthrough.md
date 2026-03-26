# Component: Result Passthrough

Files: `eval/runner/output.rs`, `eval/runner/replay.rs`

## Purpose

Add `phase` to `ScenarioResult` (runner copy) and copy it from `record.context.phase`
inside `replay_scenario`. Phase is strictly metadata — it must NOT reach
`ServiceSearchParams` or `AuditContext`. Retrieval must reproduce the original query
unchanged.

---

## 1. `eval/runner/output.rs`

### Change: Add `phase` field to `ScenarioResult`

Current `ScenarioResult` struct (lines 76-82):
```
pub struct ScenarioResult {
    pub scenario_id: String,
    pub query: String,
    pub profiles: HashMap<String, ProfileResult>,
    pub comparison: ComparisonMetrics,
}
```

Add `phase` with `#[serde(default)]` ONLY — no `skip_serializing_if`:
```
pub struct ScenarioResult {
    pub scenario_id: String,
    pub query: String,
    pub profiles: HashMap<String, ProfileResult>,
    pub comparison: ComparisonMetrics,
    #[serde(default)]
    pub phase: Option<String>,
}
```

Serde annotation rules (ADR-001):
- `#[serde(default)]` — backward-compatible deserialization if result files are read back
- NO `skip_serializing_if` — the runner always emits `"phase":null` or `"phase":"delivery"`;
  consistent key presence lets the report module and any external tooling rely on the key
  being present
- This is the opposite annotation from `ScenarioContext.phase` in `types.rs` — the
  runner side EMITS (always writes the key); the scenario side OMITS when null

No other changes to `output.rs`.

---

## 2. `eval/runner/replay.rs`

### Change: Set `phase` on `ScenarioResult` in `replay_scenario`

Current `replay_scenario` return value (lines 74-81):
```
Ok(ScenarioResult {
    scenario_id: record.id.clone(),
    query: record.query.clone(),
    profiles: profile_results,
    comparison,
})
```

Add `phase` field — copy from context:
```
Ok(ScenarioResult {
    scenario_id: record.id.clone(),
    query: record.query.clone(),
    profiles: profile_results,
    comparison,
    phase: record.context.phase.clone(),    // NEW — metadata passthrough only
})
```

### Constraint: Phase must NOT reach ServiceSearchParams or AuditContext

Inspection of `run_single_profile` (lines 83-168) reveals the two places where phase
must NOT appear:

**ServiceSearchParams construction (lines 95-107):**
```
let params = ServiceSearchParams {
    query: record.query.clone(),
    k,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: Some(record.context.agent_id.clone()),
    retrieval_mode,
    session_id: None,
    category_histogram: None,
    // phase must NOT be added here
};
```

`ServiceSearchParams` has no `phase` field; no change is needed here. The constraint
is satisfied automatically — but implementation must not add a `phase` field to
`ServiceSearchParams` as part of this feature.

**AuditContext construction (lines 109-120):**
```
let audit_ctx = AuditContext {
    source: AuditSource::Internal { service: "eval-runner".to_string() },
    caller_id: record.context.agent_id.clone(),
    session_id: Some(record.context.session_id.clone()),
    feature_cycle: if record.context.feature_cycle.is_empty() {
        None
    } else {
        Some(record.context.feature_cycle.clone())
    },
    // phase must NOT be added here
};
```

`AuditContext` has no `phase` field; no change is needed here. The constraint is
satisfied automatically.

`phase` is assigned ONLY to the `ScenarioResult` struct in the return statement of
`replay_scenario`. It is NOT assigned inside `run_single_profile`. The correct
placement is the outer function, after the search has completed.

---

## Error Handling

No new error paths introduced. `record.context.phase.clone()` on `Option<String>` is
infallible — it returns `None` or `Some(String)`.

If `ScenarioContext` was deserialized from a pre-nan-009 JSONL file (no `phase` key),
`context.phase` is `None` (via `#[serde(default)]` on the extraction side). The `clone()`
gives `None`, and `write_scenario_result` emits `"phase":null` in the output JSON.
This is correct per ADR-001.

---

## Key Test Scenarios

Tests live in `eval/runner/output.rs` (unit) and `eval/report/tests.rs` (round-trip).

**T1: `test_scenario_result_phase_null_serialized_as_null`** (R-05)
- Unit test in `runner/output.rs` tests module
- Construct `ScenarioResult { phase: None, ... }`
- Serialize with `serde_json::to_string`
- Assert output JSON CONTAINS `"phase":null` — key is present even for None
- This guards against accidentally placing `skip_serializing_if` on the runner copy

**T2: `test_scenario_result_phase_non_null_serialized`** (AC-03)
- Construct `ScenarioResult { phase: Some("delivery"), ... }`
- Serialize
- Assert output contains `"phase":"delivery"`

**T3: `test_replay_scenario_phase_not_in_search_params`** (R-06)
- Construct a `ScenarioRecord` with `context.phase = Some("design")`
- Call or inspect `run_single_profile` to confirm `ServiceSearchParams` does not have
  a phase field set
- This is primarily a code-review checkpoint; if `ServiceSearchParams` gains a phase
  field, this test should assert it is NOT set to `Some("design")`
- Asserting `ScenarioResult.phase == Some("design")` after replay confirms passthrough

**T4 (round-trip, in report/tests.rs):** `test_report_round_trip_phase_section_7_distribution`
- Writes a `ScenarioResult` with `phase: Some("delivery")` via runner-side type
- Reads it back via report-side `run_report`
- Asserts `"delivery"` appears in section 6 of rendered report
- This is the dual-type guard (ADR-002, R-03) — failure here means one copy was missed

---

## Notes

- `phase` is placed as the last field in `ScenarioResult` for append-only ordering.
- The existing `tests` module in `runner/output.rs` (lines 106-210) creates
  `ScenarioResult` instances via helper functions. Those helpers must be updated to
  include `phase` in `ScenarioResult` construction after the field is added — or the
  compiler will require it if `phase` lacks a default. Since it has `#[serde(default)]`
  for deserialization only, struct literal construction requires explicit `phase: None`
  or `phase: Some(...)`. All test helpers in `output.rs` that construct `ScenarioResult`
  must add `phase: None` (or a non-null value as appropriate).
