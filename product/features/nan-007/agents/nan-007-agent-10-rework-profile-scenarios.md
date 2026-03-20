# Agent Report: nan-007-agent-10-rework-profile-scenarios

## Task
Gate 3b REWORKABLE FAIL — fix two issues in eval/profile.rs and eval/scenarios.rs.

## Issue 1: SqlxStore::open() on snapshot (FR-24, C-02)

**Root cause analysis**: `VectorIndex::new()` accepts `Arc<SqlxStore>` as a concrete type (not a trait object). The original implementer used `SqlxStore::open()` on the snapshot because they could not pass a raw `SqlitePool` to `VectorIndex::new()`. This was documented as OQ-A resolution.

**Solution chosen**: Added `SqlxStore::open_readonly(path)` constructor to `unimatrix-store/src/db.rs`.

This constructor:
- Opens a read-only pool only (no write pool, shares read pool clone for `write_pool_server()` callers)
- Runs no migrations
- Spawns no drain task
- Creates an analytics channel but immediately drops the receiver, making all `enqueue_analytics` calls hit the `Closed` branch (silent no-op)
- Returns a `SqlxStore` that satisfies `Arc<SqlxStore>` for `VectorIndex::new()`

`EvalServiceLayer::from_profile()` now calls `SqlxStore::open_readonly()` instead of `SqlxStore::open()`.

**Verification**: `grep -r "SqlxStore::open\b" crates/unimatrix-server/src/eval/` finds only test helpers (which correctly use `open()` to create pre-migrated test snapshots) and comments. Zero production uses.

## Issue 2: File line limits

Split both oversized files into submodule trees following the `eval/report/` pattern:

### eval/profile/ (was 1031 lines)
- `mod.rs` — 25 lines
- `types.rs` — 64 lines (AnalyticsMode, EvalProfile)
- `error.rs` — 85 lines (EvalError + Display + Error)
- `validation.rs` — 109 lines (validate_confidence_weights, parse_profile_toml)
- `layer.rs` — 262 lines (EvalServiceLayer struct + impl)
- `tests.rs` — 443 lines (all #[cfg(test)] content)

### eval/scenarios/ (was 900 lines)
- `mod.rs` — 20 lines
- `types.rs` — 83 lines (ScenarioSource, ScenarioRecord, ScenarioContext, ScenarioBaseline)
- `extract.rs` — 93 lines (build_scenario_record)
- `output.rs` — 148 lines (run_scenarios, do_scenarios)
- `tests.rs` — 462 lines (all #[cfg(test)] content)

All files under 500 lines. `eval/mod.rs` unchanged — Rust resolves `pub mod profile` to `eval/profile/mod.rs` automatically.

## Tests

`cargo test -p unimatrix-server --lib`: **1588 passed, 0 failed**

`cargo build --workspace`: Clean (6 pre-existing warnings, no errors)

`cargo test --workspace`: 1 pre-existing doctest failure in `infra/config.rs line 21` (documented in gate-3b-report.md, unrelated to nan-007).

## Files Created/Modified

Modified:
- `/workspaces/unimatrix/crates/unimatrix-store/src/db.rs` — added `SqlxStore::open_readonly()`

Deleted (replaced by submodule trees):
- `crates/unimatrix-server/src/eval/profile.rs`
- `crates/unimatrix-server/src/eval/scenarios.rs`

Created — eval/profile/:
- `crates/unimatrix-server/src/eval/profile/mod.rs`
- `crates/unimatrix-server/src/eval/profile/types.rs`
- `crates/unimatrix-server/src/eval/profile/error.rs`
- `crates/unimatrix-server/src/eval/profile/validation.rs`
- `crates/unimatrix-server/src/eval/profile/layer.rs`
- `crates/unimatrix-server/src/eval/profile/tests.rs`

Created — eval/scenarios/:
- `crates/unimatrix-server/src/eval/scenarios/mod.rs`
- `crates/unimatrix-server/src/eval/scenarios/types.rs`
- `crates/unimatrix-server/src/eval/scenarios/extract.rs`
- `crates/unimatrix-server/src/eval/scenarios/output.rs`
- `crates/unimatrix-server/src/eval/scenarios/tests.rs`

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "SqlxStore open readonly snapshot eval no migration" -- found entry #2607 (Store = SqlxStore type alias pattern) confirming VectorIndex/AuditLog accept Arc<SqlxStore> directly. No existing pattern for open_readonly approach.
- Stored: entry via /uni-store-pattern (see below) — `SqlxStore::open_readonly()` for eval snapshot access: open pool without migrations or drain task.
