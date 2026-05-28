# Pseudocode: import-pipeline (import/mod.rs)

## Purpose

Extend the import orchestration layer to handle 3 new table types: update ImportCounts, add ingest_rows match arms, extend drop_all_data with FK-safe ordering, update format_version validation, extend print_summary, and extend record_provenance.

## ImportCounts Extension

Add 3 new fields with Default-compatible initialization (u64 defaults to 0):

```
#[derive(Debug, Default)]
pub struct ImportCounts {
    pub counters: u64,
    pub entries: u64,
    pub entry_tags: u64,
    pub co_access: u64,
    pub feature_entries: u64,
    pub outcome_index: u64,
    pub agent_registry: u64,
    pub audit_log: u64,
    // NEW
    pub graph_edges: u64,
    pub observations: u64,
    pub cycle_events: u64,
}
```

## use Block Extension

Extend the inserters import to include new functions:

```
use inserters::{
    insert_agent_registry, insert_audit_log, insert_co_access, insert_counter, insert_entry,
    insert_entry_tag, insert_feature_entry, insert_outcome_index,
    // NEW
    insert_graph_edge, insert_observation, insert_cycle_event,
};
```

## ingest_rows Match Arms

Add 3 new arms after the existing `ExportRow::AuditLog` arm:

```
ExportRow::GraphEdge(r) => {
    insert_graph_edge(conn, &r).await?;
    counts.graph_edges += 1;
}
ExportRow::Observation(r) => {
    insert_observation(conn, &r).await?;
    counts.observations += 1;
}
ExportRow::CycleEvent(r) => {
    insert_cycle_event(conn, &r).await?;
    counts.cycle_events += 1;
}
```

No progress reporting (eprintln every N rows) for the new tables -- only entries has this because it is the dominant table for user-visible progress.

## drop_all_data Extension (ADR-001)

Replace the existing single query string with an expanded one. The 5 new DELETEs go between `DELETE FROM vector_map` and `DELETE FROM entries`:

```
async fn drop_all_data(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    sqlx::query(
        "DELETE FROM entry_tags;
         DELETE FROM co_access;
         DELETE FROM feature_entries;
         DELETE FROM outcome_index;
         DELETE FROM agent_registry;
         DELETE FROM vector_map;
         DELETE FROM observation_phase_metrics;
         DELETE FROM observation_metrics;
         DELETE FROM graph_edges;
         DELETE FROM observations;
         DELETE FROM cycle_events;
         DELETE FROM entries;
         DELETE FROM counters;"
    )
    .execute(pool)
    .await?;
    Ok(())
}
```

**Critical ordering (ADR-001)**:
1. `observation_phase_metrics` BEFORE `observation_metrics` (FK: observation_phase_metrics -> observation_metrics ON DELETE CASCADE)
2. `observation_metrics` BEFORE `observations` (derived data, clear to prevent stale aggregates)
3. `graph_edges`, `observations`, `cycle_events` BEFORE `entries` (graph_edges.source_id/target_id reference entry IDs, though no FK constraint exists)

This ordering is safe regardless of `PRAGMA foreign_keys` state.

## format_version Validation (ADR-002)

Replace the existing check:
```
if header.format_version != 1 {
    return Err(format!(
        "unsupported format_version: {}. Only format_version 1 is supported.",
        header.format_version
    ).into());
}
```

With:
```
match header.format_version {
    1 | 2 => { /* ok */ }
    v => {
        return Err(format!(
            "unsupported format_version: {v}. This binary supports format_version 1 and 2."
        ).into());
    }
}
```

This accepts both v1 (legacy 8 tables) and v2 (11 tables). For v1 files, the 3 new ExportRow variants never appear, so their match arms are never reached and ImportCounts for the new tables remain 0.

## record_provenance Extension (FR-17)

Extend the detail format string to include 3 new table counts:

```
let detail = format!(
    "Imported from '{}': {} entries, {} tags, {} co-access pairs, {} counters, \
     {} graph_edges, {} observations, {} cycle_events",
    input_path.display(),
    counts.entries,
    counts.entry_tags,
    counts.co_access,
    counts.counters,
    counts.graph_edges,
    counts.observations,
    counts.cycle_events
);
```

## print_summary Extension (FR-13)

Add 3 new lines after the existing `Audit log:` line:

```
fn print_summary(counts: &ImportCounts, skip_hash_validation: bool) {
    eprintln!("Import complete:");
    eprintln!("  Counters:        {}", counts.counters);
    eprintln!("  Entries:         {}", counts.entries);
    eprintln!("  Entry tags:      {}", counts.entry_tags);
    eprintln!("  Co-access pairs: {}", counts.co_access);
    eprintln!("  Feature entries: {}", counts.feature_entries);
    eprintln!("  Outcome index:   {}", counts.outcome_index);
    eprintln!("  Agent registry:  {}", counts.agent_registry);
    eprintln!("  Audit log:       {}", counts.audit_log);
    // NEW
    eprintln!("  Graph edges:     {}", counts.graph_edges);
    eprintln!("  Observations:    {}", counts.observations);
    eprintln!("  Cycle events:    {}", counts.cycle_events);

    if skip_hash_validation {
        eprintln!("  Hash validation: SKIPPED");
    } else {
        eprintln!("  Hash validation: PASSED");
    }
}
```

## Error Handling

- ingest_rows: errors from new inserters propagate via `?`, causing ROLLBACK in run_import_async (existing error path at line 199)
- drop_all_data: sqlx errors propagate via `?`
- format_version validation: explicit error message with version number and supported range
- record_provenance: uses existing SqlxStore::log_audit_event path

## Key Test Scenarios

1. **format_version 1 import succeeds** -- v1 file imports cleanly, new table counts are 0 (FR-05, AC-05).
2. **format_version 2 import succeeds** -- v2 file with all 11 tables, all counts non-zero (FR-06, AC-06).
3. **format_version 0 rejected** -- error message includes "0" and supported range (FR-08, AC-07).
4. **format_version 3 rejected** -- error message includes "3" and supported range (FR-07, AC-07).
5. **format_version 999 rejected** -- boundary test.
6. **drop_all_data clears all new tables** -- populate graph_edges, observations, cycle_events, observation_metrics, observation_phase_metrics. Run --force import. Verify all 5 tables empty (FR-12, AC-13).
7. **drop_all_data FK ordering** -- populate observation_phase_metrics + observation_metrics (no observations). --force import succeeds without FK violation.
8. **ingest_rows routes GraphEdge correctly** -- craft JSONL with graph_edges rows, import, verify rows in DB.
9. **ingest_rows routes Observation correctly** -- craft JSONL with observations rows, import, verify rows in DB.
10. **ingest_rows routes CycleEvent correctly** -- craft JSONL with cycle_events rows, import, verify rows in DB.
11. **print_summary includes 3 new lines** -- capture stderr, verify graph_edges/observations/cycle_events lines present (FR-13, AC-20).
12. **record_provenance includes new counts** -- after import, query audit_log for provenance event, verify detail string mentions all 3 new tables (FR-17).
13. **v1 import print_summary shows 0 for new tables** -- import v1 file, verify summary shows 0 for graph_edges, observations, cycle_events.
