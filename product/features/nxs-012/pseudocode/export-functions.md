# Pseudocode: export-functions (export.rs)

## Purpose

Add 3 new per-table export functions for `graph_edges`, `observations`, and `cycle_events`. Bump `format_version` from 1 to 2. Integrate the 3 new functions into `do_export` after the existing 8 table exports.

## format_version Bump

In `write_header`, change:
```
map.insert("format_version", Value::Number(1.into()));
```
To:
```
map.insert("format_version", Value::Number(2.into()));
```

No other header changes in this component (the `skip_quarantined` header field is handled in skip-quarantined.md).

## New Function: export_graph_edges

```
async fn export_graph_edges(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn Error>>
{
    let rows = sqlx::query(
        "SELECT source_id, target_id, relation_type, weight,
                created_at, created_by, source, bootstrap_only, metadata
         FROM graph_edges
         ORDER BY source_id, target_id, relation_type"
    ).fetch_all(pool).await?;

    for row in &rows {
        let mut map = Map::new();
        map.insert("_table", "graph_edges");

        // i64 NOT NULL columns
        map.insert("source_id",      row.get::<i64, _>(0).into());
        map.insert("target_id",      row.get::<i64, _>(1).into());
        map.insert("relation_type",  row.get::<String, _>(2));

        // f64 weight with NaN safety (ADR-003): fallback to 1.0, not 0
        let weight: f64 = row.get::<f64, _>(3);
        map.insert("weight",
            Number::from_f64(weight)
                .unwrap_or_else(|| Number::from_f64(1.0).unwrap())
        );

        map.insert("created_at",     row.get::<i64, _>(4).into());
        map.insert("created_by",     row.get::<String, _>(5));
        map.insert("source",         row.get::<String, _>(6));
        map.insert("bootstrap_only", row.get::<i64, _>(7).into());

        // nullable TEXT
        map.insert("metadata", nullable_text(row, 8));

        write_row(map, writer)?;
    }
    Ok(())
}
```

**Key decisions**:
- ORDER BY (source_id, target_id, relation_type) for deterministic output (FR-01, AC-08)
- `id` column is NOT selected (ADR-005)
- Weight NaN fallback is 1.0 (ADR-003), differs from entries.confidence fallback of 0

## New Function: export_observations

```
async fn export_observations(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn Error>>
{
    let rows = sqlx::query(
        "SELECT id, session_id, ts_millis, hook, tool, input,
                response_size, response_snippet, topic_signal, phase
         FROM observations
         ORDER BY id"
    ).fetch_all(pool).await?;

    for row in &rows {
        let mut map = Map::new();
        map.insert("_table", "observations");

        // i64 NOT NULL
        map.insert("id",         row.get::<i64, _>(0).into());
        // String NOT NULL
        map.insert("session_id", row.get::<String, _>(1));
        // i64 NOT NULL
        map.insert("ts_millis",  row.get::<i64, _>(2).into());
        // String NOT NULL
        map.insert("hook",       row.get::<String, _>(3));

        // nullable TEXT
        map.insert("tool",             nullable_text(row, 4));
        map.insert("input",            nullable_text(row, 5));
        // nullable INTEGER
        map.insert("response_size",    nullable_int(row, 6));
        // nullable TEXT
        map.insert("response_snippet", nullable_text(row, 7));
        map.insert("topic_signal",     nullable_text(row, 8));
        map.insert("phase",            nullable_text(row, 9));

        write_row(map, writer)?;
    }
    Ok(())
}
```

**Key decisions**:
- ORDER BY id (FR-02, AC-09)
- `id` IS included (ADR-006 -- preserved through import)
- `response_size` uses `nullable_int` (it is an INTEGER column, not TEXT)

## New Function: export_cycle_events

