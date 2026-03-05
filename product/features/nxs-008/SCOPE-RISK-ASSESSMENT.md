# nxs-008: Scope Risk Assessment

## Scope Summary

nxs-008 normalizes the storage schema from bincode blobs to SQL columns across 7 tables, eliminates 5 manual index tables (replaced by SQL indexes), removes the redb-pattern compat layer (handles.rs, dispatch.rs), and replaces client-side HashSet query logic with SQL WHERE clauses. The feature migrates schema v5 to v6, touches 21+ files across store and server crates, and removes ~500 lines of compat abstractions while rewriting read/write paths. The Store public API is unchanged — this is an internal restructuring with behavioral parity.

---

## Risk Catalog

### SR-01: Migration Data Fidelity — Bincode Deserialization During v5-to-v6
**Severity: HIGH** | **Likelihood: HIGH**

The v5-to-v6 migration must deserialize every existing bincode blob, then INSERT the fields as SQL columns. This is a one-way door — old tables are dropped after migration. The migration depends on the current bincode deserialization code to read v5 data, but nxs-008 also removes that same deserialization infrastructure.

Specific concerns:
- 71 bincode references across 13 files must be evaluated: which are needed for migration vs. which are runtime paths being replaced
- `serialize_entry`/`deserialize_entry` in schema.rs are used by migration.rs — they must survive until after migration runs
- Historical schema versions (v0-v5) have accumulated `serde(default)` fields. The migration must handle entries written at any prior schema version, not just v5
- `EntryRecord` has 10 `serde(default)` fields added across v0-v5. A v2-era entry deserialized then re-inserted as SQL columns must produce correct defaults
- No rollback path: if migration corrupts data, the database is unrecoverable without external backup

**Impact**: Silent data loss or corruption. Entries could lose field values, confidence scores could reset to 0.0, supersedes chains could break.

**Mitigation**:
1. Migration code must be written and tested BEFORE removing bincode infrastructure — ordering within waves is critical
2. Keep `deserialize_entry` (and per-table deserializers) available to migration.rs even if removed from runtime paths — consider a `migration_compat` module that is compiled but not used at runtime
3. Round-trip test: deserialize a v5 blob, insert as columns, read back via new column path, assert field-by-field equality against original
4. Test with synthetic entries from each historical schema version (v0, v1, v2, v3, v5) to verify serde(default) fields populate correctly
5. Migration should copy the database file before starting (automatic backup)

### SR-02: 24-Column EntryRecord Decomposition Touches Every Read/Write Path
**Severity: HIGH** | **Likelihood: CERTAIN**

The ENTRIES table is the core entity. Decomposing its bincode blob into 24 columns requires simultaneous changes to:
- write.rs: insert path (currently 107 lines, 7+ SQL statements with 24 bind parameters)
- write.rs: update path (currently 121 lines of diff-based index sync becomes single UPDATE with 24 columns)
- read.rs: query path (currently ~200 lines of HashSet intersection becomes SQL WHERE clause builder)
- read.rs: get-by-id path (currently deserialize blob, becomes construct from row columns)
- All server files that bypass Store API and access tables directly (store_ops.rs, store_correct.rs, status.rs, contradiction.rs)

A bind-parameter mismatch (wrong column order, wrong count, wrong type) in any of these paths causes silent data corruption or runtime panics. With 24 columns, the probability of a positional error in at least one path is high.

**Impact**: Compilation may succeed but queries return wrong data, or inserts silently swap field values (e.g., `created_at` written to `updated_at` column).

**Mitigation**:
1. Use named parameters (`:id`, `:title`, `:content`, etc.) in all SQL statements instead of positional `?` placeholders — rusqlite supports `named_params!{}` macro which eliminates column-order bugs
2. Add a compile-time or startup assertion that the column count in INSERT statements matches `EntryRecord` field count
3. Round-trip integration test: insert an EntryRecord with distinct non-default values in ALL 24 fields, read it back, assert field-by-field equality

### SR-03: SQL Query Semantic Equivalence for read.rs Rewrite
**Severity: HIGH** | **Likelihood: MEDIUM**

The read.rs rewrite replaces ~200 lines of HashSet intersection logic with SQL WHERE clauses. The current logic has specific semantics around:
- Tag filtering: current code intersects TAG_INDEX results (AND semantics across tags? OR? The scope says `WHERE tag IN (?,?)` which is OR)
- Status filtering: maps enum integer values, must match `#[repr(u8)]` encoding exactly
- Time range queries: current code scans TIME_INDEX, new code uses `WHERE created_at BETWEEN ? AND ?`
- Empty filter handling: what happens when no topic/category/status filter is provided — current code skips that HashSet, new SQL must not add a WHERE clause for it
- NULL handling for Option fields (supersedes, superseded_by): bincode stores None as absent, SQL stores as NULL. Filter logic must account for this

