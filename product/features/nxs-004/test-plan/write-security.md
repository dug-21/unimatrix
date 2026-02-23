# Test Plan: write-security

## Scope

Verify insert() and update() correctly handle security fields: content_hash, version, modified_by, previous_hash. Verify update_status() does NOT change version or hash.

## Unit Tests (in crates/unimatrix-store/src/write.rs or tests/)

### test_insert_sets_content_hash
- Insert entry with title="Hello" and content="World".
- Read back the entry.
- Assert: content_hash equals SHA-256 hex of "Hello: World".

### test_insert_sets_version_1
- Insert an entry.
- Read back.
- Assert: version == 1.

### test_insert_sets_modified_by_to_created_by
- Insert entry with created_by = "agent-42".
- Read back.
- Assert: modified_by == "agent-42".

### test_insert_sets_previous_hash_empty
- Insert an entry.
- Read back.
- Assert: previous_hash == "".

### test_insert_preserves_caller_fields
- Insert entry with created_by = "agent-1", feature_cycle = "nxs-004", trust_source = "human".
- Read back.
- Assert: created_by == "agent-1", feature_cycle == "nxs-004", trust_source == "human".

### test_insert_empty_fields_hash
- Insert entry with title="" and content="".
- Read back.
- Assert: content_hash == SHA-256 of "" == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".

### test_update_increments_version
- Insert entry (version=1). Update it. Read back.
- Assert: version == 2.

### test_update_version_multiple
- Insert entry. Update 10 times. Read back.
- Assert: version == 11.

### test_update_sets_previous_hash
- Insert entry with title="A", content="B" (hash=H1).
- Update title to "C" (new hash=H2).
- Read back.
- Assert: previous_hash == H1.
- Assert: content_hash == H2 (SHA-256 of "C: B").

### test_update_hash_chain_three_steps
- Insert entry (hash=H1, previous_hash="").
- Update title (hash=H2, previous_hash=H1).
- Update content (hash=H3, previous_hash=H2).
- Read after each step and verify the chain: "" -> H1 -> H2 -> H3.

### test_update_no_content_change
- Insert entry with title="X", content="Y" (hash=H1).
- Update with same title and content, but change category.
- Read back.
- Assert: content_hash == H1 (unchanged, same input).
- Assert: previous_hash == H1 (set to old hash, which happens to be same).
- Assert: version == 2 (still increments).

### test_update_status_no_version_change
- Insert entry (version=1).
- Call update_status(id, Status::Deprecated).
- Read back.
- Assert: version == 1 (unchanged).
- Assert: content_hash unchanged.
- Assert: previous_hash unchanged (still "").

### test_update_status_no_hash_change
- Insert entry with known hash.
- Call update_status twice.
- Read back.
- Assert: content_hash identical to post-insert value.

## Edge Case Tests

### test_insert_large_content_hash
- Insert entry with title = "x".repeat(10_000), content = "y".repeat(100_000).
- Assert: content_hash is 64 chars, valid hex.

### test_insert_all_default_security_fields
- Insert entry with created_by = "", feature_cycle = "", trust_source = "".
- Assert: insert succeeds, version == 1, content_hash computed.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-02 | test_insert_sets_content_hash, test_insert_empty_fields_hash, test_update_no_content_change |
| R-03 | test_insert_sets_version_1, test_update_increments_version, test_update_version_multiple, test_update_status_no_version_change |
| R-10 | test_update_sets_previous_hash, test_update_hash_chain_three_steps, test_update_no_content_change |
| EC-01 | test_insert_large_content_hash |
| EC-02 | test_insert_all_default_security_fields |
| EC-04 | test_update_no_content_change |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-07 | test_insert_sets_content_hash, test_insert_sets_version_1, test_insert_sets_modified_by_to_created_by, test_insert_sets_previous_hash_empty |
| AC-08 | test_update_sets_previous_hash, test_update_increments_version, test_update_hash_chain_three_steps |
| AC-19 | test_insert_sets_version_1, test_update_increments_version, test_update_version_multiple |
