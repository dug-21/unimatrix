# Pseudocode: server.rs Refactor

## Scope

This file covers BOTH coordinated changes to `server.rs` (C-04 joint gate):

1. **UnimatrixServer Clone Verification** (Component 3) — confirm `#[derive(Clone)]` is
   correct for multi-session use; document what must NOT be constructed per-session.

2. **`PendingEntriesAnalysis` Two-Level Refactor** (Component 5) — replace the flat
   `HashMap<u64, EntryAnalysis>` with `HashMap<String, FeatureBucket>` plus the new
   `upsert`, `drain_for`, and `evict_stale` methods.

These are ONE implementation task, reviewed as ONE unit. The implementation agent must not
merge either change without the other (C-04).

---

## Files Affected

- **Modified**: `crates/unimatrix-server/src/server.rs`
- No other files changed by this component (caller updates in `main.rs` are handled in
  the stop/main pseudocode; shutdown changes are in the shutdown pseudocode).

---

## Part 1: UnimatrixServer Clone Model

### Current State (confirmed from source)

`server.rs` line 95: `#[derive(Clone)]` is already present on `UnimatrixServer`.
All fields are `Arc`-wrapped. No structural change to the type itself is needed.

### What Must Not Change

Per ADR-003, `UnimatrixServer::new` currently builds its own internal `ServiceLayer`. This
creates the correct sharing model only because `main.rs` then overwrites
`server.pending_entries_analysis` and `server.session_registry` with the shared Arc
references. This pattern must be preserved exactly.

The `ServiceLayer` constructed inside `UnimatrixServer::new` is the one that gets moved into
`LifecycleHandles`. `main.rs` already does this correctly at lines 321-326 (confidence,
effectiveness, supersession, contradiction handles are extracted from `services` before it is
moved). The implementation agent must verify this still holds after the daemon startup path
is added.

### What Must Not Be Done (ADR-003 consequence)

Never construct a new `ServiceLayer` inside a session task spawn closure:

```
// WRONG — divergent Arc<Store> clone breaks Arc::try_unwrap at shutdown
tokio::spawn(async move {
    let server = UnimatrixServer::new(...); // creates a second ServiceLayer!
    server.serve(transport).await
})

// CORRECT — clone is cheap (Arc refcount increments only)
tokio::spawn(async move {
    server_clone.serve(transport).await  // server_clone = server.clone() from outer scope
})
```

### C-07 Exemption Site

The `CallerId::UdsSession` rate-limit exemption is in server.rs (or delegated through the
tool pipeline). The implementation agent must locate the existing match arm for `UdsSession`
and add the required comment (this is a gate requirement per C-07, R-07):

```rust
// C-07: UDS is filesystem-gated (0600 socket) — rate-limit exemption is
// local-only. When HTTP transport is introduced (W2-2), the HTTP CallerId
// variant MUST NOT inherit this exemption. See W2-2 in PRODUCT-VISION.md.
CallerId::UdsSession => { /* no rate limit */ }
```

### `usage_dedup` per-session reset

`UsageDedup` is `Arc<UsageDedup>` — it is shared across all sessions. Dedup semantics in
daemon mode mean that a store call from Session A deduplicates store calls from Session B
for the same entry within the dedup window. This is the correct behavior (daemon-wide
dedup, not per-session dedup). No change needed. Document this in the implementation with a
comment above the `usage_dedup` field.

---

## Part 2: PendingEntriesAnalysis Refactor

### Current State (from source)

```
pub struct PendingEntriesAnalysis {
    pub entries: HashMap<u64, EntryAnalysis>,   // flat map, keyed by entry_id
    pub created_at: u64,
}
```

Methods: `upsert(analysis)`, `drain_all()`.

### New State (ADR-004, WARN-01 resolution)

```
pub struct PendingEntriesAnalysis {
    /// Outer key: feature_cycle string (e.g., "vnc-005").
    /// Inner key: entry_id u64.
    pub buckets: HashMap<String, FeatureBucket>,
    pub created_at: u64,
}

pub struct FeatureBucket {
    pub entries: HashMap<u64, EntryAnalysis>,
    pub last_updated: u64,  // unix seconds — TTL eviction reference
}
```

The `entries` field is removed. `drain_all()` is replaced by `drain_for(feature_cycle)`.
The existing `upsert(analysis)` signature changes to `upsert(feature_cycle, analysis)`.

---

## Struct Definitions

### `FeatureBucket`

```
pub struct FeatureBucket {
    pub entries: HashMap<u64, EntryAnalysis>,
    pub last_updated: u64,
}

impl FeatureBucket {
    fn new() -> Self {
        FeatureBucket {
            entries: HashMap::new(),
            last_updated: unix_now_secs(),
        }
    }
}
```

### `PendingEntriesAnalysis` (refactored)

