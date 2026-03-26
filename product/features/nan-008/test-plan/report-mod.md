# Test Plan: report/mod.rs

## Component Responsibility

Local deserialization-only copies of the runner types. Provides `run_report` entry
point. Must mirror `runner/output.rs` field-for-field so that result JSON files
are deserializable without a compile-time dependency on runner. New fields carry
`#[serde(default)]` for backward compatibility.

## Risks Covered

R-01 (dual copy divergence — deserialization side), R-07 (backward compat for
pre-nan-008 JSON), R-08 (category field present in deserialization copy).

---

## Tests in `report/tests.rs`

### `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (MANDATORY — ADR-003, R-01, R-02)

This is the single most important test in nan-008. It guards R-01 (dual type copy
divergence) and R-02 (section-order regression) simultaneously.

```
Arrange:
  results_dir = TempDir
  out_path = TempDir/report.md

  // Build ScenarioResult with all new fields populated with non-zero, non-trivial values
  entry = ScoredEntry {
      id: 1,
      title: "Test Entry",
      category: "decision",      // NEW — must be non-empty
      final_score: 0.9,
      similarity: 0.85,
      confidence: 0.7,
      status: "Active",
      nli_rerank_delta: None,
  }

  baseline_profile = ProfileResult {
      entries: vec![entry.clone()],
      latency_ms: 50,
      p_at_k: 0.6,
      mrr: 0.5,
      cc_at_k: 0.714,           // NEW — 5/7 categories covered
      icd: 0.857,               // NEW — entropy value
  }

  candidate_profile = ProfileResult {
      entries: vec![entry],
      latency_ms: 60,
      p_at_k: 0.7,
      mrr: 0.6,
      cc_at_k: 0.857,           // NEW — higher coverage
      icd: 1.234,               // NEW — higher entropy
  }

  comparison = ComparisonMetrics {
      kendall_tau: 0.8,
      rank_changes: vec![],
      mrr_delta: 0.1,
      p_at_k_delta: 0.1,
      latency_overhead_ms: 10,
      cc_at_k_delta: 0.143,     // NEW — 0.857 - 0.714
      icd_delta: 0.377,         // NEW — 1.234 - 0.857
  }

  result = ScenarioResult {
      scenario_id: "sc-roundtrip",
      query: "round trip query",
      profiles: {"baseline": baseline_profile, "candidate": candidate_profile},
      comparison,
  }

  write JSON of result to results_dir/sc-roundtrip.json

Act:
  run_report(results_dir.path(), None, &out_path)?

Assert (all must pass):
  content = read(out_path)

  // R-01: non-zero values survived the JSON round-trip (serde(default) did not zero them)
  content.contains("0.857")  // cc_at_k value appears somewhere in report
  content.contains("1.234")  // icd value
  content.contains("0.143")  // cc_at_k_delta
  content.contains("decision")  // category appears in entry-level analysis

  // R-02: sections appear in correct order
  pos5 = content.find("## 5.").unwrap()
  pos6 = content.find("## 6.").unwrap()
  pos5 < pos6

  // Section 6 exists and contains Distribution Analysis content
  content.contains("Distribution Analysis")

  // AC-14: ICD annotation with ln(
  content.contains("ln(")

  // AC-13: full section order
  pos1 < pos2 && pos2 < pos3 && pos3 < pos4 && pos4 < pos5 && pos5 < pos6
```

### `test_report_backward_compat_pre_nan008_json` (R-07, AC-07)

```
Arrange:
  // Build a ScenarioResult using ONLY pre-nan-008 fields — no cc_at_k, icd, category
  old_json = r#"{
    "scenario_id": "old-sc",
    "query": "old query",
    "profiles": {
      "baseline": {
        "entries": [{"id":1,"title":"T","final_score":0.9,"similarity":0.8,"confidence":0.7,"status":"Active","nli_rerank_delta":null}],
        "latency_ms": 50,
        "p_at_k": 0.6,
        "mrr": 0.5
      }
    },
    "comparison": {
      "kendall_tau": 0.9,
      "rank_changes": [],
      "mrr_delta": 0.0,
      "p_at_k_delta": 0.0,
      "latency_overhead_ms": 0
    }
  }"#

  write old_json to results_dir/old-sc.json

Act:
  result = run_report(results_dir.path(), None, &out_path)

Assert:
  result.is_ok()
  // cc_at_k and icd defaulted to 0.0 — report produces successfully
  content = read(out_path)
  content.contains("## 1. Summary")
  content.contains("## 5.")
  // No deserialization error, no panic
```

