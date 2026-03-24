# Component: context_cycle_review Lookup Restructure
# File: crates/unimatrix-server/src/mcp/tools.rs

## Purpose

Restructure the observation-loading block inside `context_cycle_review` from two-path
(direct feature_cycle + unattributed fallback) to three-path (primary cycle_events +
legacy sessions + legacy content-scan). Add structured `tracing::debug!` events on each
fallback transition (ADR-003).

Nothing outside the observation-loading block changes: identity resolution, validation,
MetricVector caching, detection pipeline, report formatting, and the timeout wrapper are
all unchanged.

## New/Modified Functions

### `context_cycle_review` inner observation-loading closure (modified)

The closure passed to `spawn_blocking_with_timeout` currently (pre-col-024):
1. calls `load_feature_observations` (fast path)
2. if empty: calls `load_unattributed_sessions` + `attribute_sessions`

After col-024, the same closure becomes:
1. calls `load_cycle_observations` (primary path — new)
2. if empty: debug log + calls `load_feature_observations` (legacy-1)
3. if empty: debug log + calls `load_unattributed_sessions` + `attribute_sessions` (legacy-2)

The surrounding code (outside this closure) does NOT change.

```
// MODIFIED closure inside spawn_blocking_with_timeout:
// (variable names match the existing code: feature_cycle_for_load, source, etc.)

move || -> std::result::Result<Vec<unimatrix_observe::ObservationRecord>, unimatrix_observe::ObserveError> {
    use unimatrix_observe::ObservationSource;
    let source = crate::services::observation::SqlObservationSource::new(
        store_for_obs,
        registry_for_obs,
    );

    // ---- Path 1: Primary (cycle_events-based) ----
    // load_cycle_observations returns Ok(vec![]) for pre-col-024 features and
    // for enrichment gaps. Returns Err only on genuine SQL failure.
    let primary = source.load_cycle_observations(&feature_cycle_for_load)?;
    if !primary.is_empty() {
        return Ok(primary);
    }

    // Primary path returned empty. Log fallback transition (ADR-003).
    // Suppressed in production (debug level). Visible with RUST_LOG=debug.
    tracing::debug!(
        cycle_id = %feature_cycle_for_load,
        path = "load_feature_observations",
        "CycleReview: primary path empty, falling back to legacy sessions path"
    );

    // ---- Path 2: Legacy-1 (sessions.feature_cycle) ----
    let legacy1 = source.load_feature_observations(&feature_cycle_for_load)?;
    if !legacy1.is_empty() {
        return Ok(legacy1);
    }

    // Legacy-1 also returned empty. Log second fallback transition (ADR-003).
    tracing::debug!(
        cycle_id = %feature_cycle_for_load,
        path = "load_unattributed_sessions",
        "CycleReview: legacy sessions path empty, falling back to content attribution"
    );

    // ---- Path 3: Legacy-2 (content-based attribution) ----
    // Unchanged from pre-col-024. load_unattributed_sessions returns ParsedSession
    // structs; attribute_sessions filters to the cycle_id.
    let unattributed = source.load_unattributed_sessions()?;
    if unattributed.is_empty() {
        return Ok(vec![]);
    }

    Ok(unimatrix_observe::attribute_sessions(
        &unattributed,
        &feature_cycle_for_load,
    ))
}
```

## Critical Fallback Semantics (FM-01)

The `?` operator after `load_cycle_observations` propagates `Err(ObserveError)` to the
`spawn_blocking_with_timeout` caller, which maps it to a `ServerError` and returns an
MCP error response to the agent.

The fallback to `load_feature_observations` MUST activate only on `Ok(vec![])`, never
on `Err(...)`. This is expressed by the `?` on `load_cycle_observations`: errors escape
the closure immediately, bypassing the fallback. The `if !primary.is_empty()` check
only runs when the `?` was NOT taken (i.e., the result was `Ok`).

This is already the correct Rust semantics for `let primary = source.load_cycle_observations(...)? ;`.
No additional guard is needed.

