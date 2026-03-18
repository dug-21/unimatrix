# Agent Report: nxs-011-agent-2-testplan (Stage 3a)

**Phase**: Test Plan Design (Stage 3a)
**Feature**: nxs-011 — sqlx Migration, Connection Pools, Async-Native Storage
**Date**: 2026-03-17

---

## Deliverables Produced

| File | Lines | Purpose |
|------|-------|---------|
| `test-plan/OVERVIEW.md` | ~120 | Overall strategy, risk matrix, AC checklist, integration harness plan, test count tracking, critical ordering |
| `test-plan/pool-config.md` | ~80 | PoolConfig unit/integration tests (R-01, R-11, R-12; AC-02, AC-09, AC-10) |
| `test-plan/analytics-queue.md` | ~130 | AnalyticsQueue unit/integration tests (R-02, R-04, R-06, R-13; AC-06, AC-07, AC-08, AC-15, AC-18, AC-19) |
| `test-plan/migration.md` | ~90 | Migration regression harness (R-03; AC-11, AC-17) — 14 new tests across 12 version transitions |
| `test-plan/sqlx-store.md` | ~90 | SqlxStore lifecycle integration tests (R-01, R-02, R-06, R-10; AC-08, AC-10, AC-19) |
| `test-plan/entry-store-trait.md` | ~75 | RPITIT compile-time correctness + impl-completeness (R-07; AC-20) |
| `test-plan/async-wrappers.md` | ~55 | AsyncEntryStore deletion verification; AsyncVectorStore/EmbedService safety (R-07, R-15; AC-04) |
| `test-plan/server-migration.md` | ~90 | Transaction rollback at 6 call sites, context_status shed field, grep gates (R-09, R-15; AC-05, AC-16, AC-18) |
| `test-plan/observe-migration.md` | ~80 | ExtractionRule async conversion, no-block_on verification (R-08; AC-13) |
| `test-plan/ci-offline.md` | ~90 | sqlx-data.json, CI SQLX_OFFLINE, rusqlite removal, source cleanness (R-05, R-15; AC-01, AC-03, AC-12, AC-13) |

---

## Risk Coverage Summary

| Risk | Severity | Test Files | Estimated Scenarios |
|------|----------|-----------|---------------------|
| R-01 | Critical | pool-config.md, sqlx-store.md | 6 |
| R-02 | Critical | analytics-queue.md, sqlx-store.md | 5 |
| R-03 | Critical | migration.md | 5 |
| R-04 | High | analytics-queue.md, server-migration.md | 6 |
| R-05 | High | ci-offline.md | 5 |
| R-06 | High | analytics-queue.md, sqlx-store.md | 4 |
| R-07 | High | entry-store-trait.md, async-wrappers.md | 4 |
| R-08 | High | observe-migration.md | 5 |
| R-09 | High | server-migration.md | 6 |
| R-10 | Med | sqlx-store.md | 3 |
| R-11 | Med | pool-config.md | 4 |
| R-12 | Low | pool-config.md | 1 |
| R-13 | Med | analytics-queue.md | 4 |
| R-14 | High | OVERVIEW.md (count gate) | 1 |
| R-15 | High | server-migration.md, ci-offline.md | 5 |

**Total scenarios**: ~64 (exceeds the 44-minimum from RISK-TEST-STRATEGY.md)

---

## Integration Suite Plan

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory gate |
| `tools` | All 9 tools now route through SqlxStore |
| `lifecycle` | Multi-step flows exercise write_pool + read_pool separation |
| `volume` | Analytics queue shed under scale |
| `security` | Integrity write bypass under queue saturation |

New integration tests to write in Stage 3c:
1. `suites/test_tools.py`: `test_context_status_shed_events_total_present` — AC-18 via MCP
2. `suites/test_lifecycle.py`: `test_store_open_close_preserves_entries` — AC-19 proxy via server restart

---

## AC Coverage Summary

All 20 ACs have direct test coverage. ACs AC-01, AC-03, AC-04, AC-05, AC-13, AC-16 are covered by grep/static checks (ci-offline.md, server-migration.md). ACs AC-02, AC-06, AC-07, AC-08, AC-09, AC-10, AC-11, AC-14, AC-15, AC-17, AC-18, AC-19, AC-20 are covered by Rust unit/integration tests. AC-12 is covered by file existence + CI log review.

---

## Open Questions for Stage 3b

1. **Dynamic dispatch mechanism for ExtractionRule (ADR-006)**: The test plan assumes Option A (async RPITIT). The delivery agent must choose between explicit enum (recommended) or `async_trait` macro for `Vec<Box<dyn ExtractionRule>>`. OB-U-04 must be adapted to whichever mechanism is chosen.

2. **Queue saturation in unit tests**: AQ-U-01, AQ-U-02, AQ-I-04, AQ-I-05 require pausing the drain task to fill the analytics channel. The delivery agent should add a `#[cfg(test)]` mechanism (e.g., a `pause_drain` test helper or a channel that blocks drain iteration) when implementing `analytics.rs`.

3. **OQ-BLOCK-02 (call site count)**: IMPLEMENTATION-BRIEF.md mentions "5 call sites" but lists 6 (server.rs ×3, store_correct.rs, store_ops.rs, audit.rs). The test plan covers all 6 (SM-I-01 through SM-I-06). If audit.rs turns out to NOT have a transaction call site, SM-I-06 becomes a no-op and can be skipped with a note.

4. **Migration version seeding helper**: The 12 version transition tests (MG-I-02 through MG-I-12) require a `seed_schema_at_version(path, version)` async helper. This must be implemented in `test_helpers.rs` to avoid duplication across 12 tests.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (procedure) for "testing procedures async store tokio test teardown drain task" — found #487 (workspace test hang prevention), #750 (pipeline validation tests), #375 (migration before DDL in Store::open) — #375 directly confirms ADR-003 pattern is established convention.
- Queried: `/uni-knowledge-search` (procedure) for "integration test migration schema version transitions sqlx" — found #374 (in-place SQLite schema migration), #836 (new table migration), #1263 (new field migration) — all confirm the migration test isolation pattern (fresh temp DB per test).
- Queried: `/uni-knowledge-search` (general) for "pool starvation write pool connection cap async test patterns" — found #735 (spawn_blocking pool saturation lesson-learned), #2058 (ADR-001 pool acquire timeout, directly informs PC-U-05, PC-U-06, PC-I-05, PC-I-06), #2060 (ADR-003 migration sequencing, directly informs MG-I-14, MG-I-15).
- Stored: nothing novel to store — all test infrastructure patterns here are applications of existing conventions (#374, #375, #735, #2057, #2058, #2060). The per-version-transition isolation pattern and the drain-task-saturation-pause pattern are specific to nxs-011 and too narrow to generalize.
