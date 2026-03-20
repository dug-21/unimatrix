# Test Plan: `eval/scenarios.rs` (D2)

**Component**: `crates/unimatrix-server/src/eval/scenarios.rs`
**Function under test**: `run_scenarios(db: &Path, source: ScenarioSource, limit: Option<usize>, out: &Path) -> Result<(), Box<dyn Error>>`
**AC coverage**: AC-03 (JSONL format), AC-04 (source filter)
**Risk coverage**: R-02 (read-only enforcement), R-11 (block_export_sync), R-16 (length parity)

---

## Unit Tests

Location: `crates/unimatrix-server/src/eval/scenarios.rs` (inline `#[cfg(test)]`)

### Test: `test_run_scenarios_produces_valid_jsonl`

**Purpose**: AC-03 — each output line is a valid JSON object with all required fields.
**Arrange**: Prepare an in-memory or temp SQLite snapshot with known `query_log` rows (2 rows with `source="mcp"`). Supply `ScenarioSource::All`, no limit.
**Act**: Call `run_scenarios(&snapshot_path, ScenarioSource::All, None, &out_path)`.
**Assert**:
- Returns `Ok(())`.
- `out_path` exists.
- Each line of `out_path` parses as JSON.
- Each parsed object has keys: `id` (string), `query` (string), `context` (object with `agent_id`, `feature_cycle`, `session_id`, `retrieval_mode`), `baseline` (object with `entry_ids` (array of integers), `scores` (array of numbers)), `source` (string, one of `"mcp"` or `"uds"`), `expected` (null).
**Risk**: AC-03, R-16

### Test: `test_run_scenarios_length_parity`

**Purpose**: R-16 — `baseline.entry_ids` and `baseline.scores` have the same length for every scenario line.
**Arrange**: Snapshot with `query_log` rows where the `result_entry_ids` and `similarity_scores` columns have mismatched lengths in one row (controlled corruption).
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::All, None, &out_path)`.
**Assert**:
- Either: the mismatched row is excluded with a warning (and valid rows remain), OR
- Returns `Err(...)` with a structured error identifying the bad row.
- For all output lines: `len(baseline.entry_ids) == len(baseline.scores)`.
**Risk**: R-16

### Test: `test_run_scenarios_source_filter_mcp`

**Purpose**: AC-04 — `ScenarioSource::Mcp` filters to only `source="mcp"` rows.
**Arrange**: Snapshot with 2 `query_log` rows: `source="mcp"` and `source="uds"`.
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::Mcp, None, &out_path)`.
**Assert**: Output JSONL has exactly 1 line. That line has `"source": "mcp"`.
**Risk**: AC-04

### Test: `test_run_scenarios_source_filter_uds`

**Purpose**: AC-04 — `ScenarioSource::Uds` filters to only `source="uds"` rows.
**Arrange**: Same snapshot as above.
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::Uds, None, &out_path)`.
**Assert**: Output has exactly 1 line with `"source": "uds"`.
**Risk**: AC-04

### Test: `test_run_scenarios_source_filter_all`

**Purpose**: AC-04 — `ScenarioSource::All` returns both sources.
**Arrange**: Same snapshot.
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::All, None, &out_path)`.
**Assert**: Output has 2 lines. One with `"mcp"`, one with `"uds"`.
**Risk**: AC-04

### Test: `test_run_scenarios_empty_query_log`

**Purpose**: Edge case — empty `query_log` table produces an empty JSONL file, not an error.
**Arrange**: Snapshot with valid schema but zero `query_log` rows.
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::All, None, &out_path)`.
**Assert**: Returns `Ok(())`. Output file exists. File is empty (0 bytes) or contains zero lines.
**Risk**: Edge case (from RISK-TEST-STRATEGY.md)

### Test: `test_run_scenarios_limit_applied`

**Purpose**: `--limit N` produces at most N output lines.
**Arrange**: Snapshot with 10 `query_log` rows.
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::All, Some(3), &out_path)`.
**Assert**: Output has exactly 3 lines.
**Risk**: FR-08

