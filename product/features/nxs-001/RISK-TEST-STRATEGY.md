# nxs-001: Embedded Storage Engine — Risk-Based Test Strategy

**Feature**: nxs-001 (Embedded Storage Engine)
**Agent**: nxs-001-agent-3-risk
**Date**: 2026-02-22

---

## 1. Risk Identification

### R1: Index-Entry Desynchronization (CRITICAL)

**Category**: Data Integrity
**Description**: Every insert, update, and status change writes to ENTRIES plus up to 6 index tables (TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, COUNTERS) within a single write transaction. If any index becomes inconsistent with ENTRIES, queries silently return wrong results — entries appear in index scans but don't exist, or entries exist but are invisible to queries.
**Likelihood**: Medium — manual index maintenance across multiple tables is error-prone.
**Impact**: Critical — silent data corruption. Downstream features (MCP tools, vector search) inherit corrupted query results.

### R2: Update Path Stale Index Orphaning (CRITICAL)

**Category**: Data Integrity
**Description**: When updating an entry (AC-18) — e.g., changing topic from "auth" to "security" — the engine must diff the old and new records, remove stale index entries for changed fields, and insert new ones. If stale entries remain, phantom results appear in queries. If new entries aren't inserted, the updated entry becomes invisible to the changed field's index.
**Likelihood**: High — field-level diffing across 5 indexed dimensions (topic, category, tags, time, status) with different table types (Table vs MultimapTable) is the most complex write path.
**Impact**: Critical — phantom query results and invisible entries.

### R3: Bincode Serialization Round-Trip Fidelity (HIGH)

**Category**: Data Integrity
**Description**: EntryRecord is serialized via bincode v2 and stored as `&[u8]` in ENTRIES. Any round-trip loss (field ordering, Option encoding, Vec encoding, String encoding, f32 precision, u64 boundary values) means corrupted entries. bincode v2 is a different wire format from v1 — configuration must be correct.
**Likelihood**: Low — bincode is well-tested, but misconfigured options or edge-case values (NaN f32, empty strings, empty Vec) could cause issues.
**Impact**: High — data corruption on read, potentially silent if defaults mask missing data.

### R4: Schema Evolution via serde(default) (HIGH)

**Category**: Schema Evolution
**Description**: The zero-migration strategy depends on `#[serde(default)]` working correctly: records serialized with the current schema must deserialize correctly when future fields are added. If this breaks, every stored entry becomes unreadable after a schema change, requiring manual migration of the entire database.
**Likelihood**: Low — `#[serde(default)]` is a well-established pattern, but bincode v2's specific handling must be verified.
**Impact**: Critical — total data loss on schema evolution if broken. This is foundational to the entire product's evolution strategy (M1 through M9).

### R5: Monotonic ID Generation Correctness (HIGH)

**Category**: Concurrency / Data Integrity
**Description**: The COUNTERS table generates entry IDs via atomic read-increment-write within a write transaction (AC-05). If IDs are ever duplicated, an insert overwrites an existing entry. If IDs skip, it's wasteful but safe. redb's single-writer model prevents true concurrent writes, but the read-modify-write pattern must be correct.
**Likelihood**: Low — single-writer model prevents races. But bugs in the counter logic (off-by-one, missing increment, counter not initialized) would be catastrophic.
**Impact**: Critical — duplicate IDs silently overwrite existing entries.

### R6: Transaction Atomicity on Partial Failure (HIGH)

**Category**: Error Handling
**Description**: If a write to one of the index tables fails mid-transaction (e.g., serialization error on a specific entry, disk full), the entire transaction must abort cleanly (AC-04). redb provides this guarantee via drop-without-commit = abort, but the API must not catch and swallow errors in a way that commits a partial transaction.
**Likelihood**: Low — redb's transaction model is robust. Risk is in application-level error handling that might call commit() in an error path.
**Impact**: Critical — partial writes leave the database in an inconsistent state.

