# Architecture: crt-048 — Drop Freshness from Lambda

## System Overview

Lambda is the composite coherence health metric introduced in crt-005. It gates maintenance recommendations in `context_status` and is reported as a top-level health indicator across all three output formats (text, markdown, JSON). crt-048 reduces Lambda from a 4-dimension to a 3-dimension structural integrity metric by removing the `confidence_freshness` dimension entirely.

This change touches three components in a single crate (`unimatrix-server`):

1. `infra/coherence.rs` — the computation layer (pure functions, no I/O)
2. `services/status.rs` — the orchestration layer (calls coherence functions, assembles `StatusReport`)
3. `mcp/response/status.rs` + `mcp/response/mod.rs` — the presentation layer (structs, serialization, tests)

No schema migrations, no new tables, no config changes. No other crates are affected.

## Component Breakdown

### Component A: `infra/coherence.rs` — Coherence Computation

Owns all pure Lambda math. Contains:
- `CoherenceWeights` struct — field set changes from 4 to 3
- `DEFAULT_WEIGHTS` constant — values change to 3-dimension re-normalized weights
- `compute_lambda()` — signature loses the `freshness: f64` parameter
- `confidence_freshness_score()` — deleted entirely
- `oldest_stale_age()` — deleted entirely
- `generate_recommendations()` — signature shrinks (two parameters removed)
- `DEFAULT_STALENESS_THRESHOLD_SECS` — **retained** (surviving caller in `run_maintenance()`)

**Responsibility after crt-048**: compute structural Lambda from graph quality, contradiction density, and embedding consistency only.

### Component B: `services/status.rs` — Status Orchestration

Calls Component A functions and writes results into `StatusReport`. Contains two call sites that must be updated:

**Phase 5 main path** (lines 695–818):
- Line 695–701: `confidence_freshness_score()` call + two field assignments — deleted
- Line 766–770: `oldest_stale_age()` call — deleted
- Line 771–777: `compute_lambda()` call — `freshness` argument removed
- Lines 793–804: `coherence_by_source` loop calls `confidence_freshness_score()` per source, then passes `source_freshness` to `compute_lambda()` — both calls removed; loop simplified

**`generate_recommendations()` call** (line 811–818): two arguments (`stale_confidence_count`, `oldest_stale`) removed from call site.

**`run_maintenance()`** (line 1242): uses `DEFAULT_STALENESS_THRESHOLD_SECS` for confidence refresh targeting. This call site is unrelated to Lambda and survives unchanged. The constant must not be removed.

**`active_entries` allocation**: retained — still needed by `coherence_by_source` grouping.

### Component C: `mcp/response/status.rs` — Status Report Types and Formatting

Contains `StatusReport` struct, `StatusReportJson` struct, `Default` impl, `From<&StatusReport>` impl, and three format branches.

Fields removed from `StatusReport`:
- `confidence_freshness_score: f64`
- `stale_confidence_count: u64`

Fields removed from `StatusReportJson`:
- `confidence_freshness_score: f64`
- `stale_confidence_count: u64`

Format branch changes:
- **Summary**: remove `confidence_freshness: {:.4}` from the coherence line; remove the `stale_confidence_count > 0` conditional block
- **Markdown**: remove `- **Confidence Freshness**: {:.4}` line; remove `Stale confidence entries: {}` line
- **JSON**: fields absent from `StatusReportJson` are automatically absent from output — no additional change beyond struct field removal

`From<&StatusReport>` impl: remove the `confidence_freshness_score` and `stale_confidence_count` field assignments.

`Default` impl in `status.rs`: remove `confidence_freshness_score: 1.0` and `stale_confidence_count: 0` from the literal block.

### Component D: `mcp/response/mod.rs` — Test Fixtures

Contains test helper functions and integration tests that construct `StatusReport` literals. These all fail to compile when struct fields are removed.

