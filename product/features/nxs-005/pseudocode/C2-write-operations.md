# C2: SQLite Write Operations

## File: `crates/unimatrix-store/src/sqlite/write.rs`

All write operations follow the same pattern:
1. Lock mutex: `self.conn.lock().unwrap_or_else(|e| e.into_inner())`
2. Begin transaction: `conn.execute_batch("BEGIN IMMEDIATE")`
3. Execute SQL statements with parameterized queries
4. Commit: `conn.execute_batch("COMMIT")`
5. MutexGuard drops automatically

On any error, the transaction rolls back when the connection is reused (SQLite auto-rollback on uncommitted transactions).

## insert(entry: NewEntry) -> Result<u64>

```
lock conn
BEGIN IMMEDIATE
  read next_entry_id from counters -> id
  UPDATE counters SET value = id + 1 WHERE name = 'next_entry_id'
  compute content_hash
  build EntryRecord with id, timestamps, defaults
  serialize to bincode bytes
  INSERT INTO entries (id, data) VALUES (?, ?)
  INSERT INTO topic_index (topic, entry_id) VALUES (?, ?)
  INSERT INTO category_index (category, entry_id) VALUES (?, ?)
  for each tag:
    INSERT INTO tag_index (tag, entry_id) VALUES (?, ?)
  INSERT INTO time_index (timestamp, entry_id) VALUES (?, ?)
  INSERT INTO status_index (status, entry_id) VALUES (?, ?)
  read total_{status} counter -> current
  UPDATE counters SET value = current + 1 WHERE name = 'total_{status}'
    (or INSERT OR REPLACE)
COMMIT
return id
```

## update(entry_id, new: NewEntry) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  SELECT data FROM entries WHERE id = ?
  if None -> return EntryNotFound
  deserialize old record
  compute new content_hash
  build updated EntryRecord (preserve id, created_at, access_count, etc.)
  serialize to bincode
  UPDATE entries SET data = ? WHERE id = ?
  -- Index diff: remove old, insert new
  if topic changed:
    DELETE FROM topic_index WHERE topic = ? AND entry_id = ?
    INSERT INTO topic_index (topic, entry_id) VALUES (?, ?)
  if category changed:
    DELETE FROM category_index WHERE category = ? AND entry_id = ?
    INSERT INTO category_index (category, entry_id) VALUES (?, ?)
  -- Tags: remove old tags not in new, add new tags not in old
  for removed_tag:
    DELETE FROM tag_index WHERE tag = ? AND entry_id = ?
  for added_tag:
    INSERT INTO tag_index (tag, entry_id) VALUES (?, ?)
  -- Time index always updated (updated_at changes)
  DELETE FROM time_index WHERE timestamp = ? AND entry_id = ?
  INSERT INTO time_index (timestamp, entry_id) VALUES (?, ?)
  if status changed:
    DELETE FROM status_index WHERE status = ? AND entry_id = ?
    INSERT INTO status_index (status, entry_id) VALUES (?, ?)
    decrement old status counter, increment new status counter
COMMIT
```

## update_status(entry_id, new_status) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  SELECT data FROM entries WHERE id = ?
  if None -> return EntryNotFound
  deserialize -> record
  old_status = record.status
  record.status = new_status
  record.updated_at = now
  serialize -> bytes
  UPDATE entries SET data = ? WHERE id = ?
  DELETE FROM status_index WHERE status = old_status AND entry_id = ?
  INSERT INTO status_index (status, entry_id) VALUES (new_status, ?)
  decrement total_{old_status}, increment total_{new_status}
COMMIT
```

## delete(entry_id) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  SELECT data FROM entries WHERE id = ?
  if None -> return EntryNotFound
  deserialize -> record
  DELETE FROM entries WHERE id = ?
  DELETE FROM topic_index WHERE topic = ? AND entry_id = ?
  DELETE FROM category_index WHERE category = ? AND entry_id = ?
  for each tag: DELETE FROM tag_index WHERE tag = ? AND entry_id = ?
  DELETE FROM time_index WHERE timestamp = ? AND entry_id = ?
  DELETE FROM status_index WHERE status = ? AND entry_id = ?
  DELETE FROM vector_map WHERE entry_id = ?
  decrement total_{status} counter
COMMIT
```

## record_usage(entry_id, is_helpful, now) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  SELECT data FROM entries WHERE id = ?
  if None -> return (silent)
  deserialize -> record
  record.access_count += 1
  record.last_accessed_at = now
  if is_helpful: record.helpful_count += 1
  else: record.unhelpful_count += 1
  serialize -> bytes
  UPDATE entries SET data = ? WHERE id = ?
COMMIT
```

## record_usage_with_confidence(entry_id, is_helpful, confidence, now) -> Result<()>

Same as record_usage but also sets record.confidence = confidence.

## update_confidence(entry_id, confidence) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  SELECT data FROM entries WHERE id = ?
  if None -> return EntryNotFound
  deserialize -> record
  record.confidence = confidence
  serialize -> bytes
  UPDATE entries SET data = ? WHERE id = ?
COMMIT
```

## put_vector_mapping(entry_id, hnsw_data_id) -> Result<()>

```
lock conn
INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?, ?)
```

## rewrite_vector_map(mappings: &[(u64, u64)]) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  DELETE FROM vector_map
  for each (entry_id, data_id):
    INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?, ?)
COMMIT
```

## record_feature_entries(feature_cycle, entry_ids) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  for each entry_id:
    INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?, ?)
COMMIT
```

## record_co_access_pairs(pairs: &[(u64, u64)], now) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  for each (a, b):
    key = co_access_key(a, b)
    SELECT data FROM co_access WHERE entry_id_a = ? AND entry_id_b = ?
    if exists:
      deserialize -> record
      record.count += 1
      record.last_updated = now
    else:
      record = CoAccessRecord { count: 1, last_updated: now }
    serialize -> bytes
    INSERT OR REPLACE INTO co_access (entry_id_a, entry_id_b, data) VALUES (?, ?, ?)
COMMIT
```

## cleanup_stale_co_access(staleness_cutoff) -> Result<u64>

```
lock conn
BEGIN IMMEDIATE
  SELECT entry_id_a, entry_id_b, data FROM co_access
  collect stale keys where record.last_updated < staleness_cutoff
  for each stale key:
    DELETE FROM co_access WHERE entry_id_a = ? AND entry_id_b = ?
COMMIT
return deleted_count
```

## store_metrics(feature_cycle, data: &[u8]) -> Result<()>

```
lock conn
INSERT OR REPLACE INTO observation_metrics (feature_cycle, data) VALUES (?, ?)
```

## Counter Helpers (inline, not separate module)

For SQLite, counter operations are done inline within each method's transaction using:
```sql
SELECT value FROM counters WHERE name = ?
UPDATE counters SET value = ? WHERE name = ?
-- or --
INSERT OR REPLACE INTO counters (name, value) VALUES (?, ?)
```

The key difference from redb: redb has separate `counter.rs` functions that take `&WriteTransaction`. For SQLite, the connection is already locked by the calling method, so counter reads/writes are just additional SQL statements within the same transaction.
