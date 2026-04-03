# Agent Report: crt-043-agent-3-schema-migration

## Summary

Implemented the schema-migration component for crt-043 (Behavioral Signal Infrastructure).

## Files Modified

- `crates/unimatrix-store/src/embedding.rs` — CREATED. `pub encode_goal_embedding` / `pub decode_goal_embedding` helpers using bincode v2 `config::standard()`. Visibility promoted to `pub` (not `pub(crate)`) per OVERVIEW.md WARN-2 resolution: `encode_goal_embedding` must be callable cross-crate from `unimatrix-server`.
- `crates/unimatrix-store/src/lib.rs` — Added `pub mod embedding` and re-exported `encode_goal_embedding` / `decode_goal_embedding`.
- `crates/unimatrix-store/src/migration.rs` — Bumped `CURRENT_SCHEMA_VERSION` from 20 to 21. Added `current_version < 21` block: two `pragma_table_info` pre-checks, two conditional `ALTER TABLE` statements, one `CREATE INDEX IF NOT EXISTS` for composite index `idx_observations_topic_phase`, and in-transaction `UPDATE counters SET value = 21`.
- `crates/unimatrix-store/src/db.rs` — Added `SqlxStore::update_cycle_start_goal_embedding(cycle_id, bytes)` method. Updated `create_tables_if_needed` DDL: `cycle_events` gains `goal_embedding BLOB`, `observations` gains `phase TEXT`, new composite index `idx_observations_topic_phase` added.
- `crates/unimatrix-store/tests/migration_v20_v21.rs` — CREATED. 8 tests: MIG-V21-U-01 through MIG-V21-U-06, STORE-U-01, STORE-U-02.
- `crates/unimatrix-store/tests/migration_v19_v20.rs` — Updated 5 hardcoded version assertions from 20 to 21 (a v19 database now migrates all the way to v21 through the chain).
- `crates/unimatrix-store/tests/sqlite_parity.rs` — Updated `test_schema_version_is_14` assertion from 20 to 21.

## Tests

- Unit tests (lib): 232 passed, 0 failed
- Integration tests (all, `--features test-support`): 409 passed, 0 failed

## Issues / Notable Decisions

**WARN-2 resolved:** `encode_goal_embedding` and `decode_goal_embedding` are `pub` (not `pub(crate)`) and re-exported from `lib.rs`. This is required because `unimatrix-server` calls `encode_goal_embedding` from the fire-and-forget embedding spawn in Wave 2 (goal-embedding component). `pub(crate)` would be inaccessible across crate boundaries.

**FR-C-07 resolved:** Composite index `idx_observations_topic_phase ON observations (topic_signal, phase)` added in the v21 migration. Justification: Group 6 S6/S7 phase-stratification queries will filter on both columns; the observations table grows continuously; adding the index at migration time costs microseconds and prevents a full-table-scan regression at Group 6 ship time.

**Whitespace-only goal:** Resolved in pseudocode (OVERVIEW.md) — trimmed before check in Wave 2 goal-embedding component. Not in scope for this component.

**Malformed bytes test:** The test plan specified `[0x00, 0xFF, 0x42, 0x13, 0x37]` as the malformed input. In bincode v2 standard config, `0x00` is a valid varint for length=0 (empty vec) — this input decodes successfully. Changed to `[0x0A, 0x01, 0x02, 0x03, 0x04]` where `0x0A` = varint 10, claiming 40 bytes but only 4 follow — bincode returns `UnexpectedEnd`. Added comment documenting the encoding format.

**Migration chain effect:** Bumping CURRENT_SCHEMA_VERSION to 21 causes any v19 database opened by the current `Store::open()` to be migrated all the way to v21 (both v20 and v21 blocks run in sequence). Updated migration_v19_v20.rs and sqlite_parity.rs accordingly.

**insert_observation / insert_observations_batch:** These are private functions in `unimatrix-server/src/uds/listener.rs`, not in `db.rs`. The pseudocode confirms this. Their modification (adding `phase` bind) is Wave 2 work for the goal-embedding/phase-capture agent. Only the `db.rs` DDL (`create_tables_if_needed`) was updated here.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — Unimatrix MCP server unavailable at agent spawn time. Read ADR files and pseudocode directly from product/features/crt-043/.
- Stored: nothing via `/uni-store-pattern` — Unimatrix MCP tools unavailable in this agent context. Pattern documented here for manual store or retro pick-up.

**Pattern (for manual store, topic: unimatrix-store):**

What: In bincode v2 standard config, `[0x00, ...]` is a valid Vec<f32> encoding (empty vec, varint 0 = length 0). Test plans using `[0x00, 0xFF, ...]` as "malformed" bytes will get `Ok(vec![])` not `DecodeError`.

Why: Breaks negative tests for decode helpers — the test asserts `is_err()` but bincode succeeds silently. Discovery cost: one test failure, investigation of varint encoding required.

Scope: unimatrix-store embedding.rs, any bincode v2 Vec<T> decode test. Fix: use a varint that claims more elements than bytes remain, e.g. `[0x0A, 0x01, 0x02, 0x03, 0x04]` (varint 10 → 40 bytes, only 4 follow → UnexpectedEnd).
