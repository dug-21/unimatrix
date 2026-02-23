# Test Plan Overview: nxs-004

## Test Strategy

Three levels of testing:

1. **Unit tests** -- Per-function assertions within each crate. Live in `#[cfg(test)] mod tests` in the source files.
2. **Integration tests** -- Cross-component interactions (migration + write, adapter + trait). Live in `tests/` directories.
3. **Compilation tests** -- Trait object safety, Send+Sync bounds, re-export completeness. Verified by `fn _check()` patterns and `dyn Trait` usage.

## Risk-to-Test Mapping

| Risk | Priority | Component(s) | Test Coverage |
|------|----------|-------------|---------------|
| R-01 | Critical | migration | test_migration_preserves_entries, test_migration_empty_db, test_migration_unicode, test_migration_content_hash_computed |
| R-02 | Critical | content-hash, write-security | test_content_hash_known_values, test_hash_consistency_insert_update, test_hash_empty_fields, test_hash_matches_prepare_text |
| R-04 | Critical | migration | test_legacy_deserialization, test_migration_all_status_variants |
| R-07 | Critical | security-schema | test_new_entry_extended_fields, all existing tests pass after TestEntry builder update |
| R-12 | Critical | security-schema | cargo test -p unimatrix-store (85 tests), cargo test -p unimatrix-vector (85 tests), cargo test -p unimatrix-embed (76 tests) |
| R-03 | High | write-security | test_version_starts_at_1, test_version_increments, test_version_multiple_updates, test_update_status_no_version_change |
| R-05 | High | core-traits | test_object_safety_entry_store, test_object_safety_vector_store, test_object_safety_embed_service, test_arc_dyn_trait |
| R-08 | High | adapters, core-error | test_store_error_preservation, test_vector_error_preservation, test_error_source_chain |
| R-10 | High | write-security | test_hash_chain_three_updates, test_hash_chain_no_content_change |
| R-06 | Medium | async-wrappers | test_async_insert_success, test_async_error_propagation, test_join_error_conversion |
| R-09 | Low | migration | test_migration_idempotent |
| R-11 | Medium | re-exports | test_all_reexported_types |

## Cross-Component Test Dependencies

- **migration depends on security-schema + content-hash**: Migration creates 24-field records and computes content_hash.
- **write-security depends on content-hash**: insert() and update() call compute_content_hash().
- **adapters depend on core-traits + core-error**: Adapters implement traits and convert errors.
- **async-wrappers depend on adapters**: Async wrappers wrap types that implement core traits.
- **re-exports depend on crate-setup**: Re-exports require unimatrix-core to compile.

## Integration Test Scenarios

| Scenario | Components | Description |
|----------|-----------|-------------|
| Adapter through trait object | adapters + core-traits | Construct StoreAdapter, cast to `dyn EntryStore`, call methods |
| Migration then insert | migration + write-security | Migrate old DB, insert new entry, verify both old and new entries have correct fields |
| Hash chain through adapter | adapters + write-security | Insert and update through StoreAdapter, verify hash chain |
| Async insert round-trip | async-wrappers + adapters | AsyncEntryStore insert, then get, verify record |
| Error propagation chain | adapters + core-error | Trigger StoreError through StoreAdapter, verify CoreError preservation |

## Test Infrastructure

Extends existing `TestDb` and `TestEntry` from `unimatrix-store/src/test_helpers.rs`. The `TestEntry` builder gains three new methods (`with_created_by`, `with_feature_cycle`, `with_trust_source`) and the `build()` method includes the new fields with empty-string defaults.

For unimatrix-core tests, a new `TestDb` wrapper is needed that provides adapter instances. This uses unimatrix-store's `test-support` feature.
