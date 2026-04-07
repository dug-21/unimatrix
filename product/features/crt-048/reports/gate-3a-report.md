# Gate 3a Report: crt-048

> Gate: 3a (Component Design Review)
> Date: 2026-04-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 4 components match architecture decomposition; interfaces, signatures, and deletion inventory are exact |
| Specification coverage | PASS | All FR-01–FR-18 and NFR-01–06 have corresponding pseudocode; no scope additions |
| Risk coverage | WARN | All 10 risks mapped; `test_coherence_markdown_section` assertion removal not named in test plan; `lambda_custom_weights_zero_embedding` uses different test design in test plan vs pseudocode |
| Interface consistency | PASS | OVERVIEW.md shared types consistent with all per-component pseudocode |
| Knowledge stewardship compliance | PASS | Both agent reports have stewardship sections with Queried and Stored entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:
- OVERVIEW.md data flow diagram matches ARCHITECTURE.md §Component Interactions exactly: `load_active_entries_with_tags()` retained, freshness calls deleted, `compute_lambda()` at 4 params, `generate_recommendations()` at 5 params.
- Component A (`coherence.md`): `CoherenceWeights` 3-field struct, `DEFAULT_WEIGHTS` with locked literals (0.46/0.31/0.23), `compute_lambda()` 4-param signature, `generate_recommendations()` 5-param signature, `DEFAULT_STALENESS_THRESHOLD_SECS` retained with updated comment — all match ARCHITECTURE.md §Component A and §Integration Surface table.
- Component B (`status.md`): 5 numbered blocks match ARCHITECTURE.md §Component B line ranges precisely (695-701, 766-770, 771-777, 793-804, 811-818).
- Component C (`response-status.md`): Field removals from `StatusReport`, `StatusReportJson`, `Default`, `From`, and 3 format branches match ARCHITECTURE.md §Component C exactly.
- Component D (`response-mod.md`): All 8 fixture sites enumerated with exact line numbers matching ARCHITECTURE.md §Component D table (lines 614/618, 710/714, 973/977, 1054/1058, 1137/1141, 1212/1216, 1291/1295, 1434/1438). 4 test deletions match the architecture list. Lines 1731, 1794, 1798 additional cleanup sites are addressed.
- ADR files (ADR-001 and ADR-002) are referenced correctly in pseudocode with locked weight literals and constant retention constraint.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

All functional requirements traced:

| FR | Pseudocode Coverage |
|----|---------------------|
| FR-01 | `coherence.md` — CoherenceWeights 3-field struct |
| FR-02 | `coherence.md` — DEFAULT_WEIGHTS with exact 0.46/0.31/0.23 literals, doc comment |
| FR-03 | `coherence.md` — `confidence_freshness_score()` deleted; grep-zero note in delivery pre-flight |
| FR-04 | `coherence.md` — `oldest_stale_age()` deleted |
| FR-05 | `coherence.md` — `compute_lambda()` 4-param signature with body pseudocode |
| FR-06 | `coherence.md` — `generate_recommendations()` 5-param signature; stale branch deleted |
| FR-07 | `status.md` — Blocks 1, 2, 3, 4 delete/update both Phase 5 call sites |
| FR-08 | `response-status.md` — `StatusReport` field deletions from struct, Default, From |
| FR-09 | `response-status.md` — All 3 format branches addressed (Summary, Markdown, JSON) |
| FR-10 | `coherence.md` — `DEFAULT_STALENESS_THRESHOLD_SECS` retained with updated comment per AC-11 |
| FR-11 | `status.md` §active_entries Retention — `load_active_entries_with_tags()` retention explicit |
| FR-12 | `status.md` Block 4 — `coherence_by_source` loop updated to 4-param signature |
| FR-13 | Not touched — `[inference] freshness_half_life_hours` not mentioned; correct omission |
| FR-14 | Not touched — timestamps retained; correct omission |
| FR-15 | `coherence.md` §Tests DELETED — all 11 tests listed by name |
| FR-16 | `coherence.md` §Tests UPDATED — all 11 updated tests with new signatures and expected values |
| FR-17 | `response-mod.md` — all 8 fixture sites with 16 field references enumerated |
| FR-18 | `status.md` §now_ts audit — dead-code warning prevention addressed; `response-mod.md` delivery pre-flight grep specified |

