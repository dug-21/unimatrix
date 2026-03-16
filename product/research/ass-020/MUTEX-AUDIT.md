# Mutex Audit: Every `lock_conn()` Call Site

All call sites for `Store::lock_conn()` and `Store::begin_write()` (which calls `lock_conn()` internally).

---

## Lock Acquisition Pattern

`Store::lock_conn()` is defined at `crates/unimatrix-store/src/db.rs:86`:

```rust
pub fn lock_conn(&self) -> MutexGuard<'_, Connection> {
    self.conn.lock().unwrap_or_else(|e| e.into_inner())
}
```

`Store::begin_write()` calls `lock_conn()` at `db.rs:81`, wrapping it in a `SqliteWriteTransaction`. The lock is held for the lifetime of the transaction guard.

**Key fact**: SQLite WAL mode (`PRAGMA journal_mode = WAL`) allows concurrent readers but only one writer at a time. However, this is a Rust `Mutex<Connection>` — **all access, both reads and writes, is serialized through this single mutex**. WAL's concurrent-reader benefit is entirely negated because every read also holds the mutex.

---

## Complete `lock_conn()` Call Map

### unimatrix-store/src/db.rs

| Line | Context | Hold Time Estimate | Notes |
|------|---------|-------------------|-------|
| 63 | `create_tables()` during `Store::open()` | Long (many DDL statements) | Startup only |
| 81 | `begin_write()` → wraps in `SqliteWriteTransaction` | Varies — held until guard drops | Callers below |
| 87 | `lock_conn()` public API | Varies | Direct callers below |

### unimatrix-store/src/write.rs

| Line | Function | SQL Statements | Hold Time |
|------|----------|---------------|-----------|
| 23 | `Store::insert()` | `BEGIN IMMEDIATE` + SELECT counter + INSERT entries + N×INSERT tags + UPDATE counter + COMMIT | ~5–15 SQL ops, medium |
| 111 | `Store::update()` | `BEGIN IMMEDIATE` + SELECT old_status + UPDATE entries + DELETE/INSERT tags + UPDATE counter + COMMIT | ~6–20 SQL ops, medium |

### unimatrix-store/src/write_ext.rs

| Line | Function | SQL Statements | Hold Time | Notes |
|------|----------|---------------|-----------|-------|
| 76 | `record_usage_with_confidence()` | `BEGIN IMMEDIATE` + N×(SELECT EXISTS + dynamic UPDATE + optional SELECT + UPDATE confidence) + COMMIT | **Per-entry: 3–4 SQL ops; N entries = O(N) hold** | Hot path, batch |
| (via) | `record_usage()` | Same as above, no confidence_fn | Same | |

### unimatrix-store/src/read.rs

| Line | Function | SQL Statements | Hold Time |
|------|----------|---------------|-----------|
| 106 | `Store::get()` | SELECT entry + SELECT tags | Short (~2 SQL) |
| 126 | `Store::exists()` | SELECT 1 | Very short |
| 141 | `Store::query_by_topic()` | SELECT entries + SELECT tags (batch) | Short–medium |
| 164 | `Store::query_by_category()` | Same | Short–medium |
| 187 | `Store::query_by_tags()` | Complex JOIN + SELECT tags | Medium |
| 229 | `Store::query_by_time_range()` | SELECT + tags | Short–medium |
| 257 | `Store::query_by_status()` | SELECT all with status + tags | **Medium–Long** (full table scan with tags batch) |
| 285 | `Store::query_all_entries()` | SELECT ALL entries + ALL tags | **Long** (full table scan, all entries, all tags) |

### unimatrix-store/src/counters.rs

| Function | SQL | Hold Time |
|----------|-----|-----------|
| `read_counter()` | SELECT 1 row | Very short — but called from within existing locks (nested via `lock_conn()`) |
| `set_counter()` | UPDATE 1 row | Very short |
| `next_entry_id()` | SELECT + UPDATE | Very short |
| `increment_counter()` | UPDATE | Very short |

### unimatrix-server/src/background.rs

| Line | Call | SQL Statements | Frequency | Hold Time |
|------|------|---------------|-----------|-----------|
| 131 | `store.lock_conn()` in `persist_shadow_evaluations()` | `prepare_cached` + N×INSERT shadow_evaluations | Per extraction tick | Medium (N inserts) |
| 808 | `store.lock_conn()` in `extraction_tick()` step 1 | SELECT observations WHERE id > watermark LIMIT 10000 | Per tick | **Long** (up to 10k rows) |
| 879 | `store.lock_conn()` in `run_maintenance()` step 2b | SELECT helpful/unhelpful votes + SELECT confidence values | Per tick | **Long** (two full table scans of active entries) |
| 1004 | `store.lock_conn()` in `run_maintenance()` step 4 | DELETE FROM observations WHERE ts_millis < cutoff | Per tick | Short–medium |

