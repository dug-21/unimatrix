# Agent Report: crt-047-agent-2-testplan

Phase: Stage 3a (Test Plan Design)
Feature: crt-047 — Curation Health Metrics

---

## Output Files

All test plan files written to `product/features/crt-047/test-plan/`:

| File | Lines | Primary Coverage |
|------|-------|-----------------|
| `OVERVIEW.md` | 103 | Risk-to-test mapping, cross-component dependencies, integration harness plan |
| `cycle_review_index.md` | 107 | R-02, R-07, R-09, R-12: upsert preservation, window ordering, SUMMARY_SCHEMA_VERSION bump |
| `migration.md` | 115 | R-03, R-10: AC-14, schema cascade touchpoints, v23 DB builder spec |
| `curation_health.md` | 185 | R-01, R-04, R-05, R-06, R-11: all six pure function AC-15 sub-cases, trust-source bucketing |
| `context_cycle_review.md` | 130 | R-01, R-07, R-08, R-12, I-01–I-04: handler step ordering, pool discipline, advisory path |
| `context_status_phase7c.md` | 107 | R-02, I-01, I-04: Phase 7c read-only path, trend boundaries, pool verification |

---

## Risk Coverage Mapping

| Risk ID | Priority | Coverage | Key Tests |
|---------|----------|----------|-----------|
| R-01 (Critical) | Critical | Full | CH-U-01 to CH-U-06 (`compute_snapshot` ENTRIES-only, chain exclusion, window filter) |
| R-02 (Critical) | Critical | Full | CRS-V24-U-06 to U-08, CS7C-U-01/U-06 (ordering, force=true stability) |
| R-03 (High) | High | Full | MIG-V24-U-01 to U-05, cascade touchpoints (all 7 columns, pragma_table_info idempotency) |
| R-04 (High) | High | Full | CH-U-02 (all 6 trust_source values, corrections_total == agent+human) |
| R-05 (High) | High | Full | CH-U-14 (legacy rows excluded), CH-U-15 (genuine zero included) |
| R-06 (High) | High | Full | CH-U-12, CH-U-13 (zero deprecations → 0.0 ratio, no NaN in mixed window) |
| R-07 (High) | High | Full | CRS-V24-U-03 (first_computed_at preserved on overwrite — the critical round-trip test) |
| R-08 (Medium) | Medium | Partial | AC-13 grep (no AUDIT_LOG query issued — vacuous per ADR-003) |
| R-09 (Medium) | Medium | Full | CRS-V24-U-09 (corrections_system round-trips through store) |
| R-10 (Medium) | Medium | Full | migration.md cascade touchpoints (migration_v22_to_v23.rs, sqlite_parity.rs, server.rs) |
| R-11 (Medium) | Medium | Full | CH-U-18 to CH-U-22, cold-start boundary suite at 2/3/5/6/10 rows |
| R-12 (Medium) | Medium | Full | CCR-U-04 (advisory present), CCR-U-05 (no silent recompute), CCR-U-06 (force=true upgrades) |
| R-13 (Low) | Low | Partial | CH-U-06 (window boundary correct); future mutation risk is documentation only |
| R-14 (Low) | Low | Full | CH-U-05 (orphan outside window excluded), AC-18 |

---

## Integration Harness Plan Summary

Suites to run in Stage 3c:

| Suite | Rationale |
|-------|-----------|
| `smoke` (mandatory) | Gate for any change |
| `lifecycle` | `context_cycle_review` → `context_status` chain, curation_health block presence |
| `tools` | `context_cycle_review` parameter coverage, response shape |
| `edge_cases` | Cold-start, no-cycle-start-event fallback |
| `volume` | `get_curation_baseline_window(10)` at scale |

New integration tests to add during Stage 3c (5 total):
- `test_lifecycle.py`: 4 tests for cold-start, baseline presence, status block, force=true stability
- `test_tools.py`: 1 test for curation_health snapshot fields in response

---

## Open Questions for Stage 3b

1. **`CurationBaselineRow` legacy detection field**: The spec mentions `schema_version < 2`
   as the legacy-row exclusion criterion (R-05). The implementor must confirm whether
   `CurationBaselineRow` carries `schema_version: i64` (as shown in the IMPLEMENTATION-BRIEF
   `get_curation_baseline_window()` SELECT) or uses a `has_real_data: bool` derivation.
   The test for CH-U-14 depends on this shape.

2. **Advisory string exact format**: AC-11 test asserts the exact advisory string.
   The implementor must use: `"computed with schema_version 1, current is 2 — use force=true to recompute"`
   (em-dash, not hyphen). The test plan uses this string verbatim.

3. **`context_status` test access**: The Phase 7c unit tests call `compute_report()` or a
   sub-routine. If that function is not publicly accessible from tests, the test must go
   through the handler infrastructure. Stage 3b should expose a test seam or confirm the
   existing `status.rs` test pattern.

4. **`force=false` fresh call on new cycle**: The plan assumes `force=false` on a cycle
   with no prior row computes a fresh snapshot (same as `force=true`). Stage 3b must
   confirm this from the handler logic — if `force=false` on a new cycle returns early
   (no-op), the CCR-U-01 cold-start test needs adjustment.

---

## Self-Check

- [x] OVERVIEW.md maps all 14 risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness section with suite selection and 5 new tests
- [x] Per-component test plans match architecture component boundaries (5 components → 5 files)
- [x] Every high-priority risk (R-01 to R-07) has at least one specific test expectation with function name
- [x] Integration tests defined for component boundaries (I-01, I-02, I-03, I-04)
- [x] All output files within `product/features/crt-047/test-plan/`
- [x] Knowledge Stewardship report block included below

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 15 entries; top match was entry #4179 (ADR-001, crt-047 itself). Also surfaced entry #3806 (gate-3b test omission lesson), entry #4125 (schema version cascade pattern), entry #238 (test infrastructure conventions). All applied.
- Queried: `context_search("crt-047 architectural decisions")` — returned ADR-001 (#4179), ADR-002 (#4180), ADR-004 (#4182). All three read and applied throughout test plans.
- Queried: `context_search("cycle_review_index migration test patterns")` — returned #378 (old-schema DB test lesson), #4153 (schema version cascade pattern), #375 (migration init ordering), #3894 (schema cascade checklist, deprecated), #4125 (updated cascade checklist). Entry #4125 directly informed migration.md cascade section.
- Stored: entry #4185 "Two-step upsert preservation test: assert first-write column survives overwrite, other columns update" via `context_store` (tags parameter could not serialize as array; tags embedded in content). Topic: testing. Category: pattern.
