# crt-020 Researcher Agent Report

## Summary

SCOPE.md written to `/workspaces/unimatrix/product/features/crt-020/SCOPE.md`.

Explored: injection_log, sessions, signal path, write_ext.rs, usage.rs, background.rs,
session.rs, confidence.rs, crt-018 SCOPE, crt-019 SCOPE, schema.rs, migration.rs,
usage_dedup.rs, signal.rs.

## Key Findings

### infrastructure already exists

Every piece of data needed is present in the live SQLite schema (v12):
- `injection_log`: indexed by `session_id` and `entry_id`, batch-readable via
  `scan_injection_log_by_sessions` (already chunks 50 IDs)
- `sessions.outcome`: "success" / "rework" / "abandoned", set at Stop hook
- `helpful_count` / `unhelpful_count`: writable via `record_usage_with_confidence`, which
  also recomputes confidence inline using the crt-019 Bayesian prior
- Background tick: 15-minute maintenance loop already exists in `background.rs`;
  `maintenance_tick` is the correct insertion point

### what's actually missing

One thing and one decision:

1. **Persistent dedup**: There is no way to track which sessions have already received
   implicit votes. `UsageDedup` is in-memory and scoped to a single server instance.
   The recommended solution is a new `implicit_votes_applied` column on `sessions`
   (schema migration v12 → v13), with a covering index. This is a 2-line DDL change.

2. **Half-weight integer problem**: `unhelpful_count` is `u32`. The spec says 0.5
   implicit unhelpful vote for rework/abandoned — this cannot be stored fractionally.
   Multiple implementation strategies are viable; human decision required (see Open
   Questions in SCOPE.md).

### existing signal_queue interaction is the critical design risk

The existing `run_confidence_consumer` path (Stop hook → signal_queue → drain →
`record_usage_with_confidence`) already applies helpful votes for some sessions. If
the implicit vote tick processes the same sessions without coordination, entries will
receive double votes. The flag-on-sessions approach addresses this at the session level,
but the real-time path does not currently set the flag. This requires either:
- The Stop hook handler setting `implicit_votes_applied = 1` when it drains signals, OR
- Accepting at-most-2x votes (one real-time + one background) for sessions that close
  via Stop hook

This is Open Question 5 in SCOPE.md and requires human input before design.

### scope boundary is tight

The feature is well-bounded. No retrieval changes, no new MCP tools, no new entries table
columns. The only schema change is the sessions table flag. The entire implementation fits
in three locations:
- `crates/unimatrix-store/src/migration.rs` (v13 migration)
- `crates/unimatrix-store/src/sessions.rs` (SessionRecord struct + filter query)
- `crates/unimatrix-server/src/background.rs` (implicit vote step in maintenance_tick)

Plus a new store method (either in `write_ext.rs` or a new `implicit_votes.rs`).

### crt-019 dependency confirmed

`ConfidenceStateHandle` (with `alpha0`, `beta0`) is already threaded through
`maintenance_tick` via `spawn_background_tick`. The snapshot-before-spawn_blocking pattern
is established in `UsageService::record_mcp_usage`. No architecture change needed.

## Open Questions (Human Input Required)

1. **Half-weight for rework/abandoned**: Conservative default (0 unhelpful votes, success-only)
   vs probabilistic 50% vs pair-accumulation. Recommend starting with option F (conservative)
   to keep v1 simple.

2. **Cold-start processing**: Is the `IMPLICIT_VOTE_BATCH_LIMIT = 500` cap acceptable for
   first-upgrade scenarios where thousands of historical sessions exist?

3. **Abandoned sessions signal**: Should `outcome = "abandoned"` receive an unhelpful vote,
   or zero signal? Abandoned sessions are explicitly excluded from retrospective metrics.

4. **TimedOut sessions**: `status = TimedOut` has `outcome = NULL`. Exclude entirely
   (recommended) or assign a default outcome?

5. **Real-time path coordination**: Should the Stop hook handler set
   `implicit_votes_applied = 1` when it processes signals, to prevent the background tick
   from double-counting?

## Knowledge Stewardship

- Queried: /uni-query-patterns for "implicit feedback injection log session outcomes background
  tick" -- no results (Unimatrix MCP tools not reachable in this worktree environment)
- Stored: nothing stored -- Unimatrix MCP tools not reachable; pattern documented in
  SCOPE.md Background Research section ("sessions table flag pattern for persistent dedup
  in background tick operations")
