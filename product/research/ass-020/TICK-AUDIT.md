# Background Tick Audit

Tick interval: `TICK_INTERVAL_SECS = 900` (15 minutes), defined at `background.rs:54`.
Tick timeout: `TICK_TIMEOUT = Duration::from_secs(120)` (2 minutes), defined at `background.rs:270`.

The tick loop is at `background.rs:200–263`. It runs `run_single_tick()` which chains:
1. `maintenance_tick()` (with 120s timeout)
2. SupersessionState rebuild (with 120s timeout)
3. `extraction_tick()` (with 120s timeout)

Maximum cumulative tick duration: **up to 6 minutes** (3× 120s timeouts, each independently timed).

---

## Phase 1: `maintenance_tick()` (`background.rs:429–577`)

Calls `status_svc.compute_report(None, None, false).await`.

### compute_report() phases (`services/status.rs:200–739`)

#### Phase 1: SQL queries (spawn_blocking #1)
- `lock_conn()` — reads 4 counters
- `GROUP BY category` aggregation (no LIMIT)
- `GROUP BY topic` aggregation (no LIMIT)
- Drop guard, then `compute_status_aggregates()` → new `lock_conn()`
- `load_active_entries_with_tags()` → new `lock_conn()` (SELECT ALL active entries + tags)
- `load_outcome_entries_with_tags()` → new `lock_conn()`
- **Estimated SQL: 8–12 operations, 3–4 mutex acquisitions**
- **Hold time at current scale: ~50ms. At 500 entries: ~500ms+**

#### Phase 2: Contradiction scan (spawn_blocking #2)
- `lock_conn()` → loads active entries for scanning
- Per active entry: `embed_entry()` + HNSW search (10 neighbors each)
- For N active entries: N HNSW searches + N×potential store reads
- **At current scale (~50 entries): ~5s. At 500 entries: >30s potentially**
- This runs even on the maintenance tick without `check_embeddings=true`

#### Phase 3: Embedding consistency check
- Only runs when `check_embeddings=true` — NOT triggered from the tick (tick passes `false`).
- **Not a current concern for the tick.**

#### Phase 4: Co-access stats (spawn_blocking #3)
- `lock_conn()` → `co_access_stats()` (SELECT COUNT + complex query)
- `lock_conn()` → `top_co_access_pairs()` (SELECT TOP 5)
- N× `store.get()` → N× `lock_conn()` (one per top pair entry, up to 10 gets for 5 pairs)
- **12 mutex acquisitions for 5 top pairs**

#### Phase 5: Coherence dimensions
- Pure computation on already-loaded `active_entries` vector
- `vector_index.point_count()` and `stale_count()` — no Store mutex
- **No mutex acquisitions**

#### Phase 6: Observation stats (spawn_blocking #4)
- `lock_conn()` → SELECT COUNT + GROUP BY sessions
- **.unwrap()** at `status.rs:638` — **panic on JoinError** if blocking thread panics

#### Phase 7: Retrospected feature count (spawn_blocking #5)
- `lock_conn()` → SELECT observation_metrics
- **.unwrap()** at `status.rs:657` — **panic on JoinError** if blocking thread panics

#### Phase 8: Effectiveness analysis (spawn_blocking #6)
- `lock_conn()` → `compute_effectiveness_aggregates()` (multiple JOIN queries over injection_log, sessions, signal_queue)
- `lock_conn()` → `load_entry_classification_meta()` (SELECT entries with status='active')
- **2 mutex acquisitions, complex queries**
- **This is a new addition from crt-018b**

### maintenance_tick() after compute_report()

#### EffectivenessState write lock (in-memory)
- `effectiveness_state.write()` — `RwLock` write lock
- Rebuilds `categories` HashMap (~N entries), `consecutive_bad_cycles` cleanup
- Scans quarantine candidates
- Increments `generation`
- **Lock held for O(N) in-memory operations** — no Store mutex inside

