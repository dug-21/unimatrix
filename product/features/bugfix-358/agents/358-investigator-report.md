# Bug Investigation Report: 358-investigator

## Bug Summary

Background contradiction scan panics on every tick because `read_active_entries` calls
`tokio::runtime::Handle::current().block_on(...)` from inside a rayon worker thread, which
has no Tokio runtime. The panic is silently swallowed by rayon's pool safety mechanism,
so contradiction detection has been completely non-functional since this code path was
introduced — the only signal is the ERROR log line.

## Root Cause Analysis

### Code Path Trace

**Panic site 1 (primary — reported in issue):**
```
background.rs:583  ml_inference_pool.spawn(move || { ... })
  → contradiction.rs:160  read_active_entries(store)
    → contradiction.rs:254  tokio::runtime::Handle::current()  ← PANIC
```

**Panic site 2 (scan_contradictions inner loop):**
```
background.rs:583  ml_inference_pool.spawn(move || { ... })
  → contradiction.rs:160  scan_contradictions → loop
    → contradiction.rs:202  tokio::runtime::Handle::current().block_on(store.get(...))  ← PANIC (if site 1 didn't already kill the task)
```

**Panic site 3 (check_entry_contradiction):**
```
background.rs:1585  ml_inference_pool.spawn(move || { ... })  [quality gate]
  → contradiction.rs:1613  check_entry_contradiction(...)
    → contradiction.rs:111  tokio::runtime::Handle::current().block_on(store.get(...))  ← PANIC
```

**Panic site 4 (check_embedding_consistency):**
```
status.rs:562  rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, move || { ... })
  → contradiction.rs:270  check_embedding_consistency → read_active_entries(store)
    → contradiction.rs:254  tokio::runtime::Handle::current()  ← PANIC
```

### Why It Fails

Rayon's thread pool spawns OS threads that have no Tokio runtime associated with them.
`tokio::runtime::Handle::current()` panics (does not return an error) when called from
a thread with no runtime. Rayon's pool safety mechanism catches the panic, drops the
oneshot sender, and the awaiting async side receives `RayonError::Cancelled`.

The doc comment on `read_active_entries` claims it is "Called from `spawn_blocking`
closures where the tokio handle is available." This is incorrect — it is called from
rayon workers (ml_inference_pool and rayon_pool), not from `spawn_blocking`, and rayon
workers have no runtime.

Unimatrix entry #2126 confirms: `Handle::current().block_on` requires an existing
runtime on the calling thread. Neither `block_on` nor `block_in_place` can be used
from a rayon worker — the store reads must be hoisted entirely outside the rayon closure.

## Affected Files and Functions

| File | Function | Line | Role in Bug |
|------|----------|------|-------------|
| `crates/unimatrix-server/src/infra/contradiction.rs` | `read_active_entries` | 253–257 | Panics: Handle::current() in rayon thread |
| `crates/unimatrix-server/src/infra/contradiction.rs` | `scan_contradictions` | 154, 202 | Calls read_active_entries; also has second block_on at 202 |
| `crates/unimatrix-server/src/infra/contradiction.rs` | `check_embedding_consistency` | 264, 270 | Calls read_active_entries |
| `crates/unimatrix-server/src/infra/contradiction.rs` | `check_entry_contradiction` | 87, 111 | block_on for individual store.get() in rayon context |
| `crates/unimatrix-server/src/background.rs` | `run_single_tick` (contradiction dispatch) | ~583–591 | Spawns scan_contradictions into rayon without pre-fetching |
| `crates/unimatrix-server/src/background.rs` | quality gate dispatch | ~1585–1622 | Spawns check_entry_contradiction into rayon without pre-fetching |
| `crates/unimatrix-server/src/services/status.rs` | `StatusService::build_report` | ~562–583 | Spawns check_embedding_consistency into rayon without pre-fetching |

## Proposed Fix Approach

### Core principle
All store I/O must happen in the async Tokio context **before** entering the rayon
closure. The rayon closure receives pre-fetched `Vec<EntryRecord>` and does only
CPU-bound work (embedding, HNSW search, heuristics).

### Changes required

**1. `contradiction.rs` — `scan_contradictions` signature change**

Remove `store: &Store` parameter. Add `active_entries: Vec<EntryRecord>` parameter.
Remove the `read_active_entries(store)?` call at line 160.
Remove the `block_on(store.get(neighbor.entry_id))` at line 202 — replace with a
lookup in the pre-fetched `active_entries` vec: build a `HashMap<u64, &EntryRecord>`
from `active_entries` before the loop, then do `active_entries_map.get(&neighbor.entry_id)`.
This is valid because the `if neighbor_entry.status != Status::Active` filter at line 208
is already satisfied: all entries in `active_entries` are Active.

New signature:
```rust
pub fn scan_contradictions(
    active_entries: Vec<EntryRecord>,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Vec<ContradictionPair>, ServerError>
```

