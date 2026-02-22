# C4: Store Test Plan

## R10/AC-03: All 8 Tables Created on Open

### test_open_creates_all_tables
- Open new database via Store::open()
- Verify all 8 tables exist by opening each in a read transaction
- Specifically: ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX (multimap), TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS

## R10/AC-14: Database Lifecycle

### test_open_creates_file
- Store::open(path) where path doesn't exist
- Assert file exists on disk after open

### test_close_and_reopen_preserves_data
- Open, insert an entry, drop Store
- Reopen same path, get entry by ID, verify fields match

### test_open_with_custom_cache
- Store::open_with_config(path, config with 128 MiB cache)
- No error

### test_compact_succeeds
- Open, insert some entries, delete some
- Call compact()
- No error, data still accessible

### test_compact_empty_database
- Open, compact immediately
- No error

## Store Properties

### test_store_is_send_sync
- Static assertion: Store: Send + Sync
- fn assert_send<T: Send>() {}; assert_send::<Store>();
- fn assert_sync<T: Sync>() {}; assert_sync::<Store>();
