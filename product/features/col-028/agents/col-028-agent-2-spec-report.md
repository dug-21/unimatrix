# Agent Report: col-028-agent-2-spec

## Output

- SPECIFICATION.md: `product/features/col-028/specification/SPECIFICATION.md`
- Total AC count: **24** (AC-01 through AC-20 from SCOPE.md carried forward unchanged;
  AC-21 through AC-24 added to close SR-01, SR-02, SR-03, SR-04)

## AC Inventory

| AC-ID | Subject | Source |
|-------|---------|--------|
| AC-01 | context_search UsageContext phase | SCOPE.md |
| AC-02 | context_lookup UsageContext phase | SCOPE.md |
| AC-03 | context_get UsageContext phase | SCOPE.md |
| AC-04 | context_briefing UsageContext phase | SCOPE.md |
| AC-05 | context_get access_weight = 2 | SCOPE.md |
| AC-06 | context_briefing access_weight = 0 | SCOPE.md |
| AC-07 | D-01 dedup slot non-consumption | SCOPE.md |
| AC-08 | confirmed_entries field initialised empty | SCOPE.md |
| AC-09 | context_get populates confirmed_entries | SCOPE.md |
| AC-10 | context_lookup single-ID populates, multi-ID does not | SCOPE.md |
| AC-11 | context_lookup weight unchanged at 2 | SCOPE.md |
| AC-12 | Phase snapshot before any await, free function | SCOPE.md |
| AC-13 | CURRENT_SCHEMA_VERSION = 17 | SCOPE.md |
| AC-14 | phase column and index present after migration | SCOPE.md |
| AC-15 | Migration idempotency | SCOPE.md |
| AC-16 | context_search writes phase to query_log | SCOPE.md |
| AC-17 | QueryLogRecord.phase round-trip read-back | SCOPE.md |
| AC-18 | Pre-existing rows read back with phase = None | SCOPE.md |
| AC-19 | migration_v16_to_v17.rs with six sub-tests | SCOPE.md |
| AC-20 | make_state_with_rework updated, all tests pass | SCOPE.md |
| AC-21 | Atomic change surface: INSERT + two SELECTs + deserializer | SR-01 |
| AC-22 | Migration test cascade: migration_v15_to_v16.rs and migration_v14_to_v15.rs audited | SR-02 |
| AC-23 | UDS compile fix: uds/listener.rs:1324 passes None | SR-03 |
| AC-24 | confirmed_entries doc comment states semantic contract | SR-04 |

## Key Decisions Made

**SR-01 atomic change surface** — AC-21 declares that analytics.rs INSERT, both
scan_query_log SELECT statements, and row_to_query_log are a single atomic change unit.
AC-17's read-back round-trip is named the enforcement test. If any of the four is missed,
the integration test fails at runtime with a column-index error.

**SR-02 migration cascade** — Code search confirmed exactly two test files have relevant
assertions: `migration_v15_to_v16.rs` has eight `== 16` assertions that must become 17;
`migration_v14_to_v15.rs` uses `>= 15` guards (pattern #2933-compliant) that already
tolerate bumps — audit only, update comments. No other migration test files contain
`== 16` assertions.

**SR-03 UDS compile-fix** — AC-23 names `uds/listener.rs:1324` explicitly. Source
inspection confirmed the call site is `QueryLogRecord::new(sid, query, &entry_ids, &scores,
"strict", "uds")` — gains a seventh argument `None`.

**eval/scenarios/tests.rs helper** — `insert_query_log_row` is a raw SQL INSERT helper
(not via `QueryLogRecord::new`), so its SQL string must also add `phase` as column and
binding. FR-20 captures this. The `make_query_log` struct literal in
`mcp/knowledge_reuse.rs` uses direct struct construction (not `::new`) and must add
`phase: None` (FR-21).

**SR-07 (D-01 guard location)** — Not promoted to a new AC. Noted in C-03 as out of scope
with a reference to the architect for future ADR consideration.

## Open Questions for Architect

None blocking. Two items for architect awareness:

- **SR-07**: Should the D-01 weight-0 guard move to the `AccessSource` dispatch level for
  structural enforcement? Currently in `record_briefing_usage` per ADR-003 col-028.
  Requires separate ADR if scope is extended.

- **eval/scenarios/tests.rs caller count**: Grep shows 15 `insert_query_log_row` call
  sites. Delivery should re-confirm this count before implementation, as the helper SQL must
  also be updated (it is not a `QueryLogRecord::new` call).

## Knowledge Stewardship

- Queried: /uni-query-patterns for col-028 phase capture, session migration patterns,
  dedup slot, confirmed_entries — found ADR-001 col-028 (#3504), ADR-002 col-028 (#3505),
  ADR-003 col-028 (#3506), pattern #3503 (UsageDedup weight-0 gotcha), pattern #3510
  (shared access_counted set), pattern #2933 (schema version cascade), pattern #3004
  (analytics drain phase-snapshot causal integration test pattern), pattern #3412
  (SessionState in-memory counter pattern). All findings applied directly.
