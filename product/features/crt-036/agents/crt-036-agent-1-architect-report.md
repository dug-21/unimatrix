# Agent Report: crt-036-agent-1-architect

## Deliverables

### Files Produced

- `/workspaces/unimatrix/product/features/crt-036/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-036/architecture/ADR-001-per-cycle-transaction-granularity.md`
- `/workspaces/unimatrix/product/features/crt-036/architecture/ADR-002-max-cycles-per-tick-in-retention-config.md`
- `/workspaces/unimatrix/product/features/crt-036/architecture/ADR-003-phase-freq-table-k-cycle-alignment.md`

### Unimatrix ADR Entry IDs

| ADR | Title | Entry ID |
|-----|-------|----------|
| ADR-001 | Per-Cycle Transaction Granularity for Activity GC Pass | #3915 |
| ADR-002 | max_cycles_per_tick Cap in RetentionConfig, Not InferenceConfig | #3916 |
| ADR-003 | PhaseFreqTable / K-cycle Alignment via Tick-Time Diagnostic Warning | #3917 |

## Key Design Decisions

1. **Per-cycle transactions** (ADR-001 #3915): Each `gc_cycle_activity()` call uses one
   `pool.begin()` transaction. Connection released between cycles. Eliminates SR-01/SR-02
   deadlock risk. CASCADE order within transaction: observations → query_log →
   injection_log → sessions (sessions deleted last so subqueries remain valid).

2. **RetentionConfig owns the batch cap** (ADR-002 #3916): New `[retention]` TOML
   section with three fields: `activity_detail_retention_cycles` (default 50),
   `audit_log_retention_days` (default 180), `max_cycles_per_tick` (default 10).
   Not added to `InferenceConfig` — pure SQL GC has no inference cost and the concerns
   are different operator concerns.

3. **PhaseFreqTable alignment guard** (ADR-003 #3917): Tick-time `tracing::warn!`
   when `oldest_retained_computed_at <= now - query_log_lookback_days * 86400`. Cheap,
   non-breaking, actionable. No config enforcement.

4. **Targeted UPDATE for raw_signals_available** (SR-05 resolution): `mark_signals_purged()`
   uses `UPDATE cycle_review_index SET raw_signals_available = 0 WHERE feature_cycle = ?`
   NOT `store_cycle_review()` INSERT OR REPLACE — avoids clobbering `summary_json`.

5. **Unattributed session guard** (SR-06 resolution): Unattributed sessions with
   `status = Active` (0) are excluded from unconditional prune. Only inactive
   unattributed sessions are deleted.

6. **Both 60-day DELETE sites removed** (AC-01 / SR-04): `status.rs` lines 1372–1384
   and `tools.rs` lines 1630–1642 are deleted entirely, not conditionally guarded.

7. **New store module**: All GC methods live in `unimatrix-store/src/retention.rs` —
   keeping SQL with its table definitions, consistent with `sessions.rs`, `cycle_review_index.rs`.

## Resolved Risks

| Risk | Resolution |
|------|-----------|
| SR-01/SR-02 — write pool deadlock | Per-cycle transactions with connection release (ADR-001) |
| SR-04 — two 60-day DELETE sites | Both sites are explicit removal targets in ARCHITECTURE.md |
| SR-05 — INSERT OR REPLACE clobbers summary_json | Targeted UPDATE in mark_signals_purged() |
| SR-06 — pruning active unattributed sessions | Guard: skip if status = Active (0) |
| SR-07 — PhaseFreqTable truncated window | Tick-time tracing::warn! guard (ADR-003) |
| SR-09 — config re-read inside tick | RetentionConfig loaded once at startup, passed by ref |

## Open Questions

None. All design decisions are resolved.

## Knowledge Stewardship

Stored in Unimatrix:

| Entry ID | Title | Category |
|----------|-------|----------|
| #3915 | Per-Cycle Transaction Granularity for Activity GC Pass | decision |
| #3916 | max_cycles_per_tick Cap in RetentionConfig, Not InferenceConfig | decision |
| #3917 | PhaseFreqTable / K-cycle Alignment via Tick-Time Diagnostic Warning | decision |
