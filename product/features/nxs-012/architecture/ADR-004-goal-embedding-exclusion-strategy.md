## ADR-004: goal_embedding Excluded from Export via SELECT Omission

### Context

`cycle_events.goal_embedding` is a BLOB column containing a bincode-encoded `Vec<f32>` tied to the specific ONNX model version. The scope explicitly excludes it from export (Non-Goals) because:
1. Bincode format is not portable across model versions
2. Goal embeddings are lazily reconstructed on first cycle activity post-import
3. Exporting BLOBs increases file size significantly for no portability benefit

Risk SR-03 identifies that post-import goal-cluster affinity scores are unavailable until first cycle completion. `context_briefing` gracefully degrades when `goal_embedding` is NULL -- it falls back to pure semantic path scoring.

Two implementation options:
- Option A: Include `goal_embedding` in SELECT, emit as JSON null in the export row, add `goal_embedding: Option<serde_json::Value>` to `CycleEventRow` with `#[serde(default)]`
- Option B: Exclude `goal_embedding` from the SELECT entirely, do not include it in `CycleEventRow`

### Decision

Option B. Exclude `goal_embedding` from the export SELECT and from `CycleEventRow`.

The SELECT for `export_cycle_events` lists 9 columns explicitly (id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal) -- `goal_embedding` is not selected. The `CycleEventRow` struct has 9 fields matching these columns. The `insert_cycle_event` inserter explicitly sets `goal_embedding` to NULL in the INSERT statement.

Option A adds a field that is always null on export and always ignored on import -- unnecessary complexity. Option B keeps the format clean: the column simply does not exist in the export contract.

### Consequences

- Export files are smaller (no null BLOB placeholders for every cycle_event row)
- `CycleEventRow` has exactly 9 fields -- clean contract
- `insert_cycle_event` must explicitly include `goal_embedding` in its INSERT and bind NULL
- Post-import, goal-cluster affinity scoring returns neutral results until the next cycle completion triggers lazy reconstruction (documented degradation, SR-03 accepted)
- If a future format_version needs to include goal_embedding (e.g., after a portable embedding format is defined), it would be a new field addition to `CycleEventRow` with `#[serde(default)]`
