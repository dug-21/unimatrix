# crt-048 Pseudocode Overview — Drop Freshness from Lambda

## Feature Summary

Remove the `confidence_freshness` dimension from Lambda (the composite coherence health
metric in `context_status`). Lambda becomes a 3-dimension structural integrity metric:
graph quality (0.46), contradiction density (0.31), embedding consistency (0.23).

All changes are confined to `crates/unimatrix-server/src/`. No schema migration. No new
files. No Cargo.toml changes.

---

## Components Affected

| Component | File | Change Nature |
|-----------|------|--------------|
| A — Coherence computation | `infra/coherence.rs` | Delete 2 functions, update struct+const+2 functions, delete ~11 tests, update ~11 tests |
| B — Status orchestration | `services/status.rs` | Remove call sites at ~line 695-701, ~766-770, update both `compute_lambda()` calls (~771, ~798-804), update `generate_recommendations()` call (~811-818) |
| C — Status report types | `mcp/response/status.rs` | Remove 2 struct fields from `StatusReport` and `StatusReportJson`, update `Default`, `From`, and 3 format branches |
| D — Test fixtures | `mcp/response/mod.rs` | Remove 16 field references across 8 fixture sites, delete 4 tests, fix 3 additional lines |

---

## Data Flow After crt-048

```
services/status.rs Phase 5
  │
  ├── load_active_entries_with_tags()     [RETAINED — serves coherence_by_source]
  │
  ├── coherence::graph_quality_score()    [unchanged]
  ├── coherence::embedding_consistency_score()  [unchanged]
  ├── coherence::contradiction_density_score()  [unchanged]
  │
  ├── [DELETED] coherence::confidence_freshness_score()
  ├── [DELETED] coherence::oldest_stale_age()
  │
  ├── coherence::compute_lambda(graph, embed_dim, contradiction, &DEFAULT_WEIGHTS)
  │     ^ 4 params — was 5 (freshness f64 first param removed)
  │
  ├── coherence_by_source loop:
  │     [DELETED] confidence_freshness_score() per source
  │     coherence::compute_lambda(graph, embed_dim, contradiction, &DEFAULT_WEIGHTS)
  │       ^ identical 4-param call — no per-source freshness
  │
  ├── coherence::generate_recommendations(lambda, threshold, graph_stale_ratio,
  │     embedding_inconsistent_count, total_quarantined)
  │     ^ 5 params — was 7 (stale_confidence_count, oldest_stale_age_secs removed)
  │
  └── writes into StatusReport (2 fields removed)
        │
        └── mcp/response/status.rs
              ├── format_status_report() Summary branch  [freshness line + stale block removed]
              ├── format_status_report() Markdown branch [Confidence Freshness bullet removed]
              └── format_status_report() Json branch     [automatic — struct fields gone]
```

---

## Shared Types Modified

### CoherenceWeights (infra/coherence.rs)

Before (4 fields):
```
confidence_freshness: f64   // 0.35 — DELETED
graph_quality: f64          // 0.30 → 0.46
embedding_consistency: f64  // 0.15 → 0.23
contradiction_density: f64  // 0.20 → 0.31
```

After (3 fields):
```
graph_quality: f64          // 0.46
embedding_consistency: f64  // 0.23
contradiction_density: f64  // 0.31
```

Invariant: `graph_quality + contradiction_density + embedding_consistency == 1.0` within
f64 epsilon. Test: `lambda_weight_sum_invariant` uses `(sum - 1.0_f64).abs() < f64::EPSILON`.

### StatusReport (mcp/response/status.rs)

Removed fields:
- `confidence_freshness_score: f64`  (was default 1.0)
- `stale_confidence_count: u64`      (was default 0)

Same removals apply to `StatusReportJson`.

---

## Sequencing Constraints

1. **Component A first**: `infra/coherence.rs` defines the updated signatures.
   Components B and C are both downstream callers. B must not be touched until A's
   new signatures are known.

2. **Component C before D**: The `StatusReport` struct fields are removed in C
   (`mcp/response/status.rs`). Component D (`mcp/response/mod.rs`) constructs
   `StatusReport` literals — it will not compile until C removes the fields AND D
   removes all 16 field references. In practice, both C and D must be updated
   together before any build attempt.

3. **Build only after all 4 components are updated**: A partial update causes a
   compile error. The delivery pre-flight checklist in IMPLEMENTATION-BRIEF.md
   defines the exact grep checks to run before `cargo build --workspace`.

---

## Deleted Code Inventory (quick reference)

### Functions deleted (infra/coherence.rs)
- `confidence_freshness_score(entries, now, staleness_threshold_secs) -> (f64, u64)`
- `oldest_stale_age(entries, now, staleness_threshold_secs) -> u64`

### Tests deleted (infra/coherence.rs) — ~11 tests
`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`, `recommendations_below_threshold_stale_confidence`

### Tests deleted (mcp/response/mod.rs) — 4 tests
`test_coherence_json_all_fields`, `test_coherence_json_f64_precision`,
`test_coherence_stale_count_rendering`, `test_coherence_default_values`

### Fixture field references removed (mcp/response/mod.rs) — 8 sites, 16 refs
See Component D pseudocode for exact line-by-line specification.

---

## Critical Risk Reminders (from RISK-TEST-STRATEGY.md)

- **R-01/R-06**: Both `compute_lambda()` call sites in `services/status.rs` must be
  updated identically. Argument transposition compiles silently.
- **R-02**: `make_coherence_status_report()` at ~line 1434 sets non-default values
  (0.8200 / 15) — not found by searching for default values 1.0 / 0.
- **R-03**: `DEFAULT_STALENESS_THRESHOLD_SECS` must NOT be removed. It survives for
  `run_maintenance()` in `services/status.rs` ~line 1242.
- **R-04**: `lambda_weight_sum_invariant` must use epsilon comparison, not exact `==`.
