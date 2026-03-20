# Agent Report: nan-007-agent-5-eval-scenarios

**Component**: `eval/scenarios.rs` (D2)
**Feature**: nan-007 — W1-3 Evaluation Harness
**Agent ID**: nan-007-agent-5-eval-scenarios

---

## Summary

Implemented `crates/unimatrix-server/src/eval/scenarios.rs` per the validated pseudocode and test plan. Also updated `eval/mod.rs` to enable the module and wire `ScenarioSource` into the clap `EvalCommand::Scenarios` variant, replacing the placeholder `String` field.

---

## Files Modified

- `crates/unimatrix-server/src/eval/scenarios.rs` — created (847 lines)
- `crates/unimatrix-server/src/eval/mod.rs` — updated: `pub mod scenarios`, re-exports, `ScenarioSource` in `Scenarios` variant
- `crates/unimatrix-server/Cargo.toml` — added `features = ["test-support"]` to `unimatrix-engine` (ADR-003)

---

## Implementation Notes

### Schema deviation from pseudocode

The pseudocode (`eval-scenarios.md`) assumed `query_log` had `agent_id` and `feature_cycle` columns. The actual schema (verified in `migration.rs`) has neither. The implementation:

- Populates `ScenarioContext.agent_id` from `session_id` (closest available identifier)
- Defaults `ScenarioContext.feature_cycle` to `""` (empty string)
- The `scores` stored in `query_log.similarity_scores` are serialized as `f64` arrays (the store uses `Vec<f64>` internally), but the `ScenarioBaseline.scores` field is typed `Vec<f32>` per spec. Serde deserializes the f64 JSON values to f32 without precision loss at the magnitude used (0.0–1.0).

### SQL source filter

The `ScenarioSource::to_sql_filter()` method returns only static string literals (`"mcp"`, `"uds"`, or `None`). String interpolation into SQL is safe here because no user input reaches the SQL literal — the value is fully constrained by the enum variant.

### block_export_sync usage

Follows the established pattern from `export.rs`: sync `run_scenarios()` outer function calls `block_export_sync(async { do_scenarios(...).await })`. The private `do_scenarios()` async function owns the pool lifecycle (connect → query → flush → close).

---

## Tests

**15 tests, 15 pass, 0 fail.**

| Test | Coverage |
|------|----------|
| `test_scenario_source_to_sql_filter_mcp` | ScenarioSource::Mcp filter |
| `test_scenario_source_to_sql_filter_uds` | ScenarioSource::Uds filter |
| `test_scenario_source_to_sql_filter_all_is_none` | ScenarioSource::All passthrough |
| `test_run_scenarios_produces_valid_jsonl` | AC-03: all required fields, correct types |
| `test_run_scenarios_length_parity` | R-16: truncate to min length on mismatch |
| `test_run_scenarios_source_filter_mcp` | AC-04: mcp filter |
| `test_run_scenarios_source_filter_uds` | AC-04: uds filter |
| `test_run_scenarios_source_filter_all` | AC-04: all sources returned |
| `test_run_scenarios_empty_query_log` | Edge case: zero rows → empty file, exit 0 |
| `test_run_scenarios_limit_applied` | FR-08: --limit N produces at most N lines |
| `test_run_scenarios_expected_field_is_null` | FR-09: expected always null |
| `test_run_scenarios_unique_ids` | FR-09: all scenario IDs distinct |
| `test_run_scenarios_does_not_write_to_snapshot` | R-01, R-02: snapshot bytes unchanged |
| `test_run_scenarios_null_entry_ids_produces_null_baseline` | NULL result_entry_ids → baseline: null |
| `test_run_scenarios_unicode_query_text` | UTF-8 round-trip correctness |

Full workspace lib tests: **1588 passed, 0 failed** (net +27 vs pre-implementation baseline of 1561).

Pre-existing doctest failure in `config.rs` (line 21, `~` in path comment) is unrelated to this component and was present before this work.

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "query log scan JSONL output patterns" (category: pattern) — entry #1103 (SQL-to-JSONL serialization pattern) and #320 (intermediate serialization struct) are relevant. Applied: column-by-column `try_get` with explicit Option handling for nullable columns.
- Queried: `/uni-query-patterns` for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 through ADR-005. All applied without deviation.
- Stored: entry #2609 "query_log schema has no agent_id/feature_cycle columns — use session_id as proxy" via `/uni-store-pattern` — novel gotcha: pseudocode assumed columns that don't exist in the actual migration, plus the `Option<String>` nullability trap for `result_entry_ids`/`similarity_scores`.