#### Auto-quarantine (per candidate)
- Per candidate: `spawn_blocking` → `store.update_status()` → `lock_conn()` + `BEGIN IMMEDIATE` + UPDATE + UPDATE counters + COMMIT
- N candidates → N separate `spawn_blocking` calls, each awaited sequentially
- Plus `effectiveness_state.write()` per successful quarantine (for counter reset)
- Plus `audit_log.log_event()` per quarantine → `lock_conn()` + `BEGIN IMMEDIATE` + SELECT+UPDATE counter + INSERT + COMMIT
- **Per candidate: 2 mutex acquisitions (quarantine + audit). N candidates = 2N acquisitions**

#### run_maintenance() (`status.rs:752–1086`)

**Step 1: Co-access cleanup (spawn_blocking #7)**
- `lock_conn()` → `cleanup_stale_co_access()` — DELETE WHERE last_updated < cutoff
- **1 mutex acquisition**

**Step 2: Confidence refresh (spawn_blocking #8)**
- Builds list of stale entries (in-memory filter over already-loaded `active_entries`)
- `lock_conn()` inside spawn_blocking → loop over up to 500 entries:
  - Per entry: `store.update_confidence()` → UPDATE entries SET confidence=X WHERE id=Y
  - 200ms wall-clock budget guard
- **1 mutex acquisition held for up to 500 UPDATE statements**

**Step 2b: Empirical prior computation (spawn_blocking #9)**
- `lock_conn()` → SELECT helpful_count/unhelpful_count WHERE status='active' (full scan, no LIMIT)
- `lock_conn()` still held → SELECT confidence WHERE status='active' (second full scan, no LIMIT)
- **1 mutex acquisition held for 2 full table scans**
- After computation: `confidence_state.write()` RwLock for field update

**Step 3: Graph compaction (conditional spawn_blocking #10)**
- Only if `graph_stale_ratio > threshold`
- `embed_entry()` for all active entries (ONNX inference — CPU-bound, no mutex)
- `vector_index.compact()` in spawn_blocking — rebuilds HNSW, no Store mutex
- **0 Store mutex acquisitions if triggered, but significant CPU time**

**Step 4: Observation retention cleanup (spawn_blocking #11)**
- `lock_conn()` → DELETE FROM observations WHERE ts_millis < cutoff
- **1 mutex acquisition**

**Step 5: Stale session sweep**
- `session_registry.sweep_stale_sessions()` — in-memory, no Store mutex
- Per stale session (if any): `spawn_blocking` → `update_session_feature_cycle_pub()` → `lock_conn()`
- `write_signals_to_queue()` → spawns more blocking tasks
- `run_confidence_consumer()` → more mutex acquisitions
- `run_retrospective_consumer()` → more mutex acquisitions
- **Variable: 0 to many mutex acquisitions depending on stale session count**

**Step 6: Session GC (spawn_blocking #12)**
- `lock_conn()` → `gc_sessions()` — UPDATE sessions + DELETE sessions + DELETE injection_log (cascading)
- **1 mutex acquisition, potentially expensive DELETE cascade**

---

## Phase 2: SupersessionState rebuild (`background.rs:347–377`)

- `spawn_blocking` → `SupersessionState::rebuild(&store)` → `store.query_all_entries()`
- `lock_conn()` → SELECT ALL entries (no WHERE) + SELECT all tags (batch)
- **1 mutex acquisition, full table scan + full tag scan**
- Wrapped in 120s timeout

---

## Phase 3: `extraction_tick()` (`background.rs:795–1051`)

**Step 1: Load new observations (spawn_blocking #13)**
- `lock_conn()` → SELECT from observations WHERE id > watermark ORDER BY id ASC LIMIT 10000
- **1 mutex acquisition, up to 10,000 rows**

**Step 2: Run extraction rules (spawn_blocking #14)**
- `run_extraction_rules()` — reads Store internally via `store_for_rules`
- Multiple `lock_conn()` acquisitions inside the extraction rule implementations
- **Variable: depends on observation count and rules**

**Steps 3–3.5: Quality gate + neural enhancement**
- In-memory computation
- Shadow evaluations: `persist_shadow_evaluations()` → `lock_conn()` → N×INSERT
- **0–1 mutex acquisitions**

**Step 4: Quality gate checks 5–6 (spawn_blocking #15)**
- Per proposal: `embed_entry()` (ONNX) + `vs.search()` (HNSW) + potential `check_entry_contradiction()`
- `check_entry_contradiction()` calls into Store for neighbor lookup
- **Variable mutex acquisitions depending on proposals accepted**

**Step 5: Store accepted entries (spawn_blocking per entry)**
- Per accepted entry: `spawn_blocking` → `store.insert()` → `lock_conn()` + full insert transaction
- **N mutex acquisitions (1 per accepted entry)**

---

## Total Tick Cost Summary

| Phase | spawn_blocking calls | Mutex acquisitions | Worst-case hold time |
|-------|---------------------|-------------------|---------------------|
| compute_report() | 6 | ~8–12 | 30–120s at scale |
| Auto-quarantine (0–N) | 0–2N | 0–4N | 0–60s |
| run_maintenance() Steps 1–6 | 6 | ~6+ | 5–30s |
| SupersessionState rebuild | 1 | 1 | 1–10s |
| extraction_tick() | 5+ | 5+ | 5–30s |
| **TOTAL** | **~20–30** | **~20–30+** | **~50–240s** |

**Current tick at scale (current ~50 active entries): estimated 30–90 seconds total**
**At 3-5× scale (150–250 entries): estimated 90–360 seconds total — exceeds all three 120s timeouts**

---

## Error Handling in the Tick

| Location | On Failure | Risk |
|----------|-----------|------|
| `maintenance_tick()` timeout | Logs warn, continues to next phase | Medium — existing EffectivenessState retained |
| `SupersessionState rebuild` timeout | Logs warn, retains old state | Low — search path degrades gracefully |
| `extraction_tick()` timeout | Logs warn, continues to next cycle | Medium — watermark not advanced |
| `run_single_tick()` error | Logs error, continues loop | Low — loop survives individual tick failures |
| `status.rs:638` `.unwrap()` | **PANIC on JoinError** | **Critical — kills compute_report, aborts maintenance tick** |
| `status.rs:657` `.unwrap()` | **PANIC on JoinError** | **Critical — kills compute_report, aborts maintenance tick** |
| `background.rs:457` `.unwrap()` | **PANIC if effectiveness is None after is_some() check** | Low (checked) |

### Key Gap: Tick Loop Survives, but JoinError panics crash compute_report

The tick loop itself (`background.rs:234–263`) catches `run_single_tick()` errors and continues. However, `compute_report()` has two `.unwrap()` calls on `JoinError` from spawn_blocking (phases 6 and 7). If either of those blocking threads panics, the panic propagates out of `compute_report()`, through `maintenance_tick()`, and becomes a `ServiceError` that is logged — but since the panic hook fires first (`main.rs:108`), the panic message is printed to stderr. The tick loop continues, but the global panic hook will have printed a panic to stderr, which may alarm monitoring.

**More critically**: if a blocking thread is cancelled by the tokio timeout (TICK_TIMEOUT), the `.unwrap()` receives `Err(JoinError { ... })` — this **panics the entire spawned task** holding the `compute_report()` future, which is caught by the tick's error handling but causes a partial state update.

Wait — correction on re-reading: line 638 `.unwrap()` is on `spawn_blocking(...).await` — if the blocking thread panics, `await` returns `Err(JoinError)` and `.unwrap()` then panics on the **async task** running `compute_report()`. Since `compute_report()` is called within `maintenance_tick()` which is wrapped in `tokio::time::timeout(TICK_TIMEOUT, maintenance_tick(...))`, this panic propagates up through the timeout and through `run_single_tick()`. The tick loop at line 260 catches the returned `Err` from `run_single_tick()` — but **panics in async tasks are not returned as errors, they abort the task**. This means the entire background tick loop could be killed if either `.unwrap()` fires.

**This is the most critical bug in the codebase.**
