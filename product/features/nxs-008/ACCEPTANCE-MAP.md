# nxs-008: Schema Normalization — Acceptance Map

**Feature**: nxs-008
**Date**: 2026-03-05
**Acceptance Criteria**: AC-01 through AC-18

---

## AC-01: ENTRIES table has 24 SQL columns instead of a bincode blob

**Wave**: 1
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(entries)` returns 24 rows with correct column names and types
2. No `data BLOB` column exists
3. All INSERT/UPDATE statements use `named_params!{}` (ADR-004) — verified by code grep
4. Round-trip: insert EntryRecord with all 24 fields set to distinct non-default values, read back, assert field-by-field equality

**Risk tests**: RT-11, RT-12, RT-13, RT-14, RT-15, RT-16, RT-17
**Risks mitigated**: RISK-02 (CRITICAL)

---

## AC-02: entry_tags junction table exists with (entry_id INTEGER, tag TEXT, PRIMARY KEY(entry_id, tag))

**Wave**: 1
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(entry_tags)` returns 2 columns: `entry_id INTEGER`, `tag TEXT`
2. `PRAGMA foreign_key_list(entry_tags)` confirms FK to entries(id) with ON DELETE CASCADE
3. `SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='entry_tags'` includes `idx_entry_tags_tag` and `idx_entry_tags_entry_id`
4. Insert entry with tags, retrieve via `get()`, verify tags present
5. Delete entry, verify entry_tags rows removed by CASCADE

**Risk tests**: RT-28, RT-29, RT-30, RT-31, RT-32, RT-33, RT-34
**Risks mitigated**: RISK-04 (CRITICAL), RISK-09 (HIGH)

---

## AC-03: TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX tables eliminated

**Wave**: 1
**Verification method**: Integration test
**How to verify**:
1. `SELECT name FROM sqlite_master WHERE type='table'` does not include `topic_index`, `category_index`, `tag_index`, `time_index`, `status_index`
2. No Rust code references these table names (grep verification)
3. Fresh database creation does not create these tables
4. Migrated database does not contain these tables

**Risk tests**: RT-38 (build gate)
**Risks mitigated**: RISK-06 (HIGH)

---

## AC-04: SQL indexes exist on entries(topic), entries(category), entries(status), entries(created_at), entry_tags(tag)

**Wave**: 1
**Verification method**: Integration test
**How to verify**:
1. `SELECT name FROM sqlite_master WHERE type='index'` includes:
   - `idx_entries_topic`
   - `idx_entries_category`
   - `idx_entries_status`
   - `idx_entries_created_at`
   - `idx_entry_tags_tag`
2. Verified on both fresh databases and migrated databases

**Risk tests**: RT-38 (build gate)
**Risks mitigated**: Part of RISK-03 (query semantic equivalence)

---

## AC-05: CO_ACCESS table has SQL columns: entry_id_a, entry_id_b, count, last_updated

**Wave**: 2
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(co_access)` returns 4 columns, no `data BLOB`
2. `CHECK (entry_id_a < entry_id_b)` constraint active — attempt to insert a >= b fails
3. Index `idx_co_access_b` exists on `co_access(entry_id_b)`
4. Round-trip: record co-access, read back via `get_co_access_partners`, verify count and last_updated
5. Staleness filter produces same results as pre-normalization

**Risk tests**: RT-53, RT-54, RT-55
**Risks mitigated**: RISK-10 (MEDIUM)

---

## AC-06: SESSIONS table has SQL columns for all 9 SessionRecord fields

**Wave**: 2
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(sessions)` returns 9 columns: session_id, feature_cycle, agent_role, started_at, ended_at, status, compaction_count, outcome, total_injections
2. No `data BLOB` column
3. Indexes: `idx_sessions_feature_cycle`, `idx_sessions_status`
4. `scan_sessions_by_feature` uses `WHERE feature_cycle = ?` (no full-table scan)
5. `gc_sessions` uses `WHERE started_at < ?` (indexed query)
6. GC cascade: expired sessions deletion cascades to injection_log rows

**Risk tests**: RT-56, RT-57, RT-58, RT-44
**Risks mitigated**: RISK-11 (MEDIUM), RISK-07 (HIGH)

---

## AC-07: INJECTION_LOG table has SQL columns for all 5 fields, with indexed session_id

**Wave**: 2
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(injection_log)` returns 5 columns: log_id, session_id, entry_id, confidence, timestamp
2. No `data BLOB` column
3. Indexes: `idx_injection_log_session`, `idx_injection_log_entry`
4. GC cascade deletion uses `DELETE FROM injection_log WHERE session_id IN (...)` — indexed, no full scan
5. Round-trip: batch insert, scan by session_id, verify all fields

**Risk tests**: RT-56 (GC cascade)
**Risks mitigated**: Part of RISK-06 (cross-crate sync)

---

## AC-08: SIGNAL_QUEUE table has SQL columns with entry_ids as JSON array

**Wave**: 2
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(signal_queue)` returns 6 columns: signal_id, session_id, created_at, entry_ids, signal_type, signal_source
2. No `data BLOB` column
3. `entry_ids` stored as JSON TEXT, e.g. `[10,20,30]`
4. Round-trip: insert signal with entry_ids `[1,2,3]`, drain, verify `Vec<u64>` correct
5. `drain_signals` uses `WHERE signal_type = ?` (no full scan + deserialize)
6. Empty entry_ids stored as `[]` not NULL

