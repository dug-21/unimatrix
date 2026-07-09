# Agent Report — vnc-047-agent-3-store-core (STORE crate core: C1, C2, C3, C10, C13)

## Summary
Implemented the store-crate core for `context_cycle` whole-set-once run-identity tags:
schema v31 `cycle_tags` junction (3 paths), the `insert_cycle_start_with_tags` write
primitive (BEGIN IMMEDIATE + whole-set-once EXISTS guard, C13 trace inside), the
`get_cycle_tags` getter, and GC protection by omission with an extended regression test.

## SR-02 re-verification (recorded per Gate obligation #6)
At implementation start, HEAD read `CURRENT_SCHEMA_VERSION = 30` (migration.rs:26) — v31
was the next free number. No renumber needed. (SUMMARY_SCHEMA_VERSION cascade is out of my
scope — owned by the C7 agent.)

## Files modified
- `crates/unimatrix-store/src/migration.rs` — bumped `CURRENT_SCHEMA_VERSION` 30→31; added
  the `if current_version < 31` block (CREATE TABLE/INDEX IF NOT EXISTS, no per-block stamp;
  single stamp at txn end). Updated the v29→v30 block's trailing comment (it is no longer the
  last block). Three-path discipline preserved (const + migration step + fresh-create + guard).
- `crates/unimatrix-store/src/db.rs` — added `cycle_tags` DDL to `create_tables_if_needed`
  (beside `entry_tags`); added `insert_cycle_start_with_tags` (BEGIN IMMEDIATE on a dedicated
  connection, byte-identical 8-column cycle_start INSERT with `event_type='cycle_start'` and
  goal_embedding left NULL, EXISTS guard, per-row `ON CONFLICT DO NOTHING`, best-effort
  ROLLBACK on any error, C13 wrote-set/frozen-skip trace after COMMIT); added `get_cycle_tags`
  (`SELECT tag ... ORDER BY tag`, reads write_pool). `insert_cycle_event` UNCHANGED.
- `crates/unimatrix-store/src/retention.rs` — NO change to any DELETE path (protection by
  omission). Extended `test_gc_protected_tables_regression`: seeds 4 `cycle_tags` rows
  (incl. tags on a purged cycle and an unattributed feature_cycle), asserts count unchanged
  after BOTH `gc_cycle_activity` and `gc_unattributed_activity`, with positive controls
  (purgeable-1/-2 sessions and a NULL-feature_cycle session are all purged).
- `crates/unimatrix-store/tests/migration_v30_to_v31.rs` — NEW (pattern from v29_to_v30):
  constant bump, fresh-create (PK + index + columns), migration create, idempotent re-run,
  data-intact from populated v30, stray-table no-error, fresh/migration DDL-parity drift
  guard, no-FK contract.
- `crates/unimatrix-store/tests/cycle_tags.rs` — NEW: C2/C3 store-tier tests (atomicity,
  whole-set-once exact equality across changed/subset/superset/single, tagless-does-not-lock,
  concurrency, value-opacity, SQLi/parameterized binds, getter sorted/empty/scoped/verbatim).
- `crates/unimatrix-store/tests/sqlite_parity.rs` — updated the pinned hygiene test
  `test_schema_version_is_30` → `test_schema_version_is_31` (exact-equality pin; this is THE
  C1 pinned schema-version test my task named). In-crate, in scope for C1.

## Tests — pass/fail
- Store crate, `cargo test -p unimatrix-store --features test-support`: **rc=0, all pass**
  (0 failed). New: cycle_tags.rs 16/16, migration_v30_to_v31.rs 8/8, sqlite_parity
  test_schema_version_is_31 pass, retention test_gc_protected_tables_regression pass.
- `cargo build --workspace`: passes. `cargo clippy -p unimatrix-store --tests
  --features test-support`: no warnings. `cargo fmt` applied.

## BEGIN IMMEDIATE verification (R-15)
Verified by source review: the write primitive acquires ONE dedicated connection and issues
`BEGIN IMMEDIATE` (not `pool.begin()`), running all statements on that connection — the
pattern proven at unimatrix-server/src/import/mod.rs:196. Functional guarantee proven by
`test_concurrent_same_cycle_starts_one_whole_set` (two same-FC starts → exactly one intact
whole set, no merge, neither errors). The write pool's 10s busy_timeout serializes the loser.

## Out-of-scope breakage FLAGGED (not fixed — different crate)
`crates/unimatrix-server/tests/verify_integration.rs:413` `test_schema_version_still_30`
(nxs-014) asserts `CURRENT_SCHEMA_VERSION == 30` EXACTLY. My 30→31 bump makes it fail at
runtime (it still compiles, so `cargo build --workspace` is green). It is in unimatrix-server,
not in my file scope and not in the IMPLEMENTATION-BRIEF Files-to-Modify list. **Action needed
by the server-crate agent or leader:** rename to `test_schema_version_still_31` (or relax to
`>= 31`) and update the asserted value/message. Until then `cargo test --workspace` has this
one runtime failure. See stored pattern #5662 for the cross-crate blast-radius rule.

## Notes / non-blocking
- C13 freeze trace is emitted INSIDE the store method (per freeze-trace.md placement
  decision, to preserve the fixed `Result<()>` signature). NON-GATING; not asserted by a
  log-capture test — the two `tracing::info!` lines (wrote-set / frozen-skip) are code-review
  confirmed, per test plan allowance.
- Atomicity-under-failure (fault injection) is code-review signed off: the only realistic
  mid-txn failure is a genuine DB/IO error, and every error arm does a best-effort ROLLBACK
  before returning, so a failed tag insert rolls back the start row (no half state). No
  practical injection hook exists at the store tier.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #373 (junction-table pattern), #5651
  (ADR-001 cycle_tags source of truth), #5599 (vnc-045 entry_tags primitive), #4178
  (cycle_review_index vs cycle_events), #4457 (entries has no tags column). Applied: junction
  model, re-key entry_id→feature_cycle with NO FK, parameterized-bind opacity.
- Stored: entry #5662 "CURRENT_SCHEMA_VERSION bump has a cross-crate pinned test in
  unimatrix-server" via context_store (pattern, topic unimatrix-store) — captures the
  cross-crate schema-pin blast-radius gap that this feature hit (verify_integration.rs).