```
async fn export_cycle_events(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn Error>>
{
    let rows = sqlx::query(
        "SELECT id, cycle_id, seq, event_type, phase, outcome,
                next_phase, timestamp, goal
         FROM cycle_events
         ORDER BY id"
    ).fetch_all(pool).await?;

    for row in &rows {
        let mut map = Map::new();
        map.insert("_table", "cycle_events");

        // i64 NOT NULL
        map.insert("id",         row.get::<i64, _>(0).into());
        // String NOT NULL
        map.insert("cycle_id",   row.get::<String, _>(1));
        // i64 NOT NULL
        map.insert("seq",        row.get::<i64, _>(2).into());
        // String NOT NULL
        map.insert("event_type", row.get::<String, _>(3));

        // nullable TEXT
        map.insert("phase",      nullable_text(row, 4));
        map.insert("outcome",    nullable_text(row, 5));
        map.insert("next_phase", nullable_text(row, 6));

        // i64 NOT NULL
        map.insert("timestamp",  row.get::<i64, _>(7).into());

        // nullable TEXT
        map.insert("goal",       nullable_text(row, 8));

        write_row(map, writer)?;
    }
    Ok(())
}
```

**Key decisions**:
- ORDER BY id (FR-03, AC-10)
- `id` IS included (ADR-006)
- `goal_embedding` is NOT selected at all (ADR-004). It does not appear in the SELECT column list.
- 9 columns selected: id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal

## do_export Integration

Add 3 new calls after `export_audit_log` and before `writer.flush()`:

```
async fn do_export(pool, writer) -> Result<(), Box<dyn Error>> {
    write_header(pool, writer).await?;
    export_counters(pool, writer).await?;
    export_entries(pool, writer).await?;
    export_entry_tags(pool, writer).await?;
    export_co_access(pool, writer).await?;
    export_feature_entries(pool, writer).await?;
    export_outcome_index(pool, writer).await?;
    export_agent_registry(pool, writer).await?;
    export_audit_log(pool, writer).await?;
    // NEW: 3 additional tables (FR-14: after existing 8)
    export_graph_edges(pool, writer).await?;
    export_observations(pool, writer).await?;
    export_cycle_events(pool, writer).await?;
    writer.flush()?;
    Ok(())
}
```

Order within the 3 new tables: graph_edges, observations, cycle_events (matching FR-14, ARCHITECTURE.md component interactions diagram).

## Error Handling

All 3 new functions follow the existing pattern:
- sqlx query errors propagate via `?` as `Box<dyn Error>`
- `write_row` errors propagate via `?`
- NaN in `export_graph_edges` is handled by the `Number::from_f64(...).unwrap_or_else(...)` pattern (no panic)
- Empty tables produce zero rows and zero write_row calls (FR-15)

## Key Test Scenarios

1. **export_graph_edges with populated data** -- 9 JSON fields + `_table`, correct values.
2. **export_graph_edges ordering** -- insert edges in non-sorted order, verify output ORDER BY source_id, target_id, relation_type.
3. **export_graph_edges NaN weight** -- insert edge with f64::NAN weight, verify JSON output has 1.0 (ADR-003).
4. **export_graph_edges Infinity weight** -- verify fallback to 1.0.
5. **export_graph_edges weight=0.0** -- valid weight, must NOT be replaced by fallback (0.0 is a valid f64).
6. **export_graph_edges null metadata** -- verify JSON null.
7. **export_graph_edges no id column** -- verify `id` key absent from JSON output.
8. **export_observations with populated data** -- 10 JSON fields + `_table`, correct values.
9. **export_observations ordering** -- ORDER BY id verified.
10. **export_observations null fields** -- all 6 nullable fields null simultaneously.
11. **export_observations.input with embedded newlines** -- verify JSONL line integrity (R-09).
12. **export_cycle_events with populated data** -- 9 JSON fields + `_table`, correct values.
13. **export_cycle_events ordering** -- ORDER BY id verified.
14. **export_cycle_events null fields** -- all 4 nullable fields null simultaneously.
15. **export_cycle_events no goal_embedding** -- verify field absent from JSON output (ADR-004).
16. **do_export includes all 11 tables** -- header + data lines from all tables present.
17. **do_export ordering** -- new tables appear after audit_log lines (FR-14).
18. **Empty tables produce no output** -- all 3 new tables empty, zero lines emitted.
19. **format_version is 2** -- parse header, assert `format_version == 2` (FR-04).
