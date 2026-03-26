# Test Plan: runner/output.rs

## Component Responsibility

Type definitions for all JSON result structures produced by `eval run`. These types
are `Serialize + Deserialize` and are the canonical source of the JSON schema. The
three types with new fields are `ScoredEntry`, `ProfileResult`, and `ComparisonMetrics`.

## Risks Covered

R-01 (dual copy divergence — canonical side), R-08 (ScoredEntry.category absent),
R-07 (backward compat — as producer).

---

## Key Assertions

The output types in `runner/output.rs` are the **source of truth**. Tests validate
that the fields serialize to JSON with the correct keys and types. The corresponding
round-trip test in `report/tests.rs` validates the deserialization side.

### Serialization Field Presence

These are validated by the round-trip test in `report/tests.rs`
(`test_report_round_trip_cc_at_k_icd_fields_and_section_6`). That test:

1. Constructs a `ScenarioResult` with `ProfileResult { cc_at_k: 0.857, icd: 1.234, ... }`
   and `ComparisonMetrics { cc_at_k_delta: 0.143, icd_delta: 0.211, ... }` and
   `ScoredEntry { category: "decision", ... }`.
2. Serializes with `serde_json::to_string`.
3. Passes the JSON to `run_report`.
4. Asserts the rendered report contains non-zero values.

The test fails at compile time if any new field is missing from `runner/output.rs`,
and fails at runtime if the field is absent from the serialized JSON.

### `ScoredEntry.category` Struct Literal Test

To confirm the struct compiles with the new field, the existing `make_entries` helper
in `tests_metrics.rs` must be updated to include `category: String`:

```rust
fn make_entries(ids: &[u64]) -> Vec<ScoredEntry> {
    ids.iter()
        .map(|&id| ScoredEntry {
            id,
            title: format!("Entry {id}"),
            category: String::new(),  // NEW — must be present or struct literal fails to compile
            final_score: 0.9,
            similarity: 0.85,
            confidence: 0.7,
            status: "Active".to_string(),
            nli_rerank_delta: None,
        })
        .collect()
}
```

Compile-time assertion: if `category` is missing from `ScoredEntry`, all tests that
construct a `ScoredEntry` literal fail to compile. This is the compile-time half of
the dual-copy guard.

---

## JSON Schema Tests (in report/tests.rs, deserializing runner output)

### `test_scored_entry_category_serializes`

```
Arrange: entry = ScoredEntry { category: "lesson-learned", ... }
Act:     json = serde_json::to_string(&entry)
Assert:  json.contains("\"category\"")
         json.contains("\"lesson-learned\"")
```

### `test_profile_result_cc_at_k_icd_serialize`

```
Arrange: result = ProfileResult { cc_at_k: 0.75, icd: 1.1, ... }
Act:     json = serde_json::to_string(&result)
Assert:  json.contains("\"cc_at_k\"")
         json.contains("\"icd\"")
         json.contains("0.75")
         json.contains("1.1")
```

### `test_comparison_metrics_delta_fields_serialize`

```
Arrange: cm = ComparisonMetrics { cc_at_k_delta: 0.15, icd_delta: -0.05, ... }
Act:     json = serde_json::to_string(&cm)
Assert:  json.contains("\"cc_at_k_delta\"")
         json.contains("\"icd_delta\"")
```

These three tests may live in `tests_metrics.rs` or in a small inline `#[cfg(test)]`
block within `runner/output.rs`. They serve as serialization smoke tests.

---

## NFR Checks (code review)

- `ScoredEntry`, `ProfileResult`, `ComparisonMetrics` are all `#[derive(Serialize, Deserialize)]`
- New fields do NOT carry `#[serde(default)]` on the runner side (serde(default) is only on the
  report/mod.rs deserialization copy — the runner side is authoritative)
- `category: String` is a value field (not `Option<String>`) on the runner side;
  the field is always populated from `se.entry.category` in `replay.rs`
- No async or tokio imports in this file
