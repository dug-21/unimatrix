# nxs-008: Test Plan Overview

## Test Strategy

All tests are rooted in the Risk-Based Test Strategy (21 risks, 85 risk tests). Testing follows wave execution order: each wave must pass `cargo build --workspace && cargo test --workspace` before the next wave begins.

### Test Pyramid

| Layer | Count | Purpose |
|-------|-------|---------|
| Unit | ~15 | Enum TryFrom, JSON serde, counter arithmetic, migration_compat deserializers |
| Integration | ~70 | Round-trip CRUD, query semantics, migration data fidelity, GC cascade, MCP parity |
| Static | 3 | Grep for compat references, named_params usage, entry_from_row column-name access |
| Build | 2 | `cargo build --workspace` after each wave, `cargo clippy` |
| Performance | 1 | Migration of 500-row DB within 5s |

### Risk Coverage Mapping

| Risk | Severity | Component Test Plan | Primary Tests |
|------|----------|-------------------|---------------|
| RISK-01 (Migration Data Fidelity) | CRITICAL | migration.md | RT-01 to RT-10 |
| RISK-02 (24-Column Bind Params) | CRITICAL | write-paths.md, schema-ddl.md | RT-11 to RT-17 |
| RISK-03 (SQL Query Semantics) | CRITICAL | read-paths.md | RT-18 to RT-27 |
| RISK-04 (entry_tags Consistency) | CRITICAL | schema-ddl.md, write-paths.md | RT-28 to RT-34 |
| RISK-05 (Compat Removal) | HIGH | compat-removal.md | RT-35 to RT-37 |
| RISK-06 (Cross-Crate Compilation) | HIGH | server-entries.md | RT-38 to RT-40 |
| RISK-07 (Enum-to-Integer Mapping) | HIGH | migration-compat.md, operational-tables.md | RT-41 to RT-45 |
| RISK-08 (JSON Array Deser) | HIGH | operational-tables.md, server-tables.md | RT-46 to RT-50 |
| RISK-09 (PRAGMA FK Side Effects) | HIGH | schema-ddl.md | RT-51 to RT-52 |
| RISK-10 (co_access Staleness) | MEDIUM | read-paths.md | RT-53 to RT-55 |
| RISK-11 (Session GC Cascade) | MEDIUM | operational-tables.md | RT-56 to RT-58 |
| RISK-12 (Signal Drain Parity) | MEDIUM | operational-tables.md | RT-59 to RT-60 |
| RISK-13 (Audit write_in_txn) | MEDIUM | server-tables.md | RT-61 to RT-63 |
| RISK-14 (Agent Capability JSON) | MEDIUM | server-tables.md | RT-64 to RT-66 |
| RISK-15 (Counter Consolidation) | MEDIUM | counters.md | RT-67 to RT-68 |
| RISK-16 (Migration Txn Size) | MEDIUM | migration.md | RT-69 to RT-70 |
| RISK-17 (Time Index Shift) | LOW | read-paths.md | RT-71 |
| RISK-18 (N+1 Elimination) | LOW | read-paths.md | RT-72 |
| RISK-19 (serde_json Dep) | LOW | operational-tables.md | RT-73 |
| RISK-20 (Schema Version) | LOW | migration.md | RT-74 to RT-75 |
| RISK-21 (MCP Tool Parity) | LOW | server-entries.md | RT-76 to RT-85 |

## Integration Harness Plan

### Existing Suites

| Suite | Location | Applicability |
|-------|----------|--------------|
| MCP smoke tests | `product/test/infra-001/suites/` | Run after all waves to verify MCP tool parity (RT-76 to RT-85) |

### New Integration Tests

All new integration tests go in the respective crate's `tests/` directory:

| Area | Location | Risk Tests | Wave |
|------|----------|-----------|------|
| Migration round-trip | `crates/unimatrix-store/tests/migration_v5_to_v6.rs` | RT-01 to RT-10, RT-44, RT-45, RT-69, RT-70, RT-74 | 0 |
| Entry CRUD round-trip | `crates/unimatrix-store/tests/normalized_entries.rs` | RT-11 to RT-14, RT-28 to RT-31 | 1 |
| Query semantics | `crates/unimatrix-store/tests/query_semantics.rs` | RT-18 to RT-27, RT-71 | 1 |
| Tag junction | `crates/unimatrix-store/tests/entry_tags.rs` | RT-32 to RT-34 | 1 |
| Schema verification | `crates/unimatrix-store/tests/schema_v6.rs` | RT-32, RT-51, RT-52, RT-75 | 1 |
| Operational round-trip | `crates/unimatrix-store/tests/operational_tables.rs` | RT-46, RT-53 to RT-60 | 2 |
| Server write paths | `crates/unimatrix-server/tests/normalized_write.rs` | RT-13, RT-34, RT-40 | 1 |
| Server tables | `crates/unimatrix-server/tests/normalized_server_tables.rs` | RT-47 to RT-49, RT-61 to RT-66 | 3 |
| MCP parity | Integration via `product/test/infra-001/` | RT-76 to RT-85 | 5 |

### Test Execution Order

1. **Wave 0**: `cargo test -p unimatrix-store` (migration, counters, migration_compat)
2. **Wave 1**: `cargo test --workspace` (entries, tags, queries, server writes)
3. **Wave 2**: `cargo test -p unimatrix-store` (sessions, injection_log, signals, co_access)
4. **Wave 3**: `cargo test --workspace` (registry, audit, server tables)
5. **Wave 4**: `cargo build --workspace && cargo test --workspace` (compat removed)
6. **Wave 5**: Full test suite + integration smoke tests

## Acceptance Criteria Verification

| AC | Test Plan | Verification |
|----|-----------|-------------|
| AC-01 | write-paths.md, schema-ddl.md | PRAGMA table_info + round-trip |
| AC-02 | schema-ddl.md | PRAGMA + FK cascade + round-trip |
| AC-03 | compat-removal.md | sqlite_master query + grep |
| AC-04 | schema-ddl.md | sqlite_master index query |
| AC-05 | read-paths.md | PRAGMA + round-trip + staleness |
| AC-06 | operational-tables.md | PRAGMA + indexed queries + GC |
| AC-07 | operational-tables.md | PRAGMA + batch insert + scan |
| AC-08 | operational-tables.md | PRAGMA + JSON round-trip + drain |
| AC-09 | server-tables.md | PRAGMA + JSON capability + protection |
| AC-10 | server-tables.md | PRAGMA + JSON target_ids + txn |
| AC-11 | read-paths.md | Code review + query tests |
| AC-12 | read-paths.md | Code review + batch load tests |
| AC-13 | compat-removal.md | Filesystem + build gate |
| AC-14 | migration.md | Schema version + round-trip |
| AC-15 | compat-removal.md | Grep for bincode usage |
| AC-16 | All | Build + test after every wave |
| AC-17 | server-entries.md | MCP smoke tests |
| AC-18 | migration.md | Documentation review |
