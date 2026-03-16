# ASS-020 Findings: Availability and Scalability Review

**Date**: 2026-03-14
**Reviewer**: ASS-020 Research Spike
**Scope**: Unimatrix MCP server (crt-014 → crt-018b → crt-019 regression analysis)

---

## Priority-Ordered Findings

| Priority | Finding | Severity | Scalability Impact | File:Line | Fix Type |
|----------|---------|----------|-------------------|-----------|----------|
| P1 | **Naked `.unwrap()` on JoinError kills background tick** | Critical | Any scale | `services/status.rs:638, 657` | Tactical |
| P2 | **Tick holds store mutex for 6 sequential 120s windows** | Critical | Worsens at scale | `background.rs:309–409` | Tactical |
| P3 | **Most MCP hot-path spawn_blocking calls have NO timeout protection** | Critical | Any scale | `services/search.rs`, `store_ops.rs`, `mcp/tools.rs` | Tactical |
| P4 | **Contradiction scan runs on EVERY maintenance tick, O(N²) ONNX calls** | High | Breaks at >100 entries | `services/status.rs:425–479` | Architectural |
| P5 | **Step 2b holds lock for two unbounded full table scans** | High | Worsens linearly | `services/status.rs:878–919` | Tactical |
| P6 | **`record_usage_with_confidence()` holds mutex for O(N) SQL ops in a single transaction** | High | Concurrent requests pile up | `write_ext.rs:61–165` | Architectural |
| P7 | **Extraction tick queries up to 10,000 observations in a single lock hold** | High | Grows indefinitely | `background.rs:807–870` | Tactical |
| P8 | **Embed model init race: context_store silently fails during model loading** | High | Any scale, startup window | `infra/embed_handle.rs:116–131` | Tactical |
| P9 | **Fire-and-forget spawn_blocking tasks have no backpressure or cap** | High | Worsens with volume | `services/usage.rs:212` | Architectural |
| P10 | **AuditLog.log_event() is synchronous BEGIN IMMEDIATE per event** | Medium | Worsens with audit volume | `infra/audit.rs:35–82` | Architectural |
| P11 | **rmcp has no idle timeout; blocking spawn_blocking tasks can stall transport** | Medium | Any scale | `main.rs:369–398` | Architectural |
| P12 | **SupersessionState rebuild does full table scan every 15 minutes** | Medium | Worsens linearly | `services/supersession.rs:88–97` | Tactical |
| P13 | **Session GC DELETE CASCADE on injection_log is unbounded** | Medium | Grows over time | `services/status.rs:1059–1079` | Tactical |
| P14 | **EffectivenessState write lock held for O(N) in-memory iteration before SQL** | Low | Low — in-memory only | `background.rs:462–545` | Low |
| P15 | **Dual `EffectivenessStateHandle` created (server.rs:177 + main.rs:323)** | Low | Code smell only | `server.rs:176–177`, `main.rs:321–323` | Tactical |

---

## Detailed Findings

### P1: Naked `.unwrap()` on JoinError kills background tick (CRITICAL)

**File**: `crates/unimatrix-server/src/services/status.rs:638, 657`

```rust
// Line 636-638:
.await
.unwrap()  // <-- panics if spawn_blocking thread panics
.unwrap_or_else(|_| ...);

// Line 656-658:
.await
.unwrap()  // <-- panics if spawn_blocking thread panics
.unwrap_or_else(|_| vec![]);
```

**Impact**: If either blocking thread panics (OOM, lock poisoned, SQLite error that becomes a panic, or future tokio regression), the `.unwrap()` call panics inside the async task running `compute_report()`. Panics in async tasks are not catchable by the tick loop's `if let Err(e) = tick_result` handler at `background.rs:260` — the task itself aborts. This can kill the maintenance tick permanently for the server session.

The panic hook at `main.rs:108` logs to stderr, but the background tick task is a fire-and-forget `tokio::spawn`. Once the task is dead, no maintenance, confidence refresh, effectiveness classification, or extraction runs again until server restart.

**Evidence**: The background tick at `background.rs:234–263` has no restart-on-panic logic. `run_single_tick()` returns `Result<(), String>` but a panic bypasses this entirely.

---

### P2: Tick holds store mutex in three sequential 120-second windows (CRITICAL)

**File**: `background.rs:309–409`

The `run_single_tick()` function runs:
1. `maintenance_tick()` → wrapped in `tokio::time::timeout(TICK_TIMEOUT=120s, ...)`
2. SupersessionState rebuild → wrapped in `tokio::time::timeout(TICK_TIMEOUT=120s, ...)`
3. `extraction_tick()` → wrapped in `tokio::time::timeout(TICK_TIMEOUT=120s, ...)`

These three phases run **sequentially**. Each has its own 120s timeout, for a total potential tick duration of **up to 360 seconds (6 minutes)**. During most of this time, one or more spawn_blocking threads are competing with MCP request handlers for the store mutex.

