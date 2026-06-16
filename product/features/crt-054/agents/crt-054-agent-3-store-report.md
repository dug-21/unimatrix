# Agent Report — crt-054-agent-3-store

**Scope**: Surface A STORE-CRATE spine — Component 7 (compaction_events table + migration) and Component 8 store-level half (raw INSERT helper + failure-counter const) in `unimatrix-store` ONLY. No `unimatrix-server` files touched.

**Schema version chosen**: 29 (confirmed: current = 28; crt-055 not merged, so crt-054 takes 29; verified via `grep CURRENT_SCHEMA_VERSION migration.rs` — lesson #4095).

## Files modified
- `crates/unimatrix-store/src/migration.rs` — bumped `CURRENT_SCHEMA_VERSION` 28→29; added v28-block intra-stamp (`UPDATE counters SET value = 28`); added guarded `if current_version < 29` block creating `compaction_events` + `idx_compaction_events_session` (CREATE TABLE/INDEX IF NOT EXISTS, idempotent). Final INSERT OR REPLACE stamp binds the const, so it re-stamps 29 automatically.
- `crates/unimatrix-store/src/db.rs` — added byte-identical `compaction_events` table + index to `create_tables_if_needed` fresh-create path.
- `crates/unimatrix-store/src/counters.rs` — added `pub const COMPACTION_EVENTS_INSERT_FAILED: &str = "compaction_events_insert_failed";` (const only; the failure-bump itself is the server-side wrapper's job).
- `crates/unimatrix-store/src/write_ext.rs` — added `pub async fn insert_compaction_event(&self, session_id, compacted_at_secs, high_water) -> Result<()>`: single autocommit parameterized INSERT (`?1/?2/?3`) on `write_pool`, id omitted (auto rowid), no explicit transaction.
- `crates/unimatrix-store/tests/sqlite_parity.rs` — bumped `test_schema_version_is_28` → `test_schema_version_is_29` (hard `assert_eq!`); added fresh-create existence + column-contract + session-index tests, helper happy-path/autocommit/parameterized-no-injection round-trips, and the AC-01a "Unix SECONDS" dual-path comment grep (via `include_str!`).

## Files created
- `crates/unimatrix-store/tests/migration_v28_to_v29.rs` — v28→v29 upgrade tests: adds-table+index+stamp, idempotent re-open, column-contract match, `CURRENT_SCHEMA_VERSION >= 29` const guard. Minimal v28 DB (entries gates migration + counters@28; lower-N blocks skipped).

## DDL (identical in both paths, AC-01a)
```
compaction_events: id INTEGER PRIMARY KEY, session_id TEXT NOT NULL,
                   compacted_at INTEGER NOT NULL,   -- Unix SECONDS (NOT millis)
                   high_water INTEGER NOT NULL DEFAULT 0
index: idx_compaction_events_session ON compaction_events(session_id)
```

## Tests
- `cargo test -p unimatrix-store --features test-support`: **PASS** — all suites green (lib 344, sqlite_parity +new compaction tests, migration_v28_to_v29 4 tests). 0 failed across all binaries.
- Build: `cargo build -p unimatrix-store` clean.
- Clippy: no new warnings from this change (the 5 pre-existing lib-test warnings and the older migration-test const-assertion warnings are not mine; my new const-assert test carries `#[allow(clippy::assertions_on_constants)]` matching convention).

## Out-of-scope confirmed untouched (AC-15)
- `SUMMARY_SCHEMA_VERSION`, `cycle_review_index`, all other tables — not modified.
- No `unimatrix-server` files (store_ops wrapper + listener writer are other agents).

## Issues / blockers
- None. Merge-order note for SM: if crt-055 merges first claiming 29, crt-054 retroactively moves to 30 (const + v29 guard + intra-stamp target + `test_schema_version_is_29` + `migration_v28_to_v29.rs` name/asserts in one change).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search(category=pattern)` — surfaced #4373 (schema-version cascade checklist), #4092/#4398 (counters upsert), #2149 (test-support activation). Applied #4373's cascade touchpoints and the existing single-statement autocommit convention (`update_confidence` style `.execute(&self.write_pool)`).
- Stored: entry #5052 "Appending a new last migration block requires an intra-stamp on the now-prior block" via `/uni-store-pattern` (Prerequisite edge → #4373). Novel gotcha not covered by #4373's test-cascade scope: the intra-stamp ordering requirement on the previously-last migration block.