The N+1 elimination (AC-12) changes from per-entry SELECT to batch SELECT. Tag loading changes from blob deserialization to a separate `SELECT tag FROM entry_tags WHERE entry_id IN (...)` query. This second query must be correctly correlated.

**Impact**: Query results differ from current behavior — breaks AC-17 (behavioral parity). Could surface as missing search results, incorrect ordering, or confidence score mismatches in MCP tools.

**Mitigation**:
1. Before rewriting read.rs, document the exact current query semantics: tag AND vs OR, empty filter behavior, NULL handling
2. Write integration tests that capture current query behavior (various filter combinations) BEFORE the rewrite, then assert identical results after
3. The entry_tags JOIN query must handle the case where an entry has zero tags (LEFT JOIN or separate query, not INNER JOIN on the main query)

### SR-04: Compat Layer Removal Has Unresolved Architectural Dependencies
**Severity: MEDIUM** | **Likelihood: HIGH**

Wave 4 removes handles.rs, dispatch.rs, and simplifies txn.rs. But the SCOPE.md has three open questions that affect all waves:
- **txn.rs**: Should SqliteReadTransaction/SqliteWriteTransaction wrappers be kept as thin connection wrappers?
- **tables.rs**: Counter helpers may need to survive. Where do they live?
- **Enum storage**: INTEGER vs TEXT for enum columns?

These are not implementation details — they are architectural decisions that affect the interface between waves. If Waves 1-3 are built assuming compat types still exist (e.g., using `SqliteWriteTransaction` in new SQL column write paths), then Wave 4's removal requires rewriting those paths again. Conversely, if Waves 1-3 bypass compat types, they may need to duplicate functionality that tables.rs/txn.rs currently provides.

**Impact**: Rework in Wave 4 that forces re-visiting Wave 1-3 code paths. Or compat types leak into the normalized code and are never fully removed.

**Mitigation**: The architect must resolve all three open questions BEFORE Wave 1 begins. The answers dictate whether Waves 1-3 use compat types or bypass them. Recommended: resolve toward removal (Waves 1-3 write direct SQL without compat types), so Wave 4 only deletes dead code rather than performing a second rewrite.

### SR-05: Server Direct Table Access Creates Cross-Crate Coupling During Migration
**Severity: MEDIUM** | **Likelihood: CERTAIN**

Per ADR #352, the server's direct table access is accepted. But this means schema changes in unimatrix-store require synchronized changes in unimatrix-server. The SCOPE.md identifies 6 server files that need mechanical updates. During implementation:
- If store-crate schema changes land without corresponding server updates, the server won't compile
- If store and server changes are in the same wave, the wave is large and harder to validate at compilation gates
- The server files (store_ops.rs, store_correct.rs, audit.rs, registry.rs, status.rs, contradiction.rs) import table constants and serialization helpers that may be removed in Wave 4 but needed in Waves 1-3

**Impact**: Compilation failures during wave transitions. Large waves that are difficult to review.

**Mitigation**:
1. Each wave must include BOTH store and server changes for the tables it normalizes. Wave 1 (ENTRIES) must update store_ops.rs, store_correct.rs, status.rs, contradiction.rs simultaneously. Wave 3 (AGENT_REGISTRY, AUDIT_LOG) updates registry.rs and audit.rs
2. Compilation gate after each wave must run `cargo build --workspace`, not just the store crate

### SR-06: JSON Array Columns May Constrain Future ASS-016 Analytics
**Severity: MEDIUM** | **Likelihood: MEDIUM**

Five Vec fields are stored as JSON array columns (entry_ids in SIGNAL_QUEUE, capabilities/allowed_topics/allowed_categories in AGENT_REGISTRY, target_ids in AUDIT_LOG). The SCOPE justifies this because these fields are not queried by element. The additional context explicitly states schema decisions should not block future ASS-016 queries.

ASS-016 plans multi-table JOINs across INJECTION_LOG, SESSIONS, and ENTRIES. The critical JOIN path (INJECTION_LOG.session_id, correctly decomposed as an indexed column per AC-07) is not at risk. However:
- If future analytics need "which agents modified entry X?" queries against AUDIT_LOG.target_ids, `json_each()` virtual table JOINs are functional but slower than junction tables
- AUDIT_LOG is append-only and will grow unboundedly — `json_each(target_ids)` performance degrades with table size

**Impact**: Future analytics queries may require a second schema migration to decompose JSON columns into junction tables.

