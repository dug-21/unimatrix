# nxs-004 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `unimatrix-core` crate exists with `EntryStore`, `VectorStore`, `EmbedService` traits | file-check + grep | `ls crates/unimatrix-core/src/traits.rs` exists; grep for `pub trait EntryStore`, `pub trait VectorStore`, `pub trait EmbedService` | PENDING |
| AC-02 | `EntryStore` has 16 methods matching Store's public API | grep | grep `traits.rs` for all 16 method names: insert, update, update_status, delete, get, exists, query, query_by_topic, query_by_category, query_by_tags, query_by_time_range, query_by_status, put_vector_mapping, get_vector_mapping, iter_vector_mappings, read_counter | PENDING |
| AC-03 | `VectorStore` has 6 methods matching VectorIndex's public API | grep | grep `traits.rs` for: insert, search, search_filtered, point_count, contains, stale_count | PENDING |
| AC-04 | `EmbedService` has 3 methods for entry-level embedding | grep | grep `traits.rs` for: embed_entry, embed_entries, dimension | PENDING |
| AC-05 | `EntryRecord` has 7 new security fields with `#[serde(default)]` | test | `cargo test -p unimatrix-store test_schema_evolution` -- roundtrip serialization with new fields | PENDING |
| AC-06 | `NewEntry` accepts `created_by`, `feature_cycle`, `trust_source` | test | `cargo test -p unimatrix-store test_insert` -- insert with all new fields populated | PENDING |
| AC-07 | `insert()` computes content_hash, sets version=1, modified_by=created_by, previous_hash="" | test | `cargo test -p unimatrix-store test_insert_security_fields` -- verify all computed fields | PENDING |
| AC-08 | `update()` sets previous_hash, computes new content_hash, increments version | test | `cargo test -p unimatrix-store test_update_hash_chain` -- verify hash chain through updates | PENDING |
| AC-09 | Migration runs on schema version mismatch, rewrites entries | test | `cargo test -p unimatrix-store test_migration` -- create pre-migration DB, reopen, verify fields | PENDING |
| AC-10 | Migration is atomic (single write transaction) | manual | Architecture review: migration.rs uses single txn.commit() | PENDING |
| AC-11 | After migration, schema_version equals CURRENT_SCHEMA_VERSION | test | `cargo test -p unimatrix-store test_migration_version` -- read counter after migration | PENDING |
| AC-12 | Domain adapters implement core traits | test | `cargo test -p unimatrix-core test_adapter` -- construct adapters, call through trait | PENDING |
| AC-13 | Async wrappers use spawn_blocking | test | `cargo test -p unimatrix-core --features async test_async` -- async method calls succeed | PENDING |
| AC-14 | All unimatrix-store tests pass | shell | `cargo test -p unimatrix-store 2>&1 \| grep "test result"` -- 0 failures | PENDING |
| AC-15 | All unimatrix-vector tests pass | shell | `cargo test -p unimatrix-vector 2>&1 \| grep "test result"` -- 0 failures | PENDING |
| AC-16 | All unimatrix-embed tests pass | shell | `cargo test -p unimatrix-embed 2>&1 \| grep "test result"` -- 0 failures | PENDING |
| AC-17 | Roundtrip serialization with all 7 new fields | test | `cargo test -p unimatrix-store test_roundtrip_security_fields` | PENDING |
| AC-18 | content_hash is SHA-256 hex of `"{title}: {content}"` | test | `cargo test -p unimatrix-store test_content_hash_known_values` -- compare against known SHA-256 | PENDING |
| AC-19 | version starts at 1, increments on update | test | `cargo test -p unimatrix-store test_version_tracking` -- insert=1, update=2, update=3 | PENDING |
| AC-20 | Traits are object-safe | test | `cargo test -p unimatrix-core test_object_safety` -- `fn _check(_: &dyn EntryStore)` compiles | PENDING |
| AC-21 | Traits require Send + Sync | test | `cargo test -p unimatrix-core test_send_sync` -- `Arc<dyn EntryStore>` compiles | PENDING |
| AC-22 | `#![forbid(unsafe_code)]` on all crates | grep | grep `forbid(unsafe_code)` in all lib.rs files | PENDING |