### `test_report_contains_all_six_sections` (R-02, AC-13)

Extends the existing `test_report_contains_all_five_sections`. The existing test
must be updated to also assert section 6.

```
Arrange: same as existing test (two scenarios with baseline + candidate)
         BUT profiles must include cc_at_k and icd fields so section 6 renders

Act:     run_report(...)

Assert:
  // All six sections present
  content.contains("## 1. Summary")
  content.contains("## 2. Notable Ranking Changes")
  content.contains("## 3. Latency Distribution")
  content.contains("## 4. Entry-Level Analysis")
  content.contains("## 5. Zero-Regression Check")
  content.contains("## 6.")  // Distribution Analysis

  // Strict position ordering
  pos1 < pos2 && pos2 < pos3 && pos3 < pos4 && pos4 < pos5 && pos5 < pos6

  // Summary table has new metric columns
  content.contains("CC@k")
  content.contains("ICD")
```

### `test_report_single_profile_section_6_omits_comparison_subtable` (FR-09)

```
Arrange: one profile only ("baseline") with cc_at_k and icd populated

Act:     run_report(...)

Assert:
  content.contains("## 6.")  // Section 6 present
  content.contains("Distribution Analysis")
  // Improvement/degradation sub-table is absent in single-profile mode
  !content.contains("Top 5 Improved") && !content.contains("Top 5 Degraded")
  // (or equivalent heading text from the implementation)
```

### `test_report_icd_column_annotated_with_ln_n` (R-04, AC-14)

```
Arrange: any valid two-profile result set with non-zero icd values

Act:     run_report(...)

Assert:
  content.contains("ln(")
  // The ICD annotation appears in either the Summary table header or the
  // Distribution Analysis section
```

### `test_serde_default_on_missing_cc_at_k_field`

```
// Direct deserialization test on ProfileResult
Arrange: json = r#"{"entries":[],"latency_ms":50,"p_at_k":0.6,"mrr":0.5}"#
         // cc_at_k and icd absent

Act:     result: ProfileResult = serde_json::from_str(json)?

Assert:
  result.cc_at_k == 0.0
  result.icd == 0.0
  // no deserialization error
```

### `test_serde_default_on_missing_category_field`

```
Arrange: json = r#"{"id":1,"title":"T","final_score":0.9,"similarity":0.8,"confidence":0.7,"status":"A","nli_rerank_delta":null}"#
         // category field absent

Act:     entry: ScoredEntry = serde_json::from_str(json)?

Assert:
  entry.category == ""  // serde(default) for String
  // no deserialization error
```

---

## Existing Tests That Must Be Updated

### `test_report_contains_all_five_sections`

Must be extended to assert section 6 (becomes `test_report_contains_all_six_sections`).

### `make_profile_result` helper

Must be extended to accept `cc_at_k` and `icd` parameters (or add a new variant):

```rust
fn make_profile_result_with_metrics(
    p_at_k: f64, mrr: f64, latency_ms: u64, cc_at_k: f64, icd: f64
) -> ProfileResult { ... }
```

Alternatively, the existing helper returns defaults (0.0) for the new fields and
callers that need non-zero values use the extended version.

### `make_scenario_result` helper

Must include `cc_at_k_delta` and `icd_delta` in the `ComparisonMetrics` it builds
(both default to 0.0 unless the test requires specific values).

---

## NFR Checks (code review)

- All new fields in `report/mod.rs` copies carry `#[serde(default)]`
- No `tokio`, `async`, or `spawn_blocking` in any file under `report/`
- `default_comparison` function updated to return a `ComparisonMetrics` with
  `cc_at_k_delta: 0.0` and `icd_delta: 0.0`
- `AggregateStats` gains four new `f64` fields with no `serde(default)` required
  (it is an internal computed type, not deserialized from external JSON)