### R7: QueryFilter Multi-Index Intersection Correctness (HIGH)

**Category**: Data Integrity
**Description**: The `query(QueryFilter)` function (AC-17) intersects result sets from multiple index queries (topic, category, tags, status, time_range). The intersection logic must handle: all fields empty (return all active), single field, all fields, disjoint sets (empty result), and partial overlaps. Empty filter must default to all active entries.
**Likelihood**: Medium — set intersection across heterogeneous index types with optional fields has many corner cases.
**Impact**: High — wrong query results propagate to all downstream consumers.

### R8: Status Transition Atomicity (HIGH)

**Category**: Data Integrity
**Description**: Status changes (AC-12) require removing the old `(status_byte, entry_id)` from STATUS_INDEX, inserting the new one, updating the serialized EntryRecord in ENTRIES, and updating COUNTERS (decrement old status count, increment new). If any step is missed, status queries return stale results or counters drift.
**Likelihood**: Medium — four coordinated operations across three tables.
**Impact**: High — status filtering is used by every downstream query path.

### R9: Tag Index Set Operations (MEDIUM)

**Category**: Data Integrity / Performance
**Description**: TAG_INDEX uses MultimapTable (one tag → many entry_ids). Tag queries (AC-09) require intersection across multiple tags. The MultimapTable API differs from regular Table — iteration, insertion, and removal have different semantics. Tag removal on entry update must remove only the specific entry_id from each tag's set, not the entire tag entry.
**Likelihood**: Medium — MultimapTable API differences from Table are a common source of bugs.
**Impact**: Medium-High — incorrect tag queries affect context_lookup accuracy.

### R10: Database Lifecycle (Open/Create/Compact) (MEDIUM)

**Category**: Integration
**Description**: Database open-or-create (AC-14) must handle: fresh creation (no file), opening existing file, corrupted file, wrong permissions, cache size configuration, and compaction. Compaction on a database with active read transactions could be problematic.
**Likelihood**: Low — redb handles most of this. Risk is in error handling and lifecycle sequencing.
**Impact**: Medium — startup failures or data loss during compaction.

### R11: VECTOR_MAP Bridge Table Correctness (MEDIUM)

**Category**: Integration
**Description**: VECTOR_MAP stores entry_id → u64 (hnsw_data_id) for use by nxs-002 (AC-13). The value is stored as u64 in redb but used as usize by hnsw_rs. On 64-bit platforms this is identical; on 32-bit it would truncate. Insert and lookup must be correct and the table must survive entry updates that don't change the vector mapping.
**Likelihood**: Low — simple key-value table. Platform-specific usize concern is theoretical (no 32-bit target planned).
**Impact**: Medium — broken bridge table would block nxs-002 entirely.

### R12: Error Type Discrimination (MEDIUM)

**Category**: Error Handling
**Description**: All public API functions must return typed Result errors distinguishing redb errors, serialization errors, and application-level constraint violations like entry-not-found and duplicate-ID (AC-15). Downstream consumers (MCP server, CLI) depend on error types to produce appropriate user-facing messages.
**Likelihood**: Low — straightforward enum design.
**Impact**: Medium — poor error types cause confusing downstream error messages.

---

## 2. Risk-to-Testing-Scenario Mapping

### R1: Index-Entry Desynchronization

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Insert entry, verify all 6 index tables contain matching entries | Integration | AC-04, AC-07, AC-08, AC-09, AC-10, AC-11 |
| Insert entry, read back via each individual index query, compare to direct ENTRIES lookup | Integration | AC-06, AC-07, AC-08, AC-09, AC-10, AC-11 |
| Insert 50 entries with varied topics/categories/tags/statuses, verify every entry is reachable via every applicable index | Integration | AC-04, AC-17 |
| Insert entry with multiple tags, verify TAG_INDEX multimap contains entry under each tag | Unit | AC-09 |
| Insert entry, verify TIME_INDEX contains (created_at, entry_id) tuple | Unit | AC-10 |
| Insert entry, verify COUNTERS incremented (next_entry_id, total_active) | Unit | AC-05 |

