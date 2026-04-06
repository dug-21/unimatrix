# crt-047 Test Plan — Overview

GH Issue: #529

---

## Overall Test Strategy

crt-047 spans two crates and five logical components. The test strategy uses three
layers in sequence:

1. **Unit tests** — pure functions (all `compute_*` functions in `services/curation_health.rs`)
   and store-layer round-trips (`cycle_review_index.rs`). These are `#[tokio::test]` async
   tests inside `#[cfg(test)]` blocks or in the component source file.

2. **Store integration tests** — migration correctness (`migration_v23_to_v24.rs`), sqlite
   parity (`sqlite_parity.rs`), and cascade touchpoints (`server.rs`, older migration files).
   These live in `crates/unimatrix-store/tests/`.

3. **Integration smoke + lifecycle suites** — MCP-level round-trips through the compiled
   binary verifying `context_cycle_review` and `context_status` curation output is
   present and structurally valid.

---

## Risk-to-Test Mapping

| Risk ID | Description | Primary Layer | Key Test(s) | AC Coverage |
|---------|-------------|--------------|-------------|-------------|
| R-01 (Critical) | ENTRIES-only vs AUDIT_LOG orphan query contradiction | Unit | `test_compute_snapshot_orphan_uses_entries_only`, `test_orphan_excludes_chain_deprecations` | AC-02, AC-04, AC-18 |
| R-02 (Critical) | `first_computed_at` ordering key vs `feature_cycle DESC` | Unit + Integration | `test_baseline_window_ordered_by_first_computed_at`, `test_force_true_historical_does_not_perturb_window` | AC-R02, AC-R03, AC-09 |
| R-03 (High) | Schema cascade — 7 columns × 3 paths | Store integration | `test_v23_to_v24_migration_adds_all_seven_columns`, `test_fresh_db_cycle_review_index_has_v24_columns` | AC-01, AC-14, AC-R04 |
| R-04 (High) | `corrections_total = agent + human` (NOT including system) | Unit | `test_corrections_total_excludes_system_bucket`, `test_trust_source_bucketing_all_values` | AC-03 |
| R-05 (High) | Legacy DEFAULT-0 rows biasing baseline | Unit | `test_baseline_excludes_legacy_zero_rows`, `test_genuine_zero_cycle_is_included` | AC-15(f) |
| R-06 (High) | NaN from zero `deprecations_total` division | Unit | `test_orphan_ratio_zero_deprecations_is_zero`, `test_baseline_nan_free_when_all_deprecations_zero` | AC-15(e) |
| R-07 (High) | Upsert clobbering `first_computed_at` on overwrite | Unit | `test_store_cycle_review_preserves_first_computed_at_on_overwrite` | AC-05, AC-R01 |
| R-08 (Medium) | AUDIT_LOG outcome filter (vacuous — ADR-003 closes it) | Unit (negative) | `test_compute_snapshot_issues_no_audit_log_query` (inspect SQL string via grep AC-13) | AC-02 |
| R-09 (Medium) | `corrections_system` stored or omitted inconsistently | Unit | `test_corrections_system_round_trips_through_cycle_review_index` | AC-03 |
| R-10 (Medium) | Schema cascade test failures in migration test files | Store integration | All cascade touchpoints enumerated below | AC-R04 |
| R-11 (Medium) | Cold-start boundary conditions (2, 3, 5, 6, 10) | Unit | `test_baseline_boundary_*` suite per AC-R05 | AC-08, AC-07, AC-10, AC-R05 |
| R-12 (Medium) | SUMMARY_SCHEMA_VERSION blast radius | Unit + Integration | `test_summary_schema_version_is_two`, `test_stale_schema_version_advisory` | AC-11, AC-12 |
| R-13 (Low) | `updated_at` future mutation risk | Unit | `test_orphan_window_excludes_entries_updated_after_review` (documentation) | AC-17 |
| R-14 (Low) | Out-of-cycle orphans silently excluded | Unit | `test_orphan_outside_cycle_window_excluded` | AC-18 |

---

## Cross-Component Test Dependencies

