# Test Plan: migration

## Scope

Verify scan-and-rewrite migration from schema v0 to v1, legacy deserialization, and migration idempotency.

## Unit Tests (in crates/unimatrix-store/src/migration.rs)

### test_legacy_entry_record_deserialization
- Construct a LegacyEntryRecord with all 17 fields populated.
- Serialize it with bincode serde path.
- Deserialize with `deserialize_legacy_entry()`.
- Assert all 17 fields match.
- This validates R-04: the legacy deserialization path works.

### test_legacy_deserialization_all_status_variants
- For each Status variant (Active, Deprecated, Proposed):
  - Construct a LegacyEntryRecord with that status.
  - Serialize, deserialize. Assert status preserved.

## Integration Tests (in crates/unimatrix-store, tests/ or inline)

### test_migration_preserves_entries
- Open a Store (this creates a v0 database because schema_version counter does not exist yet).
- Insert 10 entries with varied fields (different topics, categories, tags, statuses).
- Close the Store (drop).
- Reopen the Store (triggers migration).
- Assert: all 10 entries readable via store.get(id).
- Assert: all original fields (title, content, topic, category, tags, status, etc.) match.
- Assert: entry count is still 10.

### test_migration_populates_security_fields
- Open Store, insert 3 entries. Close. Reopen.
- For each entry:
  - Assert content_hash is 64 hex characters (non-empty).
  - Assert version == 1.
  - Assert created_by == "".
  - Assert modified_by == "".
  - Assert previous_hash == "".
  - Assert trust_source == "system".
  - Assert feature_cycle == "".

### test_migration_content_hash_computed_correctly
- Open Store, insert an entry with title="Known" and content="Value". Close. Reopen.
- Read the migrated entry.
- Independently compute SHA-256 of "Known: Value".
- Assert entry.content_hash equals the independently computed hash.

### test_migration_empty_database
- Open Store (creates tables, no entries). Close. Reopen.
- Assert: schema_version counter equals 1 (CURRENT_SCHEMA_VERSION).
- Assert: no crash, no entries created.

### test_migration_idempotent
- Open Store, insert entries. Close. Reopen (migration runs). Close. Reopen again.
- Assert: schema_version is still 1.
- Assert: entries unchanged from first migration.
- Assert: no performance regression from scanning (migration detects version==1 and skips).

### test_migration_preserves_counters
- Open Store, insert 5 entries (3 Active, 1 Deprecated, 1 Proposed). Close. Reopen.
- Assert: read_counter("next_entry_id") == 6 (next available ID).
- Assert: read_counter("total_active") == 3.
- Assert: read_counter("total_deprecated") == 1.
- Assert: read_counter("total_proposed") == 1.
- Assert: read_counter("schema_version") == 1.

### test_migration_unicode_content
- Open Store, insert entry with title containing CJK characters and content with emoji. Close. Reopen.
- Assert: title and content match original values byte-for-byte.
- Assert: content_hash is valid 64-char hex.

### test_migration_empty_string_fields
- Open Store, insert entry with empty title and empty content. Close. Reopen.
- Assert: content_hash equals SHA-256 of "" (the well-known hash).
- Assert: version == 1.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-01 | test_migration_preserves_entries, test_migration_empty_database, test_migration_unicode_content |
| R-04 | test_legacy_entry_record_deserialization, test_legacy_deserialization_all_status_variants, test_migration_preserves_entries |
| R-09 | test_migration_idempotent |
| R-02 (partial) | test_migration_content_hash_computed_correctly |
| IR-03 | test_migration_preserves_counters |
| EC-05 | test_migration_empty_database |
| EC-06 | test_migration_unicode_content |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-09 | test_migration_preserves_entries, test_migration_populates_security_fields |
| AC-10 | Architecture review (single write transaction in migration.rs) |
| AC-11 | test_migration_preserves_counters (schema_version == 1) |
