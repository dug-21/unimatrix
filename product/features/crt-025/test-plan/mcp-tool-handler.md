# Test Plan: MCP Tool Handler (Component 2)

File: `crates/unimatrix-server/src/mcp/tools.rs`
Risks: R-01 (partial), R-03 (partial), R-08, AC-01, AC-04, AC-05, AC-07, AC-08

---

## Unit Test Expectations

All inline `#[cfg(test)]` functions. Focus: `CycleParams` deserialization and wire schema.

### `CycleParams` Deserialization (AC-01, FR-01)

**`test_cycle_params_deserialize_minimal`**
- Arrange: `r#"{"type":"start","topic":"crt-025"}"#`
- Act: `serde_json::from_str::<CycleParams>(...)`
- Assert: `Ok(_)`, `params.phase == None`, `params.outcome == None`, `params.next_phase == None`

**`test_cycle_params_deserialize_with_phase_fields`**
- Arrange: `r#"{"type":"phase-end","topic":"t","phase":"scope","outcome":"done","next_phase":"design"}"#`
- Assert: `Ok(params)` where `params.phase == Some("scope")`, `params.outcome == Some("done")`,
  `params.next_phase == Some("design")`

**`test_cycle_params_keywords_silently_discarded`** (AC-01, FR-01.3)
- Arrange: `r#"{"type":"start","topic":"crt-025","keywords":["k1","k2"]}"#`
- Assert: `Ok(_)` — deserialization succeeds
- Assert: `CycleParams` struct has no `keywords` field accessible (compile-time verification)

**`test_cycle_params_missing_type_fails`** (FR-01.4)
- Arrange: `r#"{"topic":"crt-025"}"#`
- Assert: `Err(_)` — missing required field

**`test_cycle_params_missing_topic_fails`** (FR-01.4)
- Arrange: `r#"{"type":"start"}"#`
- Assert: `Err(_)` — missing required field

**`test_cycle_params_all_optional_none`**
- Arrange: JSON with only `type` and `topic`
- Assert: `phase`, `outcome`, `next_phase` all `None`; `agent_id`, `format` all `None`

---

## Integration Test Expectations

The MCP tool handler is exercised through infra-001 tests and server-level integration tests.

### infra-001 `tools` suite (new tests)

**`test_cycle_phase_end_type_accepted`** (AC-02)
- Send `context_cycle(type="phase-end", topic="test-topic", phase="scope")`
- Assert success response

**`test_cycle_phase_end_stores_row`** (AC-04, AC-08)
- Send three sequential `context_cycle` calls (start, phase-end, stop) for same topic
- Call `context_cycle_review(feature_cycle=topic, format="json")`
- Assert `phase_narrative` present in response (proves rows were stored)

**`test_cycle_invalid_type_rejected`** (AC-02)
- Send `context_cycle(type="pause", topic="test-topic")`
- Assert error response containing valid type names

**`test_cycle_phase_with_space_rejected`** (AC-03)
- Send `context_cycle(type="phase-end", topic="test-topic", phase="scope review")`
- Assert error response mentioning `phase` field

**`test_cycle_outcome_category_rejected`** (AC-15, R-03)
- Send `context_store(content="x", topic="t", category="outcome", agent_id="human")`
- Assert error with "category" or "InvalidCategory" in response

### Server-level integration (inline or separate integration test)

**`test_context_cycle_start_with_next_phase_then_store`** (AC-05, R-01)
- Register a session
- Call `context_cycle(type="start", ..., next_phase="scope")` through full MCP handler
- Immediately call `context_store(...)`
- Query `feature_entries` for the stored entry
- Assert `feature_entries.phase = "scope"` — not NULL

This is the causal R-01 test. It verifies that the synchronous `set_current_phase` call in the
UDS listener completes before any `context_store` can observe the session state. The key
assertion: there is no async interleaving between the phase mutation and the store read.

**`test_context_cycle_stop_then_store_phase_null`** (AC-07, R-01)
- After a `stop` event, `context_store` must write `phase = NULL` to `feature_entries`

**`test_cycle_events_seq_monotonic`** (AC-08, R-07)
- Call `context_cycle` three times for the same `cycle_id`
- Query `cycle_events WHERE cycle_id = ?` ordered by `timestamp ASC, seq ASC`
- Assert three rows with `seq` values 0, 1, 2

**`test_cycle_review_phase_narrative_absent_no_events`** (AC-13, R-08)
- Call `context_cycle_review` for a feature_cycle with no CYCLE_EVENTS rows
- Deserialize response as JSON
- Assert no `phase_narrative` key in the top-level object

---

## Backward Compatibility Assertions

- `keywords` in input JSON: silently discarded, no error (AC-01)
- `context_cycle` callers without `phase`/`outcome`/`next_phase`: unchanged behavior (NFR-06)
- `context_cycle_review` for pre-WA-1 features: unchanged output, no `phase_narrative` key (AC-13)
