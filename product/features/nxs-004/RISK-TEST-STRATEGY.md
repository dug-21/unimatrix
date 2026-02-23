# Risk-Based Test Strategy: nxs-004

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Schema migration corrupts or loses existing entries | High | Medium | Critical |
| R-02 | Content hash computation inconsistency between insert and update paths | High | Medium | Critical |
| R-03 | Version counter desync (skips, resets, or fails to increment) | Medium | Medium | High |
| R-04 | Legacy deserialization fails on pre-nxs-004 entries | High | Medium | Critical |
| R-05 | Trait object safety violation prevents dyn usage | High | Low | High |
| R-06 | Async wrapper deadlock or task panic loss | Medium | Low | Medium |
| R-07 | NewEntry backward compatibility broken (existing callers fail to compile) | High | Medium | Critical |
| R-08 | Domain adapter error conversion loses error context | Medium | Medium | High |
| R-09 | Migration runs on every open (schema_version not persisted) | Low | Low | Low |
| R-10 | Previous_hash chain broken on sequential updates | Medium | Medium | High |
| R-11 | Re-export gaps (consumer can't find types from unimatrix-core) | Medium | Low | Medium |
| R-12 | Existing tests fail after EntryRecord schema change | High | High | Critical |

## Risk-to-Scenario Mapping

### R-01: Schema Migration Corrupts or Loses Existing Entries

**Severity**: High
**Likelihood**: Medium
**Impact**: All stored knowledge is lost or corrupted. Database becomes unusable.

**Test Scenarios**:
1. Create a database with pre-nxs-004 Store::open(), insert 10 entries with various fields populated. Close. Reopen with nxs-004 Store::open(). Verify all 10 entries are readable and original fields match.
2. Create a database with entries, run migration, verify entry count before and after is identical.
3. Create a database with entries having Unicode content, special characters in title/content. Migrate. Verify content integrity.
4. Create an empty database (no entries). Run migration. Verify schema_version is set and no crash occurs.
5. Verify migration populates content_hash correctly by computing SHA-256 of existing title+content and comparing.

**Coverage Requirement**: Migration must be tested with: empty DB, single entry, multiple entries, entries with edge-case content (empty strings, Unicode, max-length), entries with all Status variants.

### R-02: Content Hash Computation Inconsistency

**Severity**: High
**Likelihood**: Medium
**Impact**: Hash chain is unreliable. Tamper detection becomes useless. content_hash from insert differs from content_hash computed during migration for identical content.

**Test Scenarios**:
1. Insert an entry with known title and content. Read back. Verify content_hash matches independently computed SHA-256 of `"{title}: {content}"`.
2. Insert entry, then update with identical title and content. Verify content_hash is unchanged (same input = same hash).
3. Insert entry with empty title. Verify hash is SHA-256 of content only.
4. Insert entry with empty content. Verify hash is SHA-256 of title only.
5. Insert entry with both empty. Verify hash is SHA-256 of empty string.
6. Verify migration-computed content_hash matches insert-computed content_hash for identical content.

**Coverage Requirement**: Hash computation tested for all `prepare_text` branches (both present, title-only, content-only, both empty). Hash output format verified (lowercase hex, 64 chars).

### R-03: Version Counter Desync

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Version field becomes unreliable. Cannot determine how many times an entry has been modified.

**Test Scenarios**:
1. Insert entry. Verify version = 1.
2. Update entry once. Verify version = 2.
3. Update entry 10 times. Verify version = 11.
4. Update status (not full update). Verify version does NOT change (status update is not a content update).
5. Migrate pre-nxs-004 entry. Verify version = 1 post-migration.

**Coverage Requirement**: Version tracking tested through insert, single update, multiple updates, and migration. Verify status-only updates do not increment version.

### R-04: Legacy Deserialization Fails on Pre-nxs-004 Entries

**Severity**: High
**Likelihood**: Medium
**Impact**: Migration crashes on open, database is inaccessible.

**Test Scenarios**:
1. Serialize an EntryRecord with the OLD schema (17 fields, no security fields) using bincode. Attempt to deserialize with the NEW schema. Verify this fails (bincode positional encoding).
2. Use legacy deserialization path to read old-format entries and convert to new format. Verify all original fields are preserved.
3. Create a real pre-nxs-004 database (build with current Store::insert, which writes 17-field records). Run nxs-004 migration. Verify success.

**Coverage Requirement**: Legacy deserialization must be tested with actual bincode bytes from the old schema. The migration path must handle the binary format transition explicitly.

### R-05: Trait Object Safety Violation

**Severity**: High
**Likelihood**: Low
**Impact**: `dyn EntryStore` unusable. MCP server architecture must change to generics-only.

**Test Scenarios**:
1. Compile-time check: `fn _check(_: &dyn EntryStore) {}` compiles.
2. Compile-time check: `fn _check(_: &dyn VectorStore) {}` compiles.
3. Compile-time check: `fn _check(_: &dyn EmbedService) {}` compiles.
4. Compile-time check: `fn _check(_: Arc<dyn EntryStore>) {}` compiles.
5. Construct `Box<dyn EntryStore>` from a `StoreAdapter`. Call methods through the trait object.

**Coverage Requirement**: All three traits must pass object-safety compilation checks. At least one integration test uses `dyn Trait` invocation.

### R-06: Async Wrapper Deadlock or Task Panic Loss

**Severity**: Medium
**Likelihood**: Low
**Impact**: MCP server hangs or silently drops errors from blocking tasks.

**Test Scenarios**:
1. Call async wrapper method for a successful operation. Verify result matches sync equivalent.
2. Call async wrapper method for a failing operation (e.g., get nonexistent entry). Verify error is propagated as CoreError.
3. Verify JoinError is converted to CoreError::JoinError (simulate by testing the conversion path).

**Coverage Requirement**: Async wrappers tested for success and failure paths. Error propagation verified.

### R-07: NewEntry Backward Compatibility Broken

**Severity**: High
**Likelihood**: Medium
**Impact**: All existing code that constructs NewEntry fails to compile (missing new required fields).

**Test Scenarios**:
1. All existing unimatrix-store tests must compile and pass after NewEntry extension. This means existing test code that constructs NewEntry must be updated to include the new fields (or the test helper must supply defaults).
2. TestEntry builder in test_helpers must be extended to include new fields with sensible defaults.

**Coverage Requirement**: All 85 existing unimatrix-store tests pass. TestEntry builder provides default values for new fields.

### R-08: Domain Adapter Error Conversion Loses Context

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Error messages seen by consumers lose detail about the underlying failure.

**Test Scenarios**:
1. Trigger a StoreError (e.g., EntryNotFound) through StoreAdapter. Verify CoreError preserves the original error and its Display message.
2. Trigger a VectorError (e.g., DimensionMismatch) through VectorAdapter. Verify error detail preserved.
3. Verify `CoreError::source()` returns the underlying error.

**Coverage Requirement**: Error conversion tested for at least one error variant per source crate.

### R-09: Migration Runs on Every Open

**Severity**: Low
**Likelihood**: Low
**Impact**: Minor performance degradation on startup. Not a correctness issue.

**Test Scenarios**:
1. Open database, close, reopen. Verify migration does not run a second time (schema_version already at current).
2. Verify schema_version counter is readable after migration.

**Coverage Requirement**: Double-open test confirms idempotency.

### R-10: Previous_hash Chain Broken on Sequential Updates

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Hash chain does not accurately reflect update history. Previous_hash points to wrong version.

**Test Scenarios**:
1. Insert entry (hash=H1, previous_hash=""). Update title (hash=H2, previous_hash=H1). Update content (hash=H3, previous_hash=H2). Verify chain: H3 -> H2 -> H1 -> "".
2. Update entry without changing title or content. Verify content_hash unchanged and previous_hash is set to the old (identical) content_hash.

**Coverage Requirement**: Hash chain tested through at least 3 sequential updates. Verify previous_hash at each step.

### R-11: Re-export Gaps

**Severity**: Medium
**Likelihood**: Low
**Impact**: Consumer must add extra dependencies instead of using unimatrix-core alone.

**Test Scenarios**:
1. Write a test in unimatrix-core that imports all re-exported types and uses them (EntryRecord, NewEntry, QueryFilter, Status, TimeRange, SearchResult, etc.).
2. Verify no `pub use` path is missing for types needed by vnc-001.

**Coverage Requirement**: Compilation test importing all re-exported types.

### R-12: Existing Tests Fail After EntryRecord Schema Change

**Severity**: High
**Likelihood**: High
**Impact**: Regression in existing functionality.

**Test Scenarios**:
1. Run `cargo test -p unimatrix-store` -- all 85 tests pass.
2. Run `cargo test -p unimatrix-vector` -- all 85 tests pass.
3. Run `cargo test -p unimatrix-embed` -- all 76 tests pass.
4. Verify no test constructing EntryRecord directly is broken by the new fields (test helpers updated).

**Coverage Requirement**: Full test suite passes for all three existing crates.

## Integration Risks

### IR-01: unimatrix-vector depends on unimatrix-store's EntryRecord

VectorIndex doesn't use EntryRecord directly (it works with entry IDs and embeddings), but the test helpers in unimatrix-vector create `NewEntry` instances via `unimatrix-store::NewEntry`. These test helpers must be updated to include the new fields.

**Test Scenario**: `cargo test -p unimatrix-vector` passes after schema change.

### IR-02: unimatrix-core depends on all three crates

Circular dependency risk: if any lower crate imports from core, there's a cycle. The architecture ensures one-way dependency (core -> store/vector/embed).

**Test Scenario**: `cargo build` succeeds. No circular dependency errors.

### IR-03: Migration interacts with counter.rs

The migration reads from and writes to the COUNTERS table, which is also used by `next_entry_id` and status counters. Migration must not corrupt these counters.

**Test Scenario**: Insert entries, migrate, verify `next_entry_id` and status counters unchanged.

### IR-04: Content hash computation depends on title format

If `prepare_text` semantics in unimatrix-embed change, the hash format diverges from the embedding format. This is a cross-crate alignment risk.

**Test Scenario**: Verify `compute_content_hash("title", "content")` produces the same text as `prepare_text("title", "content", ": ")` fed to SHA-256.

## Edge Cases

### EC-01: EntryRecord with Maximum Field Lengths

Insert an entry with title = 10KB, content = 100KB, tags = 100 items. Verify content_hash is computed correctly and migration handles large entries.

### EC-02: Entry with All-Default Security Fields

NewEntry with `created_by = ""`, `feature_cycle = ""`, `trust_source = ""`. Verify insert succeeds, content_hash is computed, version = 1.

### EC-03: Concurrent Opens with Migration

Two processes open the same database file simultaneously when migration is needed. redb's file locking should prevent corruption, but verify migration completes once and subsequent opens see the migrated state.

### EC-04: Update Without Content Change

Update an entry changing only metadata fields (e.g., tags, topic, category) but not title or content. Verify: content_hash unchanged, version increments, previous_hash set to current (identical) content_hash.

### EC-05: Migration of Zero Entries

Database with tables created but no entries. Migration should set schema_version without scanning.

### EC-06: SHA-256 of Unicode Content

Content hash of entries with CJK characters, emoji, combining characters. Verify deterministic hash output.

## Security Risks

### SR-01: Content Hash Bypass

**Untrusted input**: Callers could provide a pre-computed content_hash to bypass integrity checking.
**Assessment**: Not a risk in nxs-004 -- content_hash is engine-computed, not caller-provided. The caller has no way to set content_hash on insert or update. The engine always overwrites it.
**Blast radius**: None -- the engine is the sole authority on content_hash.

### SR-02: Trust Source Spoofing

**Untrusted input**: Callers set `trust_source` on NewEntry. A malicious caller could set `trust_source = "human"` for agent-written entries.
**Assessment**: This is a known limitation accepted in the security research. At the storage layer (nxs-004), trust_source is a string field with no enforcement. Enforcement happens at the MCP layer (vnc-001) which validates the caller's identity before setting trust_source.
**Blast radius**: Misleading attribution in stored entries. Mitigated by vnc-001's agent identity pipeline.

### SR-03: Migration Deserialization Attack

**Untrusted input**: If an attacker could write arbitrary bytes to the ENTRIES table before migration runs, the legacy deserialization path could encounter crafted input.
**Assessment**: Low risk. The attacker would need direct file system access to the redb database. At that point, they have full system compromise and storage layer defenses are irrelevant.
**Blast radius**: Migration failure (transaction rollback, database at old version). Not data loss.

### SR-04: SHA-256 Collision

**Untrusted input**: Theoretically, two different entries could have the same content_hash.
**Assessment**: SHA-256 has no known practical collision attacks. At Unimatrix scale (<100K entries), the probability of accidental collision is negligible (~2^-256).
**Blast radius**: Two entries would share a content_hash. The previous_hash chain would still be distinct per entry.

## Failure Modes

### FM-01: Migration Transaction Failure

**Behavior**: If the migration write transaction fails (I/O error, disk full), redb rolls back the entire transaction. The database remains at the old schema version. The next `Store::open()` call retries the migration.
**Recovery**: Automatic retry on next open. If persistent I/O failure, requires disk space resolution.

### FM-02: SHA-256 Computation Failure

**Behavior**: SHA-256 computation is a pure function that cannot fail. The `sha2` crate has no error paths for `Digest::digest()`. This failure mode does not exist.

### FM-03: Async Wrapper Task Panic

**Behavior**: If the blocking task panics inside `spawn_blocking`, tokio converts this to a `JoinError`. The async wrapper converts this to `CoreError::JoinError(message)`. The caller receives an error, not a hang.
**Recovery**: Caller handles the error. The underlying data is not corrupted (redb transactions are atomic).

### FM-04: Incompatible bincode Version

**Behavior**: If bincode crate version changes (e.g., during dependency update), existing serialized data may not deserialize. This is mitigated by the workspace pinning bincode to version 2 in Cargo.toml.
**Recovery**: Restore from backup or pin bincode version.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-04, R-07, R-12) | 18 scenarios |
| High | 4 (R-03, R-05, R-08, R-10) | 14 scenarios |
| Medium | 3 (R-06, R-09, R-11) | 6 scenarios |
| Low | 0 | 0 scenarios |
| **Total** | **12 risks** | **38 scenarios** |

## Test Priority Order

1. **R-12**: Existing tests pass (gate: if these fail, nothing else matters)
2. **R-04**: Legacy deserialization (gate: if migration can't read old data, it's DOA)
3. **R-01**: Migration correctness (gate: data integrity)
4. **R-07**: NewEntry compatibility (gate: compilation)
5. **R-02**: Content hash consistency
6. **R-10**: Previous_hash chain
7. **R-03**: Version counter
8. **R-05**: Trait object safety
9. **R-08**: Error conversion
10. **R-06**: Async wrappers
11. **R-11**: Re-export coverage
12. **R-09**: Migration idempotency
