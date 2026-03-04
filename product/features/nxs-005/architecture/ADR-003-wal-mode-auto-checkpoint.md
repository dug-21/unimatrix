## ADR-003: WAL Mode with Auto-Checkpoint

### Context

SQLite offers multiple journal modes (DELETE, TRUNCATE, PERSIST, MEMORY, WAL, OFF). The choice affects concurrency, durability, and performance. The user explicitly requested auto-checkpoint and does NOT want checkpoint-on-compact().

### Decision

Configure SQLite with WAL (Write-Ahead Logging) mode and rely on SQLite's built-in auto-checkpoint mechanism:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA wal_autocheckpoint = 1000;  -- default: checkpoint after 1000 pages (~4MB)
PRAGMA busy_timeout = 5000;        -- 5s retry on SQLITE_BUSY
```

**WAL mode** provides:
- Concurrent readers during writes (matching redb's MVCC semantics)
- Better write performance (sequential WAL appends vs random page writes)
- Crash recovery via the WAL file

**synchronous = NORMAL** provides:
- fsync on WAL checkpoint (not every commit)
- Durability guarantee: data survives process crash; may lose last few transactions on OS crash
- This matches redb's behavior (fsync on commit, but our writes are low-frequency)

**auto-checkpoint = 1000 pages** provides:
- WAL file stays bounded (~4MB max before checkpoint)
- No application-level checkpoint management
- Checkpoint runs automatically within the next write transaction after threshold

**Store::compact()** becomes a no-op under the SQLite backend. SQLite does not need COW page reclamation like redb. If space reclamation is ever needed, VACUUM can be exposed as a separate operation (not part of nxs-005).

### Consequences

- No explicit checkpoint calls needed -- SQLite manages this automatically.
- WAL file and -shm file are created alongside the main database file (3 files total vs 1 for redb).
- compact() is a no-op, which changes its semantic meaning. Callers that relied on compact() for space reclamation get no benefit under SQLite. This is acceptable because SQLite's WAL auto-checkpoint prevents unbounded growth.
- busy_timeout=5000 addresses the SQLITE_BUSY risk identified in SR assumption #1.
