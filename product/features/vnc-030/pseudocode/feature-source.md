# session.rs — FeatureSource Precedence + apply_stamp + Sweep Flip

**Source**: `crates/unimatrix-server/src/infra/session.rs` (extend). **ADR**:
ADR-004. **Constraints**: C-09 registry 4-touchpoint fence (nothing beyond the
enum field, the source assignments at existing set sites, `apply_stamp`, and the
two `or_else` flips), C-10 minimal-diff inversion fixes (crt-052 rebases over
this — keep the close/sweep diff one guard + one short-circuit).

## Purpose

Give the registry a precedence CLASS (`FeatureSource`) so a declared feature
(cycle_start / stamp) can no longer be vote-inverted, add the idempotent
`apply_stamp` that re-establishes `Declared` after server-state loss, and flip the
sweep inversion at `session.rs:628`.

## New types (near SessionState, ~session.rs:110)

```rust
/// Precedence class for a session's resolved feature (ADR-004). The TWO variants
/// are the precedence classes; all precedence checks are matches!(src, Declared).
#[derive(Clone, Debug, PartialEq)]
pub enum FeatureSource {
    Declared,                  // cycle_start (set_feature_force) or stamp (apply_stamp)
    Inferred(InferredOrigin),  // never beats a declared feature; sub-origin for topic_source only
}

/// Sub-origin of an Inferred feature. Exists SOLELY so topic_source can split
/// registry-fill from vote-derived fill (ADR-005 / SR-04). Never affects precedence.
#[derive(Clone, Debug, PartialEq)]
pub enum InferredOrigin {
    Registered,   // register_session feature param (SessionStart) → topic_source 'registry-fill'
    Voted,        // set_feature_if_absent eager (#198) → topic_source 'vote'
}
```

## SessionState field (session.rs:117-162)

Add after `feature: Option<String>` (so the source travels with the feature):
```rust
    pub feature_source: FeatureSource,   // default Inferred(Registered)
```
Every `SessionState { ... }` literal must initialize it. Production construction
site: `register_session` (:201). Test construction sites at :1395-ish and
:1888-ish also need the field — grep `SessionState {` and add
`feature_source: FeatureSource::Inferred(InferredOrigin::Registered)` to each.

## Source assignment — exactly these existing set sites (C-09 fence)

### `register_session` (:193) — feature param → `Inferred(Registered)`

The inserted `SessionState` literal sets:
```rust
    feature_source: FeatureSource::Inferred(InferredOrigin::Registered),
```
(Default regardless of whether `feature` is Some/None — registration is the
lowest-precedence origin. Resume/compact overwrite resets to this; the next
stamped event restores Declared via apply_stamp — accepted consequence, R-13.)

### `set_feature_force` (:415, cycle_start) → `Declared`

In each arm that writes `state.feature`, also set the source:
```rust
    Some(state) => match &state.feature {
        None => {
            state.feature = Some(feature.to_string());
            state.feature_source = FeatureSource::Declared;   // ADD
            SetFeatureResult::Set
        }
        Some(existing) if existing == feature => {
            state.feature_source = FeatureSource::Declared;   // ADD: idempotent re-affirm; no log
            SetFeatureResult::AlreadyMatches
        }
        Some(existing) => {
            let previous = existing.clone();
            state.feature = Some(feature.to_string());
            state.feature_source = FeatureSource::Declared;   // ADD
            SetFeatureResult::Overridden { previous }
        }
    },
```
The `None` (session absent) arm stays a no-op (no state to set).

### `set_feature_if_absent` (:399, eager #198) → `Inferred(Voted)`

```rust
    if state.feature.is_none() {
        state.feature = Some(feature.to_string());
        state.feature_source = FeatureSource::Inferred(InferredOrigin::Voted);   // ADD
        return true;
    }
```
(Used by eager attribution AND the #198 payload-feature_cycle path. Both are
heuristic/vote-class fills — `Voted` is correct for both per ADR-005: they fill
only when feature was absent, i.e. never-declared sessions.)

## NEW `apply_stamp(&self, session_id, topic)` — idempotent Declared set

