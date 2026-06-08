# listener.rs — Stamp Read (3 sites) + topic_source + Close Flip + Enrich Guard

**Source**: `crates/unimatrix-server/src/uds/listener.rs` (extend). **ADR**:
ADR-004 (precedence), ADR-005 (topic_source). **Constraints**: C-09 fence, C-10
minimal-diff close flip, ADR-003 mandate: ONE shared helper across all three
record sites (the #3486 anti-drift requirement, R-01).

## Purpose

Read `event.cycle_stamp` at all three observation record sites, attribute the row
from the stamp when present, assign `topic_source` per ADR-005's one-value-per-
write-site taxonomy, add the FeatureSource guard to `enrich_topic_signal`, flip
the close-path inversion, extend `ObservationRow` + both local INSERTs with
`topic_source` (`?10`).

## ObservationRow + extract_observation_fields (:2893, :2909)

Add the column to the struct (after `phase`):
```rust
    /// Per-row attribution provenance (ADR-005). 'declared'|'extracted'|
    /// 'registry-fill'|'vote'|NULL. Set at insert time only; never updated.
    topic_source: Option<String>,
```
`extract_observation_fields` initializes it `None` (like `phase`); the record-path
helper sets the real value before insert:
```rust
    ObservationRow { ..., phase: None, topic_source: None }   // at :2992-3002
```

## Shared helper — `apply_stamp_to_row` (NEW; ADR-003 mandate)

One function called by ALL THREE record sites. Collapses the 3-site drift risk
(R-01). Sets topic_signal, phase, topic_source, and touches the registry, OR
delegates to the enrich decision tree when no stamp.

```rust
/// Attribute one observation row from the event (ADR-004 §4). Either:
///  - stamp present: row = declared, registry.apply_stamp, SKIP tally + enrich; OR
///  - CYCLE_* event: row = declared (topic_signal already IS the declaration); OR
///  - else: enrich-with-source decision tree.
/// Returns (topic_signal, topic_source) and mutates obs.phase as needed.
fn apply_stamp_to_row(
    obs: &mut ObservationRow,
    event: &ImplantEvent,
    session_registry: &SessionRegistry,
) {
    if let Some(stamp) = &event.cycle_stamp {
        // STAMP path — contractual.
        obs.topic_signal = Some(stamp.topic.clone());
        obs.phase = stamp.phase.clone().or_else(|| {
            session_registry.get_state(&event.session_id)
                .and_then(|s| s.current_phase.clone())     // stamp.phase ?? registry phase
        });
        obs.topic_source = Some("declared".to_string());
        session_registry.apply_stamp(&event.session_id, &stamp.topic);   // idempotent Declared
        // SKIP record_topic_signal tally (handled at call site — see below)
        // SKIP enrich_topic_signal
        return;
    }
    if is_cycle_event(&event.event_type) {
        // CYCLE_* event (any client): topic_signal already = the declaration.
        obs.topic_source = Some("declared".to_string());
        // phase: keep existing registry-phase capture (call site sets obs.phase)
        return;
    }
    // Heuristic path — enrich WITH source (replaces enrich_topic_signal at this site).
    let (signal, source) =
        enrich_topic_signal_with_source(event.topic_signal.clone(), &event.session_id, session_registry);
    obs.topic_signal = signal;
    obs.topic_source = source;
}

fn is_cycle_event(t: &str) -> bool {
    t == CYCLE_START_EVENT || t == CYCLE_PHASE_END_EVENT || t == CYCLE_STOP_EVENT
}
```

### Tally-skip coupling (R-05 server side)

A stamped event must NOT feed the vote tally and must NOT run eager attribution.
At each call site, the existing `record_topic_signal` / `set_feature_if_absent` /
`check_eager_attribution` blocks (which key off `event.topic_signal`) must be
GUARDED so they do not run when `event.cycle_stamp.is_some()`:
```rust
    if event.cycle_stamp.is_none() {
        // existing col-017 record_topic_signal + #198 eager blocks run here only
    }
```
(The client already strips `topic_signal` from stamped non-CYCLE_* frames, so
`event.topic_signal` is None on stamped events — but the server guard is the
belt-and-suspenders against a mixed/buggy client double-attributing. Both ends
enforce the boundary, R-05 integration risk.)

## enrich_topic_signal → enrich_topic_signal_with_source (:148)

Extend (do NOT delete) `enrich_topic_signal`. New function returns
`(Option<String>, Option<String>)` = (signal, topic_source). The old
`enrich_topic_signal` may remain as a thin wrapper for any non-record-path caller,
OR be replaced everywhere — but every record-path call must use the
source-returning variant. Decision tree (ADR-004 §4, FR-21):

```rust
fn enrich_topic_signal_with_source(
    extracted: Option<String>,
    session_id: &str,
    reg: &SessionRegistry,
) -> (Option<String>, Option<String>) {
    let state = reg.get_state(session_id);               // ONE get_state, as today
    let feat = state.as_ref().and_then(|s| s.feature.clone());
    let src  = state.as_ref().map(|s| s.feature_source.clone());

    // 1. declared registry feature beats extraction (the unstamped-window #588 remedy)
    if let (Some(f), Some(FeatureSource::Declared)) = (&feat, &src) {
        if let Some(ref ex) = extracted {
            if ex != f {
                tracing::debug!(session_id, extracted=%ex, declared=%f,
                    "enrich: declared registry feature overrides extraction (#588 remedy)");
            }
        }
        return (Some(f.clone()), Some("declared".to_string()));
    }
    // 2. extracted present → extraction wins (only against Inferred/absent registry now)
    if let Some(ex) = extracted {
        return (Some(ex), Some("extracted".to_string()));
    }
    // 3. registry fill, split by InferredOrigin
    match (&feat, &src) {
        (Some(f), Some(FeatureSource::Inferred(InferredOrigin::Registered))) =>
            (Some(f.clone()), Some("registry-fill".to_string())),
        (Some(f), Some(FeatureSource::Inferred(InferredOrigin::Voted))) =>
            (Some(f.clone()), Some("vote".to_string())),
        // Declared with no extraction (case 1 handled the with-extraction subcase;
        // this is declared NULL-fill) → declared
        (Some(f), Some(FeatureSource::Declared)) =>
            (Some(f.clone()), Some("declared".to_string())),
        // 4. nothing
        _ => (None, None),    // topic_source NULL — UNATTRIBUTED
    }
}
```
Note: the old behavior ("explicit wins UNCONDITIONALLY") is now "explicit wins
ONLY against Inferred/absent registry" — case 1 short-circuits it for `Declared`.
The forensics log INVERTS (now logs when DECLARED overrides extraction) per
ADR-004 consequence. The doc comment on `enrich_topic_signal` must be rewritten
(its "explicit always wins" claim is no longer true).

## The THREE record sites — wire in the helper

All three currently do: `obs = extract_observation_fields(event); obs.topic_signal
= enrich_topic_signal(...); obs.phase = registry.current_phase`. Replace the
`topic_signal`/`phase` assignment with `apply_stamp_to_row(&mut obs, event, reg)`
(which sets topic_signal, phase, topic_source together), keeping the existing
`obs.phase` registry capture as the fallback inside the helper for the non-stamp
cases.

1. **Site A — rework_candidate** (`listener.rs:786-792`): single
   `post_tool_use_rework_candidate` arm.
2. **Site B — general RecordEvent** (`listener.rs:951-957`): the col-012 single
   arm (after lifecycle routing + #198 + col-017, which must be cycle_stamp-guarded
   per the tally-skip coupling).
3. **Site C — batch RecordEvents** (`listener.rs:1108-1121`): inside the
   `obs_batch` map closure — call `apply_stamp_to_row(&mut obs, event, reg)` per
   event (every member of the batch, R-06).

Each site keeps its existing structure (extract → enrich → spawn_blocking insert);
the only change is the enrich step becomes the shared helper. The col-017
record_topic_signal / #198 eager blocks at sites B and C are wrapped in
`if event.cycle_stamp.is_none()` (tally-skip).

## Inversion Flip 2 — close (`process_session_close`, :1939)

The `feature_source` is captured in the EXISTING `get_state` snapshot at :1951
(add it to the destructured tuple — minimal diff):
```rust
let (feature_cycle, injection_count, compaction_count, topic_signals, feature_source) = {
    if let Some(state) = session_registry.get_state(session_id) {
        ( state.feature.clone(), ..., state.topic_signals.clone(), state.feature_source.clone() )
    } else {
        (None, 0, 0, HashMap::new(), FeatureSource::Inferred(InferredOrigin::Registered))
    }
};
```
Then the `final_feature_cycle` computation (:2010) short-circuits for declared
(ADR-004 §6, FR-17):
```rust
let final_feature_cycle = if matches!(feature_source, FeatureSource::Declared)
    && feature_cycle.is_some()
{
    feature_cycle.clone()                    // declared wins — vote + content fallback skipped
} else if let Some(ref topic) = resolved_topic {
    Some(topic.clone())                      // existing vote path, UNCHANGED
} else {
    // existing content-based fallback → feature_cycle.clone(), UNCHANGED
    ... content_based_attribution_fallback ... .or(feature_cycle.clone())
};
```
This is the ONLY change in `process_session_close`'s resolution. `majority_vote`,
`content_based_attribution_fallback`, the sweep loop, purge emission — UNTOUCHED.
Minimal-diff (C-10, crt-052 rebases over this).

## INSERTs gain `?10` (:3073, :3113)

Both `insert_observation` and `insert_observations_batch` add `topic_source` to
the column list and a `?10` bind (topic content traverses the parameterized bind,
never interpolation — security):
```sql
INSERT INTO observations
  (session_id, ts_millis, hook, tool, input, response_size, response_snippet,
   topic_signal, phase, topic_source)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
```
```rust
    .bind(&obs.phase)         // ?9 (existing)
    .bind(&obs.topic_source)  // ?10 NEW
```
Both the single (:3073) and batch (:3113) INSERT statements. Other INSERT sites
(store-crate `insert_observation` in observations.rs:82, analytics/export/
background) are NOT extended — they are not record-path; their rows are NULL-source
by design (ADR-005 §4). **Delivery: grep-audit `INSERT INTO observations`** to
confirm exactly these two record-path sites gained the column (R-12, #4372 lesson).

## Imports

`use crate::infra::session::{FeatureSource, InferredOrigin};` (and `apply_stamp`
is a method on `SessionRegistry`, already in scope). `CYCLE_*_EVENT` constants are
already imported (`listener.rs:47`).

## Data Flow

```
ImplantEvent (event.cycle_stamp: Option) ─┬─ Some → row.{topic_signal=stamp.topic,
                                          │         phase=stamp.phase??reg, source='declared'}
                                          │         + registry.apply_stamp; skip tally/enrich
                                          ├─ CYCLE_* → source='declared' (signal already set)
                                          └─ else → enrich_with_source tree → (signal, source)
   → ObservationRow → INSERT ?10 → observations.topic_source
session close/sweep → feature_source==Declared&&feature ? feature : vote→fallback→feature
```

## Error Handling

`apply_stamp_to_row` is synchronous registry reads/writes under the existing lock
poison-recovery. Inserts keep their existing `StoreError::Database` map_err. No new
panics. `apply_stamp` no-ops on absent session (row still gets 'declared').

## Key Test Scenarios

- Per-site round-trip (R-01): stamped event through Site A, Site B, Site C each
  lands a row `topic_signal=stamp.topic`, `topic_source='declared'` — asserted
  INDEPENDENTLY per site (not once). Batch of N stamped → N declared rows.
- Negative: unstamped Rust-hook frame through all three → legacy chain.
- Stamped event SKIPS tally (vote tally does not grow) and SKIPS enrich (R-05 / FR-14).
- enrich decision tree, one case per branch (R-04): declared-registry +
  contradicting extraction → 'declared'; Inferred(Registered) + extraction →
  'extracted'; Inferred(Registered) + no extraction → 'registry-fill';
  Inferred(Voted) + no extraction → 'vote'; nothing → NULL. Inverted forensics log
  fires on declared-overrides-extraction.
- Close flip (R-17/FR-17): declared session + contradicting vote → declared wins;
  Inferred session → vote/content path as today.
- One integration case per topic_source value asserts the column matches the path
  (R-12); grep-audit confirms only the two record-path INSERTs gained `?10`.
- Minimal-diff: close change is one short-circuit; zero edits to
  drain/clear/transcript (R-18).

## Open Questions / Gaps

- **OQ-A — `topic_source='vote'` row-level write site (RESOLVED here, confirm at
  gate)**: there is NO dedicated row-level vote write. `'vote'` rows are produced
  ONLY via the enrich decision-tree branch where an unstamped event with no
  extraction fills from a registry feature whose `feature_source ==
  Inferred(Voted)` (eager-set, #198). The session-level majority vote at
  close/sweep resolves `sessions.feature_cycle`, never observation rows (rows are
  immutable, never retro-stamped — ADR-005 §1). FR-21's one-source-per-write-site
  holds: `'vote'` has exactly one write site (the enrich `Inferred(Voted)` arm).
  Delivery must confirm no other path writes `'vote'` to a row before closing
  Gate 3; if a future eager path sets `Voted` then immediately records a row in
  the same arm, ensure that row still routes through `enrich_topic_signal_with_source`
  (it does — eager runs in the cycle_stamp.is_none() block, the obs build runs
  after, reading the now-Voted registry).
- **enrich wrapper vs replace**: whether to keep `enrich_topic_signal` as a thin
  wrapper or replace all callers with `_with_source` is a delivery cleanliness
  choice; the binding requirement is every RECORD-PATH call returns a source.
