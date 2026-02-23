# Pseudocode Overview: nxs-004

## Components

10 components in dependency order:

1. **crate-setup** -- Create unimatrix-core crate structure
2. **security-schema** -- Add 7 fields to EntryRecord, extend NewEntry
3. **content-hash** -- SHA-256 computation module
4. **migration** -- Scan-and-rewrite migration on Store::open()
5. **write-security** -- Update insert() and update() with security logic
6. **core-error** -- CoreError enum with From conversions
7. **core-traits** -- EntryStore, VectorStore, EmbedService trait definitions
8. **re-exports** -- Type re-exports in unimatrix-core lib.rs
9. **adapters** -- StoreAdapter, VectorAdapter, EmbedAdapter
10. **async-wrappers** -- Feature-gated AsyncEntryStore, AsyncVectorStore, AsyncEmbedService

## Data Flow

```
NewEntry (with created_by, feature_cycle, trust_source)
  -> Store::insert()
    -> compute_content_hash(title, content)
    -> build EntryRecord (version=1, modified_by=created_by, previous_hash="")
    -> serialize + write ENTRIES + indexes
    -> return entry_id

EntryRecord (with modified_by set by caller)
  -> Store::update()
    -> read old record
    -> previous_hash = old.content_hash
    -> compute new content_hash
    -> version = old.version + 1
    -> diff indexes + write

Store::open(path)
  -> create tables
  -> migrate_if_needed(db)
    -> read schema_version (default 0)
    -> if < CURRENT_SCHEMA_VERSION: scan-rewrite all entries
    -> set schema_version = 1
```

## Shared Types

- `EntryRecord` (24 fields) -- defined in unimatrix-store, re-exported from unimatrix-core
- `NewEntry` (10 fields) -- defined in unimatrix-store, re-exported from unimatrix-core
- `CoreError` -- defined in unimatrix-core, wraps StoreError/VectorError/EmbedError
- `SearchResult` -- defined in unimatrix-vector, re-exported from unimatrix-core

## Sequencing

Components 2-5 modify unimatrix-store (must be done before core depends on it).
Components 1, 6-10 create unimatrix-core (depends on store being updated).
Within unimatrix-store: 2+3 parallel -> 4 -> 5.
Within unimatrix-core: 6 -> 7 -> 8 -> 9 -> 10.
