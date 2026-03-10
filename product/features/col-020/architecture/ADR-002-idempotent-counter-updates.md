## ADR-002: Idempotent Counter Updates via Absolute-Set

### Context

AC-12 requires `context_retrospective` to update `topic_deliveries` aggregate counters (total_sessions, total_tool_calls, total_duration_secs) after computation. The existing API is `Store::update_topic_delivery_counters(topic, sessions_delta, tool_calls_delta, duration_delta)` which applies additive increments.

The retrospective is a repeatable operation. A user may run `context_retrospective` for the same topic multiple times (debugging, updated data, periodic review). If each run adds deltas, counter values grow incorrectly on each re-run. This is SR-09 — a correctness bug.

Three options:

**Option A**: Track "last retrospective run" state and compute deltas from the difference. Requires storing the previous computation's counters somewhere, adding state management complexity.

**Option B**: Use `upsert_topic_delivery()` to overwrite the entire record. This overwrites non-counter fields (status, github_issue, phases_completed) which the retrospective handler should not control.

**Option C**: Add a new Store method `set_topic_delivery_counters()` that performs `UPDATE SET total_sessions = ?1, total_tool_calls = ?2, total_duration_secs = ?3 WHERE topic = ?4`. Absolute write — idempotent by nature.

### Decision

Add `Store::set_topic_delivery_counters(topic, total_sessions, total_tool_calls, total_duration_secs)` that performs an absolute UPDATE (not additive). The handler computes correct totals from source data:

- `total_sessions`: count of SessionRecords returned by `scan_sessions_by_feature(topic)` (excludes Abandoned sessions)
- `total_tool_calls`: sum of PreToolUse events across all observation records
- `total_duration_secs`: difference between max and min observation timestamps (in seconds)

These are the same values already computed by the existing `compute_metric_vector()`. The handler reuses the MetricVector values rather than recomputing.

If no `topic_deliveries` record exists for the topic, the handler creates one via `upsert_topic_delivery()` before setting counters. This handles the case where retrospective runs before the topic_deliveries backfill created the record.

### Consequences

- **Easier**: Idempotent by construction. No state tracking, no delta computation. Run retrospective 10 times — same result.
- **Easier**: Counter values always reflect the actual computed data, not accumulated approximations.
- **Harder**: Requires a new Store method (small addition). The existing `update_topic_delivery_counters` (additive) remains available for other consumers that need incremental updates.
- **Neutral**: If observation data changes between runs (new sessions attributed), the counters correctly reflect the updated data. This is desirable — counters should match reality.
