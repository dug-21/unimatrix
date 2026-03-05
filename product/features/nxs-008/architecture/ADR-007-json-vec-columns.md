# ADR-007: JSON Array Columns for Non-Queried Vec Fields

**Status**: Accepted
**Context**: nxs-008
**Mitigates**: SR-06 (JSON Array Columns May Constrain Future Analytics)

## Decision

Vec fields that are not queried by element are stored as `TEXT` columns containing JSON arrays. Specifically:

| Table | Field | JSON Column |
|-------|-------|-------------|
| signal_queue | entry_ids | `entry_ids TEXT NOT NULL DEFAULT '[]'` |
| agent_registry | capabilities | `capabilities TEXT NOT NULL DEFAULT '[]'` |
| agent_registry | allowed_topics | `allowed_topics TEXT` (nullable) |
| agent_registry | allowed_categories | `allowed_categories TEXT` (nullable) |
| audit_log | target_ids | `target_ids TEXT NOT NULL DEFAULT '[]'` |

### Rationale

1. **These fields are never queried by element**: `entry_ids` in signal_queue is read as a complete list during drain. `capabilities` is loaded and checked in-memory. `target_ids` is append-only audit data.

2. **Junction tables would be over-engineering**: Creating 5 junction tables for fields that are never WHERE-filtered adds complexity without benefit.

3. **SQLite JSON support**: `json_each()` is available if future queries need element-level access. Performance is acceptable for these low-volume tables.

4. **ASS-016 critical path is unaffected**: The entry effectiveness query (`INJECTION_LOG JOIN SESSIONS JOIN ENTRIES`) uses indexed integer columns, not JSON. The JSON columns are on tables outside the analytics critical path.

### Serialization

```rust
// Write: Vec<u64> -> JSON string
let json = serde_json::to_string(&record.entry_ids)?;

// Read: JSON string -> Vec<u64>
let entry_ids: Vec<u64> = serde_json::from_str(&json_str)?;
```

### Revisit Criteria

Convert to junction tables if:
- AUDIT_LOG exceeds 100K rows AND element-level queries are needed
- `json_each()` JOIN performance becomes measurable in profiling
- ASS-016 analytics require "which agents modified entry X?" queries against target_ids

## Consequences

- 5 fewer junction tables than a fully normalized schema
- `serde_json` dependency added to unimatrix-store (already a transitive dep)
- Nullable JSON columns use `Option<Vec<T>>` → `NULL` mapping