**2. `contradiction.rs` — `check_embedding_consistency` signature change**

Same pattern: remove `store: &Store`, add `active_entries: Vec<EntryRecord>`.
Remove the `read_active_entries(store)?` call at line 270.

New signature:
```rust
pub fn check_embedding_consistency(
    active_entries: Vec<EntryRecord>,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Vec<EmbeddingInconsistency>, ServerError>
```

**3. `contradiction.rs` — `check_entry_contradiction` signature change**

Remove `store: &Store` parameter. Add `active_entries: &[EntryRecord]` parameter
(slice, since the quality-gate context may pass a subset or full set).
Replace `block_on(store.get(neighbor.entry_id))` at line 111 with a lookup in
`active_entries` via `.iter().find(|e| e.id == neighbor.entry_id)`.

New signature:
```rust
pub fn check_entry_contradiction(
    content: &str,
    title: &str,
    active_entries: &[EntryRecord],
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Option<ContradictionPair>, ServerError>
```

**4. `contradiction.rs` — remove `read_active_entries`**

The function is no longer needed. Remove it entirely (and the `Store` import if it
becomes unused).

**5. `background.rs` — contradiction scan dispatch (line ~574)**

Before the `ml_inference_pool.spawn(...)` call, add an async store fetch:
```rust
let active_entries = store.query_by_status(Status::Active).await
    .map_err(|e| ServerError::Core(unimatrix_core::CoreError::Store(e)))?;
```
Pass `active_entries` into the closure instead of `store_for_scan`.

**6. `background.rs` — quality gate dispatch (line ~1585)**

Before the `ml_inference_pool.spawn(...)` call, fetch active entries async and pass
`active_entries` into `check_entry_contradiction`.

**7. `status.rs` — embedding consistency dispatch (line ~557)**

Before the `rayon_pool.spawn_with_timeout(...)` call, fetch active entries async and
pass them into `check_embedding_consistency`.

### Why this fix

- Keeps all I/O in the async Tokio context where it belongs
- Rayon workers become purely CPU-bound (embedding, HNSW, heuristics) — correct
- No new dependencies introduced
- Pre-fetching active entries for scan_contradictions is already what the old code intended (via read_active_entries) — we just move it earlier
- For scan_contradictions neighbor lookup: since we already have all active entries, a HashMap lookup replaces the store.get() with zero additional I/O
- For check_entry_contradiction neighbor lookup: same HashMap/slice approach

## Risk Assessment

- **Blast radius**: Three public functions change signature — all three call sites are
  in the same crate (background.rs ×2, status.rs ×1). No external callers.
- **Regression risk**: Low. The functions currently always panic in rayon context, so
  any working state is an improvement. The logic inside the functions is unchanged.
- **HashMap lookup vs store.get()**: The active_entries pre-fetch captures a snapshot.
  An entry written between the pre-fetch and the HNSW search might appear as a neighbor
  but not be in the map. This is acceptable: `store.get` miss already results in `continue`
  (graceful degradation), and the map miss will do the same.
- **Confidence**: High. Root cause is deterministic and fully traced. The panic message,
  line number (contradiction.rs:254), and code all agree.

## Missing Test

**What should have caught this**: An integration test that invokes the contradiction
scan from a rayon thread pool (simulating the background tick). The test should:
1. Create a `RayonPool` (not `spawn_blocking`)
2. Call `scan_contradictions` inside `pool.spawn(...)`
3. Assert the result is `Ok(...)` — not a rayon `Cancelled` error

Currently, all tests of contradiction functions are pure unit tests of heuristic logic
(`#[test]` against static strings). None test `scan_contradictions` or
`check_embedding_consistency` with actual store/vector access.

A simpler unit test that also would have caught this:
```rust
#[test]
fn scan_contradictions_does_not_require_tokio_runtime() {
    // If called outside a tokio runtime, must not panic.
    // Pass empty active_entries and a mock vector_store.
    let result = scan_contradictions(vec![], &MockVectorStore, &MockEmbed, &ContradictionConfig::default());
    assert!(result.is_ok());
}
```

## Reproduction Scenario

Deterministic. Occurs on every contradiction scan tick (every
`CONTRADICTION_SCAN_INTERVAL_TICKS` ticks, starting at tick 0). The error log
`contradiction scan rayon task cancelled; cache retained` appears on every affected tick.

## Knowledge Stewardship

- Queried: context_search for "rayon thread pool tokio runtime panic block_on background worker" — found #771 (blocking in tokio context), #2126 (block_in_place vs block_on), #1366 (tick error recovery)
- Queried: context_search for "contradiction scan active entries store pre-fetch async sync boundary" — found #61 (ADR-004 spawn_blocking delegation), #2062 (ADR-005 native async trait)
- Stored: will be stored by bugfix leader post-merge via /uni-store-lesson — the generalizable lesson is: "rayon worker threads have no Tokio runtime; Handle::current().block_on() panics; all async store I/O must be hoisted before rayon::spawn()"
