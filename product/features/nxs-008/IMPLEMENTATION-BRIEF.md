# nxs-008: Schema Normalization — Implementation Brief

**Feature**: nxs-008
**Date**: 2026-03-05
**Status**: Ready for Implementation
**Predecessor**: nxs-007 (redb removal)

---

## Source Documents

| Document | Path | Purpose |
|----------|------|---------|
| SCOPE | `product/features/nxs-008/SCOPE.md` | Goals, acceptance criteria, constraints |
| Scope Risk Assessment | `product/features/nxs-008/SCOPE-RISK-ASSESSMENT.md` | 8 scope risks (SR-01 through SR-08) |
| Specification | `product/features/nxs-008/specification/SPECIFICATION.md` | Domain models, query semantics, write paths, migration spec |
| Architecture | `product/features/nxs-008/architecture/ARCHITECTURE.md` | Target schema, wave plan, integration surface |
| Risk Test Strategy | `product/features/nxs-008/RISK-TEST-STRATEGY.md` | 21 risks, 85 risk tests (RT-01 through RT-85) |
| Alignment Report | `product/features/nxs-008/ALIGNMENT-REPORT.md` | 3 PASS, 2 WARN, 0 VARIANCE, 0 FAIL |
| ADR-001 | `architecture/ADR-001-txn-wrappers.md` | Keep SqliteWriteTransaction, remove SqliteReadTransaction |
| ADR-002 | `architecture/ADR-002-counter-helpers.md` | Counter helpers move to counters.rs |
| ADR-003 | `architecture/ADR-003-enum-storage-integer.md` | Enum storage as INTEGER with repr(u8) |
| ADR-004 | `architecture/ADR-004-named-params.md` | Mandatory named_params!{} for multi-column SQL |
| ADR-005 | `architecture/ADR-005-migration-compat-module.md` | migration_compat.rs for bincode deserializers |
| ADR-006 | `architecture/ADR-006-entry-tags-cascade.md` | entry_tags junction table with FK CASCADE |
| ADR-007 | `architecture/ADR-007-json-vec-columns.md` | JSON array columns for non-queried Vec fields |
| ADR-008 | `architecture/ADR-008-wave-ordering.md` | Wave ordering and cross-crate synchronization |

---

## Goal

Decompose 7 tables from bincode blobs to SQL columns, eliminate 5 manual index tables (replaced by SQL `CREATE INDEX`), remove the redb-pattern compat layer (`handles.rs`, `dispatch.rs`), and replace client-side HashSet intersection query logic with SQL WHERE clauses. Schema version advances v5 to v6. Store public API (`EntryRecord`, `Store` methods) is unchanged. All 12 MCP tools produce identical results. Net result: ~-800 lines, SQL-native queries, and future field additions via `ALTER TABLE ADD COLUMN`.

---

## Resolved Decisions

