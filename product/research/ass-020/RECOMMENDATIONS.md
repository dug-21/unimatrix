# ASS-020 Recommendations

---

## Immediate Tactical Fixes (1–2 days each, ship as bugfixes)

### FIX-1: Replace `.unwrap()` with `.map_err()` in compute_report() [CRITICAL — P1]

**File**: `crates/unimatrix-server/src/services/status.rs:638, 657`

Replace:
```rust
.await
.unwrap()
.unwrap_or_else(|_| ...)
```

With:
```rust
.await
.unwrap_or_else(|_| Ok(unimatrix_observe::ObservationStats { ... }))
.unwrap_or_else(|_| ...)
```

Or propagate the error properly. The `JoinError` case (task panicked) must be handled gracefully — log the panic, return a default value, and continue. Do not panic in an async task called from the background tick loop.

This is the highest-priority fix. It prevents the background tick from being permanently killed by a single blocking thread panic.

---

### FIX-2: Add `spawn_blocking_with_timeout` to all hot-path MCP handlers [CRITICAL — P3]

**Files**: `services/search.rs`, `services/store_ops.rs`, `services/briefing.rs`, `mcp/tools.rs`

Apply `spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)` to every `tokio::task::spawn_blocking()` call in the request hot path. This is already done for `context_retrospective` — extend the same pattern to:

- `SearchService::search()` — query embedding step (`search.rs:224`)
- `StoreService::insert()` — embedding step (`store_ops.rs:116`) and insert transaction (`store_ops.rs:187`)
- Every spawn_blocking in `context_status` that is not already timeout-protected

The 30-second `MCP_HANDLER_TIMEOUT` is the right value: it's short enough to prevent indefinite client hangs, long enough for normal operations.

---

### FIX-3: Cap the extraction tick observation batch [HIGH — P7]

**File**: `crates/unimatrix-server/src/background.rs:809`

Change `LIMIT 10000` to `LIMIT 500` or `LIMIT 1000`. The extraction rules run in-memory over the loaded observations — loading 10,000 rows in one mutex hold is disproportionate to the value. Add a second constant `EXTRACTION_BATCH_SIZE` to make this tunable.

If more than N observations accumulate between ticks, the watermark advances by N and the remainder is processed next tick. This is already how the watermark works — the limit just needs to be smaller.

---

### FIX-4: Move Step 2b to a separate spawn_blocking with its own lock acquisition [HIGH — P5]

**File**: `crates/unimatrix-server/src/services/status.rs:877–919`

The current implementation acquires `lock_conn()` and runs two full table scans inside the same spawn_blocking closure. Split this into two separate `lock_conn()` acquisitions (one per query), releasing the mutex between scans. This allows concurrent MCP requests to acquire the mutex between the two scans.

Better yet: add a `LIMIT` to both queries (e.g., 1000 rows for the voted_pairs scan) and use the already-loaded `active_entries` vector (already in memory from Phase 1) to avoid the second scan entirely. The active entries list is passed into `run_maintenance()` — extract confidence values from it in-memory.

---

### FIX-5: Add LIMIT to Session GC DELETE [MEDIUM — P13]

**File**: `crates/unimatrix-store/src/sessions.rs` (wherever `gc_sessions` is implemented)

Add a `LIMIT N` to the DELETE FROM sessions query to bound the GC operation. Run in batches if more than N sessions need deletion. Prevents the GC from holding the mutex for unbounded time as session count grows.

---

### FIX-6: Restart background tick loop on panic [CRITICAL — P1 followup]

**File**: `crates/unimatrix-server/src/background.rs:179–196`

Wrap the `tokio::spawn(background_tick_loop(...))` in a supervisor that detects task completion and respawns:

```rust
// Conceptual structure:
tokio::spawn(async move {
    loop {
        let handle = tokio::spawn(background_tick_loop_inner(...));
        match handle.await {
            Ok(()) => break, // clean exit (shutdown signal)
            Err(join_err) => {
                tracing::error!("background tick panicked: {join_err}; restarting");
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
    }
});
```

This ensures the background tick recovers from unexpected panics without requiring a full server restart.

---

## Short-Term Architectural Improvements (Feature scope)

### ARCH-1: Make contradiction scan opt-in or rate-limited [HIGH — P4]

**File**: `crates/unimatrix-server/src/services/status.rs:424–479`

The contradiction scan runs on every maintenance tick even though its results change slowly (entries are rarely added/corrected). Options:

1. **Rate-limit**: Run contradiction scan only every N ticks (e.g., every 4th tick = every hour). Maintain a counter in `TickMetadata`.
2. **Threshold-gate**: Only scan when active entry count has changed by >X since last scan.
3. **Background-only mode**: Move contradiction scan entirely out of `compute_report()` (which is called by `context_status` MCP tool as well as the tick). When called by the tick, scan; when called by `context_status`, return cached results from the last tick scan.

Option 3 is most impactful. Currently `context_status` from an agent call also runs the contradiction scan (when embed model is ready), adding ONNX inference cost to an interactive MCP response.

---

### ARCH-2: Decouple `compute_report()` from the maintenance tick [HIGH — P2, P4]

**File**: `crates/unimatrix-server/src/services/status.rs`

`compute_report()` was designed for `context_status` (an interactive MCP tool) and is repurposed by the maintenance tick to get the `active_entries` list needed for confidence refresh and graph compaction. This conflation is the root architectural problem.

The maintenance tick needs: active entries list, effectiveness aggregates, and co-access stats. It does NOT need: category/topic distributions, outcome stats, observation stats, retrospected feature count, or contradiction scan results.

