## ADR-003: Attribution Metadata on Report

### Context

SR-07 identifies that col-017 (topic attribution) quality directly bounds col-020 metric accuracy. If only 3 of 10 sessions are attributed to the topic, session summaries cover 30% of activity, knowledge reuse undercounts, and rework session count is incomplete.

Currently the report has `session_count` (number of sessions in the observation data) but no signal about how many sessions were expected or how many lack attribution. Consumers cannot distinguish "this topic had 3 sessions" from "this topic had 10 sessions but 7 are unattributed."

Two options:

**Option A**: Add `AttributionMetadata { attributed_session_count, total_session_count }` to the report. Consumers compare the two numbers to assess coverage.

**Option B**: Do nothing — consumers already know if attribution is incomplete from their workflow context.

Option B is insufficient because retrospective consumers (vnc-011 ReportFormatter, future dashboards, lesson-learned extraction) have no inherent knowledge of attribution completeness.

### Decision

Add `attribution: Option<AttributionMetadata>` to `RetrospectiveReport`. The handler populates it:

```rust
pub struct AttributionMetadata {
    /// Sessions with feature_cycle matching the requested topic.
    pub attributed_session_count: usize,
    /// Total sessions discovered (including unattributed fallback matches).
    pub total_session_count: usize,
}
```

`attributed_session_count` comes from `discover_sessions_for_feature()` (direct SQL match on `sessions.feature_cycle`). `total_session_count` is the same value when using the fast path, or may be larger when the content-based attribution fallback contributes additional sessions.

For backward compatibility, `attribution` uses `#[serde(default, skip_serializing_if = "Option::is_none")]`.

### Consequences

- **Easier**: Consumers (ReportFormatter, dashboards) can display attribution coverage and flag low-coverage reports.
- **Easier**: Downstream features (crt-018 knowledge effectiveness, crt-019 search quality) can weight their analysis by attribution completeness.
- **Harder**: One more optional field on the report struct. Marginal complexity.
- **Neutral**: Does not improve attribution quality itself — that is col-017's responsibility. This only surfaces the quality signal.
