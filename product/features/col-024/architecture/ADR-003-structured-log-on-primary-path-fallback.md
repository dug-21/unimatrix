## ADR-003: Structured Log Event on Primary-Path Fallback Activation

### Context

`context_cycle_review` will have three lookup paths after col-024:

1. `load_cycle_observations` (primary: cycle_events-based)
2. `load_feature_observations` (legacy: sessions.feature_cycle)
3. `load_unattributed_sessions` + `attribute_sessions` (legacy: content-scan)

The primary path returns an empty `Vec` in two distinct situations:
- No `cycle_events` rows exist for the `cycle_id` (pre-col-024 features, AC-09).
- `cycle_events` rows exist but no observation has a matching `topic_signal` within
  the time windows (enrichment gap: topic_signal was never written, e.g., the
  feature predates enrichment or the session registry did not have the feature set
  before the first observation).

Both cases produce the same `Ok(vec![])` return, triggering the legacy fallback
silently. SR-06 identifies this as the highest-value observability gap: an
enrichment failure post-deploy is indistinguishable from a legitimate legacy feature
without any signal in the logs.

### Decision

When `load_cycle_observations` returns an empty vec and `context_cycle_review`
transitions to `load_feature_observations`, emit a single structured log event at
`tracing::debug!` level:

```rust
tracing::debug!(
    cycle_id = %feature_cycle,
    path = "load_feature_observations",
    "CycleReview: primary path empty, falling back to legacy sessions path"
);
```

Similarly, when `load_feature_observations` also returns empty and the code
transitions to the unattributed-sessions scan:

```rust
tracing::debug!(
    cycle_id = %feature_cycle,
    path = "load_unattributed_sessions",
    "CycleReview: legacy sessions path empty, falling back to content attribution"
);
```

`debug!` level is chosen deliberately: these events are diagnostic noise in
production for pre-col-024 features (where primary-path empty is expected) but
visible with `RUST_LOG=debug` or structured log export when investigating
attribution gaps.

The log events use structured fields (`cycle_id`, `path`) so they can be filtered
and counted by a log aggregator without parsing message strings.

### Consequences

- Easier: engineers can confirm whether enrichment is working post-deploy by
  filtering logs for `CycleReview: primary path empty` on features known to have
  `cycle_events` rows.
- Easier: the two-step fallback chain is auditable in logs, providing a diagnostic
  trail when `context_cycle_review` returns unexpected results.
- Harder: for pre-col-024 features every `context_cycle_review` call emits a
  `debug` fallback log; this is acceptable (debug level, suppressed by default).
- No change to error return semantics, MetricVector caching, or the
  `ERROR_NO_OBSERVATION_DATA` path — those are unaffected.