Called from the listener record paths when `cycle_stamp` is present
(listener-stamp-read.md). Covers server restart mid-session (no post-restart
cycle_start). Idempotent: NO-OP when feature+source already match (no `Overridden`
log noise per stamped event — R-17).
```rust
pub fn apply_stamp(&self, session_id: &str, topic: &str) {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = sessions.get_mut(session_id) {
        let already = state.feature.as_deref() == Some(topic)
            && matches!(state.feature_source, FeatureSource::Declared);
        if already {
            return;   // idempotent no-op: no churn, no log
        }
        // last-writer-wins; this is a genuine (re)declaration via stamp
        if state.feature.as_deref() != Some(topic) {
            tracing::info!(session_id, topic, "apply_stamp: feature set/overridden by cycle_stamp");
        }
        state.feature = Some(topic.to_string());
        state.feature_source = FeatureSource::Declared;
    }
    // session absent → no-op (the record path operates on observation rows
    // regardless; the row still gets topic_source='declared' from the stamp).
}
```
Note: unlike `set_feature_force`, `apply_stamp` does NOT pre-register an evicted
session — it is a best-effort registry affirmation. The row attribution comes from
the stamp directly (listener), so apply_stamp's no-op on an absent session does
not lose row-level attribution.

## Inversion Flip 1 — sweep (`session.rs:627-628`)

Current:
```rust
let resolved_feature =
    majority_vote_internal(&state.topic_signals).or_else(|| state.feature.clone());
```
Replace with the minimal guard (ADR-004 §5, FR-16):
```rust
let resolved_feature = if matches!(state.feature_source, FeatureSource::Declared)
    && state.feature.is_some()
{
    state.feature.clone()                                   // declared wins (inversion fixed)
} else {
    majority_vote_internal(&state.topic_signals).or_else(|| state.feature.clone())  // unchanged path
};
```
This is the ONLY change in `sweep_stale_sessions`. `SweepResult.resolved_feature`
keeps its type/shape (crt-052 cites it, ADR-007 §2). `drain_and_signal_session`,
`build_signal_output_from_state`, the purge logic — all UNTOUCHED.

## Citable Interface (crt-052, ADR-007 §2)

After this change: `sweep_stale_sessions` returns
`SweepResult.resolved_feature = state.feature` whenever
`feature_source == Declared && feature.is_some()`; majority vote applies only
otherwise. A declared session's feature can no longer be vote-flipped at sweep.
The close-path mirror lives in listener-stamp-read.md (Inversion Flip 2).

## Initialization Sequence / State

`SessionState.feature_source` defaults to `Inferred(Registered)` on every
`register_session`. It transitions: `Declared` on cycle_start (`set_feature_force`)
or stamp (`apply_stamp`); `Inferred(Voted)` on eager fill; reset to
`Inferred(Registered)` on resume/compact re-register (accepted, then restored by
the next stamp). Never persisted — in-memory registry state only (matches the
existing `feature` field's lifecycle).

## Error Handling

All mutations under the existing `self.sessions.lock().unwrap_or_else(|e|
e.into_inner())` poison-recovery pattern. `apply_stamp` adds one mutex acquisition
per stamped record (microseconds, same class as `record_topic_signal`) — and
no-ops when nothing changed, so steady-state stamped traffic adds no churn.

## Key Test Scenarios

- `apply_stamp` on a fresh session sets feature+Declared; second identical
  apply_stamp → no-op (no log, no churn) — R-17 idempotency.
- `apply_stamp` with a contradicting topic → last-writer-wins, logged once.
- `apply_stamp` on an ABSENT session → no panic, no-op (row attribution still
  comes from the stamp at the listener).
- sweep: declared session + contradicting majority vote → resolves to the DECLARED
  feature (FR-16); Inferred session → majority vote as today.
- `set_feature_force` sets Declared; `set_feature_if_absent` sets Inferred(Voted);
  `register_session` sets Inferred(Registered).
- Re-register resets to Inferred(Registered); a following stamped event restores
  Declared (R-13 accepted-consequence boundary).
- Minimal-diff review: sweep change is one guard around the existing `or_else`;
  zero changes to drain/clear/transcript (R-18, C-10).

## Open Questions / Gaps

- **OQ-A (vote row write site)** is resolved at the SERVER decision-tree level in
  listener-stamp-read.md: `Inferred(Voted)` is set HERE (set_feature_if_absent),
  and the `'vote'` topic_source value is written at the enrich site when an
  unstamped event with no extraction fills from a `Voted` registry feature. No
  separate row-level vote write exists on this (session.rs) side — confirmed: the
  session-level majority vote at close/sweep resolves `sessions.feature_cycle`,
  not observation rows. See listener-stamp-read.md OQ-A note.