### unimatrix-server/src/infra/audit.rs

| Line | Function | SQL | Hold Time | Notes |
|------|----------|-----|-----------|-------|
| 35 | `log_event()` | `BEGIN IMMEDIATE` + SELECT counter + UPDATE counter + INSERT + COMMIT | Short | Called from fire-and-forget spawn_blocking; also from background tick (emit_auto_quarantine_audit, emit_tick_skipped_audit) — synchronous, holds lock for entire transaction |
| 88 | `write_count_since()` | SELECT COUNT(*) | Very short | Called from SecurityGateway rate check — on request hot path |

### unimatrix-server/src/services/status.rs

| Phase | Call | SQL | Hold Time |
|-------|------|-----|-----------|
| Phase 1 (line 210) | `store.lock_conn()` in `compute_report()` | 4× SELECT counters + 1× SELECT category GROUP BY + 1× SELECT topic GROUP BY + drop + `compute_status_aggregates()` + `load_active_entries_with_tags()` + `load_outcome_entries_with_tags()` | **Very Long** — multi-operation, holds across all Phase 1 |
| Phase 4 (line 491) | `store.lock_conn()` via `co_access_stats()` + `top_co_access_pairs()` + N× `store.get()` | ~3 + N SQL queries | Medium–Long |
| Phase 6 (line 632) | `SqlObservationSource::new(store)` → observation_stats | SELECT COUNT + GROUP BY sessions | Medium |
| Phase 7 (line 652) | `store.list_all_metrics()` | SELECT observation_metrics | Short–medium |
| Phase 8 (line 662) | `compute_effectiveness_aggregates()` + `load_entry_classification_meta()` | Multiple aggregation queries | **Long** |
| Step 2b (line 878) | `store.lock_conn()` | 2× SELECT (voted_pairs + all_confidences) | **Long** (two full scans of entries table) |

### unimatrix-server/src/services/search.rs

| Step | Call | SQL | Hold Time |
|------|------|-----|-----------|
| Step 6 | `entry_store.get()` per HNSW result (N=k results) | N× SELECT entry + N× SELECT tags | Medium (per request, N=5 default) |

---

## Critical Observations

### 1. Mutex hold in `record_usage_with_confidence()`
`write_ext.rs:76` holds the mutex for `O(N)` SQL operations — one `EXISTS` check + one dynamic `UPDATE` + optionally two more SELECTs for confidence recomputation **per entry ID in the batch**. For a k=5 search, this is 5–15 SQL statements inside a single `BEGIN IMMEDIATE` transaction. This is the primary contention source for concurrent requests.

### 2. Step 2b in `run_maintenance()` holds lock for dual table scans
`status.rs:878–919`: A single `lock_conn()` runs two full `SELECT` statements over the `entries` table — one for voted entries (no LIMIT), one for all confidence values (no LIMIT). At current scale (~50 active entries) this is fast. At 500+ entries, this becomes the dominant blocking hold in the tick.

### 3. `compute_report()` Phase 1 takes multiple SQL operations under one lock
`status.rs:208–411`: The `spawn_blocking` closure acquires `lock_conn()` once via `conn = store.lock_conn()` and runs through: 4 counter reads, 2 GROUP BY aggregations, drops the guard, then re-acquires for `compute_status_aggregates()`, `load_active_entries_with_tags()`, `load_outcome_entries_with_tags()`. The guard is dropped at line 302 before these subsequent calls, which each re-acquire. However, this still generates **5+ distinct mutex acquisitions** in rapid serial succession during the tick.

### 4. `query_all_entries()` in SupersessionState rebuild
`read.rs:285`: SELECT all entries + batch tag load — full table scans with no WHERE clause. At 500+ entries with tags, this holds the mutex for the entire duration of a potentially large result set iteration.

### 5. Audit log holds mutex with `BEGIN IMMEDIATE` per event
`audit.rs:35`: Each `log_event()` call starts a `BEGIN IMMEDIATE` transaction. During auto-quarantine processing (up to N candidates per tick), this is called N+1 times (one per candidate + one tick_skipped if failed). Each holds the mutex.

### 6. `AuditLog::write_count_since()` on every MCP request
`audit.rs:88`: Called from `SecurityGateway` on the request hot path. While the query is fast, it competes with write transactions from the background tick.
