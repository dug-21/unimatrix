# Pseudocode: format-types (format.rs)

## Purpose

Define typed deserialization structs for 3 new tables (`graph_edges`, `observations`, `cycle_events`) and add corresponding variants to the `ExportRow` tagged enum. These types form the serialization/deserialization contract between export and import.

## New Structs

### GraphEdgeRow (9 fields -- no `id`, per ADR-005)

```
#[derive(Deserialize, Debug)]
struct GraphEdgeRow {
    source_id: i64,          // NOT NULL
    target_id: i64,          // NOT NULL
    relation_type: String,   // NOT NULL
    weight: f64,             // NOT NULL, REAL
    created_at: i64,         // NOT NULL
    created_by: String,      // NOT NULL
    source: String,          // NOT NULL
    bootstrap_only: i64,     // NOT NULL
    metadata: Option<String> // nullable TEXT
}
```

### ObservationRow (10 fields -- includes `id`, per ADR-006)

```
#[derive(Deserialize, Debug)]
struct ObservationRow {
    id: i64,                         // NOT NULL (preserved through import)
    session_id: String,              // NOT NULL
    ts_millis: i64,                  // NOT NULL
    hook: String,                    // NOT NULL
    tool: Option<String>,            // nullable TEXT
    input: Option<String>,           // nullable TEXT
    response_size: Option<i64>,      // nullable INTEGER
    response_snippet: Option<String>,// nullable TEXT
    topic_signal: Option<String>,    // nullable TEXT
    phase: Option<String>            // nullable TEXT
}
```

### CycleEventRow (9 fields -- excludes `goal_embedding`, per ADR-004)

```
#[derive(Deserialize, Debug)]
struct CycleEventRow {
    id: i64,                    // NOT NULL (preserved through import)
    cycle_id: String,           // NOT NULL
    seq: i64,                   // NOT NULL
    event_type: String,         // NOT NULL
    phase: Option<String>,      // nullable TEXT
    outcome: Option<String>,    // nullable TEXT
    next_phase: Option<String>, // nullable TEXT
    timestamp: i64,             // NOT NULL
    goal: Option<String>        // nullable TEXT
}
```

## ExportRow Enum -- 3 New Variants

Add after the existing `AuditLog` variant:

```
#[serde(rename = "graph_edges")]
GraphEdge(GraphEdgeRow),

#[serde(rename = "observations")]
Observation(ObservationRow),

#[serde(rename = "cycle_events")]
CycleEvent(CycleEventRow),
```

The `serde(rename)` values match the `_table` discriminator strings used in JSONL. These must exactly match the strings used in `map.insert("_table", ...)` calls in the export functions.

## Module-Level Doc Comment Update

Change the doc comment from:
```
//! Shared typed deserialization structs for JSONL format_version 1 (ADR-001).
```
To:
```
//! Shared typed deserialization structs for JSONL format_version 1-2.
```

## Import for New Types in inserters.rs

The `use crate::format::{...}` import in `import/inserters.rs` must be extended to include:
```
GraphEdgeRow, ObservationRow, CycleEventRow
```

## Error Handling

No explicit error handling in this component. Serde deserialization errors propagate through `serde_json::from_str` in `ingest_rows`. Unknown `_table` values produce a serde deserialization error with the unknown variant name in the message (existing behavior -- see `test_export_row_unknown_table_errors`).

## Key Test Scenarios

1. **Deserialize GraphEdgeRow with all fields** -- JSON with 9 data fields + `_table` deserializes correctly via `ExportRow::GraphEdge`.
2. **Deserialize GraphEdgeRow with null metadata** -- `metadata: null` maps to `None`.
3. **Deserialize ObservationRow with all fields** -- JSON with 10 data fields + `_table` deserializes correctly via `ExportRow::Observation`.
4. **Deserialize ObservationRow with all nullable fields null** -- `tool`, `input`, `response_size`, `response_snippet`, `topic_signal`, `phase` all null.
5. **Deserialize CycleEventRow with all fields** -- JSON with 9 data fields + `_table` deserializes correctly via `ExportRow::CycleEvent`.
6. **Deserialize CycleEventRow with all nullable fields null** -- `phase`, `outcome`, `next_phase`, `goal` all null.
7. **CycleEventRow does NOT include goal_embedding** -- a JSON line without `goal_embedding` deserializes successfully. A JSON line WITH `goal_embedding: null` should also succeed (serde ignores unknown fields by default with `#[serde(tag)]`).
8. **GraphEdgeRow field count guard** -- exactly 9 data fields. Removing any required field causes deserialization error.
9. **ObservationRow field count guard** -- exactly 10 data fields.
10. **CycleEventRow field count guard** -- exactly 9 data fields.
11. **Unknown _table value still errors** -- existing test continues to pass (no regression).
