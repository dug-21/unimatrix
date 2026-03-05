# nxs-008: Risk Test Strategy

**Feature**: nxs-008 — Schema Normalization
**Date**: 2026-03-05
**Risk Agent**: uni-risk-strategist
**ADRs Referenced**: ADR-001 through ADR-008 (#355-#362)

---

## 1. Risk Register

21 risks identified. 4 Critical, 6 High, 7 Medium, 4 Low.

### RISK-01: Migration Data Fidelity — One-Way Door Corruption
**Severity: CRITICAL** | **Likelihood: HIGH** | **Wave: 0**
**Traces**: SR-01, ADR-005

The v5-to-v6 migration deserializes every bincode blob across 7 tables, then INSERTs as SQL columns. Old tables are dropped — no rollback without external backup. The migration depends on bincode deserializers that nxs-008 subsequently removes.

**Specific failure modes**:
- `serde(default)` fields added across schema v0-v5 may produce incorrect defaults when deserializing entries written at earlier versions (schema.rs:58-95 shows 14 `serde(default)` fields accumulated over 5 schema versions)
- bincode v2 positional encoding means a v1-era entry with 16 fields cannot be deserialized by the current 24-field EntryRecord struct — the migration must handle this gracefully (migration.rs:104-119 currently silently skips unparseable entries)
- Server-crate types (AgentRecord, AuditEvent) must be accessible to store-crate migration code — dependency inversion risk
- If migration fails mid-transaction, partial writes could leave database in inconsistent state (current migration.rs uses BEGIN IMMEDIATE but the v5-to-v6 migration will be much larger)

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-01: Round-trip every table: v5 blob → deserialize → SQL INSERT → read back → assert field equality | Integration | 0 | Data survives migration |
| RT-02: Historical schema entries (v0, v1, v2, v3, v5) all migrate correctly with proper defaults | Integration | 0 | serde(default) field handling |
| RT-03: Entry with all 24 fields set to non-default values survives migration | Integration | 0 | No silent value truncation |
| RT-04: Entry with Option fields (supersedes=Some, superseded_by=Some) → SQL nullable columns | Integration | 0 | NULL/Some mapping |
| RT-05: Empty database (v5, 0 rows in all tables) migrates to v6 cleanly | Integration | 0 | Edge case: no data |
| RT-06: 200-entry database with diverse field values → all survive | Integration | 0 | Scale: batch migration |
| RT-07: Backup file (.db.v5-backup) exists after migration starts | Integration | 0 | Rollback safety net |
| RT-08: Migration runs within single transaction — failure at step N rolls back ALL tables | Integration | 0 | Atomicity |
| RT-09: AgentRecord and AuditEvent from server crate deserialize correctly in migration_compat | Unit | 0 | Cross-crate type access |
| RT-10: Tags extracted from EntryRecord.tags Vec populate entry_tags rows correctly | Integration | 0 | Junction table migration |

### RISK-02: 24-Column Bind Parameter Accuracy
**Severity: CRITICAL** | **Likelihood: CERTAIN** | **Wave: 1**
**Traces**: SR-02, ADR-004

Every read/write path for ENTRIES must use 24 SQL columns. Current code (write.rs:107-108) uses 2 parameters (id, data blob). The new INSERT uses 24 named parameters, the UPDATE uses 24, and the SELECT must map 24 columns back to EntryRecord fields. A positional error causes silent data corruption.

**Specific failure modes**:
- Column order in INSERT VALUES clause doesn't match column list — `named_params!{}` prevents this only if param names match SQL placeholders
- `entry_from_row()` reads column "id" but the SELECT lists columns in a different order than the struct — rusqlite column-by-name access mitigates but `get::<_, i64>(0)?` positional access does not
- u64-to-i64 cast overflow: EntryRecord stores u64 but SQLite stores i64 — values > i64::MAX corrupt silently (schema.rs:50 shows `id: u64`)
- write.rs and store_ops.rs both write to entries — they must use identical column lists or data diverges

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-11: Insert EntryRecord with ALL 24 fields set to distinct non-default values, read back, assert field-by-field equality | Integration | 1 | Round-trip: no column swap |
| RT-12: Insert via Store::insert(), read back via Store::get() — assert all fields including tags | Integration | 1 | Store API round-trip |
| RT-13: Insert via store_ops.rs server path, read back via Store::get() — assert field equality | Integration | 1 | Cross-crate write path parity |
| RT-14: Update every field of an entry (including status change), read back, assert all changed | Integration | 1 | UPDATE column coverage |
| RT-15: Verify named_params!{} used in all INSERT/UPDATE SQL for entries (code review + grep) | Static | 1 | ADR-004 compliance |
| RT-16: EntryRecord with u64::MAX id, created_at, updated_at — verify i64 cast handling | Unit | 1 | Boundary: u64 overflow |
| RT-17: entry_from_row() uses column names not positions (verify no `get::<_, T>(n)` for entries) | Static | 1 | Position-independent reads |

### RISK-03: SQL Query Semantic Equivalence
**Severity: CRITICAL** | **Likelihood: HIGH** | **Wave: 1**
**Traces**: SR-03

read.rs rewrite replaces ~250 lines of HashSet intersection (lines 1-253) with SQL WHERE clauses. Five query semantic contracts must be preserved exactly:

1. **Tag AND semantics**: `collect_ids_by_tags` (read.rs:68-92) intersects per-tag ID sets. The SQL replacement `GROUP BY entry_id HAVING COUNT(DISTINCT tag) = :tag_count` must produce identical results.
2. **Empty filter → Active**: `query()` (read.rs:208-218) defaults to `Status::Active` when all filters are None.
3. **Empty tags skip**: `filter.tags = Some(vec![])` (read.rs:228-231 with `&& !tags.is_empty()`) skips tag filtering entirely.
4. **Invalid time range → empty**: `query_by_time_range` (read.rs:189-191) returns empty when `start > end`.
5. **Status as u8 integer**: `collect_ids_by_status` (read.rs:121) casts `status as u8 as i64`.

**Specific failure modes**:
- Tag subquery uses `HAVING COUNT(tag)` instead of `COUNT(DISTINCT tag)` — duplicate tag rows (if any) would change results
- `WHERE status = :status` applied even when no status filter, returning only Active (different from "return all regardless of status")
- `BETWEEN :start AND :end` is inclusive on both ends in SQL, matching current `>= start AND <= end` — confirmed equivalent
- entry_tags JOIN on main query instead of subquery → entries with 0 tags excluded from all results (INNER JOIN hazard)
- N+1 elimination changes result ordering — current code iterates HashSet (unordered), SQL returns in rowid order. If any caller depends on ordering, behavior diverges.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-18: query_by_tags([A, B]) — entry with [A,B] matches, entry with [A,C] does not, entry with [A] does not | Integration | 1 | Tag AND semantics |
| RT-19: query_by_tags([A]) — entry with [A,B] matches (superset tags are OK) | Integration | 1 | Tag subset matching |
| RT-20: query(empty filter) returns only Active entries | Integration | 1 | Default status |
| RT-21: query(tags=Some(vec![])) returns entries (tag filter skipped) | Integration | 1 | Empty tags bypass |
| RT-22: query_by_time_range(start > end) returns empty | Integration | 1 | Invalid range guard |
| RT-23: query_by_time_range(start == end) returns entries at that exact timestamp | Integration | 1 | Boundary: single-point |
| RT-24: query with all 5 filters set simultaneously — intersection semantics | Integration | 1 | Multi-filter AND |
| RT-25: query_by_status for each Status variant — correct entries returned | Integration | 1 | Enum integer mapping |
| RT-26: Entry with 0 tags appears in non-tag-filtered queries | Integration | 1 | No JOIN exclusion |
| RT-27: query() and query_by_* return same entries as pre-normalization (golden snapshot test) | Integration | 5 | Full behavioral parity |

### RISK-04: entry_tags Junction Table Consistency
**Severity: CRITICAL** | **Likelihood: MEDIUM** | **Wave: 1**
**Traces**: SR-08, ADR-006

Tags move from EntryRecord.tags (Vec<String> inside bincode blob) to a separate junction table. Every code path that constructs EntryRecord must also query entry_tags. If any path forgets, entries silently appear to have empty tags.

**Specific failure modes**:
- `entry_from_row()` sets `tags: vec![]` then `load_tags_for_entries()` is never called — all tag queries return empty
- Delete entry without CASCADE → orphan entry_tags rows accumulate, eventually corrupting tag queries for recycled IDs (if IDs are ever reused)
- `PRAGMA foreign_keys = ON` not set before CREATE TABLE → CASCADE declaration has no effect
- Update path deletes all tags then re-inserts — if crash between DELETE and INSERT, entry loses all tags (must be within transaction)
- store_correct.rs creates replacement entry in server crate — must INSERT entry_tags for new entry's tags

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-28: Insert entry with tags, get() returns correct tags | Integration | 1 | Basic tag storage |
| RT-29: Insert entry, delete entry, verify entry_tags rows gone (CASCADE) | Integration | 1 | FK CASCADE delete |
| RT-30: Update entry tags (add/remove), get() returns updated tags | Integration | 1 | Tag replacement |
| RT-31: Insert entry with 0 tags, get() returns empty tags Vec | Integration | 1 | Empty tags |
| RT-32: PRAGMA foreign_keys returns ON after Store::open() | Integration | 1 | PRAGMA enforcement |
| RT-33: query_by_tags returns entries with correct tags attached (not just IDs) | Integration | 1 | Tags populated in query results |
| RT-34: store_correct replacement entry has tags from NewEntry | Integration | 1 | Server path tags |

### RISK-05: Compat Layer Removal Leaves Dangling References
**Severity: HIGH** | **Likelihood: HIGH** | **Wave: 4**
**Traces**: SR-04, ADR-001, ADR-002, ADR-003

Wave 4 removes handles.rs (~428 lines), dispatch.rs (~134 lines), and guts tables.rs (~182 lines). Server code that imports from these modules will fail to compile. The risk is that Waves 1-3 accidentally introduce new references to compat types, or that existing references are missed.

**Specific failure modes**:
- Server imports `use unimatrix_store::tables::*` survive into Wave 4 — compilation failure
- `SqliteWriteTransaction::open_table()` method removed but server code still calls it
- `Store::begin_read()` removed but read paths in server still call it
- Counter helpers moved to `counters.rs` but some call sites still import from `tables.rs`

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-35: `cargo build --workspace` after Wave 4 deletions | Build | 4 | No dangling imports |
| RT-36: `grep -r "open_table\|open_multimap\|begin_read\|TableU64Blob\|TableStrU64\|MultimapSpec\|TableSpec" crates/ --include="*.rs"` returns 0 hits | Static | 4 | Complete removal |
| RT-37: All existing tests pass after compat removal | Test | 4 | No behavioral regression |

### RISK-06: Cross-Crate Compilation During Wave Transitions
**Severity: HIGH** | **Likelihood: CERTAIN** | **Wave: 1-3**
**Traces**: SR-05, ADR-008

store_ops.rs and store_correct.rs in the server crate directly access store-crate table schemas. Schema changes in store must be synchronized with server changes in the same wave.

**Specific failure modes**:
- Wave 1 changes entries DDL but store_ops.rs still writes to old schema → runtime SQL error
- Wave 1 drops topic_index but status.rs still queries it → compilation succeeds (SQL is a string) but runtime panic
- store_ops.rs references `serialize_entry` after it's moved to migration_compat → compilation failure

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-38: `cargo build --workspace` gate after every wave | Build | 0-4 | Cross-crate sync |
| RT-39: `cargo test --workspace` gate after every wave | Test | 0-4 | Runtime parity |
| RT-40: Server integration tests exercise store_ops insert, store_correct correction, status scan, contradiction scan | Integration | 1 | Server path coverage |

### RISK-07: Enum-to-Integer Mapping Divergence
**Severity: HIGH** | **Likelihood: MEDIUM** | **Wave: 0, 1**
**Traces**: SR-07, ADR-003

Seven enums transition from bincode-serialized to INTEGER columns. If bincode serializes enum discriminants differently from `as u8`, migration produces wrong values.

**Specific failure modes**:
- bincode v2 varint encoding stores enum discriminant as variable-length integer, not raw u8 — migration reads the full record so this is safe, BUT raw byte extraction from blob would be wrong
- New enum variant added between design and implementation → discriminant values shift (unlikely but catastrophic)
- `TryFrom<u8>` missing for SessionLifecycleStatus, SignalType, SignalSource, TrustLevel, Capability → runtime panic on read instead of graceful error

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-41: For each of 7 enums: serialize every variant to bincode, deserialize, assert `value as u8 == discriminant` | Unit | 0 | bincode/repr(u8) consistency |
| RT-42: `TryFrom<u8>` implemented for all 7 enums, invalid values return Err | Unit | 1 | Safe conversion |
| RT-43: Store entry with each Status variant, query_by_status returns correct entries | Integration | 1 | End-to-end enum roundtrip |
| RT-44: Migration of SESSIONS with each SessionLifecycleStatus value → correct integer column | Integration | 0 | Session enum mapping |
| RT-45: Migration of SIGNAL_QUEUE with each SignalType/SignalSource combination → correct columns | Integration | 0 | Signal enum mapping |

### RISK-08: JSON Array Column Deserialization Failures
**Severity: HIGH** | **Likelihood: MEDIUM** | **Wave: 2, 3**
**Traces**: SR-06, ADR-007

Five Vec fields use JSON TEXT columns. Malformed JSON, type mismatches, or NULL vs empty array confusion could cause runtime panics.

**Specific failure modes**:
- `serde_json::from_str::<Vec<u64>>(null_string)` panics — nullable columns need Option handling
- JSON `[1,2,3]` parsed as `Vec<u64>` works, but `[1, "two", 3]` causes deserialization error
- Capability enum stored as integers in JSON `[0,1,2]` — deserialization must handle integer-to-enum conversion
- Empty string "" vs "[]" vs NULL for empty arrays — inconsistent defaults across code paths
- allowed_topics is `Option<Vec<String>>` — NULL means "all allowed", `Some(vec![])` means "none allowed". JSON NULL vs "[]" distinction must be preserved.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-46: signal_queue entry_ids: insert [1,2,3], drain, verify Vec<u64> correct | Integration | 2 | JSON roundtrip: u64 array |
| RT-47: agent_registry capabilities: insert Vec<Capability>, read back, assert equality | Integration | 3 | JSON enum array |
| RT-48: agent_registry allowed_topics: NULL (all allowed) vs Some(vec![]) (none allowed) vs Some(["a"]) | Integration | 3 | NULL/empty distinction |
| RT-49: audit_log target_ids: insert empty Vec, read back, verify "[]" not NULL | Integration | 3 | Empty array default |
| RT-50: Malformed JSON in text column → graceful error, not panic | Unit | 2 | Error handling |

### RISK-09: PRAGMA foreign_keys Side Effects
**Severity: HIGH** | **Likelihood: LOW** | **Wave: 1**
**Traces**: SR-08, ADR-006

Enabling `PRAGMA foreign_keys = ON` is a global change affecting all tables. If any existing code path violates a foreign key constraint (even one that doesn't currently exist), it will start failing.

**Specific failure modes**:
- vector_map.entry_id has no FK declaration but code might assume it does after seeing entry_tags cascade
- Deletion order matters: if code deletes from entries then tries to read entry_tags, the CASCADE already deleted the tags
- PRAGMA must be set per-connection, before any schema modification — if set after CREATE TABLE, FKs don't apply to that connection

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-51: All existing delete/update integration tests pass with foreign_keys=ON | Integration | 1 | No FK violations |
| RT-52: vector_map manual cleanup still works (no FK constraint on vector_map) | Integration | 1 | Non-FK table unaffected |

### RISK-10: co_access Staleness Filter Regression
**Severity: MEDIUM** | **Likelihood: MEDIUM** | **Wave: 2**

Current co_access reads deserialize blob to check `last_updated` (read.rs:325-326, 344-345, 368-369). After normalization, the filter moves to SQL WHERE. The SQL filter must exactly match the Rust `>=` comparison.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-53: get_co_access_partners with staleness cutoff — same results before/after | Integration | 2 | Filter equivalence |
| RT-54: co_access_stats with staleness cutoff — (total, active) counts match | Integration | 2 | Stats equivalence |
| RT-55: top_co_access_pairs ordering preserved (by count descending) | Integration | 2 | Sort equivalence |

### RISK-11: Session GC Cascade Changes
**Severity: MEDIUM** | **Likelihood: MEDIUM** | **Wave: 2**

Sessions GC currently does full-table scan + deserialize to find expired sessions. After normalization, it uses `WHERE started_at < ?` with an index. The cascade to injection_log changes from per-row delete to `DELETE FROM injection_log WHERE session_id IN (...)`.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-56: GC: create sessions, expire them, verify sessions + injection_log rows deleted | Integration | 2 | Cascade correctness |
| RT-57: GC: active sessions untouched, only expired ones removed | Integration | 2 | Filter accuracy |
| RT-58: scan_sessions_by_feature returns correct sessions with indexed query | Integration | 2 | Feature scan parity |

### RISK-12: Signal Queue Drain Behavioral Parity
**Severity: MEDIUM** | **Likelihood: LOW** | **Wave: 2**

Signal drain changes from blob deserialization to SQL column read + JSON parse for entry_ids. The drain must atomically read and delete signals.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-59: drain_signals by type returns correct signals with correct entry_ids | Integration | 2 | Drain parity |
| RT-60: drain_signals deletes returned signals atomically | Integration | 2 | Atomic drain |

### RISK-13: Server Audit write_in_txn Path
**Severity: MEDIUM** | **Likelihood: MEDIUM** | **Wave: 3**

audit.rs `write_in_txn` writes audit events within an existing transaction (shared with the operation being audited). The transition from `txn.open_table(AUDIT_LOG)` to direct SQL on `txn.guard` must maintain transaction participation.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-61: Audit event written in same transaction as store operation — both commit or both rollback | Integration | 3 | Transaction participation |
| RT-62: write_count_since uses indexed agent_id + timestamp query | Integration | 3 | Index utilization |
| RT-63: Monotonic event_id across concurrent audit writes | Integration | 3 | ID sequencing |

### RISK-14: Agent Registry Capability JSON Enum Mapping
**Severity: MEDIUM** | **Likelihood: MEDIUM** | **Wave: 3**

Capabilities stored as JSON integer array `[0,1,2]`. The mapping from integer to Capability enum must be stable and consistent with how the server checks capabilities.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-64: Enroll agent with all capability variants, read back, verify correct set | Integration | 3 | Capability roundtrip |
| RT-65: Protected agents ("system", "human") still cannot be modified | Integration | 3 | Protection preserved |
| RT-66: Self-lockout prevention still works | Integration | 3 | Authorization preserved |

### RISK-15: Counter Module Consolidation
**Severity: MEDIUM** | **Likelihood: LOW** | **Wave: 0**
**Traces**: ADR-002

Counter helpers exist in two places (write.rs:22-54, tables.rs). Consolidation to counters.rs must not break any counter operation.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-67: next_entry_id returns sequential IDs across multiple inserts | Integration | 0 | Counter sequence |
| RT-68: Status counters (total_active, etc.) accurate after insert/update_status/delete | Integration | 1 | Counter accuracy |

### RISK-16: Migration Transaction Size
**Severity: MEDIUM** | **Likelihood: LOW** | **Wave: 0**

The v5-to-v6 migration wraps ALL table migrations in a single transaction. With 200+ entries and 400+ operational records, the write-ahead log could grow large.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-69: Migration of 500-row database completes within 5 seconds | Performance | 0 | Acceptable duration |
| RT-70: Migration of empty database completes without error | Integration | 0 | Edge case |

### RISK-17: Time Index Semantic Shift
**Severity: LOW** | **Likelihood: MEDIUM** | **Wave: 1**

Current time_index stores `timestamp` (write.rs:134-138 stores `created_at`). The update path (write.rs:249-253) replaces it with `updated_at`. After normalization, `idx_entries_created_at` indexes `created_at` only. If any query path relied on time_index containing `updated_at` values, results will differ.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-71: query_by_time_range filters on created_at, matching current behavior | Integration | 1 | Semantic preservation |

### RISK-18: read.rs N+1 Elimination Changes Result Set Size
**Severity: LOW** | **Likelihood: LOW** | **Wave: 1**

Current `fetch_entries` (read.rs:14-31) silently drops entries that fail deserialization. The SQL column path has no such filter. If any existing entry has corrupted data, the SQL path will include it where the bincode path skipped it.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-72: Query result counts match between pre- and post-normalization for same dataset | Integration | 5 | Result set parity |

### RISK-19: serde_json Dependency Addition
**Severity: LOW** | **Likelihood: LOW** | **Wave: 2**

Adding `serde_json` to unimatrix-store Cargo.toml. Already a transitive dependency but making it direct.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-73: `cargo build --workspace` succeeds with serde_json in store Cargo.toml | Build | 2 | Dependency resolution |

### RISK-20: Schema Version Counter Update
**Severity: LOW** | **Likelihood: LOW** | **Wave: 0**

migration.rs:15 currently has `CURRENT_SCHEMA_VERSION: u64 = 5`. Must be updated to 6. If both old and new schema version code coexist during development, race conditions in tests are possible.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-74: After migration, `SELECT value FROM counters WHERE name='schema_version'` returns 6 | Integration | 0 | Version stamp |
| RT-75: Fresh database created at v6 schema (no migration needed) | Integration | 1 | New DB path |

### RISK-21: Behavioral Parity Across All 12 MCP Tools
**Severity: LOW** | **Likelihood: LOW** | **Wave: 5**

Final verification that no MCP tool behavior changed.

**Test strategy**:
| Test | Type | Wave | Validates |
|------|------|------|-----------|
| RT-76: context_search returns same results and scores | Integration | 5 | Search parity |
| RT-77: context_lookup returns same entries for same filters | Integration | 5 | Lookup parity |
| RT-78: context_get returns identical EntryRecord fields | Integration | 5 | Get parity |
| RT-79: context_store creates entry with correct fields | Integration | 5 | Store parity |
| RT-80: context_correct creates correction chain correctly | Integration | 5 | Correct parity |
| RT-81: context_deprecate/quarantine update status correctly | Integration | 5 | Status parity |
| RT-82: context_status returns accurate counts and lambda | Integration | 5 | Status parity |
| RT-83: context_briefing returns same entries for same agent/role | Integration | 5 | Briefing parity |
| RT-84: context_enroll agent lifecycle preserved | Integration | 5 | Enroll parity |
| RT-85: context_retrospective detection rules produce same signals | Integration | 5 | Retro parity |

---

## 2. Scope Risk Traceability

| Scope Risk | Severity | ADR | Risks Mapped | Primary Tests | Wave |
|------------|----------|-----|-------------|---------------|------|
| SR-01: Migration Data Fidelity | HIGH | ADR-005 | RISK-01, RISK-16, RISK-20 | RT-01 through RT-10, RT-69, RT-70, RT-74 | 0 |
| SR-02: 24-Column Bind Params | HIGH | ADR-004 | RISK-02 | RT-11 through RT-17 | 1 |
| SR-03: SQL Query Semantic Equivalence | HIGH | — | RISK-03, RISK-17, RISK-18 | RT-18 through RT-27, RT-71, RT-72 | 1, 5 |
| SR-04: Compat Layer Open Questions | MEDIUM | ADR-001, ADR-002, ADR-003 | RISK-05, RISK-15 | RT-35 through RT-37, RT-67, RT-68 | 0, 4 |
| SR-05: Cross-Crate Coupling | MEDIUM | ADR-008 | RISK-06 | RT-38 through RT-40 | 0-4 |
| SR-06: JSON Array Constraints | MEDIUM | ADR-007 | RISK-08 | RT-46 through RT-50 | 2, 3 |
| SR-07: Enum-to-Integer Stability | MEDIUM | ADR-003 | RISK-07 | RT-41 through RT-45 | 0, 1 |
| SR-08: entry_tags Consistency | LOW | ADR-006 | RISK-04, RISK-09 | RT-28 through RT-34, RT-51, RT-52 | 1 |

---

## 3. Test Summary by Wave

| Wave | Risk Tests | Test Types | Gate Criteria |
|------|-----------|------------|---------------|
| 0 | RT-01–10, RT-41, RT-44–45, RT-67, RT-69–70, RT-74 | 17 integration, 1 unit, 1 perf | `cargo test --workspace`, migration round-trip on synthetic v5 DB |
| 1 | RT-11–34, RT-42–43, RT-51–52, RT-68, RT-71, RT-75 | 27 integration, 2 unit, 2 static | `cargo build --workspace && cargo test --workspace` |
| 2 | RT-46, RT-50, RT-53–60, RT-73 | 10 integration, 1 unit, 1 build | `cargo build --workspace && cargo test --workspace` |
| 3 | RT-47–49, RT-61–66 | 9 integration | `cargo build --workspace && cargo test --workspace` |
| 4 | RT-35–37 | 1 build, 1 static, 1 test | `cargo build --workspace && cargo test --workspace`, zero compat refs |
| 5 | RT-27, RT-72, RT-76–85 | 12 integration | Full AC-01 through AC-18 verification |

**Total**: 85 risk tests (76 integration, 4 unit, 2 static, 2 build, 1 performance)

---

## 4. Top 3 Risks by Severity

1. **RISK-01 (Migration Data Fidelity)** — CRITICAL. One-way door. bincode blob deserialization across 7 tables with 5 historical schema versions. Silent data loss is unrecoverable. Mitigated by ADR-005 (migration_compat module), automatic backup, and 10 round-trip tests. **This risk gates all subsequent waves.**

2. **RISK-02 (24-Column Bind Parameter Accuracy)** — CRITICAL. Every write path must bind 24 parameters correctly. A single column swap produces silently corrupted data that passes compilation and type checks. Mitigated by ADR-004 (mandatory named_params!{}) and round-trip integration tests with all 24 fields distinct.

3. **RISK-03 (SQL Query Semantic Equivalence)** — CRITICAL. read.rs rewrite replaces ~250 lines of HashSet intersection with SQL WHERE clauses. Five query semantic contracts (tag AND, empty filter default, empty tags skip, invalid range guard, multi-filter intersection) must be preserved exactly. Behavioral divergence breaks AC-17 (all 12 MCP tools identical). Mitigated by 10 targeted query parity tests plus Wave 5 golden snapshot comparison.