**Exact fixture sites with `confidence_freshness_score` and `stale_confidence_count` (enumerated per SR-06 requirement):**

| Line (confidence_freshness_score) | Line (stale_confidence_count) | Location |
|-----------------------------------|-------------------------------|----------|
| 614 | 618 | `make_status_report()` helper function |
| 710 | 714 | inline fixture in one test |
| 973 | 977 | inline fixture in one test |
| 1054 | 1058 | inline fixture in one test |
| 1137 | 1141 | inline fixture in one test |
| 1212 | 1216 | inline fixture in one test |
| 1291 | 1295 | inline fixture in one test |
| 1434 | 1438 | `make_coherence_status_report()` helper (non-default values: 0.8200 / 15) |

Total: **8 fixture sites, 16 field references** to remove.

Additionally, lines 1731 (`report2.stale_confidence_count = 0`) and 1794/1798 (default assertions) must be removed or rewritten as part of affected tests.

**Tests deleted entirely** (reference removed fields or removed functions):
- `test_coherence_json_all_fields` — asserts `confidence_freshness_score` and `stale_confidence_count` present in JSON (lines 1474–1533)
- `test_coherence_json_f64_precision` — references `confidence_freshness_score` value
- `test_coherence_stale_count_rendering` — renders stale_confidence_count in summary/markdown
- `test_coherence_default_values` — asserts `confidence_freshness_score == 1.0` and `stale_confidence_count == 0`

## Component Interactions

```
services/status.rs (Phase 5)
    │
    ├── calls coherence::graph_quality_score()         [unchanged]
    ├── calls coherence::embedding_consistency_score() [unchanged]
    ├── calls coherence::contradiction_density_score() [unchanged]
    ├── calls coherence::compute_lambda(graph, embed, contradiction, weights)  [freshness param REMOVED]
    ├── calls coherence::generate_recommendations(lambda, threshold, graph_stale_ratio, embed_inconsistencies, quarantined)  [2 params REMOVED]
    │   └── [DELETED] coherence::confidence_freshness_score()
    │   └── [DELETED] coherence::oldest_stale_age()
    │
    └── writes into StatusReport
            │
            └── mcp/response/status.rs
                    ├── format_status_report() → Summary branch
                    ├── format_status_report() → Markdown branch
                    └── format_status_report() → Json branch (via StatusReportJson)
```

## Technology Decisions

See ADR-001 (weights) and ADR-002 (constant retention). Key choices:

- **3-dimension Lambda** with weights graph=0.46, contradiction=0.31, embedding=0.23 (sum=1.0). Derives from proportional re-normalization of the original 0.30:0.20:0.15 ratio.
- **`DEFAULT_STALENESS_THRESHOLD_SECS` retained** despite freshness removal. The constant serves `run_maintenance()` confidence refresh — a different subsystem with the same threshold value.
- **Clean removal, no deprecation window**. Zero live callers outside Rust test code (verified by OQ-2 grep of `product/test/`).
- **No `compute_lambda()` argument struct refactor** (SR-02 concern). The function shrinks from 5 positional parameters to 4. With freshness gone, the remaining parameters are structurally distinct types: two `f64`, one `Option<f64>`, one `&CoherenceWeights`. Mis-ordering risk is low; a named-arg struct adds indirection without meaningful safety gain for a 4-parameter pure function.

## Integration Points

- `infra/coherence.rs` is consumed only by `services/status.rs`. No other crate imports it.
- `mcp/response/status.rs` is consumed by `services/status.rs` (produces `StatusReport`) and `mcp/response/mod.rs` (tests).
- `StatusReport` JSON output is a breaking change for any external caller using the `confidence_freshness_score` or `stale_confidence_count` fields. Per OQ-2, no live callers exist in the test suite. Release notes must document field removal.
- `DEFAULT_STALENESS_THRESHOLD_SECS` cross-reference: `services/status.rs:1242` (run_maintenance) holds the surviving call site. Its removal would silently compile with a hardcoded literal — SR-03 High risk.

