# C2 — Store write primitive `insert_cycle_start_with_tags`

> File: `crates/unimatrix-store/src/db.rs` (NEW). BEGIN IMMEDIATE txn: cycle_start event INSERT +
> whole-set-once EXISTS guard (insert full set OR skip).
> Risks: **R-05 (High)**, **R-15 (High)**, R-08 (Med), R-11 (Med), + Security (SQLi). ACs: AC-02b,
> AC-EXTRA-3, AC-01, AC-07, plus store-tier support for AC-02a.
> Signature mirrors HEAD `insert_cycle_event` (db.rs:320): 8-arg `(cycle_id, seq, event_type, phase,
> outcome, next_phase, timestamp, goal)`. `goal_embedding` is NOT written in this INSERT — it stays
> NULL and is populated later by the existing `update_cycle_start_goal_embedding` UPDATE on
> `event_type='cycle_start'`. The produced `cycle_start` row must be byte-identical to what the plain
> `insert_cycle_event` path writes (same columns, `event_type` fixed to `'cycle_start'`).

## Reuse
Store test module in `db.rs`; seed pattern from `goal_clusters.rs` getter tests
(`store.insert_cycle_event(...)` then read back). `count_table`/raw `sqlx::query` on
`store.write_pool_server()` for row-count assertions (retention.rs helpers pattern).

## Atomicity (R-05 / AC-EXTRA-3)
- `test_start_row_and_tag_rows_share_commit` — one `insert_cycle_start_with_tags(fc, …, tags=[A,B])`
  call → assert the `cycle_start` `cycle_events` row AND both `cycle_tags` rows are visible after the
  single call (same commit boundary).
- `test_dup_tag_in_set_no_txn_abort` — `tags=[A, A, B]` in one call → `ON CONFLICT(feature_cycle,
  tag) DO NOTHING` per tag; stored set is exactly `{A, B}`, no txn abort, no error, start row present.
- Atomicity-under-failure: fault-injection test (e.g. inject a failing tag insert and assert the
  start-row insert also rolls back — no half state) OR explicit code-review sign-off recorded in
  RISK-COVERAGE-REPORT.md if fault injection is impractical.

## Whole-set-once (R-08 — store tier; assembled tier is listener-persistence.md)
- `test_whole_set_once_second_call_skips_when_rows_exist` — call with `{A,B}`, then call with `{C}`
  for the same `fc` → stored set EXACTLY `{A,B}` (EXISTS guard skips the whole second write).
- `test_first_call_inserts_full_set` — `{A,B,C}` on empty `fc` → all three present.
- `test_tagless_call_does_not_lock` — call with empty `tags` (should route away; but if the primitive
  is invoked with empty set, assert it writes no cycle_tags rows and does NOT create a lock sentinel).

## Concurrency / TOCTOU (R-15 / AC-02b)
- `test_begin_immediate_used` — assert (source/SQL review or an instrumented pool) that the txn opens
  with `BEGIN IMMEDIATE`, NOT the default deferred `pool.begin()`. Record in coverage report.
- `test_concurrent_same_cycle_starts_one_whole_set` — spawn two `insert_cycle_start_with_tags` calls
  for the same `fc` with `{A,B}` and `{C,D}` concurrently (`tokio::join!`) → assert stored set is
  EXACTLY one of `{A,B}` or `{C,D}`, NEVER a merge `{A,B,C,D}` or partial mix; neither call errors
  or panics.

## Value-opacity (R-11 / AC-01 / AC-07)
- `test_empty_string_tag_rejected_others_stored` — `[ "workflow:v1.3", "", "foo" ]` → empty rejected,
  the other two stored verbatim. (Non-empty check is the ONLY validation.)
- `test_colon_and_bare_stored_identically_no_branching` — `arm:A` and `arm` and `foo` all stored
  verbatim; no namespace derivation, no prefix branching (AC-07).
- `test_large_tag_not_truncated` — a very long tag stored byte-for-byte (no `MAX_GOAL_BYTES`-style
  cap; DoS accepted under Write gate).
- `test_unicode_and_whitespace_only_tag_stored_verbatim` — non-empty whitespace-only tag is stored
  (whitespace-only is non-empty → stored, per FR-2); unicode round-trips.

## Security
- `test_tag_write_uses_parameterized_binds` — assert bound parameters (parity `add_tag`
  write.rs:281); a tag containing SQL metacharacters (`'); DROP …`) is stored verbatim, no injection.
  Parameterization is the ONLY SQLi defense (opacity forbids validation) — load-bearing.
- Assert NO `LIKE`/`like_escape` on the cycle-tag write path (no namespace query ships).
