# Agent Report: 437-design-reviewer

**Feature**: bugfix-437
**Task**: Design review of proposed recurring_friction fix

## Design Assessment: REWORK NEEDED

One blocking gap makes Step 4 unimplementable as written. Four non-blocking observations. The fix strategy is architecturally sound — the rework is a one-step change that does not alter the overall approach.

## Findings

### [BLOCKING] F-1: Step 4 has no observations at the call site

`compute_friction_recommendations(observations: &[ObservationRecord])` requires observation data, but `run_single_tick()` has no `observations` variable after `extraction_tick()` returns. The observations are:

1. Fetched inside `extraction_tick()` via `fetch_observation_batch()`
2. Moved into the `spawn_blocking` closure as `obs_for_rules`
3. Never returned — `extraction_tick()` returns only `Result<ExtractionStats, ServiceError>`

Placing the call in `run_single_tick()` after the timeout block would force either a second `fetch_observation_batch()` DB call (new hot-path I/O, inconsistent with watermark semantics) or a standalone store query — both anti-patterns. The correct fix is to move the call inside `extraction_tick()` where observations are already in scope.

### [Non-blocking] F-2: extraction_tick return type must change

Implied by the rework: return type changes from `Result<ExtractionStats, ServiceError>` to `Result<(ExtractionStats, Vec<String>), ServiceError>`. Contained change — only the function body and the one match arm in `run_single_tick()` are affected.

### [Non-blocking] F-3: Ephemeral signals repeat every tick — no dedup

After the fix, `friction_signals` is re-computed every tick. This is correct for operational reporting, but the `friction_signals` field doc comment should say "re-computed each tick; repeated appearance is expected" to prevent a future reader from adding unnecessary dedup logic.

### [Non-blocking] F-4: Migration uses Quarantine not Deprecated — needs explicit comment

`run_dead_knowledge_migration_v1` uses `Status::Deprecated`. The new migration should use `Status::Quarantined`. Quarantine is the correct choice (these entries were never real knowledge), but needs an explanatory comment.

### [Non-blocking] F-5: Unconditional append changes maintenance_recommendations semantics

`maintenance_recommendations` is currently lambda-gated. Making friction signals unconditional is architecturally correct but the append site needs a comment: "friction_signals are unconditional — they report agent workflow patterns, not KM graph health."

## Revised Fix (Step 4 only — all other steps approved)

Move `compute_friction_recommendations` into the same `spawn_blocking` closure in `extraction_tick()`:

```rust
let (proposals, friction_recs) = tokio::task::spawn_blocking(move || {
    let rules = default_extraction_rules();
    let proposals = run_extraction_rules(&obs_for_rules, &store_for_rules, &rules);
    let friction_recs = compute_friction_recommendations(&obs_for_rules);
    (proposals, friction_recs)
})
.await
.map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))?;

Ok((ctx.stats.clone(), friction_recs))
```

Match arm in `run_single_tick()`:

```rust
Ok(Ok((stats, friction_recs))) => {
    if let Ok(mut meta) = tick_metadata.lock() {
        meta.last_extraction_run = Some(now_secs());
        meta.extraction_stats = stats;
        meta.friction_signals = friction_recs;
    }
}
```

`run_extraction_rules` takes `&[ObservationRecord]` (borrow, not move), so `obs_for_rules` is still available for the friction call. No second DB fetch. Pure CPU work absorbed into the existing `spawn_blocking`. Timeout wraps both operations together.

## Hot-Path Risk Summary

| Path | Risk | Verdict |
|------|------|---------|
| `extraction_tick` spawn_blocking | Adds pure CPU work (HashMap + detect_hotspots) — no I/O | Acceptable |
| `run_single_tick` match arm | One extra Vec<String> assign inside existing tick_metadata.lock() | Negligible |
| `context_status` handler | .extend() on ≤~20 strings inside existing lock scope | Negligible |
| Migration | O(1) COUNTERS fast path; DB work only on first boot after deploy | Acceptable |

## Knowledge Stewardship

- Queried: Reviewed extraction_tick, run_single_tick, run_dead_knowledge_migration_v1, TickMetadata, and context_status handler directly. Investigator report entry #3252 used for prior context.
- Stored: Declined -- blocking gap is fix-specific. Generalizable principle (ExtractionRule must not perform side effects) already exists in entry #3252.
