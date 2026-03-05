# Implementation Brief: col-012 Data Path Unification

## Summary

Eliminate JSONL observation files by persisting all hook events in a new SQLite `observations` table (schema v7). Migrate the retrospective pipeline to read from SQL via an `ObservationSource` trait. Remove JSONL write/parse infrastructure. Net code reduction.

## Resolved Decisions

| Decision | Resolution | ADR |
|----------|-----------|-----|
| Primary key for observations table | AUTOINCREMENT integer (avoids ms-collision risk) | ADR-001 (#382) |
| unimatrix-observe independence | ObservationSource trait in observe, impl in server | ADR-002 (#383) |
| Silent event loss when server down | Accepted; existing behavior + EventQueue retry | ADR-003 (#384) |
| session_hash vs session_id | Dropped session_hash; use session_id TEXT with index | ADR-001 (#382) |
| Dual-write period | None; single cutover | Human decision |
| Historical JSONL migration | None; clean break | Human decision |

## Implementation Waves

### Wave 1: Schema + Event Persistence (~80 lines)

**Crate**: unimatrix-store + unimatrix-server

1. Add `observations` table to `create_tables()` in `crates/unimatrix-store/src/db.rs`
2. Add v6->v7 migration step in `crates/unimatrix-store/src/migration.rs`
3. Update `CURRENT_SCHEMA_VERSION` to 7
4. Modify `RecordEvent` handler in `crates/unimatrix-server/src/uds/listener.rs`:
   - Extract fields from `ImplantEvent.payload`
   - Insert into observations table via `spawn_blocking`
5. Modify `RecordEvents` handler for batch insert (single transaction)

**Key risks**: R-01 (field extraction mapping), R-02 (migration), R-06 (timestamp overflow)
**Tests**: Migration integration test, RecordEvent persistence test, batch insert test

### Wave 2: ObservationSource Trait + SQL Implementation (~100 lines)

**Crate**: unimatrix-observe + unimatrix-server

1. Add `source.rs` module to `crates/unimatrix-observe/src/` defining `ObservationSource` trait
2. Update `crates/unimatrix-observe/src/lib.rs` to export the trait
3. Add `SqlObservationSource` in `crates/unimatrix-server/src/services/` (or nearby)
4. Implement 3 trait methods: `load_feature_observations`, `discover_sessions_for_feature`, `observation_stats`
5. Map observation rows to `ObservationRecord` (including SubagentStart normalization)

**Key risks**: R-03 (mapping fidelity), R-05 (NULL feature_cycle), R-10 (input type mismatch)
**Tests**: Round-trip test, NULL feature_cycle test, input deserialization test

### Wave 3: Retrospective Pipeline Migration (~60 lines)

**Crate**: unimatrix-server

1. Modify `context_retrospective` in `crates/unimatrix-server/src/mcp/tools.rs`:
   - Replace `discover_sessions()` + `parse_session_file()` + `attribute_sessions()` with `source.load_feature_observations()`
   - Wire `SqlObservationSource` into the tool handler
2. Modify `context_status` in `crates/unimatrix-server/src/services/status.rs`:
   - Replace `scan_observation_stats()` with `source.observation_stats()`
   - Update status response fields (ObservationStats revision)
3. Update retention cleanup to DELETE from observations table instead of file removal

**Key risks**: R-09 (status response fields), R-05 (NULL sessions)
**Tests**: Full retrospective integration test, status response test

### Wave 4: JSONL Removal (~-120 lines)

**Crate**: unimatrix-observe + shell hooks

1. Remove or gut `crates/unimatrix-observe/src/parser.rs` (keep `parse_timestamp` if referenced elsewhere)
2. Remove `crates/unimatrix-observe/src/files.rs`
3. Update `crates/unimatrix-observe/src/lib.rs` to remove JSONL re-exports
4. Remove `SessionFile` type if no longer referenced
5. Simplify shell hooks in `.claude/hooks/observe-*.sh`:
   - Remove JSONL file write lines
   - Keep UDS forwarding
6. Review `attribution.rs` -- retain utility functions, remove from primary pipeline path

**Key risks**: R-08 (hook script breakage)
**Tests**: Compile check (removed modules no longer imported), hook script manual review

## Files to Modify

| File | Change Type | Wave |
|------|------------|------|
| `crates/unimatrix-store/src/db.rs` | Add observations table to create_tables | 1 |
| `crates/unimatrix-store/src/migration.rs` | Add v6->v7 migration, bump version | 1 |
| `crates/unimatrix-server/src/uds/listener.rs` | RecordEvent/RecordEvents handlers | 1 |
| `crates/unimatrix-observe/src/source.rs` | NEW: ObservationSource trait | 2 |
| `crates/unimatrix-observe/src/lib.rs` | Export source module | 2, 4 |
| `crates/unimatrix-observe/src/types.rs` | Revise ObservationStats | 2 |
| `crates/unimatrix-server/src/services/` | NEW: SqlObservationSource | 2 |
| `crates/unimatrix-server/src/mcp/tools.rs` | Rewire context_retrospective | 3 |
| `crates/unimatrix-server/src/services/status.rs` | Rewire observation stats | 3 |
| `crates/unimatrix-observe/src/parser.rs` | REMOVE (or gut) | 4 |
| `crates/unimatrix-observe/src/files.rs` | REMOVE | 4 |
| `.claude/hooks/observe-pre-tool.sh` | Remove JSONL writes | 4 |
| `.claude/hooks/observe-post-tool.sh` | Remove JSONL writes | 4 |
| `.claude/hooks/observe-subagent-start.sh` | Remove JSONL writes | 4 |
| `.claude/hooks/observe-subagent-stop.sh` | Remove JSONL writes | 4 |

## Estimated Scope

- Lines added: ~240 (schema, persistence, trait, impl, migration)
- Lines removed: ~360 (parser.rs, files.rs, JSONL hook writes, status file scanning)
- Net change: ~-120 lines (net reduction)
- Test lines: ~200 new test lines

## Vision Alignment

PASS -- 0 variances. Feature is exactly positioned in the product vision dependency graph.
