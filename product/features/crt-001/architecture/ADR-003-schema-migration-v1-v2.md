## ADR-003: Schema Migration V1 to V2

### Context

crt-001 adds two new fields to EntryRecord: `helpful_count: u32` and `unhelpful_count: u32`. Because bincode v2 uses positional encoding (not field names), existing serialized records cannot be deserialized with the new struct layout. A scan-and-rewrite migration is required.

The migration infrastructure was established in nxs-004 (v0->v1): a `schema_version` counter in COUNTERS gates migration, and `migrate_if_needed()` runs on every `Store::open()`.

### Decision

Add a v1->v2 migration step following the exact same pattern as v0->v1:

1. Define `V1EntryRecord` -- a 24-field struct matching the current schema (ending with `trust_source`).
2. In `migrate_v1_to_v2`: scan all entries, deserialize with `V1EntryRecord`, construct new `EntryRecord` with `helpful_count: 0` and `unhelpful_count: 0`, serialize and overwrite.
3. Bump `CURRENT_SCHEMA_VERSION` from 1 to 2.
4. Add `if current_version < 2 { migrate_v1_to_v2(&txn)?; }` to `migrate_if_needed`.

The existing v0->v1 migration remains in place. A database that has never been opened (v0) will run both migrations sequentially: v0->v1, then v1->v2.

New fields are appended after `trust_source`:
```
... version, feature_cycle, trust_source, helpful_count, unhelpful_count
```

### Consequences

- **First Store::open() after upgrade triggers migration.** For databases with thousands of entries, this adds a one-time scan-and-rewrite. At Unimatrix scale (hundreds to low thousands of entries), this completes in milliseconds.
- **Migration is idempotent.** If `schema_version >= 2`, migration is skipped. Safe to call repeatedly.
- **Forward compatibility maintained.** The append-only field ordering contract is preserved. Future fields append after `unhelpful_count`.
- **V1EntryRecord struct is migration-only.** It is used solely for deserializing old-format entries and is not part of the public API.
- **The existing LegacyEntryRecord (v0, 17 fields) is renamed to V0EntryRecord** for clarity, since we now have three schema versions. The v0->v1 migration continues to use this struct.