All NFRs addressed: NFR-04 epsilon guard is explicitly specified in `coherence.md` and `lambda_weight_sum_invariant` pseudocode. NFR-06 breaking change noted in test plan OVERVIEW. No scope additions detected.

**Note on now_ts (additional check item 2)**: `status.md` contains a dedicated §now_ts Variable Audit section with explicit instruction to grep the function body and delete the declaration if unused — satisfying FR-18. PASS.

**Note on make_coherence_status_report() vec shrink (additional check item 3)**: `response-mod.md` addresses this explicitly in Site 8 / the "make_coherence_status_report() Remaining State After Edits" section. Post-edit state shows 1 entry (HNSW graph recommendation retained, stale-confidence string deleted). PASS.

---

### Check 3: Risk Coverage

**Status**: WARN

**Evidence**:

All 10 risks from RISK-TEST-STRATEGY.md are covered:

| Risk | Priority | Coverage in Test Plan |
|------|----------|----------------------|
| R-01 | Critical | `lambda_specific_three_dimensions` (distinct inputs, transposition detectable by ≥0.015 delta); `lambda_single_dimension_deviation` (per-slot isolation for all 3 positions) — `coherence.md` |
| R-02 | Critical | Build gate; explicit `make_coherence_status_report()` 0.8200-sentinel grep in `response-mod.md` |
| R-03 | Critical | Grep assertion (exactly 1 definition with required comment phrase); build implies `run_maintenance()` compiles — `coherence.md` |
| R-04 | High | `lambda_weight_sum_invariant` body inspection: `(sum - 1.0_f64).abs() < f64::EPSILON` form mandated — `coherence.md` |
| R-05 | Medium | `test_status_json_no_freshness_keys` unit test + `test_status_json_no_freshness_fields` integration test — `response-status.md` |
| R-06 | Critical | Grep count (exactly 2 `compute_lambda(` in `status.rs`, 4 args each); `coherence_by_source_uses_three_dim_lambda` unit test — `status.md` |
| R-07 | High | `lambda_renormalization_without_embedding` with trivial (1.0/1.0) and non-trivial (0.8/0.6) sub-cases using `0.46/0.77` and `0.31/0.77` re-derived expected values — `coherence.md` |
| R-08 | Medium | Build gate + JSON key-absence test — `response-status.md` |
| R-09 | Low | Build gate — `coherence.md` |
| R-10 | High | Manual post-delivery `context_get` on #179 and new ADR entry — OVERVIEW.md |

**Warning 1 — test_coherence_markdown_section assertion removal not in test plan**:

The pseudocode (`response-mod.md` §Tests to CHECK) explicitly identifies `test_coherence_markdown_section` as a surviving test with a `assert!(text.contains("**Confidence Freshness**"))` assertion that will fail after the Markdown formatter change. This is a targeted assertion removal within a surviving test — not a test deletion.

The test plan `response-mod.md` does NOT have an explicit section for this. The test plan `response-status.md` introduces `test_status_markdown_no_freshness_bullet` (a new test for the same coverage), but does not call out the removal of the specific assertion from the existing `test_coherence_markdown_section` as a Stage 3c verification item.

**Impact**: A rust-dev agent following only the test plan may not remove the failing assertion from `test_coherence_markdown_section`, causing a test failure (not a compile error) post-delivery. Severity: Medium. The pseudocode covers it; the test plan gap means a Stage 3b validator might miss it in test case alignment.

**Warning 2 — lambda_custom_weights_zero_embedding: test plan uses different design from pseudocode**:

The pseudocode (`coherence.md` lines 424–455) specifies:
- Weights: `{ graph_quality: 0.3, embedding_consistency: 0.0, contradiction_density: 0.2 }`
- Call: `compute_lambda(0.6, None, 0.4, &weights)` (embedding = None)
- Expected: 0.52 (via re-normalization: (0.3*0.6 + 0.2*0.4) / 0.5 = 0.52)

The test plan (`coherence.md` lines 233–249) specifies:
- Weights: `{ graph_quality: 0.5, contradiction_density: 0.5, embedding_consistency: 0.0 }`
- Call: `compute_lambda(0.8, Some(0.6), 0.4, &weights)` (embedding = Some(0.6))
- Expected: 0.6 (via: 0.8*0.5 + 0.6*0.0 + 0.4*0.5 = 0.6)

