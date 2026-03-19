# Agent Report: crt-021-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Feature: crt-021 (W1-1 Typed Relationship Graph)

## Output Files

- `/workspaces/unimatrix/product/features/crt-021/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-021/test-plan/engine-types.md`
- `/workspaces/unimatrix/product/features/crt-021/test-plan/store-schema.md`
- `/workspaces/unimatrix/product/features/crt-021/test-plan/store-migration.md`
- `/workspaces/unimatrix/product/features/crt-021/test-plan/store-analytics.md`
- `/workspaces/unimatrix/product/features/crt-021/test-plan/server-state.md`
- `/workspaces/unimatrix/product/features/crt-021/test-plan/background-tick.md`

## Risk Coverage Summary

| Risk ID | Priority | Coverage | Component File |
|---------|----------|----------|---------------|
| R-01 | Critical | Full — 34 existing tests + explicit typed-graph invocation | engine-types.md |
| R-02 | Critical | Full — mixed-type regression + `edges_of_type` filter tests + grep gate | engine-types.md |
| R-03 | Critical | Full — structural exclusion tests in `build_typed_relation_graph` | engine-types.md |
| R-04 | High | Full — orphaned-edge absent-from-rebuilt-graph test + sequential ordering review | background-tick.md |
| R-05 | High | Full — `new_handle()` use_fallback=true unit test | server-state.md |
| R-06 | Critical | Full — mandatory empty co_access test; mixed-threshold test | store-migration.md |
| R-07 | High | Full — NaN/Inf/Neg-Inf unit tests + drain integration test | store-analytics.md |
| R-08 | Med | Full — double-run idempotency test | store-migration.md |
| R-09 | High | CI gate — `SQLX_OFFLINE=true cargo build` | store-schema.md, OVERVIEW |
| R-10 | Med | Full — `from_str` unknown-string unit test + build_typed_graph silent drop test | engine-types.md |
| R-11 | High | Full — tick compaction timing test with 1000-row synthetic table | background-tick.md |
| R-12 | Med | Full — Supersedes edge from entries.supersedes authority test | engine-types.md |
| R-13 | Low | Code inspection only (accepted documented risk) | store-migration.md |
| R-14 | Med | Compile-time enforcement — grep gate + clean build | server-state.md |
| R-15 | Med | Full — weight value assertion (0.6 vs 1.0 for count=3 vs count=5) | store-migration.md |

## Integration Suite Plan

- **MANDATORY gate**: infra-001 `smoke` suite — all existing tests must pass unchanged
- **Regression coverage**: `tools`, `lifecycle`, `confidence` suites — search path behavior
  semantically unchanged, typed graph is internal infrastructure
- **No new infra-001 tests needed**: GRAPH_EDGES is not MCP-visible; migration tests
  live in `migration.rs`; analytics drain tests live in `analytics.rs`

## Alignment with Architecture Variances

- **VARIANCE 1** (Supersedes edge direction): Test AC-06 assertion uses
  `source_id = entry.supersedes` (old), `target_id = entry.id` (new) — architecture SQL governs
- **VARIANCE 2** (TypedGraphState holds pre-built graph): `server-state.md` tests assert
  `typed_graph: TypedRelationGraph` field exists and no per-query rebuild occurs; spec FR-22 governs

## Open Questions

1. **Tick trigger in test**: background-tick tests require either a direct call to the
   compaction + rebuild logic or a test-exposed `run_tick_once` helper. The implementer
   must expose one of these under `#[cfg(test)]`. If `background.rs` compaction and
   `TypedGraphState::rebuild` are private functions, the test-helper approach is needed.

2. **Drain task test access**: store-analytics drain tests assume the drain can be triggered
   synchronously in test context. If the drain task runs as an infinite loop, the implementer
   must expose a `drain_pending_for_test()` function or equivalent. Check the existing
   analytics drain test pattern before implementing.

3. **R-11 threshold**: the tick compaction timing threshold (1 second for 1000 rows) is
   an estimate. Stage 3c should measure actual baseline on CI hardware and adjust if needed.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for testing procedures — found entry #1607 (SupersessionGraph
  pattern, directly applicable as before/after baseline), entry #2403 (typed graph upgrade
  path), entry #378 (migration tests must include old-schema databases, confirms store-migration
  approach), entry #2271 (SqlxStore test setup with PoolConfig::test_default())
- Queried: /uni-knowledge-search for supersession graph penalty test patterns — found #1605
  (ADR-005 cycle detection fallback, directly covered in engine-types.md cycle detection tests)
- Stored: entry #2428 "Migration test pattern: window function weight normalization with
  empty-table guard (R-06 pattern)" via /uni-store-pattern — novel cross-feature pattern
  for testing migrations that use window functions with NULL-guard COALESCE on potentially
  empty source tables
