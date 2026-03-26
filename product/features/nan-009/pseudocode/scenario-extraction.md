# Component: Scenario Extraction

Files: `eval/scenarios/types.rs`, `eval/scenarios/output.rs`, `eval/scenarios/extract.rs`

## Purpose

Read `query_log.phase` and carry it into JSONL scenario files as `ScenarioContext.phase`.
Three coordinated changes in three files; all changes are additive.

---

## 1. `eval/scenarios/types.rs`

### Change: Add `phase` field to `ScenarioContext`

Current struct (lines 61-71):
```
pub struct ScenarioContext {
    pub agent_id: String,
    pub feature_cycle: String,
    pub session_id: String,
    pub retrieval_mode: String,
}
```

Add `phase` as the last field with both serde attributes:
```
pub struct ScenarioContext {
    pub agent_id: String,
    pub feature_cycle: String,
    pub session_id: String,
    pub retrieval_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}
```

Rationale (ADR-001):
- `#[serde(default)]` — pre-nan-009 JSONL without the key deserializes to `None`
- `#[serde(skip_serializing_if = "Option::is_none")]` — null phase is omitted from
  JSONL output entirely; no `"phase":null` key is emitted; backward wire-compat preserved
- MUST NOT add `skip_serializing_if` to the runner-side `ScenarioResult.phase` (different rule)

No other changes to `types.rs`.

---

## 2. `eval/scenarios/output.rs`

### Change: Add `phase` to SQL SELECT in `do_scenarios`

Current SQL format string (lines 107-113):
```
SELECT query_id, session_id, query_text, retrieval_mode, source,
       result_entry_ids, similarity_scores
FROM query_log
WHERE 1=1{source_clause}
ORDER BY query_id ASC{limit_clause}
```

New SQL format string — append `phase` to the SELECT list:
```
SELECT query_id, session_id, query_text, retrieval_mode, source,
       result_entry_ids, similarity_scores, phase
FROM query_log
WHERE 1=1{source_clause}
ORDER BY query_id ASC{limit_clause}
```

Note: `phase` is positioned after `similarity_scores` so existing column positions are
unchanged. sqlx `Row::try_get` accesses columns by name, not position, so column order
does not matter — but convention is to append new columns at the end of the SELECT list.

No other changes to `output.rs`.

---

## 3. `eval/scenarios/extract.rs`

### Change: Read `phase` from row in `build_scenario_record`

Current function body (lines 23-93) constructs `ScenarioContext` as:
```
context: ScenarioContext {
    agent_id: session_id.clone(),
    feature_cycle: String::new(),
    session_id,
    retrieval_mode,
},
```

Modified version — add `phase` read and populate `context.phase`:

```
FUNCTION build_scenario_record(row: &SqliteRow) -> Result<ScenarioRecord>

    // ... existing field reads unchanged ...
    query_id = row.try_get::<i64, _>("query_id")?
    session_id = row.try_get::<String, _>("session_id")?
    query_text = row.try_get::<String, _>("query_text")?
    retrieval_mode = row.try_get::<Option<String>, _>("retrieval_mode")?
                         .unwrap_or("flexible")
    source = row.try_get::<String, _>("source")?

    // NEW: read phase (nullable column — present since col-028)
    phase = row.try_get::<Option<String>, _>("phase")?
    // Returns None when the column value is SQL NULL (pre-col-028 rows or UDS sessions)
    // Returns Some("delivery") etc. when the column is non-null

    // ... existing entry_ids and scores parsing unchanged ...

    // ... existing length parity check (R-16) unchanged ...

    // ... existing baseline construction unchanged ...

    RETURN ScenarioRecord {
        id: format!("qlog-{query_id}"),
        query: query_text,
        context: ScenarioContext {
            agent_id: session_id.clone(),
            feature_cycle: String::new(),
            session_id,
            retrieval_mode,
            phase,              // NEW — populated from row
        },
        baseline,
        source,
        expected: None,
    }
```

The `phase` variable is bound BEFORE the `ScenarioContext` construction. Placement is
after `source` and before the JSON array parsing block — logical reading order follows
the flat column reads.

---

## Error Handling

`row.try_get::<Option<String>, _>("phase")?` propagates via `?`. Possible errors:
- Column name mismatch: if `output.rs` aliases the column differently from `"phase"`,
  `try_get` returns a runtime `ColumnNotFound` error. The caller (`do_scenarios`) returns
  this error up to `run_scenarios`, which returns `Box<dyn Error>` to the CLI. The eval
  command prints the error and exits non-zero.
- Type mismatch: impossible for `TEXT` column mapped to `Option<String>`.

Integration test (AC-10) covers the full SQL→struct path and catches column name mismatches.

---

## Key Test Scenarios

Tests live in `eval/scenarios/tests.rs`.

**T1: `test_scenarios_extract_phase_non_null`** (AC-10, R-04)
- Setup: extend `insert_query_log_row` helper to accept `phase: Option<&str>` parameter
- Insert row with `phase = Some("delivery")`
- Run extraction
- Assert extracted JSONL: `context.phase == Some("delivery")`
- Assert serialized JSONL line CONTAINS `"phase":"delivery"` key

**T2: `test_scenarios_extract_phase_null`** (AC-02, AC-10)
- Insert row with `phase = None`
- Run extraction
- Assert extracted JSONL: `context.phase == None`
- Assert serialized JSONL line does NOT contain `"phase"` key at all (not even `"phase":null`)
- This is the `skip_serializing_if` guard test

**T3: `test_scenario_context_phase_null_absent_from_jsonl`** (AC-09 item 2, R-05)
- Unit test (no DB): construct `ScenarioContext { phase: None, ... }`
- Serialize with `serde_json::to_string`
- Assert output does not contain `"phase"` anywhere

**T4: `test_scenario_context_phase_non_null_present_in_jsonl`** (AC-09 item 1)
- Unit test (no DB): construct `ScenarioContext { phase: Some("design"), ... }`
- Serialize with `serde_json::to_string`
- Assert output contains `"phase":"design"`

**insert_query_log_row helper update** (R-04, lesson #3543):
The existing test helper for inserting rows MUST accept `phase: Option<&str>` as a new
parameter. Callers that pass `None` reproduce pre-col-028 behaviour. Callers that pass
`Some("delivery")` test the non-null path. A helper that silently inserts NULL regardless
of the argument value would allow T1 to pass incorrectly.

---

## Notes

- `phase` is read-only here; it is never used to filter `query_log` rows (no `WHERE phase = ?`).
- The `--source` flag filters by the `source` column; there is no `--phase` filter (Non-Goal).
- UDS rows have `phase = NULL` by definition; MCP rows have `phase` populated only when
  `context_cycle` was called in that session (col-028).
