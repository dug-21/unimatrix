# ADR-003: Manual Secondary Indexes via Separate Tables

## Status

Accepted

## Context

Unimatrix requires multiple access patterns for entries: by topic, by category, by tags, by time range, and by status. redb provides only primary key lookup and range scans on a single table — it has no built-in secondary index mechanism.

We need an indexing strategy that:

1. **Supports all required query patterns** — prefix scans (topic, category), set intersection (tags), range queries (time), equality filters (status)
2. **Maintains consistency** — indexes must always reflect the current state of ENTRIES
3. **Allows atomic updates** — when an entry's topic changes, the old topic index entry must be removed and the new one inserted in the same transaction
4. **Is extensible** — future milestones add new query dimensions (feature scope, usage ranking) that will need new indexes

Alternatives considered:

- **Scan and filter** — scan all entries and filter in application code. Simple but O(n) for every query; unacceptable at 100K entries.
- **Embedded SQL (SQLite)** — has built-in secondary indexes, but we chose redb for its pure-Rust, zero-dependency properties (see ADR-001). Adding SQLite alongside redb would negate those benefits.
- **In-memory indexes** — build HashMaps on startup from ENTRIES scan. Fast reads, but loses crash safety (must rebuild on every startup) and creates consistency risks during writes.
- **Separate tables with atomic updates** — maintain one redb table per index, all updated within the same write transaction as ENTRIES. This is the standard pattern for embedded key-value stores without secondary index support.

## Decision

Maintain **separate redb tables for each secondary index**, updated atomically within the same write transaction as the ENTRIES table.

Five index tables serve the required query patterns:

| Index Table | Key Type | Value Type | Query Pattern |
|-------------|----------|------------|---------------|
| TOPIC_INDEX | `(&str, u64)` | `()` | Prefix range scan by topic |
| CATEGORY_INDEX | `(&str, u64)` | `()` | Prefix range scan by category |
| TAG_INDEX (multimap) | `&str` | `u64` | Per-tag lookup + set intersection |
| TIME_INDEX | `(u64, u64)` | `()` | Temporal range scan |
| STATUS_INDEX | `(u8, u64)` | `()` | Status equality filter |

Every write operation (insert, update, delete) touches ENTRIES and all affected index tables within a single `WriteTransaction`. redb's transaction semantics guarantee that either all tables are updated or none are (commit or abort).

On updates, the storage engine internally reads the old record, diffs indexed fields, removes stale index entries, and inserts new ones. Callers never manage index maintenance.

## Consequences

**Positive:**
- **Atomic consistency.** redb transactions span all tables. No partial index updates, no orphaned entries.
- **Drop-safe rollback.** If any index write fails, the `WriteTransaction` is dropped without commit, rolling back all changes including the ENTRIES write.
- **Independent optimization.** Each index table has its own key type optimized for its query pattern (compound tuples for prefix scans, multimap for set operations).
- **Extensible.** Adding a new index (e.g., FEATURE_INDEX for col-004) means adding one table constant and a few lines in the write/update paths.

**Negative:**
- **Write amplification.** Every insert touches 6-7 tables (ENTRIES + 5 indexes + COUNTERS). At our scale this is sub-millisecond, but it is more work per write than a single-table design.
- **Manual maintenance.** The engine must correctly maintain all indexes on every write path (insert, update, status change). Bugs in index maintenance cause silent query result errors. Mitigated by: thorough testing of index consistency (AC-04, AC-12, AC-18).
- **Storage overhead.** Index tables duplicate key information (topic strings, timestamps, status bytes). At our entry sizes (500-2000 bytes content), index overhead is <10% of total storage.
- **No ad-hoc queries.** Only indexed fields can be queried efficiently. New query patterns require new tables. Mitigated by: the QueryFilter design makes adding new filter dimensions straightforward.
