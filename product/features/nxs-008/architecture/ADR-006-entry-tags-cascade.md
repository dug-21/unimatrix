# ADR-006: entry_tags Junction Table with Foreign Key CASCADE

**Status**: Accepted
**Context**: nxs-008
**Mitigates**: SR-08 (entry_tags Junction Table Consistency)

## Decision

The `entry_tags` junction table uses `FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE`, and `PRAGMA foreign_keys = ON` is enabled in `Store::open()`.

### Schema

```sql
CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);
CREATE INDEX idx_entry_tags_tag ON entry_tags(tag);
CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id);
```

### Tag Loading

A single helper function loads tags for a batch of entries:

```rust
fn load_tags_for_entries(conn: &Connection, ids: &[u64]) -> Result<HashMap<u64, Vec<String>>> {
    // Uses IN-list query: SELECT entry_id, tag FROM entry_tags WHERE entry_id IN (?,?,?)
    // Returns a map of entry_id -> Vec<tag>
}
```

This function is called everywhere an `EntryRecord` is constructed from a row. Tags are populated from the map; entries with no tags get an empty Vec.

### Tag Query Semantics

The current `collect_ids_by_tags` uses AND semantics (intersection across tags). The normalized version preserves this:

```sql
SELECT entry_id FROM entry_tags
WHERE tag IN (:tag1, :tag2, ...)
GROUP BY entry_id
HAVING COUNT(DISTINCT tag) = :tag_count
```

### PRAGMA foreign_keys

Currently `PRAGMA foreign_keys = OFF` in db.rs:38. This is changed to `ON`. The only impact is that `entry_tags` rows are automatically deleted when an entry is deleted. No other tables use foreign keys.

## Consequences

- Entry deletion automatically cleans up tags (no orphan rows)
- Tag-based queries use SQL instead of HashSet intersection
- Every `EntryRecord` construction must go through `load_tags_for_entries`
- `PRAGMA foreign_keys = ON` is a behavioral change at the SQLite level; verified by existing delete tests
