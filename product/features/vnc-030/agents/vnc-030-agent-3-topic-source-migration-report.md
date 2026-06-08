# Agent Report — vnc-030-agent-3-topic-source-migration

## Task
Extend `crates/unimatrix-store/src/migration.rs` per ADR-005: add v27→v28
pragma-guarded idempotent ALTER adding `observations.topic_source TEXT NULL`,
bump `CURRENT_SCHEMA_VERSION = 28`, no backfill.

## Files Modified
- `crates/unimatrix-store/src/migration.rs` — `CURRENT_SCHEMA_VERSION = 28`; new
  `if current_version < 28` block (pragma_table_info pre-check → guarded ALTER),
  inserted as the LAST block before the final stamp; the final
  `INSERT OR REPLACE ... schema_version` stamps 28 in the single transaction.
- `crates/unimatrix-store/src/db.rs` — fresh-DB `observations` DDL gains
  `topic_source TEXT` (create_tables_if_needed path; #4153 three-paths rule).
- `crates/unimatrix-store/tests/sqlite_parity.rs` — `test_schema_version_is_27`
  → `test_schema_version_is_28` (exact assertion bumped).
- `crates/unimatrix-store/tests/migration_v26_to_v27.rs` — body-level
  `assert_eq!(version, 27)` → `assert!(version >= 27)` (cascade fix, #4373/#4153).
- `crates/unimatrix-store/tests/migration_v27_to_v28.rs` — NEW test family.

## Tests
- `cargo test -p unimatrix-store --features test-support`: all pass, 0 failed.
- New `migration_v27_to_v28` file: 5/5 pass — fresh DB (column present, TEXT,
  nullable, v28), already-migrated re-run (pragma-guarded no-op, no duplicate),
  pre-migration v27→v28 (column added, stamped 28), no-backfill (existing rows
  NULL), version constant >= 28.
- `cargo test --workspace --features test-support`: 0 failures (no cross-crate
  cascade breakage; server.rs has no schema-version assertions).
- `cargo clippy -p unimatrix-store --features test-support --tests`: new file is
  warning-clean (one `assertions_on_constants` on the version-constant test
  suppressed with a targeted `#[allow]`; pre-existing warnings in older
  migration test files untouched).
- `cargo fmt` applied.

## Issues / Blockers
- **R-11 version-collision check: PASS.** Pre-edit `CURRENT_SCHEMA_VERSION` was 27
  on rebased main; no other landed migration claims v28 (grep across `crates/`
  and `tests/` for `current_version < 28` / `value = 28` / `version, 28` found
  nothing). v28 is unique; the new block is the last `if current_version < N`
  before the final stamp.
- C-07 satisfied: the single pragma check precedes the only ALTER ("all checks
  before any ALTER" — trivially true for a single-column block); version stamp
  lands once at the end of `run_main_migrations` in one transaction.
- No intra-block `UPDATE counters SET value = 28` added (correct — last block;
  final stamp suffices). Inline comment flags that a future v29 block must add it.
- Scope discipline: committed only my 5 store-crate files; other agents'
  in-flight changes (server, engine bindings) left unstaged.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search — surfaced #4092
  (pragma-guarded idempotent ALTER, multi-column ordering rule), #4373 (schema
  version cascade checklist), #4153 (three-paths + previous-migration-test
  >= predicate), ADR-005 #4817. All directly applied.
- Stored: nothing novel to store — this migration is the twice-proven v9→v10
  topic_signal pattern under #4092, and every cascade surface I touched
  (db.rs fresh DDL, sqlite_parity version assertion, prior migration test
  exact→>= assertion) is already enumerated in #4373/#4153. No new gotcha,
  trap, or runtime-invisible failure mode emerged.
