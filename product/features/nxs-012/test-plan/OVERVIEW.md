# Test Strategy Overview: nxs-012

## Test Approach

Three tiers: unit tests in `#[cfg(test)]` modules within modified source files, Rust integration tests in `crates/unimatrix-server/tests/`, and Python MCP integration tests via infra-001.

Unit tests cover serialization correctness, field mapping, nullable handling, NaN safety, and format version validation. Rust integration tests cover end-to-end export/import round-trips with real databases and file I/O. MCP integration tests verify the CLI behavior is observable through the server interface (smoke gate).

## Risk-to-Test Mapping

| Risk | Priority | Component(s) | Test Tier | Key Assertions |
|------|----------|---------------|-----------|----------------|
| R-01 NaN/Infinity weight | High | export-functions, format-types | Unit | `Number::from_f64(f64::NAN)` produces 1.0 fallback; INFINITY/NEG_INFINITY also fall back; normal f64 precision preserved |
| R-02 FK-cascade ordering | High | import-pipeline | Integration | After `--force` import, `observation_phase_metrics`, `observation_metrics`, `graph_edges`, `observations`, `cycle_events` all have 0 rows |
| R-03 Duplicate graph_edges | Med | import-inserters | Integration | Plain INSERT on duplicate natural key returns UNIQUE constraint error; transaction rolls back |
| R-04 format_version boundary | Med | import-pipeline | Unit + Integration | v0 reject, v1 accept (0 new counts), v2 accept (all 11 types), v3 reject, v999 reject |
| R-05 ID collision non-force | Med | import-inserters, import-pipeline | Integration | PRIMARY KEY error on duplicate observations.id without --force; --force clears first |
| R-06 CycleEventRow unknown field | Low | format-types | Unit | Deserialize with `goal_embedding: null` key: confirm behavior (accept or reject) |
| R-07 Dangling graph_edges | Low | export-functions, import-inserters | Integration | Round-trip preserves valid edges; dangling edge with nonexistent target imports without error (no FK) |
| R-08 Export ordering | Med | export-functions | Unit | graph_edges sorted by (source_id, target_id, relation_type); observations sorted by id; cycle_events sorted by id |
| R-09 Embedded newlines | Med | export-functions | Unit + Integration | observations.input with literal `\n` serialized as single JSONL line; round-trip preserves content |
| R-10 metadata null vs empty | Med | export-functions, format-types | Unit + Integration | NULL -> JSON null -> None; "" -> JSON "" -> ""; populated string preserved |
| R-11 Old binary + new variant | Low | format-types, import-pipeline | Unit | format_version guard rejects v2 before reaching unknown _table deserialization |
| R-12 Provenance omits counts | Low | import-pipeline | Integration | audit_log detail string includes graph_edges, observations, cycle_events counts |
| R-13 Summary missing lines | Low | import-pipeline | Integration | stderr includes all 3 new table count lines for v2; shows 0 for v1 |
| R-14 Transaction isolation | Med | export-functions | Code review + Integration | All 11 export queries inside single `BEGIN DEFERRED`; integration round-trip produces consistent state |
| R-15 NULL goal_embedding crash | Med | N/A (existing graceful degradation) | Integration | context_briefing with all-NULL goal_embeddings returns valid results (pre-existing path, not new code) |
| R-16 Cascade incompleteness | High | skip-quarantined | Integration | All 5 affected tables checked against skip_ids; zero orphaned rows referencing quarantined entry IDs |
| R-17 Skip-set TOCTOU | High | skip-quarantined | Code review + Integration | skip-set query inside DEFERRED transaction; export file internally consistent |
| R-18 Default path regression | Med | skip-quarantined, export-functions | Integration | Without `--skip-quarantined`, all entries including status=3 exported; row counts match DB |
| R-19 co_access dual-column | High | skip-quarantined | Integration | 4-combination matrix: (a) neither quarantined: present, (b) entry_a quarantined: absent, (c) entry_b quarantined: absent, (d) both: absent |
| R-20 graph_edges dual-column | High | skip-quarantined | Integration | 4-combination matrix: (a) neither quarantined: present, (b) source quarantined: absent, (c) target quarantined: absent, (d) both: absent |
| R-21 Non-entry tables filtered | Low | skip-quarantined | Integration | observations, cycle_events, counters, audit_log, outcome_index, agent_registry row counts match source DB |
| R-22 Skip-count reporting | Low | skip-quarantined | Integration | stderr reports skipped entry count + per-table dependent counts when flag active; absent when inactive |
| R-23 --confirm bypass | Med | skip-quarantined | Unit + Integration | --skip-quarantined without --confirm: non-zero exit, error mentioning --confirm; no file created. All 4 flag combinations tested |
| R-24 Header missing metadata | Low | skip-quarantined, export-functions | Unit + Integration | Header has `skip_quarantined: true` when active; absent/false when inactive |

