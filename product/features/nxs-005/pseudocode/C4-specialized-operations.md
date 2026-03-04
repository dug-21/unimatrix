# C4: Specialized Operations (Signal, Sessions, Injection Log)

## Files
- `crates/unimatrix-store/src/sqlite/signal.rs`
- `crates/unimatrix-store/src/sqlite/sessions.rs`
- `crates/unimatrix-store/src/sqlite/injection_log.rs`

## Signal Queue (sqlite/signal.rs)

### insert_signal(record: &SignalRecord) -> Result<u64>

```
lock conn
BEGIN IMMEDIATE
  -- 1. Read and increment next_signal_id
  SELECT value FROM counters WHERE name = 'next_signal_id' -> next_id (default 0)
  UPDATE counters SET value = next_id + 1 WHERE name = 'next_signal_id'

  -- 2. Enforce 10K cap: count and delete oldest if needed
  SELECT COUNT(*) FROM signal_queue -> current_len
  if current_len >= 10_000:
    SELECT MIN(signal_id) FROM signal_queue -> oldest_key
    DELETE FROM signal_queue WHERE signal_id = oldest_key

  -- 3. Insert new record
  full_record = record.clone() with signal_id = next_id
  bytes = serialize_signal(&full_record)
  INSERT INTO signal_queue (signal_id, data) VALUES (next_id, bytes)
COMMIT
return next_id
```

### drain_signals(signal_type: SignalType) -> Result<Vec<SignalRecord>>

```
lock conn
BEGIN IMMEDIATE
  SELECT signal_id, data FROM signal_queue ORDER BY signal_id
  for each row:
    try deserialize -> Ok(record):
      if record.signal_type == signal_type:
        keys_to_delete.push(signal_id)
        drained.push(record)
      else: skip (different type)
    Err: keys_to_delete.push(signal_id)  -- corrupted, remove

  for key in keys_to_delete:
    DELETE FROM signal_queue WHERE signal_id = ?
COMMIT
return drained
```

### signal_queue_len() -> Result<u64>

```
lock conn
SELECT COUNT(*) FROM signal_queue
return count as u64
```

## Sessions (sqlite/sessions.rs)

### insert_session(record: &SessionRecord) -> Result<()>

```
lock conn
bytes = serialize_session(record)
INSERT OR REPLACE INTO sessions (session_id, data) VALUES (?, ?)
```

### update_session(session_id, updater: FnOnce) -> Result<()>

```
lock conn
BEGIN IMMEDIATE
  SELECT data FROM sessions WHERE session_id = ?
  if None -> return Err(Deserialization("session not found: ..."))
  deserialize -> record
  updater(&mut record)
  serialize -> bytes
  UPDATE sessions SET data = ? WHERE session_id = ?
COMMIT
```

### get_session(session_id) -> Result<Option<SessionRecord>>

```
lock conn
SELECT data FROM sessions WHERE session_id = ?
if no row -> Ok(None)
deserialize -> Ok(Some(record))
```

### scan_sessions_by_feature(feature_cycle) -> Result<Vec<SessionRecord>>

```
lock conn
SELECT data FROM sessions
for each row:
  deserialize -> record
  if record.feature_cycle == Some(feature_cycle):
    results.push(record)
return results
```

### scan_sessions_by_feature_with_status(feature_cycle, status_filter) -> Result<Vec<SessionRecord>>

Same as above with additional status filter.

### gc_sessions(timed_out_threshold, delete_threshold) -> Result<GcStats>

```
lock conn
now = current_unix_timestamp()
timed_out_boundary = now - timed_out_threshold
delete_boundary = now - delete_threshold

BEGIN IMMEDIATE
  -- Phase 1: collect sessions to delete
  SELECT session_id, data FROM sessions
  for each: if started_at < delete_boundary -> to_delete.push(session_id)

  -- Phase 2: collect injection log IDs to cascade
  SELECT log_id, data FROM injection_log
  for each: if session_id in to_delete -> log_ids_to_delete.push(log_id)

  -- Phase 3: delete injection logs
  for log_id in log_ids_to_delete:
    DELETE FROM injection_log WHERE log_id = ?
    stats.deleted_injection_log_count += 1

  -- Phase 4: delete sessions
  for session_id in to_delete:
    DELETE FROM sessions WHERE session_id = ?
    stats.deleted_session_count += 1

  -- Phase 5: timeout old active sessions
  SELECT session_id, data FROM sessions
  for each: if Active AND started_at < timed_out_boundary AND not in to_delete:
    record.status = TimedOut
    serialize -> bytes
    UPDATE sessions SET data = ? WHERE session_id = ?
    stats.timed_out_count += 1
COMMIT
return stats
```

## Injection Log (sqlite/injection_log.rs)

### insert_injection_log_batch(records: &[InjectionLogRecord]) -> Result<()>

```
if records.is_empty() -> return Ok(())
lock conn
BEGIN IMMEDIATE
  SELECT value FROM counters WHERE name = 'next_log_id' -> base_id (default 0)
  next_id = base_id + records.len()
  UPDATE counters SET value = next_id WHERE name = 'next_log_id'

  for (i, record) in records.iter().enumerate():
    r = record.clone() with log_id = base_id + i
    bytes = serialize_injection_log(&r)
    INSERT INTO injection_log (log_id, data) VALUES (r.log_id, bytes)
COMMIT
```

### scan_injection_log_by_session(session_id) -> Result<Vec<InjectionLogRecord>>

```
lock conn
SELECT data FROM injection_log ORDER BY log_id
for each row:
  deserialize -> record
  if record.session_id == session_id:
    results.push(record)
return results
```

## Pattern Notes

1. All serialization uses the SAME bincode serde path as redb (serialize_signal, serialize_session, etc.)
2. Session and injection log types are imported from the shared modules (sessions.rs types, injection_log.rs types)
3. The sqlite module re-implements the Store methods, NOT the types or serialization
4. Signal queue cap enforcement uses SQL MIN() instead of redb iter().next() -- equivalent semantics
5. GC cascade deletes use individual DELETE statements (matching redb pattern, safe at our scale)