| Decision | Resolution | ADR | Resolves |
|----------|-----------|-----|----------|
| txn.rs fate (Open Q #1) | Keep `SqliteWriteTransaction` (RAII safety); remove `SqliteReadTransaction` | [ADR-001](architecture/ADR-001-txn-wrappers.md) | SR-04 |
| Counter helpers location (Open Q #2) | New `counters.rs` module; all functions take `&Connection` | [ADR-002](architecture/ADR-002-counter-helpers.md) | SR-04 |
| Enum storage format (Open Q #3) | INTEGER using `#[repr(u8)]` discriminants | [ADR-003](architecture/ADR-003-enum-storage-integer.md) | SR-04, SR-07 |
| Bind parameter style | Mandatory `named_params!{}` for 4+ params | [ADR-004](architecture/ADR-004-named-params.md) | SR-02 |
| Bincode deserializer retention | `migration_compat.rs` module in store crate | [ADR-005](architecture/ADR-005-migration-compat-module.md) | SR-01 |
| Tag storage and FK | `entry_tags` junction + `PRAGMA foreign_keys = ON` + CASCADE | [ADR-006](architecture/ADR-006-entry-tags-cascade.md) | SR-08 |
| Vec field storage | JSON TEXT columns for non-queried Vec fields | [ADR-007](architecture/ADR-007-json-vec-columns.md) | SR-06 |
| Wave ordering | 6 waves (0-5); each wave includes both crates | [ADR-008](architecture/ADR-008-wave-ordering.md) | SR-05 |

---

## Component Map

### Wave 0: Migration Infrastructure

| Component | File | Action | Risk | Session 2 |
|-----------|------|--------|------|-----------|
| Counter module | `crates/unimatrix-store/src/counters.rs` | CREATE | Low | pseudocode + tests |
| Migration compat | `crates/unimatrix-store/src/migration_compat.rs` | CREATE | Low | pseudocode + tests |
| v5-to-v6 migration | `crates/unimatrix-store/src/migration.rs` | EXTEND | HIGH (SR-01) | pseudocode + tests |
| Module declarations | `crates/unimatrix-store/src/lib.rs` | MODIFY | Low | — |

### Wave 1: ENTRIES + entry_tags + Index Elimination

| Component | File | Action | Risk | Session 2 |
|-----------|------|--------|------|-----------|
| Schema DDL (24 cols) | `crates/unimatrix-store/src/db.rs` | REWRITE | Medium | pseudocode |
| Insert/update/delete paths | `crates/unimatrix-store/src/write.rs` | REWRITE | HIGH (SR-02) | pseudocode + tests |
| Query paths + entry_from_row | `crates/unimatrix-store/src/read.rs` | REWRITE | HIGH (SR-03) | pseudocode + tests |
| EntryRecord serde removal | `crates/unimatrix-store/src/schema.rs` | MODIFY | Medium | — |
| Server store ops | `crates/unimatrix-server/src/services/store_ops.rs` | REWRITE | HIGH (SR-02) | pseudocode |
| Server store correct | `crates/unimatrix-server/src/services/store_correct.rs` | REWRITE | HIGH (SR-02) | pseudocode |
| Server status | `crates/unimatrix-server/src/services/status.rs` | MODIFY | Low | — |
| Server contradiction | `crates/unimatrix-server/src/infra/contradiction.rs` | MODIFY | Low | — |
| Foreign keys pragma | `crates/unimatrix-store/src/db.rs` | MODIFY | Low | — |

### Wave 2: Store-Crate Operational Tables

| Component | File | Action | Risk | Session 2 |
|-----------|------|--------|------|-----------|
| Schema DDL (4 tables) | `crates/unimatrix-store/src/db.rs` | EXTEND | Low | pseudocode |
| CO_ACCESS normalization | `crates/unimatrix-store/src/write_ext.rs` | REWRITE | Medium | pseudocode |
| CO_ACCESS reads | `crates/unimatrix-store/src/read.rs` | MODIFY | Medium | pseudocode |
| Sessions normalization | `crates/unimatrix-store/src/sessions.rs` | REWRITE | Medium | pseudocode + tests |
| Injection log normalization | `crates/unimatrix-store/src/injection_log.rs` | REWRITE | Low | pseudocode |
| Signal queue normalization | `crates/unimatrix-store/src/signal.rs` | REWRITE | Medium | pseudocode + tests |

### Wave 3: Server-Crate Tables

| Component | File | Action | Risk | Session 2 |
|-----------|------|--------|------|-----------|
| Type movement to store | `crates/unimatrix-store/src/schema.rs` | EXTEND | Medium | — |
| Agent registry normalization | `crates/unimatrix-server/src/infra/registry.rs` | REWRITE | Medium | pseudocode + tests |
| Audit log normalization | `crates/unimatrix-server/src/infra/audit.rs` | REWRITE | Medium | pseudocode + tests |
| Server schema init | `crates/unimatrix-server/src/server.rs` | MODIFY | Low | — |

### Wave 4: Compat Layer Removal + Cleanup

| Component | File | Action | Risk | Session 2 |
|-----------|------|--------|------|-----------|
| Typed handles | `crates/unimatrix-store/src/handles.rs` | DELETE | Low | — |
| Dispatch traits | `crates/unimatrix-store/src/dispatch.rs` | DELETE | Low | — |
| Table constants | `crates/unimatrix-store/src/tables.rs` | DELETE | Low | — |
| Transaction wrappers | `crates/unimatrix-store/src/txn.rs` | SIMPLIFY | Low | — |
| Re-exports cleanup | `crates/unimatrix-store/src/lib.rs` | MODIFY | Low | — |
| Runtime bincode removal | multiple | MODIFY | Low | — |

### Wave 5: Verification

| Check | Method |
|-------|--------|
| AC-01 through AC-18 | Full acceptance map verification |
| 85 risk tests | RT-01 through RT-85 |
| Behavioral parity | 12 MCP tool integration tests |

---

## Files to Create

| File | Wave | Purpose | Summary |
|------|------|---------|---------|
| `crates/unimatrix-store/src/counters.rs` | 0 | Counter helpers | 5 functions (`read_counter`, `set_counter`, `increment_counter`, `decrement_counter`, `next_entry_id`), all taking `&Connection`. Consolidates from tables.rs + write.rs. ~60 lines. |
| `crates/unimatrix-store/src/migration_compat.rs` | 0 | Migration deserializers | 7 `deserialize_*_v5()` functions for v5 bincode blobs. Created before runtime bincode removal. ~100 lines. |

## Files to Modify

| File | Wave | Summary |
|------|------|---------|
| `crates/unimatrix-store/src/db.rs` | 0-3 | Replace all `CREATE TABLE` DDL with normalized schemas. Enable `PRAGMA foreign_keys = ON`. Add entry_tags, new indexes. Drop 5 index tables from DDL. |
| `crates/unimatrix-store/src/migration.rs` | 0 | Add `migrate_v5_to_v6()`: backup database, create new tables, deserialize all blobs via migration_compat, INSERT as SQL columns, drop old tables, rename, create indexes, set schema_version=6. |
| `crates/unimatrix-store/src/write.rs` | 1 | Rewrite insert (24-col named_params INSERT + entry_tags loop), update (24-col UPDATE + tag replace), delete (CASCADE-aware), update_status (direct column UPDATE). Remove diff-based index sync. Replace internal counter helpers with counters.rs calls. |
| `crates/unimatrix-store/src/read.rs` | 1-2 | Rewrite query() with SQL WHERE clause builder (dynamic filters). Add `entry_from_row()` helper. Add `load_tags_for_entries()` helper. Eliminate HashSet intersection and N+1 fetch. Rewrite co_access reads with SQL WHERE. |
| `crates/unimatrix-store/src/schema.rs` | 1, 3 | Remove `serialize_entry`/`deserialize_entry` from runtime paths. Add server-crate types (`AgentRecord`, `AuditEvent`, `TrustLevel`, `Capability`, `Outcome`). Add `TryFrom<u8>` for enums missing it. |
| `crates/unimatrix-store/src/write_ext.rs` | 2 | Rewrite co_access operations: replace blob serialize/deserialize with SQL column read/write. Use `named_params!{}` for INSERT/UPDATE. |
| `crates/unimatrix-store/src/sessions.rs` | 2 | Rewrite all session operations: INSERT/UPDATE/SELECT with 9 SQL columns. Replace full-table scan with indexed `feature_cycle` and `started_at` queries. GC cascade uses `DELETE FROM injection_log WHERE session_id IN (...)`. |
| `crates/unimatrix-store/src/injection_log.rs` | 2 | Rewrite: 5 SQL columns. Indexed `session_id` for GC cascade. Indexed `entry_id` for future analytics. |
| `crates/unimatrix-store/src/signal.rs` | 2 | Rewrite: 6 SQL columns. `entry_ids` as JSON TEXT via serde_json. Drain uses `WHERE signal_type = ?`. |
| `crates/unimatrix-store/src/txn.rs` | 4 | Remove `SqliteReadTransaction`. Remove `primary_key_column()`/`data_column()` helpers. Keep `SqliteWriteTransaction` with RAII semantics. |
| `crates/unimatrix-store/src/lib.rs` | 0, 3, 4 | Add `counters` and `migration_compat` module declarations. Re-export moved types. Remove `handles`, `dispatch`, `tables` module declarations and compat re-exports. |
| `crates/unimatrix-server/src/services/store_ops.rs` | 1 | Rewrite entry creation: direct SQL INSERT with 24 named params + entry_tags INSERT. Remove index table writes. Use counters.rs for next_entry_id. |
| `crates/unimatrix-server/src/services/store_correct.rs` | 1 | Rewrite correction: SQL UPDATE for deprecation + SQL INSERT for replacement entry. Remove multi-table index manipulation. |
| `crates/unimatrix-server/src/services/status.rs` | 1 | Replace index-table scans with `SELECT COUNT(*) FROM entries WHERE status = ?`. |
| `crates/unimatrix-server/src/infra/contradiction.rs` | 1 | Replace STATUS_INDEX scan + blob deserialize with `SELECT ... FROM entries WHERE status = ?`. |
| `crates/unimatrix-server/src/infra/registry.rs` | 3 | Rewrite all agent registry operations: SQL INSERT/UPDATE/SELECT with 8 columns. JSON serde for capabilities, allowed_topics, allowed_categories. |
| `crates/unimatrix-server/src/infra/audit.rs` | 3 | Rewrite all audit operations: SQL INSERT/SELECT with 8 columns. JSON serde for target_ids. `write_count_since` uses indexed `agent_id + timestamp` query. |
| `crates/unimatrix-store/Cargo.toml` | 2 | Add `serde_json` as direct dependency (already transitive). |

## Files to Delete

| File | Wave | Lines | Reason |
|------|------|-------|--------|
| `crates/unimatrix-store/src/handles.rs` | 4 | ~428 | Typed table handle wrappers — dead code after SQL normalization |
| `crates/unimatrix-store/src/dispatch.rs` | 4 | ~134 | TableSpec/MultimapSpec traits — dead code after SQL normalization |
| `crates/unimatrix-store/src/tables.rs` | 4 | ~182 | Table constants, marker types, guard types — all replaced |

---

## Data Structures

### EntryRecord (unchanged public API)

```rust
pub struct EntryRecord {
    pub id: u64,                         // INTEGER PK (stored as i64)
    pub title: String,                   // TEXT NOT NULL
    pub content: String,                 // TEXT NOT NULL
    pub topic: String,                   // TEXT NOT NULL, indexed
    pub category: String,               // TEXT NOT NULL, indexed
    pub tags: Vec<String>,              // entry_tags junction table
    pub source: String,                  // TEXT NOT NULL
    pub status: Status,                  // INTEGER NOT NULL (repr(u8)), indexed
    pub confidence: f64,                 // REAL NOT NULL DEFAULT 0.0
    pub created_at: u64,                // INTEGER NOT NULL, indexed
    pub updated_at: u64,                // INTEGER NOT NULL
    pub last_accessed_at: u64,          // INTEGER NOT NULL DEFAULT 0
    pub access_count: u32,              // INTEGER NOT NULL DEFAULT 0
    pub supersedes: Option<u64>,        // INTEGER (nullable)
    pub superseded_by: Option<u64>,     // INTEGER (nullable)
    pub correction_count: u32,          // INTEGER NOT NULL DEFAULT 0
    pub embedding_dim: u16,             // INTEGER NOT NULL DEFAULT 0
    pub created_by: String,             // TEXT NOT NULL DEFAULT ''
    pub modified_by: String,            // TEXT NOT NULL DEFAULT ''
    pub content_hash: String,           // TEXT NOT NULL DEFAULT ''
    pub previous_hash: String,          // TEXT NOT NULL DEFAULT ''
    pub version: u32,                   // INTEGER NOT NULL DEFAULT 0
    pub feature_cycle: String,          // TEXT NOT NULL DEFAULT ''
    pub trust_source: String,           // TEXT NOT NULL DEFAULT ''
    pub helpful_count: u32,             // INTEGER NOT NULL DEFAULT 0
    pub unhelpful_count: u32,           // INTEGER NOT NULL DEFAULT 0
}
```

### Types Moved from Server to Store

```rust
// AgentRecord, TrustLevel, Capability — from registry.rs
// AuditEvent, Outcome — from audit.rs
// These types move to crates/unimatrix-store/src/schema.rs
// Server crate re-imports them
```

---

## Key Function Signatures

### counters.rs (new)

```rust
pub(crate) fn read_counter(conn: &Connection, name: &str) -> Result<u64>;
pub(crate) fn set_counter(conn: &Connection, name: &str, value: u64) -> Result<()>;
pub(crate) fn increment_counter(conn: &Connection, name: &str, delta: u64) -> Result<()>;
pub(crate) fn decrement_counter(conn: &Connection, name: &str, delta: u64) -> Result<()>;
pub(crate) fn next_entry_id(conn: &Connection) -> Result<u64>;
```

### read.rs (rewritten helpers)

```rust
/// Construct EntryRecord from SQLite row (column-by-name access).
/// Tags are set to vec![] — caller must use load_tags_for_entries().
fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord>;

/// Batch-load tags for multiple entries. Returns map of entry_id -> Vec<tag>.
/// Every code path constructing EntryRecord MUST call this.
fn load_tags_for_entries(conn: &Connection, ids: &[u64]) -> Result<HashMap<u64, Vec<String>>>;
```

### migration_compat.rs (new)

```rust
pub(crate) fn deserialize_entry_v5(bytes: &[u8]) -> Result<EntryRecord>;
pub(crate) fn deserialize_co_access_v5(bytes: &[u8]) -> Result<CoAccessRecord>;
pub(crate) fn deserialize_session_v5(bytes: &[u8]) -> Result<SessionRecord>;
pub(crate) fn deserialize_injection_log_v5(bytes: &[u8]) -> Result<InjectionLogRecord>;
pub(crate) fn deserialize_signal_v5(bytes: &[u8]) -> Result<SignalRecord>;
pub(crate) fn deserialize_agent_v5(bytes: &[u8]) -> Result<AgentRecord>;
pub(crate) fn deserialize_audit_event_v5(bytes: &[u8]) -> Result<AuditEvent>;
```

### migration.rs (extended)

```rust
/// Migrate database from schema v5 (bincode blobs) to v6 (SQL columns).
/// Creates backup at {path}.v5-backup before starting.
/// Runs in single transaction: create new tables, migrate data, drop old, rename, index.
pub(crate) fn migrate_v5_to_v6(conn: &Connection, db_path: &Path) -> Result<()>;
```

---

## Query Semantics (Must Preserve Exactly)

| Semantic | Current Behavior | New SQL Equivalent |
|----------|-----------------|-------------------|
| Tag filtering | AND across tags (HashSet intersection) | `GROUP BY entry_id HAVING COUNT(DISTINCT tag) = :tag_count` |
| Empty filter | Default to Status::Active | Add `WHERE status = 0` when no filters set |
| Empty tags list | Skip tag filter | Omit tag subquery when `tags.is_empty()` |
| Invalid time range | Return empty when `start > end` | Guard in Rust before SQL query |
| Multi-filter | Intersection (AND across dimensions) | Multiple AND clauses in WHERE |
| Status as integer | `status as u8 as i64` | `WHERE status = :status` (same integer) |
| NULL options | bincode None → deserialized None | SQL NULL → `row.get::<_, Option<i64>>()` |

---

## Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | nxs-007 must be merged before implementation | SCOPE prerequisite |
| C-02 | Store public API unchanged | SCOPE non-goal |
| C-03 | Behavioral parity — all 12 MCP tools identical | AC-17 |
| C-04 | Test infrastructure is cumulative | CLAUDE.md |
| C-05 | One-way migration — create new tables before dropping old | SR-01 |
| C-06 | Migration code written before bincode removal | SR-01 ordering |
| C-07 | `named_params!{}` for all 4+ param SQL | ADR-004 |
| C-08 | `PRAGMA foreign_keys = ON` | ADR-006 |
| C-09 | entry_tags `ON DELETE CASCADE` | ADR-006 |
| C-10 | `load_tags_for_entries` in every EntryRecord construction | ADR-006 |
| C-11 | Each wave includes both store and server crate changes | ADR-008 |
| C-12 | `cargo build --workspace && cargo test --workspace` gate per wave | ADR-008 |
| C-13 | Round-trip test with all 24 fields distinct non-default | SR-02 |
| C-14 | Tag AND semantics preserved | SR-03 |
| C-15 | Empty QueryFilter defaults to Active | SR-03 |
| C-16 | Database backup before v5-to-v6 migration | SR-01 |
| C-17 | Waves 1-3 bypass compat types — direct SQL | ADR-008 |
| C-18 | `serde_json` for JSON array columns | ADR-007 |

---

## Dependencies

| Dependency | Status | Impact |
|-----------|--------|--------|
| nxs-007 (redb removal) | Must be merged | Prerequisite |
| rusqlite | Already in Cargo.toml | No change |
| serde_json | Add to unimatrix-store | JSON array columns (Wave 2) |
| bincode | Remains in Cargo.toml | migration_compat + OBSERVATION_METRICS |

---

## NOT in Scope

- OBSERVATION_METRICS normalization (ADR #354 — stays bincode)
- VECTOR_MAP changes (simple KV, already normalized)
- HNSW/vector index changes
- Store public API changes (EntryRecord, Store method signatures)
- New MCP tools or behavioral changes
- Server decoupling (ADR #352 — accepted coupling)
- New tables beyond entry_tags (which replaces TAG_INDEX)

---

## Alignment Status

From ALIGNMENT-REPORT.md:

| Dimension | Verdict |
|-----------|---------|
| Feature Goals vs Vision Principles | **PASS** |
| Architecture vs Existing Patterns/ADRs | **WARN** (WARN-01) |
| Scope Creep | **WARN** (WARN-02, WARN-03) |
| Risk Strategy Coverage | **PASS** |
| Non-Goals Respected | **PASS** |

**WARNs (no human approval required)**:
- **WARN-01**: Spec AD-03 hedges on counter helpers location vs Architecture ADR-002. Resolution: follow Architecture — create `counters.rs`.
- **WARN-02**: 6 additional SQL indexes beyond SCOPE.md (entry_tags entry_id, co_access_b, sessions started_at/feature_cycle, injection_log entry, audit_log agent/timestamp). Additive performance infrastructure within feature boundary.
- **WARN-03**: Type movement (AgentRecord, AuditEvent, TrustLevel, Capability, Outcome) from server to store crate. Necessary for migration; consistent with ADR #352.

**No variances. No FAIL.**

---

## Lines Impact Estimate

| Metric | Estimate |
|--------|----------|
| Lines deleted | ~1,200 (handles.rs, dispatch.rs, tables.rs, index sync, bincode helpers) |
| Lines added | ~400 (migration_compat, counters, DDL, entry_from_row, load_tags) |
| Lines rewritten | ~2,000 (read/write paths, sessions, signal, injection_log, write_ext, store_ops, store_correct, registry, audit) |
| Net change | ~-800 lines |

---

## Risk Summary

| Risk | Severity | ADR | Primary Tests |
|------|----------|-----|---------------|
| RISK-01: Migration data fidelity | CRITICAL | ADR-005 | RT-01–RT-10 |
| RISK-02: 24-column bind params | CRITICAL | ADR-004 | RT-11–RT-17 |
| RISK-03: SQL query semantic equivalence | CRITICAL | — | RT-18–RT-27 |
| RISK-04: entry_tags consistency | CRITICAL | ADR-006 | RT-28–RT-34 |
| RISK-05: Compat removal dangling refs | HIGH | ADR-001/002/003 | RT-35–RT-37 |
| RISK-06: Cross-crate compilation | HIGH | ADR-008 | RT-38–RT-40 |
| RISK-07: Enum integer mapping | HIGH | ADR-003 | RT-41–RT-45 |
| RISK-08: JSON array deserialization | HIGH | ADR-007 | RT-46–RT-50 |
| RISK-09: PRAGMA foreign_keys side effects | HIGH | ADR-006 | RT-51–RT-52 |
| RISK-10–21: Operational risks | MEDIUM–LOW | Various | RT-53–RT-85 |

**Total**: 21 risks, 85 risk tests, 18 acceptance criteria.