### R2: Update Path Stale Index Orphaning

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Update entry topic from "auth" to "security": verify old topic index entry removed, new one present | Integration | AC-18 |
| Update entry category: verify old category index entry removed, new one present | Integration | AC-18 |
| Update entry tags (add one, remove one, keep one): verify TAG_INDEX reflects exact new tag set | Integration | AC-18 |
| Update entry status via update path (not dedicated status-change): verify STATUS_INDEX updated | Integration | AC-18, AC-12 |
| Update entry with no indexed field changes: verify all indexes unchanged | Integration | AC-18 |
| Update multiple indexed fields simultaneously (topic + category + tags): verify all stale entries removed, all new entries inserted | Integration | AC-18 |
| Query by OLD topic after update: verify updated entry NOT returned | Integration | AC-18, AC-07 |
| Query by NEW topic after update: verify updated entry IS returned | Integration | AC-18, AC-07 |

### R3: Bincode Serialization Round-Trip Fidelity

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Round-trip all field types: String, Vec<String>, u64, u32, u16, f32, Option<u64>, Status enum | Unit | AC-02 |
| Round-trip with empty strings (title, content, topic, category, source) | Unit | AC-02 |
| Round-trip with empty tags Vec | Unit | AC-02 |
| Round-trip with f32 edge values: 0.0, 1.0, f32::MIN_POSITIVE, 0.999999 | Unit | AC-02 |
| Round-trip with u64 boundary values: 0, 1, u64::MAX - 1, u64::MAX | Unit | AC-02 |
| Round-trip with Option fields: None vs Some | Unit | AC-02 |
| Round-trip with all three Status variants | Unit | AC-02 |
| Round-trip with large content string (100KB) | Unit | AC-02 |
| Round-trip with unicode content (emoji, CJK, RTL, combining characters) | Unit | AC-02 |

### R4: Schema Evolution via serde(default)

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Serialize a "v1" struct (subset of fields), deserialize as full EntryRecord: defaults applied correctly | Unit | AC-16 |
| Serialize full EntryRecord, add a new `#[serde(default)]` field to struct, deserialize old bytes: new field gets default | Unit | AC-16 |
| Serialize with all Option fields as Some, deserialize into struct with additional Option field: existing fields preserved, new field is None | Unit | AC-16 |
| Verify that field ordering does not affect deserialization (bincode v2 behavior) | Unit | AC-16 |

### R5: Monotonic ID Generation Correctness

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Insert 100 entries sequentially: all IDs unique and monotonically increasing | Integration | AC-05 |
| First insert on fresh database: ID is 0 or 1 (verify initial value) | Unit | AC-05 |
| Read counter value matches number of entries inserted | Unit | AC-05 |
| Insert, crash simulation (abort transaction), insert again: no ID gap causes issues, no ID reuse | Integration | AC-05 |

