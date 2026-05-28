# Pseudocode: import-inserters (import/inserters.rs)

## Purpose

Add 3 new parameterized INSERT functions for `graph_edges`, `observations`, and `cycle_events`. Each function follows the existing pattern: accepts `&mut SqliteConnection` and a reference to the corresponding row struct from `format.rs`.

## New Import in use Block

Extend the existing `use crate::format::{...}` to include:
```
GraphEdgeRow, ObservationRow, CycleEventRow
```

## New Function: insert_graph_edge

```
pub(super) async fn insert_graph_edge(
    conn: &mut SqliteConnection,
    r: &GraphEdgeRow,
) -> Result<(), Box<dyn Error>>
{
    sqlx::query(
        "INSERT INTO graph_edges (
            source_id, target_id, relation_type, weight,
            created_at, created_by, source, bootstrap_only, metadata
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )
    .bind(r.source_id)
    .bind(r.target_id)
    .bind(&r.relation_type)
    .bind(r.weight)
    .bind(r.created_at)
    .bind(&r.created_by)
    .bind(&r.source)
    .bind(r.bootstrap_only)
    .bind(&r.metadata)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
```

**Key decisions**:
- Plain `INSERT INTO` -- NOT `INSERT OR IGNORE`, NOT `INSERT OR REPLACE` (FR-09, ADR-005)
- Duplicate (source_id, target_id, relation_type) surfaces as UNIQUE constraint error (data corruption detection)
- `id` column NOT in INSERT -- SQLite AUTOINCREMENT assigns fresh values (ADR-005)
- 9 columns bound, matching GraphEdgeRow's 9 fields

## New Function: insert_observation

```
pub(super) async fn insert_observation(
    conn: &mut SqliteConnection,
    r: &ObservationRow,
) -> Result<(), Box<dyn Error>>
{
    sqlx::query(
        "INSERT INTO observations (
            id, session_id, ts_millis, hook, tool, input,
            response_size, response_snippet, topic_signal, phase
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    )
    .bind(r.id)
    .bind(&r.session_id)
    .bind(r.ts_millis)
    .bind(&r.hook)
    .bind(&r.tool)
    .bind(&r.input)
    .bind(r.response_size)
    .bind(&r.response_snippet)
    .bind(&r.topic_signal)
    .bind(&r.phase)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
```

**Key decisions**:
- `id` IS explicitly bound (ADR-006 -- preserved through import for watermark/ordering)
- Plain INSERT (not INSERT OR IGNORE) -- duplicate ids indicate data corruption
- 10 columns bound, matching ObservationRow's 10 fields
- Option fields (`tool`, `input`, etc.) bind as NULL when `None` via sqlx's Option support

## New Function: insert_cycle_event

```
pub(super) async fn insert_cycle_event(
    conn: &mut SqliteConnection,
    r: &CycleEventRow,
) -> Result<(), Box<dyn Error>>
{
    sqlx::query(
        "INSERT INTO cycle_events (
            id, cycle_id, seq, event_type, phase, outcome,
            next_phase, timestamp, goal, goal_embedding
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)"
    )
    .bind(r.id)
    .bind(&r.cycle_id)
    .bind(r.seq)
    .bind(&r.event_type)
    .bind(&r.phase)
    .bind(&r.outcome)
    .bind(&r.next_phase)
    .bind(r.timestamp)
    .bind(&r.goal)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
```

**Key decisions**:
- `id` IS explicitly bound (ADR-006)
- `goal_embedding` is in the INSERT column list but bound as literal `NULL` in the VALUES clause (ADR-004)
- The INSERT has 10 columns but only 9 bind parameters because `goal_embedding` is hardcoded NULL
- Plain INSERT (not INSERT OR IGNORE)
- 9 bind parameters from CycleEventRow's 9 fields + 1 literal NULL = 10 total values

## Error Handling

All 3 functions follow the existing inserter pattern:
- sqlx errors propagate via `?` as `Box<dyn Error>`
- SQLite constraint violations (UNIQUE, NOT NULL, PRIMARY KEY) surface as-is -- no wrapping
- For `insert_graph_edge`: UNIQUE constraint on (source_id, target_id, relation_type) is the intended corruption detection mechanism (FR-09)
- For `insert_observation` and `insert_cycle_event`: PRIMARY KEY constraint on `id` detects duplicate rows

## Key Test Scenarios

1. **insert_graph_edge basic** -- insert a valid edge, query back, verify all 9 fields match.
2. **insert_graph_edge duplicate natural key** -- insert two edges with same (source_id, target_id, relation_type), verify UNIQUE constraint error (FR-09, AC-18).
3. **insert_graph_edge null metadata** -- insert with metadata=None, query back, verify SQL NULL.
4. **insert_observation basic** -- insert a valid observation with id, query back, verify all 10 fields.
5. **insert_observation id preserved** -- insert with id=42, query back, verify id=42 (FR-10, AC-16).
6. **insert_observation duplicate id** -- insert two with same id, verify PRIMARY KEY error.
7. **insert_observation all nullable fields null** -- insert with tool/input/response_size/response_snippet/topic_signal/phase all None.
8. **insert_cycle_event basic** -- insert a valid cycle event with id, query back, verify all 9 exported fields.
9. **insert_cycle_event id preserved** -- insert with id=99, query back, verify id=99 (FR-11, AC-17).
10. **insert_cycle_event goal_embedding is NULL** -- after insert, query `goal_embedding` column directly, verify it is SQL NULL (ADR-004, AC-19).
11. **insert_cycle_event all nullable fields null** -- insert with phase/outcome/next_phase/goal all None.
