# nan-008 Pseudocode: runner/output.rs

## Purpose

Canonical type definitions for all JSON result structures produced by `eval run`.
This file is the source of truth for the on-disk JSON schema. `report/mod.rs`
maintains an independent deserialization copy that must mirror these types exactly.

## Existing Types (context — do not remove)

```
ScoredEntry { id: u64, title: String, final_score: f64, similarity: f64,
              confidence: f64, status: String, nli_rerank_delta: Option<f64> }
ProfileResult { entries: Vec<ScoredEntry>, latency_ms: u64, p_at_k: f64, mrr: f64 }
RankChange { entry_id: u64, from_rank: usize, to_rank: usize }
ComparisonMetrics { kendall_tau: f64, rank_changes: Vec<RankChange>,
                    mrr_delta: f64, p_at_k_delta: f64, latency_overhead_ms: i64 }
ScenarioResult { scenario_id: String, query: String,
                 profiles: HashMap<String, ProfileResult>, comparison: ComparisonMetrics }
```

## New/Modified Types

### ScoredEntry — add `category` field

```
struct ScoredEntry {
    pub id: u64,
    pub title: String,
    pub category: String,          // NEW — populated from se.entry.category in replay.rs
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
    pub status: String,
    pub nli_rerank_delta: Option<f64>,
}
derive: Debug, Clone, Serialize, Deserialize
```

Field insertion position: after `title`, before `final_score`. This matches the
logical grouping (identity fields before metric fields) and must be mirrored
identically in `report/mod.rs`.

### ProfileResult — add `cc_at_k` and `icd` fields

```
struct ProfileResult {
    pub entries: Vec<ScoredEntry>,
    pub latency_ms: u64,
    pub p_at_k: f64,
    pub mrr: f64,
    pub cc_at_k: f64,              // NEW — computed by compute_cc_at_k in replay.rs
    pub icd: f64,                  // NEW — computed by compute_icd in replay.rs
}
derive: Debug, Clone, Serialize, Deserialize
```

Field insertion position: after `mrr`, appended. Both fields are `f64` with range
`[0.0, 1.0]` for `cc_at_k` and `[0.0, ln(n)]` for `icd`. No `Option` wrapping —
they are always computed and always present in new output files.

### ComparisonMetrics — add `cc_at_k_delta` and `icd_delta` fields

```
struct ComparisonMetrics {
    pub kendall_tau: f64,
    pub rank_changes: Vec<RankChange>,
    pub mrr_delta: f64,
    pub p_at_k_delta: f64,
    pub latency_overhead_ms: i64,
    pub cc_at_k_delta: f64,        // NEW — candidate.cc_at_k - baseline.cc_at_k
    pub icd_delta: f64,            // NEW — candidate.icd - baseline.icd
}
derive: Debug, Clone, Serialize, Deserialize
```

Field insertion position: after `latency_overhead_ms`, appended. Sign convention:
positive delta means candidate improved relative to baseline. This matches the
existing convention for `mrr_delta` and `p_at_k_delta`.

## Functions — unchanged

`write_scenario_result` is unchanged. It serializes `ScenarioResult` which contains
`ProfileResult` and `ComparisonMetrics`, so the new fields are automatically included
in the JSON output without any change to the write function itself.

## Error Handling

No new error paths. The new fields are plain `f64` values — no fallible operations.
`serde_json::to_string_pretty` serialization of `f64` values is well-defined for
all finite floats. `compute_icd` is guaranteed to never produce NaN (see
runner/metrics.rs pseudocode). Serialization of `f64::INFINITY` or `f64::NEG_INFINITY`
would produce the JSON token `null` in serde_json; the NaN guard in `compute_icd`
prevents this from occurring.

## Integration Points

- `runner/replay.rs` constructs `ScoredEntry` (with `category`), `ProfileResult` (with
  `cc_at_k`, `icd`), and `ComparisonMetrics` (with `cc_at_k_delta`, `icd_delta`).
- `runner/metrics.rs` imports `ScoredEntry` from `super::output` for its function
  signatures. The `category` field is read there.
- `report/mod.rs` must have an independent copy of these three types with the same
  field names and types. A divergence is a silent bug detectable only by the
  round-trip integration test (ADR-003).

## Key Test Scenarios

1. Serialize a `ScenarioResult` containing a `ProfileResult` with `cc_at_k: 0.857`
   and `icd: 1.234`; deserialize back; assert all fields round-trip with exact equality.

2. Serialize a `ComparisonMetrics` with `cc_at_k_delta: 0.143` and `icd_delta: 0.211`;
   assert the JSON string contains the keys `"cc_at_k_delta"` and `"icd_delta"`.

3. Serialize a `ScoredEntry` with `category: "decision"`; assert the JSON string
   contains `"category": "decision"`.

4. Confirm that a `ScenarioResult` JSON produced before nan-008 (lacking `category`,
   `cc_at_k`, `icd`, `cc_at_k_delta`, `icd_delta`) deserializes without error in
   `report/mod.rs` via `serde(default)`. (This test lives in report/tests.rs but
   validates the schema contract established here.)

## Dual-Copy Atomicity Checklist (ADR-003, NFR-08)

When this file is modified, the following must also be modified in the same commit:

- [ ] `report/mod.rs ScoredEntry` — add `category: String` with `#[serde(default)]`
- [ ] `report/mod.rs ProfileResult` — add `cc_at_k: f64`, `icd: f64` with `#[serde(default)]`
- [ ] `report/mod.rs ComparisonMetrics` — add `cc_at_k_delta: f64`, `icd_delta: f64` with `#[serde(default)]`
