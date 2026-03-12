## ADR-002: Direct SQL INSERT for Import, Not Store API

### Context

The `Store::insert()` method auto-generates IDs (`counters::next_entry_id()`), auto-computes `content_hash` from title+content, auto-sets `created_at`/`updated_at` to `now`, sets `confidence` to 0.0, and `version` to 1. Import must preserve the original values for all of these fields verbatim from the export dump. Using `Store::insert()` would destroy the imported data's identity, timestamps, confidence scores, and hash chains.

This is the same tradeoff documented in prior Unimatrix decisions:
- Unimatrix #336: ADR-004 (nxs-006) -- "Import Uses Store::open() Then Raw SQL"
- Unimatrix #344: Pattern -- "Store::open() + Raw SQL Hybrid for Bulk Data Import"

### Decision

Import uses `Store::open()` to create/open the database (ensuring correct DDL, PRAGMAs, foreign keys, and schema version), then inserts data via direct SQL on the underlying connection obtained through `store.lock_conn()`. The `lock_conn()` method is `pub` on `Store`, so cross-crate access from unimatrix-server is supported.

Each of the 8 table types has a dedicated insert function that maps format struct fields to SQL parameters. All inserts run within a single `BEGIN IMMEDIATE` transaction for atomicity (AC-22).

Counter values are restored from the export dump, overwriting the auto-initialized counters created by `Store::open()`. This uses `INSERT OR REPLACE INTO counters` to handle counters that already exist from schema initialization.

### Consequences

- **Easier**: Imported data preserves all original values exactly (IDs, timestamps, confidence, hashes, counters).
- **Easier**: Single transaction for all inserts gives atomic rollback on any failure.
- **Harder**: Any future schema change (new column, renamed column, new constraint) must be mirrored manually in import's INSERT statements. The shared format types (ADR-001) mitigate this by surfacing format changes at compile time.
- **Harder**: Import is coupled to the SQLite schema DDL, not the Store API abstraction. This is acceptable because import is a bulk data tool, not a long-lived API consumer.
