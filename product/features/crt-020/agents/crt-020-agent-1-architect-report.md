# Agent Report: crt-020-agent-1-architect

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/crt-020/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/crt-020/architecture/ADR-001-pair-accumulation-counter-location.md` — Unimatrix #1612
- `/workspaces/unimatrix/product/features/crt-020/architecture/ADR-002-cold-start-batch-cap-and-ordering.md` — Unimatrix #1613
- `/workspaces/unimatrix/product/features/crt-020/architecture/ADR-003-double-count-prevention-strategy.md` — Unimatrix #1614
- `/workspaces/unimatrix/product/features/crt-020/architecture/ADR-004-inline-confidence-recomputation.md` — Unimatrix #1615

## Key Decisions

| ADR | Decision | Unimatrix ID |
|-----|----------|--------------|
| ADR-001 | Pair accumulation counter in dedicated `implicit_unhelpful_pending` table | #1612 |
| ADR-002 | Batch cap 500 sessions/tick, oldest-first (`ended_at ASC`) ordering | #1613 |
| ADR-003 | Double-count prevention via `implicit_votes_applied` flag + Stop hook sets it | #1614 |
| ADR-004 | Inline confidence recomputation (confidence_fn=Some) within implicit vote step | #1615 |

## Critical Implementation Points

1. **Schema v13 adds two things**: `implicit_votes_applied INTEGER NOT NULL DEFAULT 0` on
   `sessions` table AND `CREATE TABLE implicit_unhelpful_pending (entry_id INTEGER PRIMARY KEY,
   pending INTEGER NOT NULL DEFAULT 0)`. Both guarded with idempotent checks in migration.

2. **Stop hook MUST set `implicit_votes_applied = 1`** at session close write time (in
   `uds/listener.rs` at the `insert_session` / `update_session` call). This is the sole
   mechanism preventing double-counting. All session-close code paths must be audited.

3. **TimedOut sessions excluded from SQL filter**: `WHERE status = 1` (Completed only), not
   `status IN (1, 2)`. Zero signal for TimedOut is the resolved decision regardless of outcome
   field value.

4. **Implicit vote step placement in `maintenance_tick`**: runs after `run_maintenance` returns
   (GC has already run), before the next tick's confidence refresh.

5. **Two separate `record_usage_with_confidence` calls**: one for helpful_ids (success sessions),
   one for ready_ids (unhelpful, pair accumulation threshold reached). Never merge both sets into
   a single call.

6. **`access_ids = &[]` for both calls**: background vote application does not bump `access_count`.
   Only `helpful_ids` / `unhelpful_ids` are passed.

## Open Questions for Implementation Team

1. **`apply_implicit_votes` module location**: Recommend `background.rs` as a free function (same
   pattern as `process_auto_quarantine`), but `implicit_votes.rs` in the store crate is also valid.

2. **Injection count dilution cap (SR-05)**: No `IMPLICIT_VOTE_MAX_INJECTIONS_PER_SESSION` cap
   is defined. All distinct injected entries receive votes. Consider adding a cap as a follow-up
   after observing vote volume distribution.

3. **TimedOut session clarification**: Implementation must use `status = 1` (Completed only) in
   the SQL filter, not `status IN (1, 2)`.

## Codebase Findings

- `SessionRecord` currently has 10 fields; v13 adds `implicit_votes_applied: bool` with
  `#[serde(default)]` for backward compat.
- `SESSION_COLUMNS` constant in `sessions.rs` must be updated.
- `session_from_row` in `sessions.rs` must read the new column.
- `run_confidence_consumer` in `listener.rs` uses `SignalType::Helpful` from `signal_queue` —
  completely separate from injection_log. The two paths are provably disjoint.
- `record_usage_with_confidence` silently skips non-existent entry IDs (line 99 in write_ext.rs:
  `if !exists { continue; }`). This handles the case where an entry is deleted between injection
  and tick — no special error handling needed (AC-09).