Create a lighter `MaintenanceDataSnapshot` that loads only what the maintenance tick needs — probably 2–3 targeted SQL queries instead of 8 phases — and call that from `maintenance_tick()`. Reserve `compute_report()` for the `context_status` tool path only.

---

### ARCH-3: Move fire-and-forget usage recording to a dedicated async channel [HIGH — P9]

**File**: `crates/unimatrix-server/src/services/usage.rs`

Instead of spawning a new blocking thread per request, use a bounded `tokio::sync::mpsc::channel` to send usage events to a dedicated background task that batches them and flushes to SQLite periodically. This provides:

1. **Backpressure**: Bounded channel rejects or drops usage events when the channel is full, preventing thread pool saturation.
2. **Batching**: Multiple usage events from concurrent requests are batched into a single `record_usage_with_confidence()` call, reducing mutex acquisitions.
3. **Decoupling**: Usage recording cannot block MCP request handlers even if the store is slow.

The channel consumer should be a simple tokio task (async), with a flush triggered either by batch size (e.g., 20 events) or timer (e.g., every 5s). Implement as a `UsageChannel` struct with `send()` (non-blocking, drops if full) and a background `flush_loop()`.

---

### ARCH-4: Add read-only SQLite connection for read queries [HIGH — P2, P6]

**File**: `crates/unimatrix-store/src/db.rs`

SQLite WAL mode supports true concurrent readers when accessed via separate connections. The current architecture uses a single `Mutex<Connection>` for all reads and writes, which eliminates WAL's concurrency benefit.

Add a second `Connection` configured read-only (or in WAL mode with `PRAGMA read_uncommitted=1`) for all read queries:
- `Store::get()`, `Store::exists()`, `query_by_*()`, `query_all_entries()`, `load_active_entries_with_tags()`, etc.

Write operations continue through the single write connection. Reads proceed concurrently without waiting for the write mutex. This would eliminate the primary source of MCP request blocking when the background tick is running write operations.

**Note**: This requires careful connection management to avoid WAL checkpoint starvation. Consider `rusqlite::Connection::open_with_flags()` with `OpenFlags::SQLITE_OPEN_READ_ONLY`.

---

### ARCH-5: Bound the effectiveness aggregates query [HIGH — P2, P4]

**File**: `crates/unimatrix-server/src/services/status.rs:662–736`

The effectiveness analysis in Phase 8 of `compute_report()` runs complex JOIN queries over `injection_log`, `sessions`, and entries. At scale, `compute_effectiveness_aggregates()` could be very slow.

Cache the effectiveness analysis result in `EffectivenessStateHandle` with a TTL. If the last computation was <15 minutes ago, return the cached result. Only recompute in the background tick. Interactive `context_status` calls return cached results immediately.

This decouples the effectiveness analysis cost from interactive `context_status` calls.

---

### ARCH-6: Audit log batching [MEDIUM — P10]

**File**: `crates/unimatrix-server/src/infra/audit.rs`

Audit events from the background tick (auto-quarantine events, tick-skipped events) should be batched into a single transaction. Instead of one `BEGIN IMMEDIATE` per event, accumulate events in memory and write them in one transaction at the end of the tick phase.

For interactive MCP path audit events (one per request), consider using the same async channel pattern as ARCH-3.

---

## Long-Term Scalability Changes (Future phases)

### SCALE-1: Replace Mutex<Connection> with connection pool [ARCHITECTURAL]

The fundamental scalability bottleneck is a single SQLite connection under a Rust mutex. Options:

1. **`r2d2-sqlite`**: Connection pool with multiple read connections and a single write connection. Requires careful handling of WAL checkpoints.
2. **`sqlx` with `sqlx::SqlitePool`**: Built-in connection pool with async support. Would require rewriting the store layer significantly.
3. **Move to a client-server database**: DuckDB, PostgreSQL (via `sqlx::PgPool`), or TiKV for the knowledge entries, keeping SQLite only for transient/small tables (sessions, observations, audit_log).

Option 1 is achievable in a single feature scope (~2-3 weeks). Options 2 and 3 are multi-sprint architectural changes.

---

### SCALE-2: Streaming extraction tick [ARCHITECTURAL]

Replace the current extraction tick's bulk-load-then-process pattern with a streaming cursor that processes observations in small batches, releasing and re-acquiring the mutex between batches. Eliminates the 10,000-row single-lock-hold issue entirely.

---

### SCALE-3: Background tick sharding [ARCHITECTURAL]

At 3-5× current volume, the single 15-minute tick is too coarse. Consider:

1. **Split the tick into multiple specialized tasks** with different intervals:
   - Confidence refresh: every 5 minutes, small batch
   - Effectiveness analysis: every 60 minutes
   - Contradiction scan: every 120 minutes
   - Co-access cleanup: every 30 minutes
   - Session GC: every 60 minutes
   - Extraction: every 5 minutes (small batch)

2. Each task has its own interval timer and timeout, so a slow task doesn't delay all others.

This prevents the current situation where a slow effectiveness computation delays the confidence refresh that agents depend on.

---

### SCALE-4: Read replica or CQRS pattern [ARCHITECTURAL]

Separate the "write store" (the SQLite database for writes) from the "read store" (an in-memory projection or a read-only SQLite snapshot). The background tick refreshes the in-memory projection periodically. MCP read requests (search, lookup, get) read from the in-memory projection without any SQLite access. Write requests (store, correct, deprecate) go through the write connection.

This eliminates mutex contention entirely for read requests, at the cost of slight staleness (bounded by refresh interval, e.g., 30 seconds).
