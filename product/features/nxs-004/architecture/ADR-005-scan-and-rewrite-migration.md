## ADR-005: Scan-and-Rewrite Migration with Schema Version Counter

### Context

nxs-004 adds 7 fields to EntryRecord. bincode v2 uses positional encoding, so existing serialized entries cannot be deserialized with the new schema (missing trailing fields cause deserialization failure). The PRODUCT-VISION.md specifies a scan-and-rewrite migration pattern triggered by a `schema_version` counter in the COUNTERS table.

Options:
1. Lazy migration: deserialize old entries with a fallback path, migrate on next write
2. Eager migration: scan-and-rewrite all entries on database open when version mismatch detected
3. Dual-read path: maintain old and new deserialization functions indefinitely

Option 1 leaves the database in a mixed state indefinitely. Queries that scan entries may encounter both old and new formats, complicating error handling. Option 3 accumulates technical debt with every schema change. Option 2 is clean, atomic, and establishes a pattern that works for all future migrations.

### Decision

Eager scan-and-rewrite migration on `Store::open()`:

1. After creating/opening all tables, read `schema_version` from COUNTERS (default: 0 if absent).
2. If `schema_version < CURRENT_SCHEMA_VERSION`:
   a. Begin a write transaction.
   b. Iterate all entries in the ENTRIES table.
   c. Deserialize each entry. For pre-nxs-004 entries, the 7 new fields will be absent from the byte stream. Use a legacy deserialization path that handles the shorter byte sequence.
   d. Populate new fields: `content_hash` = computed from title+content, `version` = 1, `trust_source` = "system", others = "".
   e. Re-serialize with the full current schema and write back.
   f. Set `schema_version` = `CURRENT_SCHEMA_VERSION`.
   g. Commit the transaction.
3. If the transaction fails, it rolls back atomically (redb guarantee). The database remains at the old schema version, and the next open attempt will retry.

The `CURRENT_SCHEMA_VERSION` is a constant in the migration module. For nxs-004, it is `1` (the first migration event; pre-migration databases have implicit version `0`).

**Legacy deserialization**: Since bincode v2 positional encoding cannot handle missing trailing fields, the migration must handle two cases:
- Fresh databases (no entries): just set schema_version = 1
- Pre-nxs-004 databases: deserialize entries using a legacy EntryRecord struct (without the 7 new fields), then convert to the current EntryRecord with defaults

### Consequences

- **Easier**: After migration, all code paths can assume the current schema. No dual-read complexity.
- **Easier**: Future migrations follow the same pattern: bump CURRENT_SCHEMA_VERSION, add a migration branch.
- **Easier**: Atomic transaction ensures no partial state. Crash during migration = retry on next open.
- **Harder**: Migration runs on every open until it succeeds. But it only runs once per schema version, and at Unimatrix scale (<1000 entries) it completes in milliseconds.
- **Harder**: Requires a legacy deserialization path for the transition from version 0 to version 1. But this is a one-time cost.
