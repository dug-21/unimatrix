## ADR-002: Time-Based Retention Window for Frequency Table Rebuild

### Context

`PhaseFreqTable::rebuild` must filter `query_log` rows to a recent window so
that stale phase-access patterns from long-ago sessions do not dilute the
signal. Two retention strategies were considered:

**Cycle-based retention**: Filter by `feature_cycle` — only rows from the N
most recent cycles. This would produce a workflow-representative window,
since each cycle maps to one feature delivery context. However, `query_log`
has no `feature_cycle` column (verified against `migration.rs`). This column
does not exist and cannot be assumed without a migration. Issue #409 owns
cycle-aligned GC and retention as a separate work item.

**Time-based retention**: Filter by `ts` (Unix epoch seconds, `INTEGER`):
`WHERE ts > strftime('%s', 'now') - lookback_days * 86400`. This is
correct for the verified `INTEGER` storage type of `query_log.ts`. A
`lookback_days: u32` configuration field governs the window, defaulting to 30.

A 30-day window at typical Unimatrix usage rates captures multiple workflow
cycles. The signal quality is session-frequency-dependent (SR-05): high-volume
periods compress many cycles into the window; low-volume periods may span a
fraction of one. This is an acknowledged approximation; #409 is the correct
long-term fix.

### Decision

Use `query_log_lookback_days: u32` (default `30`) in `InferenceConfig` to
govern the rebuild SQL window. The filter expression is
`WHERE q.ts > strftime('%s', 'now') - ?1 * 86400` where `?1 = lookback_days as i64`.
No JOIN with the `sessions` table. No cycle-based filter. No new migration.

`query_log_lookback_days` is validated in `InferenceConfig::validate()` with
range `[1, 3650]`. Operators may override via TOML; the 30-day default is the
calibrated starting point pending #409.

### Consequences

**Easier**:
- Zero schema changes — leverages the existing `ts` column and
  `idx_query_log_ts` index (already present).
- Operator-configurable via TOML without code changes.
- Cold-start safe: if `query_log` is empty or contains only pre-col-028 rows
  (all with `phase = NULL`), the SQL returns zero rows and `use_fallback = true`
  — degraded but not broken.

**Harder**:
- Signal quality is session-frequency-dependent, not cycle-representative.
  A deployment with sparse usage may see an effectively cold-start table even
  after 30 days.
- #409 (cycle-aligned GC) is the correct successor; until it ships, operators
  with atypical usage patterns may need to tune `query_log_lookback_days`.
