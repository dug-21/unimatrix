# Scope Risk Assessment: crt-036

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | SQLite subquery joins through `sessions` at tick time may lock the write pool longer than expected on a 152 MB `observations` table, stalling concurrent writes | High | Med | Architect should cap batch size in `RetentionConfig` and measure DELETE latency in tests; consider per-cycle transactions rather than one spanning all purgeable cycles |
| SR-02 | `write_pool_server()` has max_connections=1; a long-running GC DELETE inside a per-cycle transaction can deadlock if the drain task or audit write holds the connection (entry #2249) | High | Med | Do not wrap the full multi-cycle pass in one transaction; use per-cycle transactions and release the connection between cycles |
| SR-03 | sqlx pool does not guarantee connection identity across multiple calls within a logical "transaction" unless using `pool.begin()` — manual BEGIN/COMMIT risks silent data loss (entry #2159) | Med | Med | Always use `pool.begin()` / `tx.commit()` API for per-cycle atomic deletes; never issue raw `BEGIN` SQL |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Two independent 60-day DELETE sites (`status.rs` line 1380 + `tools.rs` line 1638) — scope states both must be removed, but the `tools.rs` path is an in-tool FR-07 call that may be overlooked during delivery | High | Med | Spec writer should call out both file+line locations as explicit AC line items and add a grep-based gate check |
| SR-05 | `raw_signals_available = 0` flag update uses `store_cycle_review()` INSERT OR REPLACE — this overwrites the full `cycle_review_index` row; if the read-modify-write races with `context_cycle_review`, the summary_json could be silently overwritten | Med | Low | Architect should confirm the flag update uses a targeted `UPDATE` rather than INSERT OR REPLACE, or verify the write path is exclusively the background tick |
| SR-06 | Unattributed sessions (`feature_cycle IS NULL`) are pruned unconditionally — this is correct policy but scope does not address sessions that are open (status = Active) with NULL cycle; pruning an active session's observations could disrupt an in-flight retrospective | Med | Low | Spec writer should add a guard: skip unattributed prune if `sessions.status = 'Active'` |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `PhaseFreqTable` lookback is governed by `query_log_lookback_days` in `InferenceConfig` — after crt-036 ships, retained data may be fewer than `query_log_lookback_days` days; the freq table silently operates on a truncated window without warning | Med | High | Architect should add a startup/tick warning when `query_log_lookback_days` exceeds the data coverage implied by K cycles |
| SR-08 | `cycle_review_index` is the single gate for all pruning; if `context_cycle_review` is never called (e.g., a cycle completes but the human skips retro), data for that cycle is retained forever — potentially defeating the purpose of the feature | Med | Med | Document as a known operational constraint; spec writer should note it in AC-04 context |
| SR-09 | Background tick pattern already has a Arc&lt;RwLock&lt;T&gt;&gt; shared-state sole-writer rule (entry #1560) — if the new RetentionConfig is read inside `run_maintenance` without going through the established cache pattern, concurrent config reloads could read a partially-written struct | Low | Low | Architect should confirm `RetentionConfig` is loaded once at startup and passed by value into `run_maintenance`, not re-read from disk each tick |

## Assumptions

- **§ "No schema migration required"**: Assumes `idx_observations_session` and `idx_query_log_session` are selective enough for DELETE subquery performance at 152 MB scale. If the planner chooses a full-table scan, the assumption breaks. Verify with EXPLAIN QUERY PLAN in the integration test.
- **§ "cycle_review_index gate"**: Assumes `context_cycle_review` is always called before cycles become eligible for pruning. If retrospective tooling is skipped, the K-window never advances and data grows without bound — opposite of the intended outcome.
- **§ "Proposed Approach / K-cycle Resolution"**: Assumes `cycle_review_index.computed_at` order is a reliable proxy for cycle chronological order. If clocks are skewed or back-filled reviews are written, `computed_at` may not reflect cycle creation order.

## Design Recommendations

- **SR-01 + SR-02**: Architect must design the GC pass as per-cycle transactions with connection release between cycles, not a single spanning transaction. Cap at a configurable `max_cycles_per_tick` in `RetentionConfig`.
- **SR-04**: Both deletion sites are non-negotiable removals; spec writer should make each a named, independently verifiable AC line item and require a grep assertion in tests.
- **SR-07**: Architect should add a tick-time diagnostic that compares `query_log_lookback_days` against observed retained data range and emits `tracing::warn!` on mismatch.
- **SR-05**: Architect should use a targeted `UPDATE cycle_review_index SET raw_signals_available = 0 WHERE feature_cycle = ?` rather than INSERT OR REPLACE to avoid clobbering `summary_json`.