Both designs are internally correct and self-consistent. However, they are different test cases testing different things:
- Pseudocode tests: re-normalization with zero embedding weight and None embedding
- Test plan tests: zero embedding weight with Some(embedding) present (embedding contributes 0)

The spawn notes explicitly flag this as requiring "test plan re-derivation should confirm" the 0.52 expected value. The test plan does not confirm 0.52 — it uses a completely different test design with 0.6 as the expected value.

**Impact**: A rust-dev agent following the test plan will implement a test with `Some(0.6)` and expected 0.6, while the pseudocode specifies `None` and expected 0.52. These represent different paths through `compute_lambda()` (the `Some` branch vs the re-normalization `None` branch). The test plan design is weaker for this test case because it doesn't exercise the re-normalization logic (the higher-risk path). This is a WARN rather than FAIL because the test plan design is valid and the test would pass; but the pseudocode's intent (testing re-normalization with zero embedding weight) is not confirmed by the test plan.

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**:

OVERVIEW.md defines the shared type changes precisely:
- `CoherenceWeights`: 3 fields (graph_quality, embedding_consistency, contradiction_density)
- `StatusReport` removed fields: `confidence_freshness_score: f64`, `stale_confidence_count: u64`
- `compute_lambda()` signature: 4 params `(graph: f64, embedding: Option<f64>, contradiction: f64, weights: &CoherenceWeights)`
- `generate_recommendations()` signature: 5 params

Cross-checking per-component pseudocode against OVERVIEW.md:
- `coherence.md` function signatures match OVERVIEW.md exactly
- `status.md` call sites use the exact 4-param order shown in OVERVIEW.md
- `response-status.md` struct field deletions match OVERVIEW.md
- `response-mod.md` fixture sites reference the exact line numbers from ARCHITECTURE.md §Component D

No contradictions found between component pseudocode files.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

**crt-048-agent-1-pseudocode report** (`agents/crt-048-agent-1-pseudocode-report.md`):
- `## Knowledge Stewardship` section present
- `Queried:` entries: 3 Unimatrix queries documented (`context_briefing`, `context_search` category=pattern, `context_search` category=decision) with entry IDs (#4193, #4189, #4199, #178)
- `Stored:` entry: "nothing novel to store — the re-normalization formula, weight sum invariant epsilon guard, and struct field deletion patterns all follow precedents in the existing codebase" — reason is specific and sufficient

**crt-048-agent-2-testplan report** (`agents/crt-048-agent-2-testplan-report.md`):
- `## Knowledge Stewardship` section present
- `Queried:` entries: 3 Unimatrix queries documented with entry IDs (#724, #2984, #2428, #179, #4199, #4193)
- `Stored:` entry: "nothing novel to store" with specific reasons for each considered pattern — reason is specific and sufficient

Both agents are read-only (pseudocode/test-plan) — `Queried:` entries are the expected stewardship form.

---

## Warnings

| Warning | Which Agent | What to Address |
|---------|-------------|-----------------|
| W-01: `test_coherence_markdown_section` not named in test plan | rust-dev (Stage 3b implementer) | The test plan `response-mod.md` does not explicitly identify `test_coherence_markdown_section` as a test requiring an assertion removal. The pseudocode `response-mod.md` covers it in §Tests to CHECK. Stage 3b gate check should verify the assertion `text.contains("**Confidence Freshness**")` is absent from `test_coherence_markdown_section`. |
| W-02: `lambda_custom_weights_zero_embedding` design divergence | rust-dev (Stage 3b implementer) | Pseudocode specifies `(0.6, None, 0.4)` with expected 0.52 (re-normalization path). Test plan specifies `(0.8, Some(0.6), 0.4)` with expected 0.6 (Some branch, zero-weight embedding). These test different paths. Implementer should reconcile: the pseudocode's re-normalization variant is higher-value for R-01/R-07 coverage. If only one design is implemented, prefer the pseudocode's version (re-normalization with None). |

---

## Knowledge Stewardship

- Stored: nothing novel to store — these WARNs are feature-specific design divergences, not cross-feature patterns. The existing lesson entries (#2398 call-site audit, #4177 tautological assertion) already cover the systemic patterns at play here. No new Unimatrix entry adds value for future features.