| Interaction | Producer | Consumer | Test Boundary |
|-------------|----------|----------|---------------|
| `compute_curation_snapshot()` reads ENTRIES → `store_cycle_review()` writes snapshot | `curation_health.rs` | `cycle_review_index.rs` | `context_cycle_review` handler integration (AC-06) |
| `get_curation_baseline_window()` ordered slice → `compute_curation_baseline()` pure fn | `cycle_review_index.rs` | `curation_health.rs` | Unit test on window + pure function together (AC-07) |
| `compute_curation_summary()` → `StatusReport.curation_health` | `curation_health.rs` | `status.rs` + `response/status.rs` | Integration test on `context_status` (AC-09, AC-10) |
| `first_computed_at` preserved on overwrite | `cycle_review_index.rs` (upsert) | `get_curation_baseline_window()` ordering | Round-trip test (AC-R01, AC-R03) |
| Schema version cascade | `migration.rs` bump to 24 | `sqlite_parity.rs`, `server.rs`, `migration_v22_to_v23.rs` | `cargo test --workspace` after bump |

---

## Integration Harness Plan

### Applicable Suites

Per the suite selection table in the tester agent definition:

| Feature touches... | Suites to run |
|--------------------|---------------|
| Store/retrieval behavior changes | `tools`, `lifecycle`, `edge_cases` |
| Schema changes | `lifecycle`, `volume` |
| Any change | `smoke` (mandatory gate) |

**Mandatory smoke gate** — `pytest suites/ -v -m smoke --timeout=60` must pass before
any other suite evaluation.

**Primary suites**: `lifecycle`, `tools`
- `lifecycle` — validates the multi-step `context_cycle_review` → `context_status` chain
  and ensures the new curation_health block appears in MCP responses.
- `tools` — validates `context_cycle_review` and `context_status` tool parameters and
  response structure.
- `edge_cases` — validates zero-history cold start, no-cycle-start-event fallback (EC-02).

**Secondary suites**: `volume` (confirms `get_curation_baseline_window(10)` at scale).

### New Integration Tests Required

The existing harness does not test the new curation_health block fields. The following
tests should be added to the integration suites during Stage 3c:

| File | Test Name | What It Validates |
|------|-----------|-------------------|
| `suites/test_lifecycle.py` | `test_cycle_review_curation_health_cold_start` | `context_cycle_review` fresh DB: `curation_health.snapshot` present, `baseline` absent (AC-06, AC-08) |
| `suites/test_lifecycle.py` | `test_cycle_review_curation_health_with_baseline` | After seeding 3+ review rows: `curation_health.baseline` present with σ annotation (AC-07) |
| `suites/test_lifecycle.py` | `test_status_curation_health_block_present` | `context_status` returns `curation_health` block with expected fields (AC-09, AC-10) |
| `suites/test_lifecycle.py` | `test_cycle_review_force_true_preserves_baseline_order` | `force=true` on historical cycle: current cycle still appears first in window (AC-R03) |
| `suites/test_tools.py` | `test_context_cycle_review_curation_snapshot_fields` | Response includes `corrections_total`, `corrections_agent`, `corrections_human`, `orphan_deprecations`, `deprecations_total` in output (AC-02) |

**When NOT to add integration tests:**
- Pure `compute_curation_baseline()` logic — unit tests are sufficient.
- `first_computed_at` upsert preservation — exercised via store-layer unit test (not MCP-visible).
- SQL bucketing correctness — unit tests on `compute_curation_snapshot()`.

**Fixtures** — Use `server` (fresh DB) for cold-start tests. Use `shared_server` for
multi-call baseline accumulation tests.

---

## Component Test File Map

| Component | Test Plan File | Key Risk Coverage |
|-----------|----------------|-------------------|
| `cycle_review_index.rs` | `test-plan/cycle_review_index.md` | R-02, R-07, R-09, R-11, R-12 |
| `migration.rs` + `db.rs` | `test-plan/migration.md` | R-03, R-10 |
| `services/curation_health.rs` | `test-plan/curation_health.md` | R-04, R-05, R-06, R-11 |
| `context_cycle_review` handler | `test-plan/context_cycle_review.md` | R-01, R-07, R-08, R-12, I-01, I-02, I-03 |
| `context_status` Phase 7c | `test-plan/context_status_phase7c.md` | R-02, I-01, I-04 |
