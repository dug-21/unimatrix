# Agent Report: dsn-001-agent-4-confidence-params

## Task
Extend `ConfidenceParams` in `crates/unimatrix-engine/src/confidence.rs` from 3 fields to 9 fields and migrate all call sites.

## Files Modified

- `crates/unimatrix-engine/src/confidence.rs` — Added `ConfidenceParams` struct (9 fields), `Default` impl, updated `freshness_score` and `compute_confidence` signatures, migrated all internal test call sites, added 7 new dsn-001 test cases
- `crates/unimatrix-engine/src/test_scenarios.rs` — Updated `assert_confidence_ordering` to use `&ConfidenceParams::default()`
- `crates/unimatrix-engine/tests/pipeline_calibration.rs` — Migrated all `compute_confidence` and `freshness_score` call sites
- `crates/unimatrix-engine/tests/pipeline_regression.rs` — Migrated all `compute_confidence` call sites
- `crates/unimatrix-engine/tests/test_scenarios_unit.rs` — Migrated `compute_confidence` call sites
- `crates/unimatrix-server/src/services/confidence.rs` — Migrated `compute_confidence`, removed dead `alpha0/beta0` snapshot block
- `crates/unimatrix-server/src/services/usage.rs` — Migrated 2 `compute_confidence` call sites, removed 2 dead `alpha0/beta0` snapshot blocks
- `crates/unimatrix-server/src/services/status.rs` — Migrated production call + test calls (including `alpha0: 8.0, beta0: 2.0` test to struct update syntax), removed dead snapshot block
- `crates/unimatrix-server/src/server.rs` — Migrated 7 `compute_confidence` call sites
- `crates/unimatrix-server/src/mcp/tools.rs` — Migrated 1 `compute_confidence` call site

## Tests

**unimatrix-engine: 300 passed, 0 failed** (across 5 test binaries + doc tests)
- lib tests: 270 passed
- pipeline_calibration: 14 passed
- pipeline_regression: 3 passed
- pipeline_retrieval: 6 passed
- test_scenarios_unit: 7 passed

## Build Status

`cargo build --workspace` — zero errors, zero new warnings (two pre-existing unused imports in other modules unaffected).

## Issues

None. All 13 workspace test failures are pre-existing:
- `import::tests` + `mcp::identity::tests` + `uds::listener::tests`: pool timeout issues (pre-existing, tracked as #303)
- `infra::config::tests::test_empty_categories_documented_behavior`: in `config.rs` created by another dsn-001 agent (not in my scope)

## Deviations from Pseudocode

None. Implementation follows ADR-001 and `pseudocode/confidence-params.md` exactly.

## Key Implementation Notes

1. `ConfidenceParams` struct is new — did not exist before; pseudocode described extending from "3 fields" but the actual pre-dsn-001 struct was never created. Implementation created it from scratch as specified.
2. All alpha0/beta0 snapshot blocks removed from server callers — these were dead code after migration and produced unused-variable warnings.
3. Static audit confirms no compiled weight constants (`W_BASE`, `W_FRESH`, etc.) appear in `compute_confidence` or `freshness_score` function bodies — only in `Default::default()` impl and test assertions.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` (via `context_search`) for `unimatrix-engine` confidence scoring weights — found ADR-001 entry #2284 confirming the design. No conflicting patterns.
- Stored: entry #2324 "ConfidenceParams migration: remove alpha0/beta0 snapshot blocks from callers" via `/uni-store-pattern` — non-obvious: the FM-03 alpha0/beta0 snapshot pattern must be removed (not prefixed with `_`) when migrating callers to use `ConfidenceParams::default()`, or unused-variable warnings accumulate.
