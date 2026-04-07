# Agent Report: crt-048-agent-1-architect

## Deliverables

- `product/features/crt-048/architecture/ARCHITECTURE.md`
- `product/features/crt-048/architecture/ADR-001-three-dimension-lambda-weights.md`
- `product/features/crt-048/architecture/ADR-002-staleness-constant-retention.md`

## Unimatrix Entry IDs

Agent lacks Write capability. Design Leader must store ADRs using privileged identity.
See "Unimatrix Storage Instructions" section below for exact parameters.

## Key Decisions

### ADR-001: Three-Dimension Lambda Weights
- Locked weights: graph_quality=0.46, contradiction_density=0.31, embedding_consistency=0.23 (sum=1.00 exact)
- Derivation: proportional re-normalization of 0.30:0.20:0.15 by factor 1/0.65, preserving 2:1.33:1 structural ratio
- `compute_lambda()` loses its `freshness: f64` first parameter
- `CoherenceWeights` loses `confidence_freshness` field
- `confidence_freshness_score()` and `oldest_stale_age()` deleted from infra/coherence.rs

### ADR-002: Retain DEFAULT_STALENESS_THRESHOLD_SECS
- The constant survives despite freshness removal
- Surviving caller: `services/status.rs:1242` inside `run_maintenance()` (confidence refresh targeting)
- Doc comment must be updated to make the surviving use explicit
- SR-03 (High risk) is resolved by this ADR being explicit — implementer must not follow Goal 7 literally

## SR-06 Resolution: Exact mod.rs Fixture Sites

Enumerated per SCOPE-RISK-ASSESSMENT.md SR-06 requirement. All sites in
`crates/unimatrix-server/src/mcp/response/mod.rs`:

| confidence_freshness_score line | stale_confidence_count line | Context |
|---------------------------------|-----------------------------|---------|
| 614 | 618 | `make_status_report()` helper |
| 710 | 714 | inline fixture (test_status_report_with_contradictions_markdown) |
| 973 | 977 | inline fixture (test_status_report_with_contradictions_markdown second fixture) |
| 1054 | 1058 | inline fixture (test_status_report_with_contradictions_json) |
| 1137 | 1141 | inline fixture (test_status_report_with_contradictions_json second fixture) |
| 1212 | 1216 | inline fixture (test_status_report_embedding_integrity_markdown) |
| 1291 | 1295 | `make_status_report_with_co_access()` helper |
| 1434 | 1438 | `make_coherence_status_report()` helper (non-default values: 0.8200 / 15) |

Plus: line 1731 (`report2.stale_confidence_count = 0`), lines 1794/1798 (default assertions in `test_coherence_default_values`).

Total: 8 struct literal sites, 16 field references to remove. Plus ~4 tests to delete.

## Tests Deleted (infra/coherence.rs, ~11 tests)

- `freshness_empty_entries`
- `freshness_all_stale`
- `freshness_none_stale`
- `freshness_uses_max_of_timestamps`
- `freshness_recently_accessed_not_stale`
- `freshness_both_timestamps_older_than_threshold`
- `oldest_stale_no_stale`
- `oldest_stale_one_stale`
- `oldest_stale_both_timestamps_zero`
- `staleness_threshold_constant_value`
- `recommendations_below_threshold_stale_confidence`

## Tests Deleted (mcp/response/mod.rs, ~4 tests)

- `test_coherence_json_all_fields` — asserts `confidence_freshness_score` and `stale_confidence_count` present in JSON
- `test_coherence_json_f64_precision` — references `confidence_freshness_score` value
- `test_coherence_stale_count_rendering` — renders `stale_confidence_count` in summary/markdown
- `test_coherence_default_values` — asserts `confidence_freshness_score == 1.0` and `stale_confidence_count == 0`

## Tests Updated (infra/coherence.rs, value changes only)

All lambda tests: remove `freshness: f64` positional argument. Rename `lambda_specific_four_dimensions` → `lambda_specific_three_dimensions`. Update expected values and comments for 3-dimension weights. Update `lambda_weight_sum_invariant` to sum 3 fields. Update `lambda_custom_weights_zero_embedding` struct literal to omit `confidence_freshness`.

## Unimatrix Storage Instructions (for Design Leader)

### ADR-001 — supersedes entry #179 (ADR-003 crt-005)

Use `context_correct`:
```
context_correct({
  original_id: 179,
  title: "ADR-001 (crt-048): Three-Dimension Lambda Weights",
  topic: "crt-048",
  category: "decision",
  tags: ["adr", "crt-048", "crt-005", "lambda", "coherence", "freshness"],
  reason: "crt-048 removes confidence_freshness dimension from Lambda. crt-036 invalidated the 24h staleness assumption. Lambda becomes 3-dimension structural metric with re-normalized weights graph_quality=0.46, contradiction_density=0.31, embedding_consistency=0.23.",
  content: <full content from ADR-001-three-dimension-lambda-weights.md>
})
```

### ADR-002 — new entry

Use `context_store`:
```
context_store({
  title: "ADR-002 (crt-048): Retain DEFAULT_STALENESS_THRESHOLD_SECS after freshness removal",
  topic: "crt-048",
  category: "decision",
  feature_cycle: "crt-048",
  tags: ["adr", "crt-048", "lambda", "coherence", "run_maintenance", "staleness"],
  content: <full content from ADR-002-staleness-constant-retention.md>
})
```

## Open Questions

None. All decisions resolved by owner review 2026-04-06 (GH #520).
