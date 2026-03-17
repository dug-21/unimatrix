## ADR-001: Pool Acquire Timeout Values

### Context

`SqlxStore` introduces two `SqlitePool` instances: `read_pool` (6–8 connections) and
`write_pool` (≤2 connections). Both pools require a configured `acquire_timeout` to prevent
callers from blocking indefinitely when all connections are in use. Without a timeout,
pool saturation silently blocks the tokio runtime — exactly the pathology documented in
entries #771 (blocking lock_conn on tokio runtime causes intermittent hangs) and #1628
(per-query full-store reads inside spawn_blocking causing MCP instability under load).

The spec (FR-02, NF-02) mandates that timeout values be defined by architect ADR before
AC-10 can be verified. The suggested defaults (read: 2s, write: 5s) from SCOPE.md Q7 are
the starting point.

**Key asymmetry to account for:**
- `read_pool` serves MCP hot-path queries. Callers expect low latency. A 2s timeout is
  long from a user perspective but appropriate as a safety net — under normal operation,
  read connections return in milliseconds. A 2s wait before a structured error is better
  than an indefinite block, and better than a 500ms timeout that trips under momentary
  read spikes.
- `write_pool` (cap 2) serves integrity writes and the drain task. Integrity writes must
  not be silently dropped. The drain task holds a write connection for the duration of
  a batch commit. Under heavy write load, a new caller may need to wait for the drain task
  to finish its current batch (≤50 events, typically <50ms at SQLite speeds). A 5s timeout
  provides ample headroom for batch commit completion without indefinite blocking.
- The drain task itself does not acquire via `acquire_timeout` — it opens a transaction for
  the batch duration and releases it promptly. The 5s timeout is for external callers (MCP
  integrity write paths) contending with the drain task.

**Why not shorter timeouts (e.g., 500ms / 1s)?**
The tokio runtime's default blocking thread keepalive was the original source of latency
spikes (entry #735, #771). With async-native sqlx pools, connection acquisition is async
and does not block a worker thread. Short timeouts risk false-positive `PoolTimeout` errors
during transient load spikes (e.g., server startup when all connections are being
established) rather than genuine saturation events.

**Why not longer timeouts (e.g., 10s / 30s)?**
The goal of the timeout is to produce a structured `StoreError::PoolTimeout` rather than an
indefinite block. A 10s+ timeout means MCP callers would appear hung for 10 seconds to the
client before receiving an error. 5s is the outer boundary for acceptable MCP response time
before the client considers the server unresponsive.

**Test context:** `PoolConfig::test_default()` uses shorter timeouts:
- `read_acquire_timeout`: 500ms
- `write_acquire_timeout`: 1s

This prevents tests from hanging for 2s/5s when exercising saturation scenarios.

### Decision

`read_pool` acquire timeout: **2 seconds** (`Duration::from_secs(2)`).
`write_pool` acquire timeout: **5 seconds** (`Duration::from_secs(5)`).

These values are defined as named public constants in `unimatrix-store/src/pool_config.rs`:

```rust
pub const READ_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);
pub const WRITE_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
```

`PoolConfig::default()` uses these constants. Test helpers use `PoolConfig::test_default()`
with shorter timeouts (500ms read, 1s write) to avoid test suite slowdown when exercising
saturation.

When either timeout elapses, the pool returns a `sqlx::Error::PoolTimedOut`, which the
store maps to `StoreError::PoolTimeout { pool: PoolKind, elapsed: Duration }`. No panic.
No indefinite block.

### Consequences

- AC-10 is now fully specifiable: "pool acquire_timeout configured on both pools; timeout
  returns StoreError::PoolTimeout within 2s (read) or 5s (write)".
- MCP callers receive a structured error within a bounded time rather than hanging.
- `PoolConfig::test_default()` provides isolation from the production timeouts in test bodies.
- If operational experience reveals the 5s write timeout is too long under sustained write
  load, it can be tightened without an ADR revision — the constant is in a single location.
- The timeout values apply to connection _acquisition_, not to query execution. Long-running
  queries are a separate concern and not bounded by these values.
