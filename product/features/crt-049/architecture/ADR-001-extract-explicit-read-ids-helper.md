## ADR-001: `extract_explicit_read_ids` as Standalone Helper in `knowledge_reuse.rs`

### Context

crt-049 requires filtering the `attributed: Vec<ObservationRecord>` slice to extract
entry IDs from `context_get` and single-ID `context_lookup` PreToolUse events.

Two placement options:
- Option A: Inline the filter logic directly inside `compute_knowledge_reuse_for_sessions`
  in `tools.rs`. Simple but untestable in isolation — `compute_knowledge_reuse_for_sessions`
  is `async` and requires a live store, making unit tests expensive.
- Option B: Extract to `fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>`
  in `knowledge_reuse.rs`. Pure function over an in-memory slice — directly unit testable
  with synthetic `ObservationRecord` values, no store required.

Option B aligns with the existing pattern: `compute_knowledge_reuse` in `knowledge_reuse.rs`
is already a pure function that receives pre-loaded slices from its caller (`tools.rs`),
keeping computation isolated from I/O for testability (col-020 ADR-001).

The extraction rule:
1. `event_type == "PreToolUse"`
2. `normalize_tool_name(tool.as_deref().unwrap_or(""))` equals `"context_get"` or
   `"context_lookup"` (case-sensitive; `normalize_tool_name` strips `mcp__unimatrix__`)
3. After parsing `input` (see Correction below), `obj["id"]` yields a valid `u64` via
   either `as_u64()` or `as_str().and_then(|s| s.parse().ok())`

Filter-based `context_lookup` calls (no `"id"` field in input, or `input["id"]` is null)
are excluded by condition 3 without special-casing.

### Decision

Add `pub(crate) fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>`
to `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`.

The function signature:
```rust
pub(crate) fn extract_explicit_read_ids(
    attributed: &[ObservationRecord],
) -> HashSet<u64>
```

`ObservationRecord` is imported from `unimatrix_core` (already a transitive dependency via
`unimatrix_observe`). No new crate dependencies are required.

`normalize_tool_name` is called via `unimatrix_observe::normalize_tool_name` (already
re-exported from `unimatrix_observe::lib.rs` and used in `tools.rs`).

`compute_knowledge_reuse_for_sessions` in `tools.rs` calls this function and passes the
resulting `HashSet<u64>` and pre-fetched `HashMap<u64, EntryMeta>` into `compute_knowledge_reuse`.

### Consequences

Easier:
- All five AC-12 unit test cases (a–e) are unit-testable without a store fixture.
- The extraction rule is in one place; future callers (e.g., ASS-040 Group 10) can reuse it.
- Mismatched prefix handling (AC-06) is structurally guaranteed by `normalize_tool_name`.

Harder:
- `compute_knowledge_reuse` gains two new parameters (`explicit_read_ids`, `explicit_read_meta`),
  requiring updates to all call sites and test fixtures that call it directly.

### Correction

**Runtime type of `ObservationRecord.input` (Issues 1 & 2)**

The original extraction predicate assumed `record.input` would always arrive as
`Some(Value::Object(_))`. This is wrong in the hook-listener code path.

`ObservationRecord.input` arrives from the hook listener as `Some(Value::String(raw_json_string))`
— the listener wraps the raw JSON string without parsing it. This is confirmed by
`extract_topic_signal` at `listener.rs:1911`, which handles the identical pattern with an
explicit two-branch match. Direct MCP calls may produce `Some(Value::Object(_))`. Indexing
a `Value::String` with `["id"]` returns `Value::Null`, so `as_u64()` would always return
`None` in the hook-listener path — a silent zero.

The correct two-branch parse (matching the `listener.rs:1911` pattern):
```rust
let obj: Option<serde_json::Value> = match &record.input {
    Some(serde_json::Value::Object(_)) => record.input.clone(),
    Some(serde_json::Value::String(s)) => serde_json::from_str(s).ok(),
    _ => None,
};
```

Additionally, `GetParams` accepts both integer-form (`{"id": 42}`) and string-form
(`{"id": "42"}`) IDs. The extraction must handle both:
```rust
let id: Option<u64> = obj["id"].as_u64()
    .or_else(|| obj["id"].as_str().and_then(|s| s.parse().ok()));
```

This is a runtime type correction, not a change of decision. The decision to use a
standalone helper in `knowledge_reuse.rs` is unchanged.