**Risk tests**: RT-46, RT-50, RT-59, RT-60, RT-45
**Risks mitigated**: RISK-08 (HIGH), RISK-12 (MEDIUM)

---

## AC-09: AGENT_REGISTRY table has SQL columns with capabilities/allowed_topics/allowed_categories as JSON arrays

**Wave**: 3
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(agent_registry)` returns 8 columns, no `data BLOB`
2. `capabilities` stored as JSON integer array, e.g. `[0,1,2]`
3. `allowed_topics`: NULL = all topics allowed; `["topic1"]` = restricted
4. `allowed_categories`: NULL = all categories allowed; `[]` = none allowed
5. Round-trip: enroll agent with all capability variants, read back, assert equality
6. Protected agents ("system", "human") cannot be modified
7. Self-lockout prevention works

**Risk tests**: RT-47, RT-48, RT-64, RT-65, RT-66
**Risks mitigated**: RISK-14 (MEDIUM), RISK-08 (HIGH)

---

## AC-10: AUDIT_LOG table has SQL columns with target_ids as JSON array

**Wave**: 3
**Verification method**: Integration test
**How to verify**:
1. `PRAGMA table_info(audit_log)` returns 8 columns: event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail
2. No `data BLOB` column
3. Indexes: `idx_audit_log_agent`, `idx_audit_log_timestamp`
4. `target_ids` stored as JSON array, e.g. `[1,2,3]`; empty = `[]` not NULL
5. `write_count_since` uses indexed `WHERE agent_id = ? AND timestamp >= ?`
6. `write_in_txn` participates in caller's transaction (both commit or both rollback)
7. Monotonic event_id across writes

**Risk tests**: RT-49, RT-61, RT-62, RT-63
**Risks mitigated**: RISK-13 (MEDIUM), RISK-08 (HIGH)

---

## AC-11: read.rs query path uses SQL WHERE clauses, not HashSet intersection

**Wave**: 1
**Verification method**: Code review + integration test
**How to verify**:
1. Code review: no `HashSet<u64>` intersection in read.rs
2. Single SQL query with dynamic WHERE clause replaces multi-table scan
3. Tag AND semantics preserved: entry with [A,B] matches query [A,B]; entry with [A] does not
4. Empty filter defaults to Active status
5. Empty tags `Some(vec![])` skips tag filter
6. Invalid time range (start > end) returns empty
7. Multi-filter intersection (all 5 filters) returns correct results

**Risk tests**: RT-18, RT-19, RT-20, RT-21, RT-22, RT-23, RT-24, RT-25, RT-26
**Risks mitigated**: RISK-03 (CRITICAL)

---

## AC-12: N+1 entry fetch pattern eliminated — queries return entries directly

**Wave**: 1
**Verification method**: Code review + integration test
**How to verify**:
1. Code review: no per-entry `SELECT data FROM entries WHERE id = ?` loop in read.rs
2. Batch SELECT returns all matching entries in one query
3. `load_tags_for_entries()` batch-loads tags for all result entries
4. Entry with 0 tags appears in non-tag-filtered queries (no INNER JOIN exclusion)
5. `entry_from_row()` helper used for all EntryRecord construction from rows

**Risk tests**: RT-26, RT-27
**Risks mitigated**: Part of RISK-03 (CRITICAL)

---

## AC-13: handles.rs and dispatch.rs removed

**Wave**: 4
**Verification method**: Filesystem check + build gate
**How to verify**:
1. `crates/unimatrix-store/src/handles.rs` does not exist
2. `crates/unimatrix-store/src/dispatch.rs` does not exist
3. `crates/unimatrix-store/src/tables.rs` does not exist (fully removed per Architecture)
4. `cargo build --workspace` succeeds with no references to deleted types
5. Grep: `open_table`, `open_multimap`, `begin_read`, `TableU64Blob`, `TableStrU64`, `MultimapSpec`, `TableSpec` — zero hits in `crates/` (excluding comments/docs)

**Risk tests**: RT-35, RT-36, RT-37
**Risks mitigated**: RISK-05 (HIGH)

---

## AC-14: Schema version is 6; migration from v5 databases works correctly

**Wave**: 0 (migration), 5 (verification)
**Verification method**: Integration test
**How to verify**:
1. `SELECT value FROM counters WHERE name = 'schema_version'` returns 6
2. Create synthetic v5 database with known data, open with new code, verify all data accessible
3. Round-trip: every field of every record type survives migration with exact values
4. Historical entries from v0, v1, v2, v3, v5 migrate with correct serde(default) values
5. Empty v5 database migrates cleanly
6. 200-entry database with diverse field values — all survive
7. `.db.v5-backup` file exists after migration
8. Migration runs in single transaction — failure rolls back ALL tables
9. Fresh database creates at v6 schema directly (no migration needed)

**Risk tests**: RT-01, RT-02, RT-03, RT-04, RT-05, RT-06, RT-07, RT-08, RT-09, RT-10, RT-69, RT-70, RT-74, RT-75
**Risks mitigated**: RISK-01 (CRITICAL), RISK-16 (MEDIUM), RISK-20 (LOW)

---

## AC-15: No bincode serialize/deserialize for any normalized table (OBSERVATION_METRICS excluded)

**Wave**: 4
**Verification method**: Static analysis (grep)
**How to verify**:
1. `grep -r 'bincode' crates/ --include="*.rs"` returns hits only in:
   - `migration_compat.rs` (deserializers for v5 blobs)
   - OBSERVATION_METRICS paths
   - `Cargo.toml` dependency declaration
   - Test code that creates synthetic v5 data
2. No runtime code path serializes/deserializes bincode for entries, co_access, sessions, injection_log, signal_queue, agent_registry, or audit_log

**Risk tests**: RT-36 (grep for compat references)
**Risks mitigated**: Part of RISK-05 (HIGH)

---

## AC-16: cargo build succeeds, cargo test --workspace passes

**Wave**: 0-5 (every wave gate)
**Verification method**: Build + test execution
**How to verify**:
1. `cargo build --workspace` — zero errors, zero warnings for removed types
2. `cargo test --workspace` — all tests pass
3. No regressions in existing test count (~1025 unit + ~174 integration)
4. Build gate runs after every wave (ADR-008)

**Risk tests**: RT-38, RT-39
**Risks mitigated**: RISK-06 (HIGH)

---

## AC-17: All 12 MCP tools produce identical results (behavioral parity)

**Wave**: 5
**Verification method**: Integration test
**How to verify**:
1. `context_search` — same results and reranking scores
2. `context_lookup` — same entries for same filters
3. `context_get` — identical EntryRecord fields including tags
4. `context_store` — creates entry with correct fields
5. `context_correct` — correction chain intact
6. `context_deprecate` — status update correct
7. `context_quarantine` — status update correct
8. `context_status` — accurate counts and lambda
9. `context_briefing` — same entries for same agent/role
10. `context_enroll` — agent lifecycle preserved
11. `context_retrospective` — detection rules produce same signals
12. `context_search` re-ranking — confidence + co-access boost unchanged

**Risk tests**: RT-76, RT-77, RT-78, RT-79, RT-80, RT-81, RT-82, RT-83, RT-84, RT-85
**Risks mitigated**: RISK-21 (LOW) — low severity because all higher-risk tests catch issues earlier

---

## AC-18: Future EntryRecord field additions use ALTER TABLE ADD COLUMN, not scan-and-rewrite

**Wave**: 5
**Verification method**: Documentation + code review
**How to verify**:
1. No bincode positional encoding constraints remain for the entries table
2. `migration.rs` contains documentation explaining the new field addition pattern:
   - `ALTER TABLE entries ADD COLUMN new_field TYPE DEFAULT value`
   - Instant, zero-downtime — no full-table rewrite needed
3. No `serialize_entry` / `deserialize_entry` in runtime paths (enforced by AC-15)
4. New `serde(default)` annotations on EntryRecord are no longer needed for schema evolution — SQL column defaults handle it

**Risk tests**: None (documentation verification)
**Risks mitigated**: None (future-proofing)

---

## Summary Matrix

| AC | Wave | Type | Risk Tests | Critical Risk |
|----|------|------|-----------|---------------|
| AC-01 | 1 | Integration + grep | RT-11–17 | RISK-02 |
| AC-02 | 1 | Integration | RT-28–34 | RISK-04 |
| AC-03 | 1 | Integration + grep | RT-38 | RISK-06 |
| AC-04 | 1 | Integration | RT-38 | — |
| AC-05 | 2 | Integration | RT-53–55 | — |
| AC-06 | 2 | Integration | RT-56–58, RT-44 | RISK-07 |
| AC-07 | 2 | Integration | RT-56 | — |
| AC-08 | 2 | Integration | RT-46, RT-50, RT-59–60, RT-45 | RISK-08 |
| AC-09 | 3 | Integration | RT-47–48, RT-64–66 | RISK-08 |
| AC-10 | 3 | Integration | RT-49, RT-61–63 | — |
| AC-11 | 1 | Code review + integration | RT-18–26 | RISK-03 |
| AC-12 | 1 | Code review + integration | RT-26–27 | RISK-03 |
| AC-13 | 4 | Filesystem + build | RT-35–37 | RISK-05 |
| AC-14 | 0, 5 | Integration | RT-01–10, RT-69–70, RT-74–75 | RISK-01 |
| AC-15 | 4 | Static (grep) | RT-36 | RISK-05 |
| AC-16 | 0-5 | Build + test | RT-38–39 | RISK-06 |
| AC-17 | 5 | Integration | RT-76–85 | — |
| AC-18 | 5 | Documentation | — | — |

**Coverage**: All 18 acceptance criteria mapped. 85 risk tests across 21 risks provide defense in depth. 4 CRITICAL risks (RISK-01 through RISK-04) are covered by 34 targeted tests.
