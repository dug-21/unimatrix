# Test Plan: Scenario Extraction (`eval/scenarios/`)

Component files: `types.rs`, `output.rs`, `extract.rs`, `tests.rs`

---

## Risk Coverage

| Risk | Tests in this component |
|------|------------------------|
| R-04 (Critical) | `test_scenarios_extract_phase_non_null`, `test_scenarios_extract_phase_null` — require updated helper |
| R-05 (High) | `test_scenario_context_phase_null_absent_from_jsonl`, `test_scenario_context_phase_non_null_present` |
| IR-01 (High) | `test_scenarios_extract_phase_non_null` exercises full SQL → struct path |

---

## Required Helper Update

### `insert_query_log_row` — extend signature (MANDATORY, lesson #3543)

The existing helper in `eval/scenarios/tests.rs` has this signature:

```rust
async fn insert_query_log_row(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    query_text: &str,
    retrieval_mode: &str,
    source: &str,
    entry_ids_json: Option<&str>,
    scores_json: Option<&str>,
)
```

It must be extended to:

```rust
async fn insert_query_log_row(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    query_text: &str,
    retrieval_mode: &str,
    source: &str,
    entry_ids_json: Option<&str>,
    scores_json: Option<&str>,
    phase: Option<&str>,       // new parameter — col-028 precedent
)
```

The `.bind(Option::<String>::None)` at position 9 must become
`.bind(phase.map(|s| s.to_string()))`.

All existing call sites in `tests.rs` must append `None` as the new last argument.
Failure to do this will cause a compile error — the compiler enforces the update of
all call sites. The `// col-028: phase=NULL for test helper rows (IR-03)` comment
must be removed; the parameter makes intent explicit.

---

## Unit Tests

### `test_scenario_context_phase_non_null_present` (AC-09 item 1)

**Location**: `eval/scenarios/tests.rs` (sync `#[test]`)

**Arrange**: Construct `ScenarioContext { phase: Some("delivery".to_string()), ... }`.

**Act**: Serialize to JSON with `serde_json::to_string`.

**Assert**:
- The JSON string contains the key `"phase"`.
- `json.contains("\"phase\":\"delivery\"")` is true.

**Rationale**: Confirms `types.rs` field presence and correct serde attribute
(`skip_serializing_if` must not suppress a Some value).

---

### `test_scenario_context_phase_null_absent_from_jsonl` (AC-02, AC-09 item 2, R-05)

**Location**: `eval/scenarios/tests.rs` (sync `#[test]`)

**Arrange**: Construct `ScenarioContext { phase: None, ... }`.

**Act**: Serialize to JSON with `serde_json::to_string`.

**Assert**:
- `!json.contains("\"phase\"")` — the key is completely absent.
- `!json.contains("null")` (no `"phase":null` emitted).

**Rationale**: Verifies `#[serde(skip_serializing_if = "Option::is_none")]` is correctly
placed on `types.rs`. A missing annotation or a wrong annotation on the runner copy
would cause this test to fail or (worse) pass the wrong copy.

---

## Integration Tests

### `test_scenarios_extract_phase_non_null` (AC-01, AC-10, R-04, IR-01)

**Location**: `eval/scenarios/tests.rs` (async `#[tokio::test(flavor = "multi_thread")]`)

**Arrange**:
- `make_snapshot_db()`.
- `open_write_pool(&db_path)`.
- Call `insert_query_log_row(..., phase: Some("delivery"))`.

**Act**: Call `run_scenarios(&db_path, ScenarioSource::All, None, &out)`.

**Assert**:
- `run_scenarios` returns `Ok(())`.
- JSONL output contains exactly one line.
- Parsed line contains `context.phase == "delivery"`:
  `assert_eq!(lines[0]["context"]["phase"].as_str(), Some("delivery"))`.

**Rationale**: Exercises the full SQL → `row.try_get` → `ScenarioContext.phase` →
JSONL serialization path. If the SELECT clause is missing `phase`, the `try_get` call
returns a runtime error (IR-01). If `build_scenario_record` omits the assignment,
`context.phase` remains `None` and this test fails.

---

### `test_scenarios_extract_phase_null` (AC-02, AC-10, R-04)

**Location**: `eval/scenarios/tests.rs` (async `#[tokio::test(flavor = "multi_thread")]`)

**Arrange**:
- `make_snapshot_db()`.
- `open_write_pool(&db_path)`.
- Call `insert_query_log_row(..., phase: None)`.

**Act**: Call `run_scenarios(&db_path, ScenarioSource::All, None, &out)`.

**Assert**:
- JSONL output contains exactly one line.
- Parsed line does NOT contain the `"phase"` key:
  `assert!(lines[0]["context"].get("phase").is_none(),
   "phase key must be absent for null phase")`
  or equivalently `assert!(lines[0]["context"]["phase"].is_null())` is insufficient —
  must use `.get("phase").is_none()` to confirm key absence, not null presence.

**Rationale**: Confirms `skip_serializing_if` on `ScenarioContext.phase` suppresses
null phase from JSONL output. The helper update with `None` confirms the DB inserts
`NULL` correctly.

---

## Existing Tests — Compatibility Verification

All existing `tests.rs` integration tests call `insert_query_log_row`. After adding
the `phase` parameter, every call site must pass `None` as the new last argument.
The delivery agent must update all call sites; the compiler enforces this.

After the update, all existing tests must continue to pass without modification to
their assertions (backward-compat: adding `None` is transparent to the rest of the
test).

---

## Edge Cases

**EC-05 (mixed pre/post col-028 files)**: Not directly testable in scenario extraction
— it is a deserialization concern for `eval report`. The extraction unit tests cover
the `skip_serializing_if` annotation that makes backward compat possible.

**EC-03/EC-04 (special chars, long strings)**: Not in scope for this component.
Phase is read verbatim from `query_log.phase`. No sanitization tests required.
