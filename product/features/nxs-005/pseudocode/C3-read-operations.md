# C3: SQLite Read Operations

## File: `crates/unimatrix-store/src/sqlite/read.rs`

All read operations:
1. Lock mutex
2. Execute SELECT queries with parameterized `?` placeholders
3. Deserialize bincode blobs
4. Drop MutexGuard
5. Return results

No explicit transactions needed for single-statement reads (SQLite autocommit provides snapshot isolation for single statements). Multi-statement reads that require consistency use BEGIN...COMMIT.

## get(entry_id) -> Result<EntryRecord>

```
lock conn
SELECT data FROM entries WHERE id = ?
if no row -> return EntryNotFound(entry_id)
deserialize bincode -> EntryRecord
return record
```

## exists(entry_id) -> Result<bool>

```
lock conn
SELECT 1 FROM entries WHERE id = ? LIMIT 1
return row.is_some()
```

## query_by_topic(topic) -> Result<Vec<EntryRecord>>

```
lock conn
SELECT entry_id FROM topic_index WHERE topic = ? ORDER BY entry_id
collect into HashSet<u64>
fetch_entries(conn, &ids) -> Vec<EntryRecord>
```

## query_by_category(category) -> Result<Vec<EntryRecord>>

```
lock conn
SELECT entry_id FROM category_index WHERE category = ? ORDER BY entry_id
collect into HashSet
fetch_entries(conn, &ids)
```

## query_by_tags(tags: &[String]) -> Result<Vec<EntryRecord>>

```
if tags.is_empty() -> return Ok(vec![])
lock conn
for each tag:
  SELECT entry_id FROM tag_index WHERE tag = ?
  collect into HashSet
intersect all sets
fetch_entries(conn, &intersection)
```

## query_by_time_range(range: TimeRange) -> Result<Vec<EntryRecord>>

```
if range.start > range.end -> return Ok(vec![])
lock conn
SELECT entry_id FROM time_index WHERE timestamp >= ? AND timestamp <= ?
  ORDER BY timestamp, entry_id
collect into HashSet
fetch_entries(conn, &ids)
```

## query_by_status(status: Status) -> Result<Vec<EntryRecord>>

```
lock conn
SELECT entry_id FROM status_index WHERE status = ? ORDER BY entry_id
collect into HashSet
fetch_entries(conn, &ids)
```

## query(filter: QueryFilter) -> Result<Vec<EntryRecord>>

Same logic as redb query.rs:
```
lock conn
determine effective_status (default Active if all None)
collect ID sets from each present filter
intersect all sets
fetch_entries
```

## get_vector_mapping(entry_id) -> Result<Option<u64>>

```
lock conn
SELECT hnsw_data_id FROM vector_map WHERE entry_id = ?
return Option
```

## iter_vector_mappings() -> Result<Vec<(u64, u64)>>

```
lock conn
SELECT entry_id, hnsw_data_id FROM vector_map ORDER BY entry_id
collect into Vec
```

Note: ORDER BY entry_id matches redb's sorted key iteration order.

## read_counter(name) -> Result<u64>

```
lock conn
SELECT value FROM counters WHERE name = ?
if no row -> return 0
return value
```

## get_co_access_partners(entry_id, staleness_cutoff) -> Result<Vec<(u64, CoAccessRecord)>>

```
lock conn
-- Partners where entry_id is entry_id_a (prefix scan equivalent)
SELECT entry_id_b, data FROM co_access WHERE entry_id_a = ?
for each row:
  deserialize data -> CoAccessRecord
  if record.last_updated >= staleness_cutoff:
    partners.push((entry_id_b, record))

-- Partners where entry_id is entry_id_b (reverse lookup using idx_co_access_b)
SELECT entry_id_a, data FROM co_access WHERE entry_id_b = ?
for each row:
  deserialize data -> CoAccessRecord
  if record.last_updated >= staleness_cutoff:
    partners.push((entry_id_a, record))

return partners
```

Key improvement over redb: the idx_co_access_b index enables an indexed reverse lookup instead of a full table scan.

## co_access_stats(staleness_cutoff) -> Result<(u64, u64)>

```
lock conn
SELECT data FROM co_access
count total, count active (where last_updated >= staleness_cutoff)
return (total, active)
```

## top_co_access_pairs(n, staleness_cutoff) -> Result<Vec<((u64, u64), CoAccessRecord)>>

```
lock conn
SELECT entry_id_a, entry_id_b, data FROM co_access
filter by staleness, sort by count descending, take top n
```

## get_metrics(feature_cycle) -> Result<Option<Vec<u8>>>

```
lock conn
SELECT data FROM observation_metrics WHERE feature_cycle = ?
return Option<Vec<u8>>
```

## list_all_metrics() -> Result<Vec<(String, Vec<u8>)>>

```
lock conn
SELECT feature_cycle, data FROM observation_metrics ORDER BY feature_cycle
collect into Vec
```

## Internal Helpers

```rust
/// Fetch full EntryRecords for a set of IDs.
fn fetch_entries(conn: &Connection, ids: &HashSet<u64>) -> Result<Vec<EntryRecord>> {
    let table = conn.open_table(ENTRIES)?;
    let mut results = Vec::with_capacity(ids.len());
    for &id in ids {
        // Use prepared statement: SELECT data FROM entries WHERE id = ?
        if let Some(bytes) = ... {
            results.push(deserialize_entry(bytes)?);
        }
    }
    Ok(results)
}
```

For efficiency with large ID sets, batch the query using IN clause or iterate with individual lookups. Individual lookups are simpler and match the redb pattern. At our scale (<1000 entries), the performance difference is negligible.
