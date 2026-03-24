# Scope Risk Assessment: col-024

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `cycle_events.timestamp` is seconds; `observations.ts_millis` is milliseconds — unit mismatch in boundary comparison is a silent correctness bug | High | High | Architect must mandate a named conversion constant or helper; no raw `* 1000` literals in query construction |
| SR-02 | `ObservationSource` is a sync trait bridged via `block_sync`/`block_in_place` — adding a multi-step method with per-window SQL loops risks blocking the async runtime longer than single-query predecessors | Med | Med | Architect should evaluate whether the per-window query loop runs inside one `block_sync` call or multiple; prefer single block entry per `load_cycle_observations` invocation |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Open-ended window (`cycle_start` with no `cycle_stop`) uses `unix_now_secs()` as implicit stop — on in-progress features this is correct, but calling `context_cycle_review` on a feature that was force-abandoned without a `cycle_stop` event will over-include subsequent observations | Med | Med | Spec writer should define the behavior for abandoned cycles; architect should document whether a maximum window cap applies |
| SR-04 | AC-08 (explicit `extract_topic_signal` result must not be overridden) interacts with AC-05–07 (registry fallback). If `extract_topic_signal` returns a signal from a *different* feature (e.g., user typed an old feature ID in input), the enrichment is correctly suppressed — but the observation ends up attributed to the wrong feature with no way to detect or correct it | Med | Low | Spec writer should clarify whether a mismatch between the extracted signal and the registry feature is a detectable anomaly or accepted behavior |
| SR-05 | The three enrichment sites (RecordEvent, RecordEvents batch loop, ContextSearch) are listed in the scope but the enrichment logic is structurally different for each — the batch path requires per-event registry lookups inside a loop, raising the risk of partial enrichment if the loop exits early on error | Low | Med | Architect should define a shared enrichment helper to avoid per-site drift |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | The three-step query in `load_cycle_observations` (cycle_events → session IDs via topic_signal → observations) produces no results if topic_signal enrichment was never applied (e.g., pre-col-024 observations in an otherwise new cycle) — the legacy fallback activates silently, masking the enrichment gap rather than surfacing it | High | Med | Architect should define an observable signal (log line, metric, or status field) when the primary path returns empty and fallback activates, so enrichment gaps are detectable post-deploy |
| SR-07 | `topic_signal` has no index; Step 2 is bounded by timestamp + table scan within the window. The scope accepts this at ~100K observations / ~3K rows per 6-hour window, but `context_cycle_review` is called once per retrospective — if window sizes grow (long-running features, dense hook traffic), scan cost compounds per window | Low | Low | Architect should record the scale assumption explicitly; the composite index deferral (Resolved Decision 4 in SCOPE.md) should include a volume threshold trigger |

## Assumptions

- **SCOPE.md §Background Research / cycle_events schema**: Assumes `cycle_events.timestamp` is always Unix epoch seconds. If any write path uses milliseconds or a different epoch, all window comparisons are wrong. No validation of the unit is performed at runtime.
- **SCOPE.md §Background Research / topic_signal write sites**: Assumes `session_registry.get_state(sid)?.feature` is always set before any observation is written for that session. If `set_feature_force` races with the first observation in a new session, enrichment silently drops to None even in the new path.
- **SCOPE.md §Legacy fallback requirement**: Assumes that an empty result from `load_cycle_observations` reliably signals "no cycle_events rows exist" and not "cycle_events rows exist but no observations matched." These two cases produce the same empty vec, causing different situations to trigger the same fallback.

## Design Recommendations

- **SR-01**: Introduce a named unit-conversion constant or a `cycle_ts_to_obs_millis(ts: i64) -> i64` helper before any SQL is written. Prevent the mismatch from appearing as a raw literal.
- **SR-03 + Assumption 2**: Define the enrichment contract as "best-effort" explicitly in the spec, and add a test that verifies the open-ended window does not include observations from a subsequent cycle that reuses the same session.
- **SR-06**: Add a structured log event (e.g., `tracing::debug!`) when the primary path returns empty and legacy fallback activates. This is the single highest-value observability addition for diagnosing attribution gaps post-deploy.