## Integration Surface

| Integration Point | Type / Signature | Change |
|-------------------|-----------------|--------|
| `CoherenceWeights` | `struct { graph_quality: f64, embedding_consistency: f64, contradiction_density: f64 }` | Remove `confidence_freshness` field |
| `DEFAULT_WEIGHTS` | `CoherenceWeights { graph_quality: 0.46, embedding_consistency: 0.23, contradiction_density: 0.31 }` | Re-normalized 3-dimension values |
| `compute_lambda` | `fn(graph_quality: f64, embedding_consistency: Option<f64>, contradiction_density: f64, weights: &CoherenceWeights) -> f64` | Remove `freshness: f64` first parameter |
| `generate_recommendations` | `fn(lambda: f64, threshold: f64, graph_stale_ratio: f64, embedding_inconsistent_count: usize, total_quarantined: u64) -> Vec<String>` | Remove `stale_confidence_count: u64` and `oldest_stale_age_secs: u64` parameters |
| `DEFAULT_STALENESS_THRESHOLD_SECS` | `pub const u64 = 86400` | Retained; comment added |
| `StatusReport.confidence_freshness_score` | `f64` | Deleted |
| `StatusReport.stale_confidence_count` | `u64` | Deleted |
| `StatusReportJson.confidence_freshness_score` | `f64` | Deleted |
| `StatusReportJson.stale_confidence_count` | `u64` | Deleted |
| JSON output | `context_status` response | `confidence_freshness_score` and `stale_confidence_count` keys no longer present |

## Deleted Code Inventory

**`infra/coherence.rs` — functions deleted:**
- `confidence_freshness_score(entries: &[EntryRecord], now: u64, staleness_threshold_secs: u64) -> (f64, u64)`
- `oldest_stale_age(entries: &[EntryRecord], now: u64, staleness_threshold_secs: u64) -> u64`

**`infra/coherence.rs` — tests deleted (~11 tests):**
- `freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`, `freshness_uses_max_of_timestamps`
- `freshness_recently_accessed_not_stale`, `freshness_both_timestamps_older_than_threshold`
- `oldest_stale_no_stale`, `oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`
- `staleness_threshold_constant_value`
- `recommendations_below_threshold_stale_confidence`

**`infra/coherence.rs` — tests updated (value changes only):**
- `lambda_all_ones` — remove `1.0` freshness arg
- `lambda_all_zeros` — remove `0.0` freshness arg
- `lambda_weighted_sum` — remove `0.5` freshness arg; update expected comment
- `lambda_renormalization_without_embedding` — remove freshness arg; update expected comment
- `lambda_renormalization_partial` — remove freshness arg; update expected comment
- `lambda_weight_sum_invariant` — remove `confidence_freshness` from sum; use epsilon comparison
- `lambda_renormalized_weights_sum_to_one` — remove `confidence_freshness` ref; verify 2-of-3 re-normalization
- `lambda_specific_four_dimensions` — rename to `lambda_specific_three_dimensions`; remove freshness arg; update expected value
- `lambda_embedding_excluded_specific` — remove freshness arg; update expected value
- `lambda_single_dimension_deviation` — remove freshness arg; update expected value
- `lambda_custom_weights_zero_embedding` — remove `confidence_freshness` from struct literal

**`mcp/response/mod.rs` — tests deleted:**
- `test_coherence_json_all_fields` (asserts removed fields exist in JSON)
- `test_coherence_json_f64_precision` (references `confidence_freshness_score`)
- `test_coherence_stale_count_rendering` (references `stale_confidence_count`)
- `test_coherence_default_values` (asserts removed fields have default values)

## Open Questions

None. All decisions resolved by owner review 2026-04-06 (GH #520 comment).

The one former open question about `DEFAULT_STALENESS_THRESHOLD_SECS` is now encoded as a hard constraint in ADR-002.
