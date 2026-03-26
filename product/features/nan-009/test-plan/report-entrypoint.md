# Test Plan: Report Entry Point (`eval/report/mod.rs`)

Component files: `mod.rs` (local `ScenarioResult` copy, `PhaseAggregateStats` struct,
`run_report` pipeline wiring)

All tests are sync `#[test]`.

---

## Risk Coverage

| Risk | Tests in this component |
|------|------------------------|
| R-03 (High) | `test_scenario_result_phase_round_trip_serde` (round-trip through report-side type) |
| IR-03 (High) | `test_report_round_trip_phase_section_7_distribution` (wiring: compute called before render) |
| EC-05 (Med) | `test_report_deserializes_legacy_result_missing_phase_key` |
| EC-06 (Med) | `test_report_deserializes_explicit_null_phase_key` |

---

## Key Invariants

1. `run_report` public signature is **unchanged**: `pub fn run_report(result_dir: &Path, output_path: &Path) -> Result<()>`.
2. The local `ScenarioResult` type gains `phase: Option<String>` with `#[serde(default)]` — no `skip_serializing_if` (report path is read-only).
3. `compute_phase_stats` is called inside `run_report` Step 4 and its result is passed to `render_report`.
4. If `compute_phase_stats` returns an empty vec, `render_report` receives an empty slice and omits section 6.

---

## Unit Tests

### `test_report_deserializes_legacy_result_missing_phase_key` (AC-06, EC-05, NFR-01)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
```rust
// JSON without any "phase" key — represents a pre-nan-009 result file
let json = r#"{
    "scenario_id": "legacy-01",
    "query": "what is context_search?",
    "profiles": {},
    "comparison": {
        "kendall_tau": 0.8,
        "rank_changes": [],
        "mrr_delta": 0.1,
        "p_at_k_delta": 0.05,
        "latency_overhead_ms": 10,
        "cc_at_k_delta": 0.0,
        "icd_delta": 0.0
    }
}"#;
```

**Act**:
```rust
let result: ScenarioResult = serde_json::from_str(json)
    .expect("legacy result must deserialize without error");
```

**Assert**:
- `result.phase.is_none()` — missing key defaults to `None`.
- No panic, no error.

**Rationale**: NFR-01 backward compatibility. `#[serde(default)]` on the report-side
`phase` field handles missing keys. Without this annotation, deserialization would
fail with "missing field 'phase'".

---

### `test_report_deserializes_explicit_null_phase_key` (AC-06, EC-06)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
```rust
// JSON with explicit "phase":null — represents output from eval run on null-phase scenario
let json = r#"{
    "scenario_id": "null-phase-01",
    "query": "some query",
    "phase": null,
    "profiles": {},
    "comparison": {
        "kendall_tau": 0.0,
        "rank_changes": [],
        "mrr_delta": 0.0,
        "p_at_k_delta": 0.0,
        "latency_overhead_ms": 0,
        "cc_at_k_delta": 0.0,
        "icd_delta": 0.0
    }
}"#;
```

**Act**:
```rust
let result: ScenarioResult = serde_json::from_str(json)
    .expect("explicit null phase must deserialize without error");
```

**Assert**: `result.phase.is_none()`.

**Rationale**: The runner copy always emits `"phase":null` for null-phase results.
The report-side `#[serde(default)]` handles both absent key (EC-05) and explicit null
(EC-06) correctly. This test confirms both paths.

---

## Pipeline Wiring Verification

The `run_report` function must call `compute_phase_stats(&results)` and pass the
result to `render_report`. This is verified indirectly by:

1. `test_report_round_trip_phase_section_7_distribution` — if the call is missing,
   section 6 is absent and assertion (1) fails.
2. `test_render_phase_section_absent_when_stats_empty` — if an empty slice is passed
   unconditionally (ignoring the actual results), section 6 is always absent.

There is no separate unit test for the wiring itself because the integration test
covers it more completely. The wiring is a compile-safe change (new parameter must
be supplied to `render_report`).

---

## Struct Definitions to Verify

During Stage 3c, the following struct definitions must be confirmed in `mod.rs`:

```rust
// Local ScenarioResult copy — report path only
struct ScenarioResult {
    scenario_id: String,
    query: String,
    profiles: HashMap<String, ProfileResult>,
    comparison: ComparisonMetrics,
    #[serde(default)]
    phase: Option<String>,   // NEW — no skip_serializing_if
}

// New struct — must be pub(super) for visibility from aggregate.rs and render.rs
#[derive(Debug, Default)]
pub(super) struct PhaseAggregateStats {
    pub phase_label: String,
    pub scenario_count: usize,
    pub mean_p_at_k: f64,
    pub mean_mrr: f64,
    pub mean_cc_at_k: f64,
    pub mean_icd: f64,
}
```

Verify that:
- `ScenarioResult.phase` has `#[serde(default)]` and NOT `skip_serializing_if`.
- `PhaseAggregateStats` is present in `mod.rs` (not in `aggregate.rs`).
- `PhaseAggregateStats` derives `Debug` and `Default`.