## Cross-Component Test Dependencies

1. **format-types <-> export-functions**: Export functions serialize data; format-types define the deserialization contract. Round-trip tests validate both together.
2. **format-types <-> import-inserters**: Inserter functions consume deserialized row structs from format-types. Field mismatch (e.g., missing bind) detected by inserter unit tests.
3. **export-functions <-> import-pipeline**: End-to-end round-trip (export from populated DB, import into fresh DB, re-export, diff) validates the full serialization/deserialization contract.
4. **skip-quarantined <-> export-functions**: Skip-quarantined is integrated into export-functions via the `skip_ids` parameter. All 5 affected exporters tested together.
5. **import-pipeline <-> import-inserters**: ingest_rows match arms route to inserter functions. Missing arm = unprocessed rows (silent data loss).

## Integration Harness Plan (infra-001)

### Existing Suites That Cover nxs-012 Behavior

nxs-012 is a CLI-only export/import feature. The MCP JSON-RPC harness exercises the running server, not the CLI subcommands directly. Coverage overlap is limited:

| Suite | Relevant Coverage | Gap |
|-------|-------------------|-----|
| `lifecycle` | Restart persistence tests verify data survives server restart -- indirect export/import validation | No direct export/import CLI testing |
| `tools` | Tool behavior depends on data integrity; if import corrupts data, tool tests fail | Indirect signal only |
| `edge_cases` | Unicode/boundary tests relevant to export content preservation | Content-level, not export-specific |

### Existing Suite Execution Plan

| Gate | Suites to Run | Rationale |
|------|---------------|-----------|
| Smoke (mandatory) | `pytest -m smoke` | Minimum regression gate -- confirms server still operates correctly with schema changes |
| Schema/storage | `lifecycle`, `edge_cases` | nxs-012 modifies `drop_all_data` and adds format_version 2; lifecycle persistence tests confirm no regression |
| Full suite | All suites | Pre-merge validation |

### New Integration Tests Needed

nxs-012 behavior is fully testable through Rust integration tests (`export_integration.rs`, `import_integration.rs`). The MCP harness tests tool-level server behavior, which nxs-012 does not modify. No new infra-001 tests are needed for this feature.

If the infra-001 harness later adds CLI subcommand testing (export/import exercised through the binary), new tests would be appropriate. For now, the Rust integration tests provide complete coverage of the export/import pipeline.

### Rationale for No New infra-001 Tests

1. Export/import are CLI subcommands, not MCP tools -- the harness communicates via JSON-RPC, not CLI flags.
2. All 31 acceptance criteria have verification methods defined as unit or Rust integration tests.
3. The existing Rust integration test files (`export_integration.rs`, `import_integration.rs`) already establish the pattern for end-to-end export/import testing with real databases and file I/O.
4. Adding CLI-level tests to infra-001 would require new harness infrastructure (subprocess invocation of export/import commands) -- this should be a separate infrastructure issue, not feature scope.

## Test Organization

```
crates/unimatrix-server/src/format.rs         -- #[cfg(test)] mod tests (format-types)
crates/unimatrix-server/src/export.rs          -- #[cfg(test)] mod tests (export-functions + skip-quarantined)
crates/unimatrix-server/src/import/inserters.rs -- #[cfg(test)] mod tests (import-inserters)
crates/unimatrix-server/src/import/mod.rs      -- #[cfg(test)] mod tests (import-pipeline)
crates/unimatrix-server/tests/export_integration.rs  -- extended with nxs-012 tests
crates/unimatrix-server/tests/import_integration.rs  -- extended with nxs-012 tests
```

All new tests extend existing files and helpers. No new test files or isolated scaffolding.
