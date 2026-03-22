## ADR-001: Phase Snapshot at Enqueue Time for FeatureEntry Analytics Path

### Context

`FEATURE_ENTRIES` is written via two paths:

1. **Direct write path** (`record_feature_entries` in `write_ext.rs`): called synchronously from the `context_store` handler after session state is read.
2. **Analytics drain path** (`AnalyticsWrite::FeatureEntry` in `analytics.rs`): events are enqueued via `try_send` and drained in a background task.

The risk (SR-07): if `current_phase` is read from live `SessionState` at drain time rather than at enqueue time, the background task may execute after the agent has advanced to a later phase. The entry would be tagged with the wrong phase. For W3-1, systematic wrong-phase tagging silently degrades training data quality with no observable error.

The analogous established pattern in the codebase is the `feature_cycle` capture in `UsageContext`: it is snapshotted from session state at call time and passed as a value through the async boundary, never re-read from live state at execution time.

### Decision

Phase is captured as a local `Option<String>` **at the point where session state is read** in the `context_store` handler — immediately after the `get_state()` call. This snapshotted value is then:

- Passed as `phase: Option<&str>` to `record_feature_entries(feature_cycle, ids, phase)` on the direct path.
- Baked into `AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase }` on the analytics drain path.

The background drain task never re-reads `SessionState`. The phase value it writes to the database is exactly what was current at enqueue time.

`UsageContext` gains a `current_phase: Option<String>` field to carry this value through the existing usage service abstraction layer.

### Consequences

**Easier**:
- Phase tagging is deterministic regardless of async scheduling delays.
- W3-1 training labels are causally correct: the phase recorded for an entry is the phase that was active when the agent chose to store that knowledge.
- No lock contention on `SessionRegistry` from background tasks.

**Harder**:
- `record_feature_entries` signature changes (adds `phase` parameter). All call sites must be updated — there are three: `server.rs`, `services/usage.rs`, and tests.
- `AnalyticsWrite::FeatureEntry` struct changes (adds `phase` field). All construction sites must be updated.
- Import tooling (`import/inserters.rs`) must be aware of the new column (handled by schema migration; existing rows get `NULL`).