## State Machines

None. The closure is stateless. It reads from SQL and returns a value.

## Initialization Sequence

The closure captures:
- `store_for_obs: Arc<SqlxStore>` — cloned before the closure, as in existing code
- `registry_for_obs: Arc<DomainPackRegistry>` — cloned before the closure, as in existing code
- `feature_cycle_for_load: String` — cloned before the closure, as in existing code

No change to what is captured. Only the closure body changes.

## Data Flow

```
Existing (pre-col-024) two-path:
  load_feature_observations  ->  Ok(records) or Ok(vec![])
  if empty: load_unattributed_sessions + attribute_sessions

New (col-024) three-path:
  load_cycle_observations    ->  Ok(records) [primary, cycle_events-based]
      |  non-empty -> return
      |  empty -> debug log
  load_feature_observations  ->  Ok(records) [legacy-1, sessions.feature_cycle]
      |  non-empty -> return
      |  empty -> debug log
  load_unattributed_sessions + attribute_sessions  ->  Ok(records) [legacy-2, content-scan]
      |  result (possibly empty) -> return
      v
  Vec<ObservationRecord>  (possibly empty)

Downstream (unchanged):
  if empty: check cached MetricVector; return ERROR_NO_OBSERVATION_DATA if no cache
  if non-empty: detection pipeline, MetricVector computation, report formatting
```

## Error Handling

| Failure Source | Behavior |
|----------------|----------|
| `load_cycle_observations` returns Err | `?` propagates; closure returns Err; legacy fallback does NOT activate (FM-01) |
| `load_feature_observations` returns Err | `?` propagates; closure returns Err |
| `load_unattributed_sessions` returns Err | `?` propagates; closure returns Err |
| `attribute_sessions` returns empty vec | closure returns `Ok(vec![])` |
| `spawn_blocking_with_timeout` timeout | existing timeout error handling unchanged |

The `.map_err(rmcp::ErrorData::from)?` chain after `spawn_blocking_with_timeout` is
unchanged. All errors reach the MCP error response path.

## Key Test Scenarios

| Test Name | Covers | Method |
|-----------|--------|--------|
| `cycle_review_uses_primary_path_first` | AC-04, AC-09, AC-12 | Mock ObservationSource: load_cycle_observations returns non-empty; assert load_feature_observations NOT called |
| `cycle_review_falls_back_to_legacy1_on_empty_primary` | AC-04, AC-09 | load_cycle_observations returns Ok(vec![]); load_feature_observations returns non-empty; assert result is non-empty |
| `cycle_review_falls_back_to_legacy2_on_both_empty` | AC-04 | Both Ok(vec![]); assert load_unattributed_sessions called |
| `cycle_review_debug_log_on_primary_empty` | AC-14, R-08 | log capture: assert debug! fires with cycle_id and path="load_feature_observations" when primary empty |
| `cycle_review_debug_log_on_legacy1_empty` | ADR-003 | log capture: assert debug! fires with path="load_unattributed_sessions" when legacy-1 also empty |
| `cycle_review_error_from_primary_does_not_fallback` | FM-01, R-03 scenario 3 | load_cycle_observations returns Err; assert error propagated to MCP, load_feature_observations NOT called |
| `cycle_review_legacy_features_unchanged` | AC-09, AC-12 | Pre-col-024 features with sessions.feature_cycle data: existing tests must pass without modification |

## Constraints

- The debug log messages are at `tracing::debug!` level (not `info!` or `warn!`). This
  is non-negotiable (ADR-003, R-08).
- The structured fields are `cycle_id` and `path`. The message strings are defined in
  ADR-003 and must match exactly for log aggregator filtering.
- The fallback only activates on `Ok(vec![])`, not on `Err(...)` (FM-01).
- All code outside the observation-loading closure is unchanged. Do not modify the
  identity resolution block, validation, MetricVector cache check, detection pipeline,
  or report formatting.
- No new imports needed for `tracing::debug!` — `tracing` is already imported in
  `tools.rs`.
