# nxs-008: Pseudocode Overview

## Component Interaction

The schema normalization proceeds in 6 waves (0-5). Each wave updates both store and server crates atomically, verified by `cargo build --workspace && cargo test --workspace`.

### Data Flow

```
Wave 0: Create counters.rs, migration_compat.rs, extend migration.rs
  - No runtime behavior changes
  - Migration tested against synthetic v5 databases

Wave 1: ENTRIES decomposition + entry_tags + index elimination
  - db.rs: New DDL (24-col entries, entry_tags, indexes, PRAGMA FK ON)
  - write.rs: Named-params INSERT/UPDATE/DELETE, entry_tags loop, counter calls
  - read.rs: SQL WHERE builder, entry_from_row(), load_tags_for_entries()
  - schema.rs: Remove runtime serialize/deserialize re-exports
  - Server: store_ops.rs, store_correct.rs, status.rs, contradiction.rs rewrite

Wave 2: Store operational tables
  - co_access: SQL columns (count, last_updated) replace blob
  - sessions: 9 SQL columns, indexed feature_cycle + started_at
  - injection_log: 5 SQL columns, indexed session_id + entry_id
  - signal_queue: 6 SQL columns, entry_ids as JSON TEXT

Wave 3: Server tables
  - Move AgentRecord, AuditEvent, TrustLevel, Capability, Outcome to store::schema
  - agent_registry: 8 SQL columns, JSON arrays for capabilities/allowed_*
  - audit_log: 8 SQL columns, JSON array for target_ids

Wave 4: Compat removal
  - Delete handles.rs, dispatch.rs, tables.rs
  - Simplify txn.rs (remove SqliteReadTransaction, mapping fns)
  - Clean lib.rs re-exports

Wave 5: Verification
  - Full acceptance criteria validation
  - MCP tool behavioral parity
```

### Shared Types

All components share:
- `EntryRecord` (unchanged public API, 24 fields)
- `Status` enum with `#[repr(u8)]` and `TryFrom<u8>`
- `NewEntry` input struct
- `QueryFilter` / `TimeRange` query types
- `CoAccessRecord` (count, last_updated)
- `StoreError` / `Result` error types

### Cross-Component Dependencies

| Producer | Consumer | Shared Artifact |
|----------|----------|----------------|
| counters | write-paths, server-entries, migration | Counter helper functions |
| schema-ddl | All waves 1+ | DDL, entry_from_row, load_tags_for_entries |
| migration-compat | migration | Bincode deserializers |
| migration | Wave 1+ (schema version gate) | v5-to-v6 migration function |
| write-paths | server-entries | Insert/update via direct SQL |
| read-paths | server-entries (status, contradiction) | SQL query patterns |
| operational-tables | server-tables (audit write_in_txn) | Session/injection log SQL |

### Integration Harness Plan

**Existing suites that apply:**
- `product/test/infra-001/suites/` - MCP tool integration tests
- Smoke tests via `pytest -m smoke`

**New integration tests needed:**
1. Migration round-trip tests (synthetic v5 DB -> v6 -> verify all data)
2. Query semantic parity tests (all 5 filter dimensions)
3. Entry CRUD round-trip with all 24 fields
4. Tag AND semantics preservation
5. Cross-crate write path parity (Store::insert vs store_ops insert)
6. GC cascade tests (sessions -> injection_log)
7. Signal queue drain with JSON entry_ids
8. Agent registry JSON capability round-trip
9. Audit log transaction participation tests
10. Behavioral parity across all 12 MCP tools

## Component List

| # | Component | Wave | Pseudocode | Test Plan |
|---|-----------|------|-----------|-----------|
| 1 | counters | 0 | pseudocode/counters.md | test-plan/counters.md |
| 2 | migration-compat | 0 | pseudocode/migration-compat.md | test-plan/migration-compat.md |
| 3 | migration | 0 | pseudocode/migration.md | test-plan/migration.md |
| 4 | schema-ddl | 1 | pseudocode/schema-ddl.md | test-plan/schema-ddl.md |
| 5 | write-paths | 1 | pseudocode/write-paths.md | test-plan/write-paths.md |
| 6 | read-paths | 1 | pseudocode/read-paths.md | test-plan/read-paths.md |
| 7 | server-entries | 1 | pseudocode/server-entries.md | test-plan/server-entries.md |
| 8 | operational-tables | 2 | pseudocode/operational-tables.md | test-plan/operational-tables.md |
| 9 | server-tables | 3 | pseudocode/server-tables.md | test-plan/server-tables.md |
| 10 | compat-removal | 4 | pseudocode/compat-removal.md | test-plan/compat-removal.md |

## Patterns Used

- **Transaction RAII**: SqliteWriteTransaction BEGIN/COMMIT/ROLLBACK-on-drop (kept per ADR-001)
- **Named params**: `rusqlite::named_params!{}` for all 4+ column SQL (ADR-004)
- **Column-by-name reads**: `row.get::<_, T>("column_name")` not positional
- **JSON serialization**: `serde_json::to_string` / `serde_json::from_str` for Vec columns (ADR-007)
- **Enum INTEGER**: `#[repr(u8)]` discriminants stored as INTEGER (ADR-003)
- **FK CASCADE**: `PRAGMA foreign_keys = ON` + `ON DELETE CASCADE` on entry_tags (ADR-006)
- **Create-new-then-swap**: Migration creates `_v6` tables, migrates data, drops old, renames (ADR-005)