### R6: Transaction Atomicity on Partial Failure

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Verify that dropping a write transaction without commit leaves database unchanged | Unit | AC-04 |
| Insert entry, verify if error occurs after ENTRIES write but before index writes, no partial state persists (redb guarantees this — verify our code doesn't circumvent it) | Integration | AC-04 |
| Verify error propagation: serialization error surfaces as typed error, not panic | Unit | AC-15 |

### R7: QueryFilter Multi-Index Intersection Correctness

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Empty QueryFilter: returns all active entries | Integration | AC-17 |
| Single-field filter (topic only, category only, status only, time_range only, tags only) | Integration | AC-17 |
| Two-field combination: topic + category | Integration | AC-17 |
| Two-field combination: tags + status | Integration | AC-17 |
| Three-field combination: topic + tags + time_range | Integration | AC-17 |
| All fields populated: topic + category + tags + status + time_range | Integration | AC-17 |
| Disjoint filter (topic exists, category exists, but no entry matches both): empty result | Integration | AC-17 |
| Filter with non-existent topic: empty result, no error | Integration | AC-17 |
| Filter with status=Deprecated: returns only deprecated entries | Integration | AC-17, AC-11 |
| 50 entries, 5 topics, 3 categories, 10 tags — verify QueryFilter returns correct subsets | Property | AC-17 |

### R8: Status Transition Atomicity

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Change Active → Deprecated: verify STATUS_INDEX updated, ENTRIES updated, COUNTERS updated | Integration | AC-12 |
| Change Active → Deprecated: verify entry no longer appears in STATUS_INDEX Active range scan | Integration | AC-12, AC-11 |
| Change Active → Deprecated: verify entry appears in STATUS_INDEX Deprecated range scan | Integration | AC-12, AC-11 |
| Change Proposed → Active: verify counters reflect new status | Integration | AC-12 |
| Verify counter consistency: total_active + total_deprecated matches actual STATUS_INDEX scan counts | Integration | AC-12 |

### R9: Tag Index Set Operations

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Single tag query: returns all entries with that tag | Unit | AC-09 |
| Two-tag intersection: returns only entries with BOTH tags | Unit | AC-09 |
| Three-tag intersection where only 1 entry matches all three | Unit | AC-09 |
| Tag intersection with one non-existent tag: empty result | Unit | AC-09 |
| Entry with 20 tags: all 20 tags indexed correctly | Integration | AC-09 |
| Remove one tag on update: entry no longer appears under removed tag | Integration | AC-09, AC-18 |

### R10: Database Lifecycle

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Open non-existent path: creates new database with all 8 tables | Integration | AC-03, AC-14 |
| Open existing database: all tables accessible, data preserved | Integration | AC-14 |
| Configure cache size: non-default value accepted | Unit | AC-14 |
| Compact database: file size decreases or stays same, data preserved | Integration | AC-14 |
| Compact after inserts + deletes: data integrity preserved | Integration | AC-14 |

### R11: VECTOR_MAP Bridge Table

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Insert mapping (entry_id → hnsw_data_id), read back: correct value | Unit | AC-13 |
| Insert mapping for entry_id that already has a mapping: overwrites cleanly | Unit | AC-13 |
| Lookup non-existent entry_id: returns None, not error | Unit | AC-13 |
| Store u64::MAX as data_id: round-trips correctly | Unit | AC-13 |

### R12: Error Type Discrimination

| Scenario | Test Type | ACs |
|----------|-----------|-----|
| Get non-existent entry by ID: returns application-level EntryNotFound error | Unit | AC-15 |
| Distinguish redb error from serialization error from application error in error type | Unit | AC-15 |
| All public functions return Result, not panic | Unit (compile-time + runtime) | AC-15 |

---

## 3. Coverage Requirements per Risk

| Risk | Severity | Required Coverage |
|------|----------|-------------------|
| R1: Index-Entry Desync | Critical | Multiple dedicated integration tests; per-index verification after every write path |
| R2: Update Path Orphaning | Critical | Dedicated integration suite covering every indexed field individually and in combination |
| R3: Serialization Round-Trip | High | Comprehensive unit tests for every field type and edge value; property tests for random EntryRecords |
| R4: Schema Evolution | High | Dedicated unit tests simulating version skew with hardcoded byte fixtures |
| R5: Monotonic ID | High | Integration test with 100+ sequential inserts; property test for uniqueness |
| R6: Transaction Atomicity | High | Integration test verifying abort-on-drop; code review for error paths |
| R7: QueryFilter Intersection | High | Combinatorial integration tests for all filter field combinations; property tests with random entry sets |
| R8: Status Transition | High | Dedicated integration test per transition; counter consistency assertions |
| R9: Tag Index Operations | Medium-High | Unit tests for single/multi-tag queries; integration tests for tag mutation on update |
| R10: Database Lifecycle | Medium | Integration tests for create/open/compact cycle |
| R11: VECTOR_MAP Bridge | Medium | Unit tests for CRUD operations; boundary value tests |
| R12: Error Types | Medium | Unit tests for each error variant; compile-time coverage via exhaustive match |

---

## 4. Prioritization

### Top 5 Risks (ranked by severity x likelihood)

**1. R2: Update Path Stale Index Orphaning** — CRITICAL, HIGH likelihood
The most complex write path in the entire feature. Field-level diffing across 5 indexed dimensions with two different table types. A bug here means phantom query results that silently corrupt every downstream feature. Test this exhaustively.

**2. R1: Index-Entry Desynchronization** — CRITICAL, MEDIUM likelihood
Every write touches multiple tables. While simpler than the update path (no diffing), the sheer number of index tables (6) means any missed table in the write path creates invisible data loss. Every write operation must have index-verification assertions.

**3. R7: QueryFilter Multi-Index Intersection Correctness** — HIGH, MEDIUM likelihood
The primary read path for all downstream features. Combinatorial explosion of filter field combinations. Property testing is essential to catch intersection edge cases that manual test cases miss.

**4. R4: Schema Evolution via serde(default)** — CRITICAL impact, LOW likelihood
Low likelihood because the pattern is well-established, but the impact is total data loss on schema change. A single test with hardcoded serialized bytes from "v1" provides high-confidence protection for the entire product roadmap.

**5. R8: Status Transition Atomicity** — HIGH, MEDIUM likelihood
Status is used as a filter in every query path. Counter drift accumulates silently and manifests as wrong stats in context_status. Four coordinated operations across three tables per transition.

### Risks 6-12 (lower priority, still tested)

6. R5: Monotonic ID — catastrophic if broken but very low likelihood
7. R3: Serialization Round-Trip — high impact but bincode is reliable
8. R6: Transaction Atomicity — redb guarantees this; verify we don't circumvent
9. R9: Tag Index Operations — tested as part of R1/R2/R7 scenarios
10. R10: Database Lifecycle — straightforward, tested with integration setup
11. R12: Error Types — tested via negative-path unit tests
12. R11: VECTOR_MAP Bridge — simple table, low risk

---

## 5. Test Strategy Recommendations

### 5.1 Temp Database Fixture

Every test gets a fresh database via `tempfile::TempDir`. This is the foundational fixture reused by all downstream features.

```
fn test_db() -> (TempDir, Database)
    - Creates TempDir
    - Opens database with all 8 tables created
    - Returns both (TempDir kept alive for RAII cleanup)
```

This fixture MUST be designed for reuse by nxs-002, vnc-001, and all subsequent features. Place in a `test_helpers` module that is `#[cfg(test)]` pub within the crate and exported via a `test-support` feature flag for downstream crate testing.

### 5.2 Entry Builders

A builder pattern for constructing test EntryRecords with sensible defaults:

```
TestEntry::new("my-topic", "my-category")
    .with_tags(&["tag1", "tag2"])
    .with_status(Status::Active)
    .with_content("detailed content here")
    .build()  // -> EntryRecord with generated timestamps, default source, etc.
```

The builder must:
- Auto-generate unique titles if not specified
- Use current timestamp for created_at/updated_at
- Default to Status::Active
- Default to empty tags if not specified
- Be reusable by downstream features

### 5.3 Assertion Helpers

Dedicated assertion functions that verify index consistency after writes:

```
assert_index_consistent(db, entry_id)
    - Reads EntryRecord from ENTRIES
    - Verifies TOPIC_INDEX contains (topic, entry_id)
    - Verifies CATEGORY_INDEX contains (category, entry_id)
    - Verifies TAG_INDEX contains entry_id under each tag
    - Verifies TIME_INDEX contains (created_at, entry_id)
    - Verifies STATUS_INDEX contains (status, entry_id)
    - Verifies COUNTERS are consistent

assert_index_absent(db, entry_id, old_topic, old_category, old_tags, old_status)
    - Verifies stale index entries do NOT exist for old values
```

These helpers eliminate repetitive verification code across tests and ensure consistent coverage.

### 5.4 Property Testing

Use `proptest` or `quickcheck` for:

1. **Serialization round-trip**: Generate random EntryRecords, verify `deserialize(serialize(record)) == record`.
2. **QueryFilter correctness**: Generate random entry sets (varied topics, categories, tags, statuses), insert all, then generate random QueryFilters and verify results match a naive brute-force filter over all entries.
3. **Monotonic ID uniqueness**: Insert N random entries, verify all IDs unique and monotonically increasing.

Property testing is particularly valuable for R7 (QueryFilter intersection) where the combinatorial space is too large for manual test cases.

### 5.5 Downstream Reuse Design (AC-19)

Test infrastructure must be structured for reuse:

- **`test_helpers` module**: Contains `test_db()`, `TestEntry` builder, assertion helpers. Accessible via `#[cfg(test)]` within the crate and via `test-support` Cargo feature for external crates.
- **Seeded test databases**: Functions that create databases pre-populated with known entry sets (e.g., `seed_50_entries(db)` with deterministic topics/categories/tags for predictable query results).
- **No hardcoded paths**: All tests use TempDir. No reliance on filesystem state.
- **No test interdependence**: Each test creates its own database. No shared mutable state.

### 5.6 Test Organization

```
tests/
├── schema.rs           # R3, R4: serialization round-trip, schema evolution
├── insert.rs           # R1, R5, R6: insert path, index consistency, ID generation, atomicity
├── update.rs           # R2: update path, stale index removal, multi-field changes
├── query.rs            # R7: QueryFilter intersection, individual index queries (AC-07–AC-11)
├── status.rs           # R8: status transitions, counter consistency
├── tags.rs             # R9: tag index operations, intersection
├── lifecycle.rs        # R10: open/create/compact/cache
├── vector_map.rs       # R11: VECTOR_MAP bridge operations
├── errors.rs           # R12: error type discrimination
└── property.rs         # Property tests: round-trip, QueryFilter, ID uniqueness
```

---

## 6. Edge Cases and Failure Modes

### Empty / Nil Values

| Case | Expected Behavior |
|------|-------------------|
| Empty topic string `""` | Stored and indexed; TOPIC_INDEX has `("", entry_id)` key. Query by `""` returns it. |
| Empty category string `""` | Same as empty topic. |
| Empty tags `vec![]` | No TAG_INDEX entries created. Tag queries never match this entry. |
| Empty content string `""` | Stored. bincode round-trips correctly. |
| Empty QueryFilter (all fields None) | Returns all active entries (AC-17 explicit requirement). |

### Maximum / Boundary Values

| Case | Expected Behavior |
|------|-------------------|
| Large content string (1 MB) | bincode serializes; redb stores. May be slow but must not fail. |
| Entry with 100 tags | All 100 tags indexed. Intersection queries work. |
| u64::MAX as timestamp | TIME_INDEX stores correctly; range queries including MAX work. |
| u64::MAX as entry_id in VECTOR_MAP | Round-trips correctly. |
| f32 confidence = 0.0 | Stored and retrieved as 0.0. |
| f32 confidence = NaN | Undefined behavior in ordering but must not crash serialization. Document as invalid input. |
| Topic string with 10,000 characters | Stored in TOPIC_INDEX as compound key. Performance may degrade but must not fail. |

### Unicode and Encoding

| Case | Expected Behavior |
|------|-------------------|
| Unicode topic (e.g., "認証") | Stored and queryable. redb sorts by byte order (UTF-8). |
| Emoji in content ("🔐 auth tokens") | bincode serializes correctly. Round-trip preserves emoji. |
| Combining characters (é as e + combining accent) | Stored as-is. No normalization. Two entries with different Unicode normalizations are distinct topics. |
| Null bytes in strings | bincode handles. redb `&str` keys may not — verify behavior. |

### Status Transition Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| Active → Active (no-op status change) | No STATUS_INDEX modification needed. Must not create duplicate index entry. |
| Deprecated → Active (reactivation) | STATUS_INDEX updated. COUNTERS updated. |
| Proposed → Deprecated (skip Active) | Valid transition. Counters adjusted correctly. |
| Two consecutive status changes on same entry | Both atomic. Final state is correct. |

### Tag Intersection Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| Intersection of 0 tags (empty tag filter) | Should not filter by tags (return all candidates from other filters). |
| Intersection of 1 tag | Returns all entries with that tag (no intersection needed). |
| Intersection where one tag has 0 entries | Result is empty set. |
| Intersection where one tag has 10,000 entries and another has 1 | Result is at most 1 entry. Intersection should iterate the smaller set. |
| Same tag listed twice in filter | Same result as listing it once. |
| Tag that was removed from all entries | TAG_INDEX may still have the key with empty value set, or key may not exist. Both must work. |

### Time Range Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| start_time == end_time | Returns entries with exactly that timestamp. |
| start_time > end_time (inverted range) | Returns empty set, not error. |
| start_time = 0 | Returns entries from the beginning of time. |
| end_time = u64::MAX | Returns entries up to the maximum timestamp. |
| No entries in time range | Returns empty set, not error. |
| Multiple entries with same timestamp | All returned; distinguished by entry_id in compound key. |

### QueryFilter Combination Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| Topic matches 100 entries, category matches 0 → intersection is empty | Empty result, not error. |
| All filters match the same single entry | Returns that one entry. |
| Tags filter + status filter where tag matches but all matching entries have wrong status | Empty result. |
| Time range filter where all entries are outside the range | Empty result; topic/category filters never even evaluated (optimization opportunity, not requirement). |

### Simultaneous Index Field Updates

| Case | Expected Behavior |
|------|-------------------|
| Change topic AND category AND tags AND status in one update | All four index tables updated atomically. Old entries removed from all four indexes. New entries inserted in all four. |
| Change topic but keep same category | Only TOPIC_INDEX updated. CATEGORY_INDEX untouched. |
| Add a tag without removing any | TAG_INDEX gains one new entry. Existing tag entries unchanged. |
| Remove all tags | All TAG_INDEX entries for this entry removed. |
| Change to same topic (e.g., "auth" → "auth") | No TOPIC_INDEX modification. Idempotent. |

### Database Lifecycle Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| Open database, insert nothing, close, reopen | Database file exists, all tables created, COUNTERS initialized. |
| Compact empty database | No error. File size may decrease to minimum. |
| Open with cache_size = 0 | redb should handle (may use minimum internal cache). No panic. |
| Open same database path twice (two Database instances) | redb file lock prevents this. Error returned, not deadlock. |

---

## 7. Open Questions

1. **bincode v2 configuration**: Which bincode v2 `Configuration` should be used? The default (`standard`) vs `legacy` affects how `#[serde(default)]` fields are handled. The schema evolution tests (R4) must verify the chosen configuration before any data is persisted.

2. **Null bytes in string keys**: redb's `&str` Key implementation may or may not handle strings containing null bytes. If TOPIC_INDEX or CATEGORY_INDEX keys can contain null bytes (from user-provided content), this needs testing. Recommend: document that topic/category/tag strings must be valid UTF-8 without null bytes, and validate at the API boundary.

3. **Tag removal semantics in MultimapTable**: When removing the last entry_id from a tag in TAG_INDEX, does the tag key remain (with empty value set) or is it automatically cleaned up? This affects tag enumeration if we ever list all known tags.

4. **Counter initialization on fresh database**: Should COUNTERS be initialized with `next_entry_id=0` during table creation, or lazily on first insert? The former is simpler to reason about; the latter avoids an extra write on database creation. Recommend: eager initialization.

5. **Update path for entries without all indexes**: If an entry was inserted before a code change that adds a new index (hypothetical future), the update path's "read old, diff, remove stale" logic must handle missing old index entries gracefully. This is a forward-compatibility concern.
