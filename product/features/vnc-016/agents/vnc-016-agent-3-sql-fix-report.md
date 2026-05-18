# Agent Report: vnc-016-agent-3-sql-fix

## Task

Fix the SQLite column-name bug in `query_stale_prerequisite_edges_for_cycle` in
`crates/unimatrix-store/src/read.rs`. Single-token change: `fe.feature_cycle` → `fe.feature_id`
in the WHERE clause on line 1618.

## Files Modified

- `crates/unimatrix-store/src/read.rs` — line 1618: `fe.feature_cycle = ?1` changed to `fe.feature_id = ?1`

## Verification Steps Completed

1. Confirmed `feature_entries` schema in `db.rs:616-621`: column is `feature_id TEXT NOT NULL`, not `feature_cycle`.
2. Located exact line 1618 in `read.rs` — the broken WHERE clause was present as documented.
3. Applied single-token fix.
4. Build: `cargo build -p unimatrix-store` — PASS (Finished dev profile, 0.10s, no errors).
5. Grep check: `grep -n 'fe\.feature_cycle' read.rs` — no results (bug removed).

## Test Results

- Build: PASS (zero errors, zero warnings)
- No Rust unit tests added — per spawn instructions, that is Wave 2 Component 2 (rust-unit-test agent)

## Issues / Blockers

None. Fix was clean. The bug was exactly as described: `feature_entries` has `feature_id` as its cycle column; the query used `feature_cycle` (the Rust parameter name), which does not exist in the schema. SQLite raised "no such column: fe.feature_cycle" at runtime, silently swallowed by `unwrap_or_else` in `tools.rs:2169`, producing a false-negative empty result.

## Commit

`4203bf8a` — `impl(sql-fix): fix fe.feature_cycle → fe.feature_id column name in query_stale_prerequisite_edges_for_cycle (#600)`

Branch: `feature/vnc-016`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #4445 surfaced directly: "When a query function references a column by the wrong name (e.g., fe.feature_cycle instead of fe.feature_id), SQLite throws a runtime error..." — this pattern was already known and pre-stored. No new discovery.
- Stored: nothing novel to store — the specific gotcha (wrong column name in this query) is already captured in entry #4445. The fix itself is a one-token correction with no reusable pattern beyond what is already stored.
