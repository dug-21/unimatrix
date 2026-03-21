# Agent Report: col-023-agent-2-testplan

**Phase**: Test Plan Design (Stage 3a)
**Status**: COMPLETE

---

## Output Files

All 9 test plan files written to `product/features/col-023/test-plan/`:

| File | Purpose |
|------|---------|
| `OVERVIEW.md` | Strategy, risk-to-test mapping, integration harness plan |
| `observation-record.md` | AC-01, R-03, R-13 |
| `domain-pack-registry.md` | AC-03, AC-04, AC-05, AC-07, AC-08, R-09, R-10 |
| `rule-dsl-evaluator.md` | AC-05 (partial), R-07, R-08 |
| `config-extension.md` | AC-03, R-09, R-10 (partial) |
| `detection-rules.md` | AC-02, AC-04, AC-05, R-01, R-02, R-13 |
| `metrics-extension.md` | AC-10, R-11, R-02 (partial), IR-03 |
| `schema-migration.md` | AC-09, R-05, R-12, FM-05 |
| `ingest-security.md` | AC-06, AC-07, AC-11, SEC-01, SEC-02, SEC-03, R-06 |

---

## Risk Coverage Mapping

| Risk | Coverage | Test Location |
|------|----------|---------------|
| R-01 (Critical) | Full | `detection-rules.md` T-DET isolation tests × 21 rules |
| R-02 (Critical) | Full | `detection-rules.md` T-DET-COMPAT-01/02 + per-rule regression |
| R-03 (Critical) | Full | `observation-record.md` static grep obligation; `detection-rules.md` test count non-regression |
| R-04 (Closed) | N/A | Structural assertion only (no write path exists) |
| R-05 (High) | Full | `schema-migration.md` T-MIG-03 named-column round-trip |
| R-06 (High) | Full | `ingest-security.md` T-SEC-12/14; `OVERVIEW.md` lifecycle suite |
| R-07 (High) | Full | `rule-dsl-evaluator.md` T-DSL-12/13 unsorted input tests |
| R-08 (High) | Full | `rule-dsl-evaluator.md` T-DSL-05/06 non-numeric/missing key |
| R-09 (High) | Full | `config-extension.md` T-CFG-07/08/09 |
| R-10 (Medium) | Full | `domain-pack-registry.md` T-DPR-12/13 |
| R-11 (Medium) | Full | `metrics-extension.md` T-MET-01/02/03 |
| R-12 (Medium) | Full | `schema-migration.md` T-MIG-07 |
| R-13 (Medium) | Full | `observation-record.md` + `detection-rules.md` static grep |
| IR-01 | Full | `domain-pack-registry.md` T-DPR-01; resolve_source_domain test |
| IR-02 | Full | `domain-pack-registry.md` T-DPR-12 + `config-extension.md` T-CFG-06 |
| IR-03 | Full | `metrics-extension.md` T-MET-10/11 |
| SEC-01 | Full | `ingest-security.md` T-SEC-01/03/04 |
| SEC-02 | Full | `ingest-security.md` T-SEC-07/08 |
| SEC-03 | Full | `ingest-security.md` T-SEC-15 |

---

## Integration Harness Decision

**No new infra-001 integration tests required.** All col-023 behavioral changes are
below the MCP interface layer. The `context_cycle_review` output shape is unchanged.

Suites to run at Stage 3c:
1. `smoke` — mandatory minimum gate
2. `lifecycle` — restart persistence validates schema migration end-to-end
3. `security` — regression baseline for existing security surface

---

## Key Stage 3b Obligations Surfaced

1. **Capture backward-compat baseline BEFORE making changes** (T-DET-COMPAT-02):
   Run the retrospective pipeline against the hardcoded fixture on pre-feature `main`
   and record findings count, finding types, and all 21 UniversalMetrics field values.
   These become the hardcoded expected values in the snapshot test.

2. **Record pre-feature test count** (AC-02/R-03):
   ```bash
   cargo test -p unimatrix-observe -- --list 2>&1 | grep "test$" | wc -l
   ```
   Run this before Wave 1 and store the result as the non-regression floor.

3. **`make_search_obs` helper in `extraction_pipeline.rs`** must be updated in Wave 4
   to use `event_type: String` and `source_domain: String` instead of `hook: HookType`.

---

## Open Questions

None. All source documents are aligned (ALIGNMENT-REPORT PASS). SPECIFICATION.md
AC-08 redefinition and FR-06 removal are clearly reflected in the test plans.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "col-023 architectural decisions" (category: decision, topic: col-023) — returned ADR-001 through ADR-007 entries (#2903–#2909). All 7 ADRs consumed in test plan design.
- Queried: `/uni-knowledge-search` for "observation pipeline testing patterns edge cases" — returned #750 (pipeline validation tests), #2843 (HookType blast radius pattern), #2912 (col-023 design outcome). Entry #750 confirmed the Arrange/Act/Assert test structure and inline module placement.
- Stored: entry #2928 "String-Refactor Test Plan Patterns: Domain Isolation, Backward-Compat Snapshot, Static Grep Gates" via `/uni-store-pattern` — novel pattern applicable to any future feature that replaces a closed enum with string fields across many files with domain-specific rules.