### Test: `test_run_scenarios_expected_field_is_null`

**Purpose**: All query-log-sourced scenarios have `expected: null` (not absent, not an empty array).
**Arrange**: Standard snapshot with `query_log` rows.
**Act**: `run_scenarios`.
**Assert**: Every output line's `expected` field is JSON null.
**Risk**: FR-09, AC-07 correctness

### Test: `test_run_scenarios_unique_ids`

**Purpose**: Every scenario `id` is unique within the output file.
**Arrange**: Snapshot with 5 `query_log` rows.
**Act**: `run_scenarios`.
**Assert**: All `id` values in the output are distinct strings.
**Risk**: FR-09

### Test: `test_run_scenarios_does_not_write_to_snapshot`

**Purpose**: Read-only enforcement — snapshot file is unchanged after `run_scenarios`.
**Arrange**: Record SHA-256 of snapshot before call.
**Act**: `run_scenarios(&snapshot_path, ScenarioSource::All, None, &out_path)`.
**Assert**: SHA-256 of `snapshot_path` is unchanged.
**Risk**: R-01, R-02

---

## Integration Tests (Python Subprocess)

Location: `product/test/infra-001/tests/test_eval_offline.py`

### Test: `test_eval_scenarios_jsonl_schema`

**Purpose**: AC-03 — subprocess invocation produces valid JSONL.
**Act**: `unimatrix eval scenarios --db <snapshot> --out <out.jsonl>`.
**Assert**:
- Exit code 0.
- Every line of `out.jsonl` parses as JSON with all required fields and correct types.
- `len(line["baseline"]["entry_ids"]) == len(line["baseline"]["scores"])` for every line.

### Test: `test_eval_scenarios_source_filter_mcp`

**Purpose**: AC-04 via subprocess.
**Act**: `unimatrix eval scenarios --db <snapshot> --retrieval-mode mcp --out <out.jsonl>`.
**Assert**: Exit 0. Every output line has `source == "mcp"`.

### Test: `test_eval_scenarios_source_filter_uds`

**Purpose**: AC-04 via subprocess.
**Assert**: Every output line has `source == "uds"`.

### Test: `test_eval_scenarios_source_filter_all`

**Purpose**: AC-04 via subprocess.
**Assert**: Output contains both `"mcp"` and `"uds"` lines (if snapshot contains both).

### Test: `test_eval_scenarios_empty_query_log`

**Purpose**: Empty `query_log` → exit 0, empty output file.
**Act**: Use a snapshot with empty `query_log`.
**Assert**: Exit 0. `out.jsonl` is empty.

### Test: `test_eval_scenarios_invalid_source_rejected`

**Purpose**: Invalid `--retrieval-mode` value is rejected by clap at parse time.
**Act**: `unimatrix eval scenarios --db <snapshot> --retrieval-mode invalid --out <out>`.
**Assert**: Exit code != 0. stderr contains clap help text.

---

## Edge Cases from Risk Strategy

- Mismatched `entry_ids` / `scores` array lengths: must be detected and either rejected or excluded. Silent pass-through into `eval run` causes metric corruption (R-16).
- Unicode query text: multi-byte UTF-8 characters in the `query` field must be encoded correctly in JSON (not ASCII-escaped).
- `--limit 0`: Behavior is not specified but should either produce 0 lines or be rejected. Document the actual behavior in the implementation pseudocode.
- `query_log` rows with NULL `source` field: must be handled gracefully; include with `source="mcp"` as fallback or exclude with a warning.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #157 (Test infrastructure is cumulative), #229 (Tester Role Duties)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 (read-only pool, no SqlxStore::open), ADR-004 (scenarios module in unimatrix-server src/eval/)
Queried: /uni-query-patterns for "snapshot database testing patterns" — found entries #748 (TestHarness Server Integration Pattern), #128 (Risk drives testing), #238 (Testing Infrastructure Convention)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
