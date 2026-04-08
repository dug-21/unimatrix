## ADR-005: observations.input Storage Contract — No Double-Encoding on Hook Path

### Context

SR-01 identified a critical risk: if the hook-listener path stores
`observations.input` as a double-encoded JSON string (i.e., the column contains
`'"{\\"id\\":42}"'` rather than `'{"id":42}'`), then `json_extract(input, '$.id')`
returns NULL for all hook-path rows silently, making Query A return zero rows
for the most common observation source.

The concern originated from `knowledge_reuse.rs` line 1897 in the historical
codebase, where the hook-listener read path wraps the stored string as
`input_str.map(serde_json::Value::String)` — creating a `Value::String(raw_json)`.
The `extract_explicit_read_ids` function in `knowledge_reuse.rs` handles this
at the Rust layer with a two-branch parse (ADR-001 correction in crt-049).

The question for crt-050 is: does the **stored** value in the `observations.input`
column use double-encoding, or is it a plain JSON string?

**Verification of the write path in `listener.rs`:**

The hook-listener's `extract_observation_fields()` function (lines 2686–2697):

```rust
"PreToolUse" => {
    let tool = event.payload.get("tool_name")...;
    let input = event.payload
        .get("tool_input")
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    (tool, input, None, None)
}
```

`event.payload.get("tool_input")` returns a `&serde_json::Value`. For a
`context_get` call, `tool_input` is the JSON object `{"id": 42}`, represented
as `Value::Object(...)`. `serde_json::to_string(Value::Object {...})` serializes
it to the string `'{"id":42}'`. This is stored as `Option<String>` in
`ObservationRow.input` and written as-is to the `input TEXT` column.

**Result: no double-encoding at the write path.** The column stores
`'{"id":42}'` not `'"{\\"id\\":42}"'`.

The double-encoding that `knowledge_reuse.rs` works around is a **read-path**
artifact: when observations are read back from the DB by the hook-listener's
`get_phase_for_session` function (lines 1886–1897), the raw string is re-wrapped
as `Value::String(raw_json)` for compatibility with the `ObservationRecord`
in-memory type. That wrapping is a read-path concern and does not affect
the stored bytes.

The `AnalyticsWrite::Observation` path (analytics.rs) also writes `input` as
`Option<String>` without any additional JSON encoding layer.

**Conclusion:** The pure-SQL approach for Query A is valid. The
`json_extract(o.input, '$.id')` expression operates on the raw stored string
`'{"id":42}'` and returns `42` correctly for all hook-path observations.

### Decision

Adopt the pure-SQL approach for Query A (as specified in ADR-002). No two-phase
SQL+Rust fallback is required for the storage encoding issue.

The implementation MUST include a doc comment on `query_phase_freq_observations`
asserting this contract:

```
/// # Input Storage Contract
///
/// Relies on observations.input storing tool_input as a serialized JSON object
/// string (e.g., '{"id":42}'), NOT as a double-encoded string.
/// This contract is established by extract_observation_fields() in listener.rs,
/// which uses serde_json::to_string(v) on the Value::Object directly.
/// Verified at crt-050 architecture review. If this contract is ever broken,
/// json_extract will silently return NULL for all hook-path rows.
```

### Consequences

- No two-phase extraction complexity in the query.
- If a future code change introduces a double-encoding write path (e.g., a new
  observation write site that uses `serde_json::to_string(&serde_json::to_string(v))`),
  the SQL query will silently return zero rows. The observations-coverage
  diagnostic (AC-11) would detect this as sparse signal and emit a warning, but
  would not identify the root cause.
- The contract assertion doc comment creates a forcing function for future
  reviewers to re-verify this assumption if the write path changes.
