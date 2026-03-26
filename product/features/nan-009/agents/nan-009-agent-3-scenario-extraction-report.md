# Agent Report: nan-009-agent-3-scenario-extraction

**Component**: Scenario Extraction (`eval/scenarios/`)
**Feature**: nan-009 Phase-Stratified Eval Scenarios
**GH Issue**: #400

---

## Files Modified

- `crates/unimatrix-server/src/eval/scenarios/types.rs`
- `crates/unimatrix-server/src/eval/scenarios/output.rs`
- `crates/unimatrix-server/src/eval/scenarios/extract.rs`
- `crates/unimatrix-server/src/eval/scenarios/tests.rs`

---

## Changes Made

### types.rs
Added `phase: Option<String>` as last field of `ScenarioContext` with both serde attributes per ADR-001:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub phase: Option<String>,
```

### output.rs
Added `phase` to the SQL SELECT list in `do_scenarios` (appended after `similarity_scores` per pseudocode convention — named column access via `try_get` so position is irrelevant to runtime, but append-at-end preserves readability):
```sql
SELECT query_id, session_id, query_text, retrieval_mode, source,
       result_entry_ids, similarity_scores, phase
FROM query_log ...
```

### extract.rs
Added `let phase: Option<String> = row.try_get("phase")?;` after `source` read (before JSON array parsing block). Populated `phase` in `ScenarioContext` struct literal. Updated module docstring to list `phase` in the schema column list.

### tests.rs
- Extended `insert_query_log_row` helper: added `phase: Option<&str>` as 8th parameter; replaced hardcoded `.bind(Option::<String>::None)` with `.bind(phase.map(|s| s.to_string()))`.
- Updated all 14 existing call sites to append `None` as the new last argument.
- Added 4 new tests:
  - `test_scenario_context_phase_non_null_present_in_jsonl` (unit, AC-09 item 1)
  - `test_scenario_context_phase_null_absent_from_jsonl` (unit, AC-02, AC-09 item 2, R-05)
  - `test_scenarios_extract_phase_non_null` (integration, AC-01, AC-10, R-04, IR-01)
  - `test_scenarios_extract_phase_null` (integration, AC-02, AC-10, R-04)

---

## Test Results

```
test result: ok. 19 passed; 0 failed
```

- 15 existing tests: all pass (unchanged assertions)
- 4 new tests: all pass
- Pre-existing workspace failures unrelated to this component: 3 (uds::listener col018 tests — embedding model not initializing in CI, pre-existing)

---

## ADR Compliance

| ADR | Constraint | Status |
|-----|-----------|--------|
| ADR-001 | `skip_serializing_if = "Option::is_none"` on `ScenarioContext.phase` only | COMPLIANT — implemented exactly |
| ADR-001 | No `skip_serializing_if` on runner-side copy | N/A for this component |
| ADR-003 | Phase is free-form `Option<String>`, no enum, no validation | COMPLIANT |

`phase` is NOT forwarded to `ServiceSearchParams` or `AuditContext` (Constraint 3 / measurement purity). This component only reads and stores the value into `ScenarioContext`.

---

## Issues / Blockers

None. All constraints satisfied. The changes were co-committed in the same branch commit as the result-passthrough agent (`26c7843`), which ran concurrently — no conflict, both sets of changes are present in that commit.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` eval harness serde phase extraction — found relevant pattern entries (#3555, #3557) and ADRs (#3562, #3563, #3565). Applied ADR-001 null suppression rule and lesson #3543 (test helper must bind phase column).
- Stored: attempted to store pattern "eval test helper extension: replace hardcoded None bind with real param for compiler-enforced call-site safety" via `/uni-store-pattern` — blocked by MCP Write capability restriction for this agent identity. Pattern to store: when extending positional SQL bind helpers, replace hardcoded `None` binds with a real parameter so the compiler enforces all call-site updates. Recommend coordinator store this under topic `unimatrix-server`, category `pattern`, tags `[eval-harness, test-helper, nan-009, col-028]`.
