# Agent Report — crt-054-agent-4-writer (Surface A SERVER half)

**Components**: 8 (server wrapper) + 6 (listener writer). Wave: Surface A server half.
**Scope**: `crates/unimatrix-server/src/services/store_ops.rs` and `crates/unimatrix-server/src/uds/listener.rs` ONLY (plus the new test sibling module).

## Files modified / created
- `crates/unimatrix-server/src/services/store_ops.rs` (modify) — Component 8: `StoreService::insert_compaction_event` wrapper + private `bump_compaction_insert_failed_counter`.
- `crates/unimatrix-server/src/uds/listener.rs` (modify) — Component 6: Surface A writer block after `increment_compaction` at the former `:1854`; plus `mod compaction_events;` test declaration.
- `crates/unimatrix-server/src/uds/listener/tests/compaction_events.rs` (create) — 10 integration tests.

## What was implemented
**Component 8 (store_ops.rs)**: `pub(crate) async fn insert_compaction_event(session_id, compacted_at_secs: i64, high_water: i64) -> Result<(), ServiceError>`. Calls `self.store.insert_compaction_event(...)` (Wave-1 store-level INSERT). On Ok → Ok. On Err → best-effort `bump_compaction_insert_failed_counter()` wrapped in `let _ =` (swallow bump failure, never panic), then return `Err(ServiceError::Core(CoreError::Store(e)))`. The bump acquires a write conn from `store.write_pool_server().acquire()` and calls `counters::increment_counter(&mut conn, COMPACTION_EVENTS_INSERT_FAILED, 1)` (Wave-1 const reused). No new store-crate code; no migration touched.

**Component 6 (listener.rs)**: Immediately after `session_registry.increment_compaction(session_id);`:
- `high_water` captured in a TIGHT block — `lock_buffer(&s.transcript).high_water()` — guard drops at end of that statement BEFORE the INSERT (pattern #3753). Absent `session_state` → `high_water = 0`.
- `compacted_at_secs = unix_now_secs() as i64` (Unix SECONDS, server wall clock; ts/1000 gate is crt-055's).
- Single autocommit INSERT via `services.store_ops.insert_compaction_event(...).await`. NO registry/session/buffer lock held across it.
- On Err: `tracing::warn!(session_id, error, ...)` (ids only, no payload), FALL THROUGH — ACK never blocked. Written regardless of feature_cycle (no feature_cycle read/written).

## Tests (10/10 pass — `cargo test -p unimatrix-server --lib compaction_events`)
Driven through the real `handle_compact_payload` seam via the `transcript.rs` harness (`Deps`/`dispatch_delta`/`dispatch_compact`):
- `test_compaction_writes_one_row` (AC-02) — one row, correct session_id.
- `test_high_water_equals_buffer_high_water` (AC-02/R-13) — row high_water == buffer high_water, non-trivial fixture.
- `test_compacted_at_is_seconds_within_tolerance` (AC-02/R-11, AC-16 producer half) — Unix seconds within [before, after].
- `test_second_compaction_adds_monotonic_row` (AC-02/R-14) — two rows, monotonic ts, insert-only.
- `test_compaction_row_written_for_undeclared_session` (AC-03) — undeclared session still gets a row.
- `test_compaction_events_no_feature_cycle_or_content_column` (AC-03) — PRAGMA: columns are exactly id/session_id/compacted_at/high_water.
- `test_compaction_row_for_absent_session_high_water_zero` (AC-03) — absent session → row written, high_water 0.
- `test_insert_failure_increments_named_counter` (AC-04a/R-15, MANDATORY) — DROP table fault-injection; named counter `compaction_events_insert_failed` increments by EXACTLY 1; ACK completes; no panic.
- `test_insert_failure_counter_is_content_free` (R-15 sc.2) — counter name is the fixed literal, no ids/bytes.
- `test_insert_failure_non_blocking_no_row` (R-15) — incompatible-table fault; ACK completes, no row lands.

Lock-ordering (R-09): documented in the code comment + verified by review — the buffer guard drops before the INSERT. Deadlock-under-contention (AC-04) and the full pre/post-boundary gate classification are deferred to Stage 3c / crt-055 per the brief.

## Quality
- `cargo build -p unimatrix-server` — clean (warnings are pre-existing / sibling components).
- `cargo clippy -p unimatrix-server --tests` — zero warnings attributable to my added lines.
- `cargo fmt` run ONLY on my two files. No `git add`/`commit` (Delivery Leader commits the wave).
- No `unwrap()` in non-test code; `#[derive(Debug)]` not applicable (no new structs in prod code).

## Coordination notes (for Delivery Leader)
- Transient working-tree breakage observed mid-run from concurrent Component 9 (config.rs: `TranscriptSignalsConfig` field/`ConfigError::TooManySignalClasses { path }` test) — self-resolved as that agent completed. No action needed; flagged only as a wave-sequencing observation. I did NOT touch config.rs.
- My writer block placement assumes `unix_now_secs()` (listener.rs, returns u64) and the already-imported `lock_buffer`. The store-level INSERT, the `compaction_events` table (db.rs + migration.rs v28→v29), and `COMPACTION_EVENTS_INSERT_FAILED` were all present from Wave 1 / Component 7.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search -- found ADR-007 (#5050) + pattern #3753 (lock capture-drop) + #4406 (named-counter const) + #3700/#1494 (snapshot-before-spawn lock patterns); applied all.
- Stored: entry #5053 "Best-effort durable named-counter bump on a write-seam wrapper failure path (increment_counter needs &mut conn)" via /uni-store-pattern.
