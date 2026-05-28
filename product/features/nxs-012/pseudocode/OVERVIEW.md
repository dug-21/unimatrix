# Pseudocode Overview: nxs-012 Export/Import Complete Persistent State Coverage

## Components

| Component | File | Purpose |
|-----------|------|---------|
| format-types | `format.rs` | 3 new row structs + 3 ExportRow enum variants |
| export-functions | `export.rs` | 3 new export functions + format_version bump + do_export integration |
| import-inserters | `import/inserters.rs` | 3 new INSERT functions |
| import-pipeline | `import/mod.rs` | ImportCounts, ingest_rows, drop_all_data, print_summary, format_version, record_provenance |
| skip-quarantined | `export.rs` + `main.rs` | CLI flags, skip-set construction, skip_ids threading to 5 exporters, skip reporting |

## Data Flow

### Export (format_version 2)

```
CLI --skip-quarantined --confirm
  |
  v
run_export_inner(skip_quarantined, confirm)
  |-- if skip_quarantined && !confirm -> abort (ADR-009)
  |-- open DB, BEGIN DEFERRED
  |-- if skip_quarantined: SELECT id FROM entries WHERE status=3 -> skip_ids: HashSet<i64>
  |-- else: skip_ids = empty HashSet
  |
  v
do_export(pool, writer, &skip_ids)
  |-- write_header [format_version=2, optional skip_quarantined field]
  |-- export_counters(pool, writer)
  |-- export_entries(pool, writer, &skip_ids)        -- check id
  |-- export_entry_tags(pool, writer, &skip_ids)     -- check entry_id
  |-- export_co_access(pool, writer, &skip_ids)      -- check entry_id_a, entry_id_b
  |-- export_feature_entries(pool, writer, &skip_ids) -- check entry_id
  |-- export_outcome_index(pool, writer)
  |-- export_agent_registry(pool, writer)
  |-- export_audit_log(pool, writer)
  |-- export_graph_edges(pool, writer, &skip_ids)     -- check source_id, target_id  [NEW]
  |-- export_observations(pool, writer)                                                [NEW]
  |-- export_cycle_events(pool, writer)                                                [NEW]
  |-- flush
  v
COMMIT, report skip counts to stderr if skip_quarantined
```

### Import (accepts format_version 1 or 2)

```
parse_header -> validate format_version (1|2 ok, else error)
check_preflight
drop_all_data (if --force) -- now includes 5 new DELETEs
BEGIN IMMEDIATE on single connection
ingest_rows:
  match ExportRow variant:
    ...existing 8...
    GraphEdge(r)   -> insert_graph_edge(conn, &r)    [NEW]
    Observation(r) -> insert_observation(conn, &r)    [NEW]
    CycleEvent(r)  -> insert_cycle_event(conn, &r)    [NEW]
validate_hashes
COMMIT
reconstruct_embeddings
record_provenance (includes 3 new table counts)
print_summary (includes 3 new table lines)
```

## Shared Types (Introduced)

| Type | Defined In | Used By |
|------|-----------|---------|
| `GraphEdgeRow` | format.rs | export-functions, import-inserters, import-pipeline |
| `ObservationRow` | format.rs | export-functions, import-inserters, import-pipeline |
| `CycleEventRow` | format.rs | export-functions, import-inserters, import-pipeline |
| `ExportRow::GraphEdge` | format.rs | import-pipeline (ingest_rows match) |
| `ExportRow::Observation` | format.rs | import-pipeline (ingest_rows match) |
| `ExportRow::CycleEvent` | format.rs | import-pipeline (ingest_rows match) |

## Build Order

1. **format-types** (no dependencies on other components)
2. **export-functions** + **import-inserters** (depend on format-types, independent of each other)
3. **import-pipeline** (depends on format-types + import-inserters)
4. **skip-quarantined** (depends on export-functions for signature changes; modifies main.rs CLI)

Wave 1: format-types
Wave 2: export-functions, import-inserters (parallel)
Wave 3: import-pipeline, skip-quarantined (parallel -- different files)
