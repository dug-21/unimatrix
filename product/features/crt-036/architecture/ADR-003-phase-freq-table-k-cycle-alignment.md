## ADR-003: PhaseFreqTable / K-cycle Alignment Strategy

### Context

`PhaseFreqTable::rebuild()` queries `query_log` with a time-based lookback window
controlled by `inference_config.query_log_lookback_days` (col-031 ADR-002, entry #3686).
The `query_log_lookback_days` field was designed as a time-based approximation of
"recent activity" — its docstring explicitly defers cycle-aligned GC to GH #409
(this feature).

After crt-036 ships, `query_log` rows for reviewed cycles outside the K retention window
are deleted. This creates a silent coherence gap: if `query_log_lookback_days` is set
to 60 days but K = 50 cycles spans only 30 days of actual data (because cycles closed
quickly), the frequency table rebuild query requests 60 days but only finds 30 days
of data. The SQL returns fewer rows than expected with no indication that the window
was truncated.

Three strategies were considered:

**Option A — Hard coupling: enforce `query_log_lookback_days` <= implied K-cycle coverage.**
At startup (or in `validate()`), compute the maximum data coverage implied by K cycles
and reject configs where `query_log_lookback_days` exceeds it.

Problem: cycle duration is not constant and is not knowable at startup. A cycle could
span 2 hours or 3 months. The implied coverage of K=50 cycles cannot be determined
statically. Rejecting configs based on an unknowable future would force operators to
set `query_log_lookback_days` to a very small conservative value, defeating the
purpose of the field.

**Option B — Remove `query_log_lookback_days` from `InferenceConfig` and derive it
from `activity_detail_retention_cycles`.**
Replace the time-based lookback with a cycle-based lookup that queries the K oldest
retained cycles' `computed_at` range.

Problem: This is a breaking config change (removes a field from `InferenceConfig`).
It also requires `PhaseFreqTable::rebuild()` to do a join through `cycle_review_index`
and `sessions` to compute its time window, which is significantly more complex. This
is the right long-term design but is out of scope for crt-036.

**Option C — Tick-time diagnostic: `tracing::warn!` when the mismatch is detected.**
At tick time, after resolving the K retain set, compare the oldest retained cycle's
`computed_at` timestamp against `now - query_log_lookback_days * 86400`. If the oldest
retained cycle was reviewed more recently than `query_log_lookback_days` days ago, emit
a `tracing::warn!` that tells the operator their lookback window exceeds their retained
data coverage.

This is a runtime diagnostic, not a config rejection. It:
- Provides actionable information without blocking startup.
- Uses real data (actual `computed_at` timestamps) rather than a static approximation.
- Can be acted upon by operators who observe the warning in logs.
- Requires no breaking changes to `InferenceConfig` or `PhaseFreqTable`.
- Does not require an exact coverage metric — the `computed_at` of the oldest retained
  cycle is a reasonable proxy for "oldest data available."

The warning is meaningful even if imprecise: if the oldest retained review was 20 days
ago but `query_log_lookback_days = 30`, the frequency table rebuild will see at most
20 days of query data, not the 30 the operator configured.

### Decision

Implement Option C: emit `tracing::warn!` at tick time when the K-cycle alignment
mismatch is detected. The check runs at the start of step 4, as a by-product of
resolving the purgeable set (the retain set is computed first, so the oldest retained
`computed_at` is available).

Specifically:
1. When resolving the purgeable set, also retrieve the `computed_at` of the K-th
   retained cycle (the oldest retained review).
2. Compute `lookback_cutoff = now_unix_secs - query_log_lookback_days * 86400`.
3. If `oldest_retained_computed_at > lookback_cutoff`:
   The oldest retained cycle was reviewed within the lookback window — no warning
   (data coverage is sufficient or unknown because fewer than K cycles exist).
4. If `oldest_retained_computed_at <= lookback_cutoff`:
   The oldest retained cycle was reviewed before the lookback window started — the
   frequency table may be operating on truncated data. Emit:
   ```
   tracing::warn!(
       query_log_lookback_days = inference_config.query_log_lookback_days,
       activity_detail_retention_cycles = retention_config.activity_detail_retention_cycles,
       oldest_retained_cycle_computed_at = oldest_retained_computed_at,
       lookback_cutoff_secs = lookback_cutoff,
       "PhaseFreqTable lookback window ({} days) likely exceeds retained query_log \
        coverage; oldest retained cycle reviewed at {}, lookback cutoff is {}. \
        Consider reducing query_log_lookback_days or increasing \
        activity_detail_retention_cycles.",
   );
   ```

The check is skipped (no warning) when `cycle_review_index` has fewer than K rows
(the system has not yet reviewed K cycles, so no pruning has occurred and no gap
can exist).

This satisfies SR-07 without a breaking config change and without a complex schema
dependency.

`activity_detail_retention_cycles` is documented in code as the governing ceiling for
`PhaseFreqTable` lookback and future GNN training window (AC-13). The docstring on
the field explicitly cross-references `query_log_lookback_days` and notes the
alignment warning.

### Consequences

Easier:
- No breaking changes to `InferenceConfig` or `PhaseFreqTable` configuration.
- Operators receive actionable diagnostics in the tick log when the gap exists.
- The check is idempotent and cheap (one read against a small `cycle_review_index`
  table, already performed for the GC pass itself).
- The relationship between `activity_detail_retention_cycles` and
  `query_log_lookback_days` is visible in the log without requiring operators to
  reason about it upfront.

Harder:
- The warning is advisory, not enforced. An operator who ignores the log message
  will continue running with a truncated frequency table window. This is the same
  behavior as today (the gap is silent without crt-036); with this ADR the gap
  becomes visible.
- The `computed_at` proxy is imprecise: it measures when the review was computed,
  not when the cycle's first session started. An early-started, long-running cycle
  reviewed recently would not trigger the warning even if its query_log data begins
  months ago. This imprecision is acceptable — the warning is a heuristic, not a
  correctness guarantee.