Within `maintenance_tick()` alone, `compute_report()` launches **6 sequential spawn_blocking calls**, each requiring the store mutex. These are not overlapping — each awaits the previous. At current scale, the total maintenance tick takes ~30–60 seconds. At 3-5× scale, it will routinely hit the 120s timeout.

**During the tick, MCP requests that call spawn_blocking and need the store mutex must wait**. With `PRAGMA busy_timeout = 5000` (`db.rs:39`), SQLite itself has a 5s busy timeout, but the Rust mutex has **no timeout** — it blocks indefinitely. An MCP tool handler's spawn_blocking task waiting for the mutex can therefore block for the entire duration of the tick's mutex hold.

---

### P3: Most MCP hot-path spawn_blocking calls have no timeout (CRITICAL)

**File**: `services/search.rs:224–230`, `services/store_ops.rs:116–122`, `mcp/tools.rs:374–382`

The `spawn_blocking_with_timeout()` utility exists at `infra/timeout.rs` with `MCP_HANDLER_TIMEOUT=30s`, but it is **only used in `context_retrospective`** (`mcp/tools.rs:1126, 1158, 1222, 1258, 1274`).

Every other MCP tool handler uses bare `tokio::task::spawn_blocking(...)` with no timeout:

- `context_search`: embedding spawn_blocking at `search.rs:224`, entry fetch loops at `search.rs:263`
- `context_store`: embedding spawn_blocking at `store_ops.rs:116`, insert transaction at `store_ops.rs:187`
- `context_lookup`: entry_store queries (async but delegates to blocking internally)
- `context_status`: 6 sequential spawn_blocking calls in `compute_report()`
- `context_briefing`: embedding + fetch spawn_blocking

If any of these spawn_blocking tasks cannot acquire the store mutex (because the background tick holds it), they block indefinitely. rmcp uses stdio transport (`main.rs:370`) with no configured idle timeout. The MCP client (Claude Code) has its own timeout, after which it disconnects — but the server-side task is still blocked on the mutex, leaking a blocking thread slot.

After the client disconnects and reconnects (the `cycle_stopped` + reconnect symptom), the new MCP session's requests also spawn new blocking tasks that queue behind the still-blocked old ones.

---

### P4: Contradiction scan runs on every maintenance tick (HIGH)

**File**: `services/status.rs:424–479`

```rust
// Phase 2: Contradiction scanning (outside read txn) — runs unconditionally
if let Ok(adapter) = self.embed_service.get_adapter().await {
    match tokio::task::spawn_blocking(move || {
        let vs = VectorAdapter::new(vi_for_scan);
        contradiction::scan_contradictions(...)
    }).await { ... }
}
```

`scan_contradictions()` at `infra/contradiction.rs:154` loads all active entries, then for **each** active entry: embeds it (ONNX inference), performs HNSW search for 10 neighbors, and applies the conflict heuristic. This is **O(N)** ONNX inference calls where N = active entry count.

This runs during every 15-minute maintenance tick as part of `compute_report()`. It also holds the store mutex to load entries, then releases it for ONNX computation. At 50 active entries: ~5 seconds. At 500 active entries: ~50 seconds. This alone can consume most of the 120-second tick budget.

The `check_embeddings` parameter does control Phase 3 (consistency check), but Phase 2 (contradiction scan) is **always active when the embed model is ready**.

---

### P5: Step 2b holds mutex for two full table scans (HIGH)

**File**: `services/status.rs:878–919`

```rust
let prior_result = tokio::task::spawn_blocking(move || -> (f64, f64, f64, f64) {
    let conn = store_for_prior.lock_conn();
    // Scan 1: all voted entries (no LIMIT)
    let voted_pairs = conn.prepare("SELECT helpful_count, unhelpful_count
        FROM entries WHERE status = 'active' AND ...").query_map(...);
    // Scan 2: all active confidence values (no LIMIT)
    let all_confidences = conn.prepare("SELECT confidence FROM entries WHERE status = 'active'").query_map(...);
    ...
})
```

Both scans run **in the same `lock_conn()` acquisition** — the mutex is held for the duration of both queries. Neither has a LIMIT clause. This is added by crt-019 and runs on every maintenance tick.

---

### P6: `record_usage_with_confidence()` holds mutex for O(N) ops in one transaction (HIGH)

**File**: `crates/unimatrix-store/src/write_ext.rs:61–165`

```rust
let conn = self.lock_conn();
conn.execute_batch("BEGIN IMMEDIATE") ...
for &id in all_ids {  // N iterations
    // SELECT EXISTS + dynamic UPDATE + optional 2× SELECT + UPDATE confidence
}
COMMIT
```

