# ADR-008: Wave Ordering and Cross-Crate Synchronization

**Status**: Accepted
**Context**: nxs-008
**Mitigates**: SR-05 (Server Direct Table Access Cross-Crate Coupling)

## Decision

Waves are restructured to ensure both crates compile at every wave boundary. Wave 0 (prep) is added before Wave 1. Each wave includes all server-crate changes for the tables it normalizes.

### Wave Structure

**Wave 0: Migration Infrastructure** (no runtime changes)
- Create `migration_compat.rs` with all bincode deserializers (ADR-005)
- Create `counters.rs` module (ADR-002)
- Write v5-to-v6 migration code in `migration.rs`
- Write migration round-trip tests for every table
- Gate: `cargo test --workspace` passes, migration tests pass on synthetic v5 database

**Wave 1: ENTRIES + entry_tags + Index Elimination**
- Normalize ENTRIES table: 24 SQL columns
- Create `entry_tags` junction table (ADR-006)
- Drop 5 index tables, create SQL indexes
- Rewrite `write.rs` insert/update/delete (named params, ADR-004)
- Rewrite `read.rs` query paths (SQL WHERE, eliminate HashSet intersection)
- Update server files: `store_ops.rs`, `store_correct.rs` (these directly write to ENTRIES, index tables)
- Update server files: `status.rs`, `contradiction.rs` (these read from STATUS_INDEX)
- Enable `PRAGMA foreign_keys = ON` (ADR-006)
- Gate: `cargo build --workspace && cargo test --workspace`

**Wave 2: Store-Crate Operational Tables**
- Normalize: CO_ACCESS, SESSIONS, INJECTION_LOG, SIGNAL_QUEUE
- Add `session_id` indexed column to INJECTION_LOG (replaces full-table scan in GC)
- Add `feature_cycle` indexed column to SESSIONS (replaces full-table scan)
- Rewrite: `sessions.rs`, `injection_log.rs`, `signal.rs`, `write_ext.rs`
- Update migration to handle these tables
- Gate: `cargo build --workspace && cargo test --workspace`

**Wave 3: Server-Crate Tables**
- Normalize: AGENT_REGISTRY, AUDIT_LOG
- Move `AgentRecord` and `AuditEvent` types to store crate (or keep in server with migration using intermediate format)
- Rewrite: `registry.rs`, `audit.rs` to use direct SQL with named params
- Update migration to handle these tables
- Gate: `cargo build --workspace && cargo test --workspace`

**Wave 4: Compat Layer Removal + Cleanup**
- Delete: `handles.rs`, `dispatch.rs`
- Gut: `tables.rs` (remove all table constants, marker types, guard types)
- Simplify: `txn.rs` (remove `SqliteReadTransaction`, column-mapping functions)
- Remove: `Store::begin_read()`, all `open_table`/`open_multimap_table` methods
- Remove: runtime bincode serialize functions for normalized tables
- Clean: `lib.rs` re-exports (remove compat layer exports)
- Gate: `cargo build --workspace && cargo test --workspace`, no references to deleted types

**Wave 5: Verification**
- All 12 MCP tools produce identical results (behavioral parity)
- No bincode serialization remains for normalized tables (OBSERVATION_METRICS excluded)
- Schema version is 6
- Migration from synthetic v5 database produces identical query results

### Compilation Gate Protocol

Every wave must pass `cargo build --workspace` before the next wave begins. This is non-negotiable because store and server crates have tight coupling via re-exports and type imports.

## Consequences

- Wave 0 front-loads migration risk (SR-01) before any runtime changes
- Waves 1-3 never use compat types for new code (per SR-04 resolution: bypass, not build-on)
- Wave 4 only deletes dead code — no second rewrite
- Server changes land in the same wave as the store changes they depend on (SR-05)
