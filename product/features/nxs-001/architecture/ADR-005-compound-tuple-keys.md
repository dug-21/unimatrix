# ADR-005: Compound Tuple Keys for Index Tables

## Status

Accepted

## Context

The secondary index tables (TOPIC_INDEX, CATEGORY_INDEX, TIME_INDEX, STATUS_INDEX) need key designs that support efficient range scans. redb stores keys in sorted B-tree order and supports range queries with standard Rust `RangeBounds`.

We need keys that enable:
- **Topic/Category queries:** "all entries with topic X" — prefix-based scan
- **Time queries:** "all entries between timestamp A and B" — range scan
- **Status queries:** "all active entries" — equality-based partition scan
- **Uniqueness:** No key collisions when multiple entries share the same topic, category, timestamp, or status

Alternatives considered:

- **Hash-based keys** — `(hash(topic), entry_id)` where the hash is a u64. Compact but loses human-readable scan capability, introduces hash collision risk, and prevents lexicographic prefix scanning on the original string value.
- **Separate value tables** — store entry_id as the value instead of in the key, e.g., `TableDefinition<&str, u64>`. Doesn't work for one-to-many relationships (many entries share one topic) since redb's `Table` maps one key to one value. Would require `MultimapTable` for all indexes.
- **Compound tuple keys** — `(&str, u64)` for string-prefix indexes, `(u64, u64)` for numeric-prefix indexes, `(u8, u64)` for enum-prefix indexes. redb tuples have lexicographic ordering, enabling prefix range scans. The entry_id suffix guarantees uniqueness. The value is `()` (unit) — existence in the table IS the index.

## Decision

Use **compound tuple keys** for all index tables, with the indexed value as the first element and the entry_id as the second element. Values are `()` (unit type) — the key itself carries all information.

**Key patterns by index:**

| Index | Key Type | Range Scan Pattern | Example |
|-------|----------|-------------------|---------|
| TOPIC_INDEX | `(&str, u64)` | `("auth", 0)..=("auth", u64::MAX)` | All entries with topic "auth" |
| CATEGORY_INDEX | `(&str, u64)` | `("convention", 0)..=("convention", u64::MAX)` | All entries with category "convention" |
| TIME_INDEX | `(u64, u64)` | `(start_ts, 0)..=(end_ts, u64::MAX)` | All entries in time window |
| STATUS_INDEX | `(u8, u64)` | `(0, 0)..=(0, u64::MAX)` | All active entries (status=0) |

**Why entry_id as suffix:**

The entry_id in the second tuple position serves two purposes:
1. **Uniqueness** — no two entries produce the same key, even with identical topic/timestamp/status values
2. **Deterministic ordering** — entries within the same prefix are ordered by ID (insertion order), providing stable iteration

**Why unit `()` values:**

The index tables are "existence indexes" — the key encodes all necessary information (the indexed field value + the entry_id). Storing `()` as the value minimizes storage overhead to just the B-tree key.

## Consequences

**Positive:**
- **Efficient prefix scans.** redb's B-tree lexicographic ordering on tuples means `("auth", 0)..=("auth", u64::MAX)` scans exactly the entries with topic "auth", no more and no less.
- **Compile-time type safety.** redb's `TableDefinition<K, V>` ensures key/value types are checked at compile time. A `(&str, u64)` key cannot be accidentally used with a `(u64, u64)` table.
- **Human-readable keys.** String-based indexes preserve the original topic/category values in the B-tree. This aids debugging (redb's internal tools can dump table contents) and avoids hash collision concerns.
- **Minimal storage overhead.** Unit values consume zero bytes per entry. The index cost is only the key size in the B-tree.
- **Deterministic iteration.** Within a prefix, entries are ordered by ID, providing stable, reproducible query results.

**Negative:**
- **String duplication.** Topic and category strings are stored in both ENTRIES (inside the serialized EntryRecord) and the index tables (as key prefixes). At our scale, this overhead is negligible (<10% of total storage).
- **Scan cost for popular values.** A topic with 10,000 entries requires scanning 10,000 B-tree leaf nodes. At our scale this is sub-millisecond, but it scales linearly with the number of matching entries.
- **No partial string matching.** The prefix scan pattern works for exact topic/category matches. Substring or fuzzy matching requires a full scan or the vector index (nxs-002). This is by design — deterministic lookups use exact matches; fuzzy matching uses semantic search.