The `BEGIN IMMEDIATE` transaction holds the mutex for all N entries. With `confidence_fn=Some(...)` (the normal path for MCP usage recording), each entry requires: EXISTS check + UPDATE + SELECT entry + SELECT tags + UPDATE confidence = **5 SQL statements per entry**. For a k=5 search returning 5 entries: **25 SQL statements in one mutex hold**.

For `context_lookup` with `access_weight=2`, each entry appears twice in `all_ids`, making this 50 SQL statements.

This runs as a fire-and-forget spawn_blocking on every `context_search`, `context_lookup`, `context_get`, and `context_briefing` call. Under concurrent requests (3-5 agents), these pile up in the blocking thread pool, each waiting for the mutex.

---

### P7: Extraction tick queries up to 10,000 observations in one lock hold (HIGH)

**File**: `background.rs:807–870`

```rust
let (observations, new_watermark) = tokio::task::spawn_blocking(move || {
    let conn = store_clone.lock_conn();
    let mut stmt = conn.prepare(
        "SELECT ... FROM observations WHERE id > ?1 ORDER BY id ASC LIMIT 10000"
    )...
```

The mutex is held for the entire result set iteration of up to 10,000 rows. At high hook event volume, this could hold the mutex for several seconds, blocking all concurrent MCP requests.

Additionally, `run_extraction_rules()` at `background.rs:881–886` runs inside a second `spawn_blocking` and also accesses the store (via `store_for_rules`), acquiring the mutex again.

---

### P8: Embed model init race: context_store fails during loading window (HIGH)

**File**: `infra/embed_handle.rs:116–131`

```rust
pub async fn get_adapter(&self) -> Result<Arc<EmbedAdapter>, ServerError> {
    let state = self.state.read().await;
    match &*state {
        EmbedState::Loading | EmbedState::Retrying { .. } => Err(ServerError::EmbedNotReady),
        EmbedState::Failed { attempts, .. } if *attempts < MAX_RETRIES => Err(ServerError::EmbedNotReady),
        EmbedState::Failed { message, .. } => Err(ServerError::EmbedFailed(message.clone())),
        EmbedState::Ready(adapter) => Ok(Arc::clone(adapter)),
    }
}
```

The embedding model is loaded lazily in a background task. Any `context_store`, `context_search`, `context_briefing`, or `context_correct` call during the loading window returns `EmbedNotReady`. This is by design, but the window is unbounded: if ONNX model loading takes >60 seconds (cold cache, slow disk, network download), all embedding-dependent tools fail for that duration.

The retry monitor (`embed_handle.rs:137`) wakes every 10s with a read lock on `state`. This is **itself a source of RwLock contention** — every 10s, the retry monitor acquires a write lock to transition `Failed` → `Retrying`, blocking any concurrent `get_adapter()` read locks briefly.

---

### P9: Fire-and-forget spawn_blocking has no backpressure (HIGH)

**File**: `services/usage.rs:212`

```rust
let _ = tokio::task::spawn_blocking(move || {
    store.record_usage_with_confidence(...)
    store.record_feature_entries(...)
    store.record_co_access_pairs(...)
});
```

Every MCP request (search, lookup, get, briefing) fires one `spawn_blocking` that is discarded (`let _ = ...`). Under concurrent load (3-5 agents, each calling search), these accumulate in the tokio blocking thread pool. Each waits for the store mutex. Tokio's default blocking pool limit is 512 threads. While actual saturation is unlikely at current scale, the interaction is:

- Concurrent requests → N fire-and-forget spawn_blocking tasks
- Background tick → 20–30 sequential spawn_blocking tasks
- All compete for the single store mutex
- Tasks that time out (if the mutex wait exceeds their implicit wait budget) are not cancelled — they remain blocked until the mutex is released, permanently consuming a blocking thread slot

Additionally, the query log fire-and-forget at `mcp/tools.rs:374–382` spawns **one additional spawn_blocking per search call** — this is never mentioned in comments and was added separately from the usage recording.

---

### P10: Audit log uses synchronous BEGIN IMMEDIATE per event (MEDIUM)

**File**: `infra/audit.rs:35–82`

Every audit event is a full `BEGIN IMMEDIATE` transaction: acquire mutex → read counter → write counter → INSERT → COMMIT. During auto-quarantine (N entries), this is called N+1 times (`emit_tick_skipped_audit` + N× `emit_auto_quarantine_audit`). Each call competes with any concurrent MCP request audit events. The IMMEDIATE lock upgrade blocks all other writers.

---

### P11: No rmcp idle timeout; blocked spawn_blocking can stall MCP transport (MEDIUM)

**File**: `main.rs:369–406`, rmcp configuration

rmcp 0.16.0 with `rmcp::transport::io::stdio()` transport has no configured idle timeout or keepalive pings. The server waits indefinitely for the next MCP message (`running.waiting().await`).

