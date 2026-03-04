## ADR-002: Mutex<Connection> for Send + Sync Store

### Context

The Store struct must be `Send + Sync` (shared via `Arc<Store>` across async handlers in unimatrix-server). redb::Database is both Send and Sync natively. rusqlite::Connection is Send but NOT Sync -- it wraps a raw SQLite handle that is not thread-safe for concurrent access.

### Decision

Wrap the SQLite connection in `std::sync::Mutex<rusqlite::Connection>`:

```rust
#[cfg(feature = "backend-sqlite")]
pub struct Store {
    conn: std::sync::Mutex<rusqlite::Connection>,
}
```

Every Store method acquires the mutex lock for the duration of the operation. This serializes all database access (both reads and writes) through a single lock.

**Why this is acceptable:**
1. redb already serializes write transactions -- only one writer at a time. The actual concurrency change is that readers now also serialize, but redb read transactions are snapshots that complete in microseconds at our scale.
2. The server uses `spawn_blocking` for all store operations -- the mutex is held inside a blocking task, not on the async executor.
3. At ~53 entries, lock contention is negligible. Even at 10K entries, store operations complete in single-digit milliseconds.

**Alternative considered**: `tokio::sync::Mutex`. Rejected because Store methods are synchronous and called from `spawn_blocking`. A tokio mutex would require `.await` which is not compatible with the synchronous Store API.

**Alternative considered**: `RwLock<Connection>`. Rejected because rusqlite::Connection is not Sync, so even shared read access through RwLock would require mutable access (defeating the purpose). SQLite WAL mode allows concurrent readers at the engine level, but the Rust binding requires exclusive access to the Connection handle.

**Alternative considered**: Connection pool (r2d2-sqlite). Rejected as over-engineering for this scale. A connection pool adds complexity (pool sizing, idle connection management) for a workload that is well-served by a single serialized connection.

### Consequences

- All Store operations are serialized. This is a slight regression from redb's concurrent readers, but unnoticeable at our scale.
- No `parking_lot::Mutex` dependency -- `std::sync::Mutex` is sufficient and avoids the external dependency.
- The `compact()` method currently takes `&mut self`. With Mutex, it can take `&self` since the mutex provides interior mutability. However, we keep the `&mut self` signature for API compatibility (existing callers already hold mutable access).