```
pub struct PendingEntriesAnalysis {
    pub buckets: HashMap<String, FeatureBucket>,
    pub created_at: u64,
}

impl PendingEntriesAnalysis {
    pub fn new() -> Self {
        PendingEntriesAnalysis {
            buckets: HashMap::new(),
            created_at: unix_now_secs(),
        }
    }
    ...
}
```

---

## Method: `upsert`

### Signature

```
pub fn upsert(&mut self, feature_cycle: &str, analysis: EntryAnalysis)
```

### Pseudocode

```
fn upsert(&mut self, feature_cycle: &str, analysis: EntryAnalysis):

    // Security: validate feature_cycle key length (RISK-TEST-STRATEGY security section)
    if feature_cycle.len() > 256:
        tracing::warn!(
            key_len = feature_cycle.len(),
            "feature_cycle key exceeds 256 bytes; entry dropped"
        )
        return   // Silent drop — not an error return (caller is fire-and-forget)

    // Get or create bucket for this feature_cycle
    bucket = self.buckets.entry(feature_cycle.to_string())
        .or_insert_with(FeatureBucket::new)

    // Upsert within the bucket (merge semantics, same as pre-vnc-005 behavior)
    if let Some(existing) = bucket.entries.get_mut(&analysis.entry_id):
        existing.rework_flag_count += analysis.rework_flag_count
        existing.rework_session_count += analysis.rework_session_count
        existing.success_session_count += analysis.success_session_count
    else:
        // Enforce per-bucket 1000-entry cap (ADR-004)
        if bucket.entries.len() >= 1000:
            // Evict entry with lowest rework_flag_count
            min_key = bucket.entries
                .iter()
                .min_by_key(|(_, v)| v.rework_flag_count)
                .map(|(k, _)| *k)
            if let Some(k) = min_key:
                bucket.entries.remove(&k)

        bucket.entries.insert(analysis.entry_id, analysis)

    // Update last_updated timestamp for TTL eviction
    bucket.last_updated = unix_now_secs()
```

### Notes

- The cap eviction (1000 entries per bucket) is enforced inside the Mutex lock (the caller
  holds `Arc<Mutex<PendingEntriesAnalysis>>`). R-15: eviction only runs while the lock is held.
- `feature_cycle` key length cap of 256 bytes prevents memory DoS (RISK-TEST-STRATEGY
  security section). Silent drop is intentional — the fire-and-forget caller cannot handle
  return values.

---

## Method: `drain_for`

### Signature

```
pub fn drain_for(&mut self, feature_cycle: &str) -> Vec<EntryAnalysis>
```

### Pseudocode

```
fn drain_for(&mut self, feature_cycle: &str) -> Vec<EntryAnalysis>:

    // Remove the entire bucket for this feature_cycle.
    // Returns all EntryAnalysis values; the bucket is gone after this call.
    // Subsequent upsert() calls for the same feature_cycle will create a fresh bucket.
    bucket = self.buckets.remove(feature_cycle)

    match bucket:
        None => Vec::new()   // bucket never existed or already drained
        Some(b) => b.entries.into_values().collect()
```

### Notes

- The drain is atomic with respect to the Mutex: the caller must hold the lock for the
  entire drain operation (R-18: no concurrent eviction during drain). Since this is a
  `&mut self` method, the lock must already be held by the caller.
- After drain, a subsequent `context_retrospective` call for the same `feature_cycle`
  returns an empty result (AC-18).
- Drain-then-respond has no rollback (RISK-TEST-STRATEGY integration risk note). This
  is an accepted trade-off: if the MCP handler panics after drain, the bucket is lost.
  Document this in the handler's comments.

---

## Method: `evict_stale`

### Signature

```
pub fn evict_stale(&mut self, now_unix_secs: u64, ttl_secs: u64)
```

### Pseudocode

```
fn evict_stale(&mut self, now_unix_secs: u64, ttl_secs: u64):

    // Identify stale buckets: not touched within ttl_secs
    let mut to_evict: Vec<String> = Vec::new()

    for (feature_cycle, bucket) in &self.buckets:
        age = now_unix_secs.saturating_sub(bucket.last_updated)
        if age > ttl_secs:
            to_evict.push(feature_cycle.clone())

    // Remove stale buckets (R-18: eviction happens entirely within the Mutex lock)
    for key in &to_evict:
        if let Some(bucket) = self.buckets.remove(key):
            tracing::warn!(
                feature_cycle = %key,
                entry_count = bucket.entries.len(),
                age_hours = age / 3600,
                "evicting stale pending_entries_analysis bucket (TTL exceeded)"
            )
```

### Caller in `background.rs`

The background tick calls `evict_stale` during its maintenance pass:

```
// In background tick (background.rs), within the existing maintenance tick body:
{
    let now = unix_now_secs()
    let ttl_secs = 72 * 3600  // 72 hours per ADR-004
    let mut analysis = pending_entries_analysis.lock()
        .unwrap_or_else(|e| e.into_inner())  // poison recovery pattern
    analysis.evict_stale(now, ttl_secs)
}
```

The tick already has access to `Arc<Mutex<PendingEntriesAnalysis>>` (it is already a
parameter to `spawn_background_tick` in `main.rs` line 342).

---

## Callers That Must Be Updated

### 1. Hook UDS listener (`uds/listener.rs`)

The existing `upsert(analysis)` call must change to `upsert(feature_cycle, analysis)`.
The listener has access to the active `feature_cycle` via `SessionRegistry` (the session's
`feature_cycle` field set by `context_cycle`). Extract it before calling upsert:

```
// In the UDS listener's SessionStop handler (or wherever upsert is called):
let feature_cycle = session_registry
    .get_feature_cycle(session_id)
    .unwrap_or_default()  // empty string for sessions without a feature cycle

pending_entries_analysis
    .lock().unwrap_or_else(|e| e.into_inner())
    .upsert(&feature_cycle, analysis)
```

An empty feature_cycle string is technically valid (edge case noted in RISK-TEST-STRATEGY).
The implementation may choose to silently skip upsert when `feature_cycle` is empty.

### 2. `context_retrospective` handler (in `server.rs` tool handler)

Replace `drain_all()` with `drain_for(topic)`:

```
// topic parameter from the MCP tool request
let entries = pending_entries_analysis
    .lock().unwrap_or_else(|e| e.into_inner())
    .drain_for(&topic)
```

### 3. `context_cycle` handler

When a cycle is closed, drain the bucket to finalize it:

```
// After recording the cycle in the store:
pending_entries_analysis
    .lock().unwrap_or_else(|e| e.into_inner())
    .drain_for(&feature_cycle)
// Drained entries are discarded — cycle close implies retrospective was already done
// or explicitly skipped. Log if entries were present:
if !drained.is_empty():
    tracing::info!(
        feature_cycle = %feature_cycle,
        entry_count = drained.len(),
        "context_cycle: cleared pending_entries_analysis bucket on cycle close"
    )
```

---

## Key Test Scenarios

### UnimatrixServer Clone

1. **Clone is cheap** — call `server.clone()` 32 times; assert each clone is created in
   under 1ms (Arc increment benchmark).

2. **Single ServiceLayer** — after constructing `UnimatrixServer::new` and overriding
   `pending_entries_analysis` + `session_registry`, assert that `Arc::strong_count(&store)`
   matches the expected count (not doubled by a second ServiceLayer).

3. **C-07 comment present** — static assertion (grep): confirm the `CallerId::UdsSession`
   match arm contains the string "C-07" and "W2-2" in a code comment.

### PendingEntriesAnalysis

4. **`upsert` creates bucket and inserts** — call `upsert("fc-001", analysis_a)`; assert
   `buckets["fc-001"].entries[analysis_a.entry_id]` exists.

5. **`upsert` merges into existing entry** — insert `analysis_a`; then insert another
   `analysis_a` with different counts; assert counts are summed.

6. **`upsert` enforces 1000-entry cap per bucket** (R-15) — insert 1000 entries with
   low `rework_flag_count`; insert entry 1001 with high count; assert bucket size is 1000
   and the low-count entry was evicted.

7. **`drain_for` returns all entries and empties bucket** — upsert 3 entries under "fc-a";
   call `drain_for("fc-a")`; assert returns 3; call again; assert returns 0 (AC-17, AC-18).

8. **`drain_for` returns empty for unknown key** — call `drain_for("nonexistent")`; assert
   returns empty Vec; assert no panic.

9. **`evict_stale` removes buckets older than TTL** — insert bucket with `last_updated`
   set to `now - 73h`; call `evict_stale(now, 72h)`; assert bucket removed; assert warn log.

10. **`evict_stale` does not remove fresh buckets** — insert bucket with `last_updated = now`;
    call `evict_stale(now, 72h)`; assert bucket still present.

11. **Feature cycle key > 256 bytes is silently dropped** — call `upsert` with a 257-byte
    key; assert `buckets` is still empty; assert no panic.

12. **Multi-session accumulation** (AC-17) — simulate two concurrent sessions both calling
    `upsert` for the same `feature_cycle`; call `drain_for`; assert all entries present.

13. **Concurrent upsert + drain safety** (R-05) — from multiple threads (via
    `std::thread::spawn` + `Arc<Mutex<PendingEntriesAnalysis>>`), simultaneously call
    `upsert` 100 times per thread and `drain_for` from a fifth thread; assert no panic and
    total entry count across all drains is bounded by total upserts.