When a MCP tool call (e.g., `context_search`) dispatches a spawn_blocking task that blocks on the store mutex for >N seconds, rmcp receives no response during that time. The MCP client (Claude Code) has its own timeout (~60s typically). When the client timeout fires:
1. Client sends a disconnect / closes stdin
2. rmcp receives EOF on stdin, `waiting()` returns `QuitReason::Closed`
3. Server starts graceful shutdown at `main.rs:396`
4. `graceful_shutdown()` is called, which aborts remaining tasks
5. The still-blocked spawn_blocking task is dropped (the blocking thread continues until it either gets the mutex or the process exits)

The **thread leak during disconnect** is a real issue: the blocked spawn_blocking thread continues executing after the MCP connection closes, consuming the mutex for a completed transaction that will be discarded. When the server restarts (a new process is launched by the MCP client), it finds the old database file possibly in mid-write.

---

### P12: SupersessionState rebuild does full table scan every 15 minutes (MEDIUM)

**File**: `services/supersession.rs:88–97`

```rust
pub fn rebuild(store: &Store) -> Result<Self, StoreError> {
    let all_entries = store.query_all_entries()?;  // Full scan, all entries, all tags
    ...
}
```

`query_all_entries()` at `read.rs:285` does `SELECT * FROM entries` with no WHERE clause, then batch-loads all tags. At 500 entries with average 3 tags each: ~1,500+ rows read in two queries. The mutex is held for the entire iteration. This replaced 4× `query_by_status()` calls (GH #266), which was an improvement, but the single full scan still holds the mutex longer than necessary.

---

### P13: Session GC DELETE CASCADE is unbounded (MEDIUM)

**File**: `services/status.rs:1059–1079`

```rust
tokio::task::spawn_blocking(move || {
    store_gc.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
})
```

`gc_sessions()` DELETEs sessions older than `DELETE_THRESHOLD_SECS`, with CASCADE to `injection_log`. With many sessions and large injection logs, this DELETE can hold the mutex for an extended period. No LIMIT clause is applied to the GC batch.

---

## Regression Attribution: crt-014 → crt-018b → crt-019

### crt-014: SupersessionGraph (PR #265, #269)
Added `SupersessionState::rebuild()` to the background tick (GH #264 fix). This is one additional full-table-scan spawn_blocking per tick (the `query_all_entries()` call). By itself: minimal impact (+1–5 seconds per tick). The real problem was that before #264, the search path called `query_by_status()` 4× per search request, which was worse on the hot path. The fix moved cost to the tick but made the tick heavier.

### crt-018b: EffectivenessStateHandle
Added Phase 8 to `compute_report()` — `compute_effectiveness_aggregates()` + `load_entry_classification_meta()` + full entry classification loop. This is the **largest single addition to the tick** from the recent regressions. At current scale it adds ~5–10 seconds. At 5× scale it will add ~30–60 seconds.

Also added the `EffectivenessState` write lock acquisition inside `maintenance_tick()` after `compute_report()` — this is pure in-memory but involves rebuilding a HashMap<u64, EffectivenessCategory> for all active entries.

Also added auto-quarantine processing (spawning N sequential spawn_blocking tasks, each awaited, each requiring the store mutex) — this is the primary source of the `cycle_stopped` errors if any one quarantine blocks and exhausts the tick's timeout budget.

### crt-019: Confidence weight adaptive blend
Added Step 2b to `run_maintenance()` — two full table scans of the entries table under a single `lock_conn()`, plus updating `ConfidenceStateHandle`. This adds ~2–5 seconds per tick at current scale, ~20–50 seconds at 5× scale.

### Combined Effect (The Stacking Problem)
None of these features individually exceeds the 120s tick timeout at current scale. But they are **additive**:

```
compute_report() phases:     ~20–40s (includes Phase 8 from crt-018b)
Auto-quarantine (N=3):       ~3–9s (from crt-018b)
run_maintenance() steps:     ~10–20s (includes Step 2b from crt-019)
SupersessionState rebuild:   ~2–5s (from crt-014)
extraction_tick():           ~5–15s

Total:                       ~40–89s (current scale, no idle)
```

Any of these operations being slow (SQLite contention, slow disk I/O under load, GC pause) can push the tick over 120s, causing the timeout. When the timeout fires on `maintenance_tick()`, the `EffectivenessState` is not updated, `confidence_state` is not updated, and auto-quarantine does not run — but the `SupersessionState rebuild` and `extraction_tick()` still proceed in sequence (each with their own 120s budget). This asymmetric behavior can lead to stale effectiveness data being used in search re-ranking.

The `~15 minute idle timeout` symptom matches exactly: the tick runs at t=0 and t=15min. If the tick at t=15min causes the mutex to be held for >30s, any MCP request during that window waits >30s (past the typical client timeout), causing a disconnect and reconnect.