**Mitigation**:
1. Verify that INJECTION_LOG, SESSIONS, and ENTRIES — the three tables in the ASS-016 critical path — have proper indexed columns for JOINs (INJECTION_LOG.session_id, ENTRIES.id). The scope already handles this
2. Document JSON array column decisions as an ADR with "revisit if" criteria tied to ASS-016 query patterns
3. Accept JSON for SIGNAL_QUEUE, AGENT_REGISTRY, and AUDIT_LOG Vec fields — these are outside the ASS-016 critical path

### SR-07: Enum-to-Integer Mapping Must Be Stable Across Migration
**Severity: MEDIUM** | **Likelihood: MEDIUM**

Seven enum types transition from bincode-serialized representation to SQL INTEGER columns: Status, SessionLifecycleStatus, SignalType, SignalSource, Outcome, TrustLevel, Capability. The SCOPE notes these use `#[repr(u8)]`.

Risk: bincode v2 may serialize enum discriminants differently from a simple `as u8` cast (varint encoding, metadata). If the migration assumes `bincode_value == repr_u8_value` but bincode stores a different representation, all enum values will be wrong after migration.

Additionally, Open Question #3 (TEXT vs INTEGER for enums) is unresolved. If TEXT is chosen, the migration must convert integer discriminants to string names, and all query code changes.

**Impact**: Status values are misinterpreted (Active entries appear Deprecated). Agent trust levels misassigned. Queries by status return wrong results.

**Mitigation**:
1. Architect must resolve Open Question #3 before implementation begins
2. Write a unit test for each enum that serializes every variant to bincode, deserializes it, then asserts the value matches `as u8`. If they differ, migration must use full bincode deserialization, not raw integer extraction
3. The migration path already deserializes full records (Migration Strategy step 2) — ensure it uses the deserialized enum values, not raw bytes

### SR-08: entry_tags Junction Table Consistency
**Severity: LOW** | **Likelihood: MEDIUM**

Moving tags from EntryRecord (Vec<String>) to a separate `entry_tags` junction table introduces a consistency surface:
- Entry deletion must CASCADE to entry_tags, or orphan rows accumulate
- Every code path that constructs EntryRecord from a row must also query entry_tags
- Write paths must wrap entry INSERT + tag INSERTs in the same transaction

If any read path forgets to load tags, entries silently appear to have no tags — breaking tag-based queries, context_lookup, and briefing tag matching.

**Impact**: Entries returned without tags. Tag-based filtering returns empty results.

**Mitigation**:
1. Use `FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE` and enable `PRAGMA foreign_keys = ON` in Store::open()
2. Create a single `load_tags_for_entries(ids: &[u64])` helper mandated everywhere an EntryRecord is constructed
3. Integration test: insert entry with tags, retrieve via get(), verify tags present. Delete entry, verify entry_tags rows are gone

---

## Dependency and Ordering Constraints

| Constraint | Rationale |
|-----------|-----------|
| nxs-007 must be merged | Prerequisite — redb fully removed, SQLite sole backend |
| Migration code before bincode removal | v5-to-v6 migration needs deserializers to read old blobs |
| Open Question #3 resolved before Wave 1 | Enum storage format affects every table's column definitions |
| Open Question #1 resolved before Wave 4 | txn.rs fate determines transaction boundary strategy |
| Wave 1 before Wave 4 | Cannot remove compat layer until all call sites rewritten |
| Each wave includes both crates | Store + server changes for each table must land together |

---

## Top 3 Risks for Architect Attention

1. **SR-01 (Migration Data Fidelity)**: The v5-to-v6 migration is a one-way door that must deserialize bincode blobs using infrastructure that nxs-008 also removes. Implementation ordering is critical — migration code must be written and tested before bincode removal. Round-trip tests across all historical schema versions are essential. Automatic backup before migration is recommended. This is the highest-consequence risk: silent data corruption with no rollback.

2. **SR-02 (24-Column Bind Parameter Accuracy)**: Every read/write path must be rewritten with 24 SQL columns. Positional `?` placeholders are error-prone at this width. The architect should mandate named parameters (`named_params!{}`) and round-trip integration tests with all 24 fields set to distinct non-default values to catch column-order bugs.

3. **SR-04 (Compat Layer Open Questions)**: Three unresolved architectural decisions (txn.rs fate, counter helper location, enum storage format) determine whether Waves 1-3 build on compat types or bypass them. Resolving these in the ARCHITECTURE.md before any wave begins prevents rework in Wave 4. Recommended direction: resolve toward removal so Waves 1-3 write direct SQL and Wave 4 only deletes dead code.
