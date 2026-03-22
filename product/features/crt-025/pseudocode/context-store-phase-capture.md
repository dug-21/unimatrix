# Component 8: Context Store Phase Capture
## Files: `crates/unimatrix-server/src/server.rs`, `crates/unimatrix-server/src/services/usage.rs`

---

## Purpose

At `context_store` call time, snapshot `SessionState.current_phase` into a local variable before any async dispatch. Pass this snapshot to both write paths:

1. **Direct write path**: `record_feature_entries(feature_cycle, ids, phase.as_deref())`
2. **Analytics drain path**: `AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase }` — phase baked into the event at enqueue time, not read from live state at flush time.

This ensures phase correctness even if `current_phase` advances (due to a subsequent `phase-end`) before the analytics drain fires (ADR-001, R-02, NFR-03).

---

## 8a: `services/usage.rs` — `UsageContext` struct

### Modified Struct

```
// BEFORE:
pub(crate) struct UsageContext {
    pub session_id:    Option<String>,
    pub agent_id:      Option<String>,
    pub helpful:       Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level:   Option<TrustLevel>,
    pub access_weight: u32,
}

// AFTER:
pub(crate) struct UsageContext {
    pub session_id:    Option<String>,
    pub agent_id:      Option<String>,
    pub helpful:       Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level:   Option<TrustLevel>,
    pub access_weight: u32,
    pub current_phase: Option<String>,   // NEW: snapshot from SessionState at call time
}
```

All existing `UsageContext { ... }` construction sites must be updated to include `current_phase: None` (default for non-store operations) or the actual snapshotted phase for `context_store` paths.

---

## 8b: `services/usage.rs` — `record_mcp_usage` phase propagation

The `record_mcp_usage` function already builds `feature_recording` inside a `tokio::spawn`. The phase must flow through to the `record_feature_entries` call and the `AnalyticsWrite::FeatureEntry` enqueue.

### Current call site in `record_mcp_usage`

```
// Current (inside tokio::spawn):
IF let Some((feature_str, ids)) = feature_recording:
    IF let Err(e) = store.record_feature_entries(&feature_str, &ids).await:
        warn!(...)
```

### Updated call site

```
// Phase captured BEFORE the spawn (from ctx, which was populated at call time)
let phase_snapshot = ctx.current_phase.clone()    // Option<String>; captured here

// Inside tokio::spawn:
IF let Some((feature_str, ids)) = feature_recording:
    IF let Err(e) = store.record_feature_entries(
        &feature_str,
        &ids,
        phase_snapshot.as_deref(),    // NEW: phase passed through
    ).await:
        warn!(...)
```

### Direct write: no AnalyticsWrite::FeatureEntry in record_mcp_usage?

Looking at the current code: `record_mcp_usage` calls `store.record_feature_entries` directly (not via analytics drain). The analytics drain path (`AnalyticsWrite::FeatureEntry`) is used in a different context. See the note below about where `FeatureEntry` analytics events are enqueued.

---

## 8c: `services/usage.rs` — `record_hook_injection` phase propagation

The hook injection path also calls `record_feature_entries`:

```
// Current (inside tokio::spawn):
IF let Some((feature_str, ids)) = feature_recording:
    IF let Err(e) = s.record_feature_entries(&feature_str, &ids).await:
        warn!(...)

// Updated:
let phase_snapshot = ctx.current_phase.clone()
// Inside tokio::spawn:
IF let Some((feature_str, ids)) = feature_recording:
    IF let Err(e) = s.record_feature_entries(
        &feature_str,
        &ids,
        phase_snapshot.as_deref(),
    ).await:
        warn!(...)
```

---

## 8d: Where `AnalyticsWrite::FeatureEntry` is enqueued

Search in usage.rs reveals `AnalyticsWrite::FeatureEntry` is not currently used in usage.rs — the direct `record_feature_entries` path is used instead. The `FeatureEntry` drain variant exists in `analytics.rs` but may be enqueued from a different location (or is reserved for future use).

Regardless, the drain handler in `analytics.rs` must be updated (component 6a) to include `phase` in the INSERT. When any call site enqueues `AnalyticsWrite::FeatureEntry`, it must include the `phase` field.

If there are call sites that enqueue `AnalyticsWrite::FeatureEntry` (search the codebase for `AnalyticsWrite::FeatureEntry {`), each must be updated to pass `phase: ctx.current_phase.clone()` captured before any `spawn_blocking` or async dispatch.

---

## 8e: `server.rs` — context_store handler phase snapshot

The `context_store` handler in `server.rs` builds a `UsageContext` and calls `services.usage.record_access`. The session state is read for other purposes (feature_cycle, etc.) at the top of the handler. The phase snapshot must happen at the same point.

### Pseudocode for phase snapshot in context_store handler

```
FUNCTION context_store(params: StoreParams):

    // Existing: resolve identity, validate params, embed content...

    // Read session state (existing — for feature_cycle and other session metadata)
    session_state = session_registry.get_state(&ctx.session_id)

    // Snapshot phase AT THIS POINT (before any async dispatch)
    // Same location where session_state is already read for feature_cycle
    current_phase_snapshot: Option<String> =
        session_state.as_ref().and_then(|s| s.current_phase.clone())

    // ... continue with store write, confidence, etc. (all existing) ...

    // Build UsageContext (modified: add current_phase)
    usage_ctx = UsageContext {
        session_id:    Some(ctx.session_id.clone()),
        agent_id:      Some(ctx.agent_id.clone()),
        helpful:       None,
        feature_cycle: session_state.and_then(|s| s.feature.clone()),
        trust_level:   Some(ctx.trust_level),
        access_weight: 1,
        current_phase: current_phase_snapshot,   // NEW: snapshotted above
    }

    // Record access (passes phase through to record_feature_entries)
    self.services.usage.record_access(&[entry_id], AccessSource::McpTool, usage_ctx)
```

The key constraint is: `current_phase_snapshot` must be set from `session_state` **before** any `await` or `tokio::spawn` that might interleave with a concurrent `phase-end` event. Since `get_state` returns a clone, the snapshot is isolated from further mutations.

---

## Phase Snapshot Timing Diagram

```
T0: context_store handler begins
T1: session_registry.get_state("s1") → clone of SessionState (current_phase = Some("impl"))
T2: current_phase_snapshot = Some("impl")   ← SNAPSHOT HERE
T3: ... async store write (embed, insert) ...
T4: cycle_phase_end arrives → set_current_phase("s1", Some("testing"))
T5: record_feature_entries("crt-025", [N], Some("impl"))
    → feature_entries.phase = "impl"   ← CORRECT (snapshot, not live value)
```

---

## All `UsageContext` Construction Sites

All existing `UsageContext { ... }` sites throughout `server.rs` and `usage.rs` must add `current_phase: None` (for non-store operations like search, lookup, get, correct, deprecate). Only the `context_store` path sets a non-None value.

---

## Error Handling

No new error paths. Phase snapshot failure is not possible — `get_state` returns `Option<SessionState>` and `and_then` returns `None` gracefully if session not found.

---

## Key Test Scenarios

1. `context_store` during active phase → `feature_entries.phase = active_phase` (direct write path)
2. `context_store` before any phase signal → `feature_entries.phase IS NULL`
3. Phase advances between `context_store` call and drain flush → `feature_entries.phase` = phase at call time (not flush time) — verifies R-02
4. Hook injection during active phase → `feature_entries.phase = active_phase` (hook path)
5. All existing `UsageContext` construction sites compile with `current_phase: None` added
6. `context_store` on session with no registration → phase snapshot = None → no crash
