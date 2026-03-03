# Risk Coverage Report: col-010b

**Feature**: Retrospective Evidence Synthesis & Lesson-Learned Persistence
**Date**: 2026-03-03
**Test Count**: 1610 total workspace (677 server, 264 observe, 170 engine)
**New Tests**: 31 new tests across 4 components

---

## Risk Coverage Matrix

| Risk ID | Description | Priority | Test Coverage | Status |
|---------|------------|----------|--------------|--------|
| R-01 | Evidence truncation mutates in-memory report | Critical | T-EL-03 (clone-and-truncate), ordering verified in code review | COVERED |
| R-02 | PROVENANCE_BOOST divergence between callsites | High | T-PB-01..04 (unit), code review (both sites import from confidence.rs) | COVERED |
| R-03 | Fire-and-forget embedding failure | Medium | T-LL-10 (insert_with_audit error on empty embedding), tracing::warn in write_lesson_learned | COVERED |
| R-04 | Concurrent supersede race | Medium | Tolerated known limitation. Supersede logic in write_lesson_learned covers single-call case | ACCEPTED |
| R-05 | Narrative synthesis edge cases | Medium | T-ES-01..08 (11 unit tests: empty, single, non-monotone, top files, summary) | COVERED |
| R-06 | evidence_limit breaks tests | Low | T-EL-01..02 (backward compat), validation.rs tests updated | COVERED |
| R-07 | CategoryAllowlist absent | Low | categories.rs confirms "lesson-learned" in INITIAL_CATEGORIES, validate() guard in write_lesson_learned | COVERED |
| R-08 | recommendations field breaks JSON | Low | T-ES-10..12 (serde skip_serializing_if, backward compat deserialization) | COVERED |
| R-09 | Empty lesson-learned content | Medium | T-LL-03 (empty fallback), build_lesson_learned_content always produces non-empty | COVERED |

---

## New Test Inventory

### Component 1: Evidence-Limiting (tools.rs)
| Test | Risk | Description |
|------|------|-------------|
| test_retrospective_params_evidence_limit | R-06 | Deserialize evidence_limit from JSON |
| test_retrospective_params_evidence_limit_zero | R-06 | evidence_limit = 0 deserialization |
| test_evidence_limit_default | R-06 | Default unwrap_or(3) |
| test_clone_and_truncate_preserves_original | R-01 | Clone preserves original (ADR-001) |

### Component 2: Evidence-Synthesis (synthesis.rs + report.rs + types.rs)
| Test | Risk | Description |
|------|------|-------------|
| test_synthesize_narratives_one_per_hotspot | R-05 | One narrative per hotspot |
| test_cluster_evidence_groups_by_window | R-05 | 30s window clustering |
| test_cluster_evidence_empty | R-05 | Empty evidence returns empty clusters |
| test_sequence_pattern_monotone | R-05 | Monotone sleep detection |
| test_sequence_pattern_non_monotone | R-05 | Non-monotone returns None |
| test_sequence_pattern_non_sleep_rule | R-05 | Non-sleep rule returns None |
| test_extract_top_files_limit | R-05 | Top 5 files limit |
| test_build_summary_non_empty | R-05 | Summary always non-empty |
| test_extract_numbers | R-05 | Number extraction helper |
| test_cluster_single_event | R-05 | Single event = single cluster |
| test_extract_file_paths | R-05 | File path extraction helper |
| test_recommendation_permission_retries | R-08 | permission_retries template |
| test_recommendation_coordinator_respawns | R-08 | coordinator_respawns template |
| test_recommendation_sleep_workarounds | R-08 | sleep_workarounds template |
| test_recommendation_compile_cycles_above_threshold | R-08 | compile_cycles above 10 |
| test_recommendation_compile_cycles_below_threshold | R-08 | compile_cycles below 10 |
| test_recommendation_unknown_type | R-08 | Unknown type returns None |
| test_recommendation_empty_hotspots | R-08 | Empty input returns empty |
| test_narratives_absent_when_none | R-08 | Skip serializing None |
| test_narratives_present_when_some | R-08 | Serialize when present |
| test_recommendations_present_when_nonempty | R-08 | Serialize non-empty vec |
| test_backward_compat_deserialization | R-08 | Pre-col-010b JSON compat |

### Component 3: Lesson-Learned (tools.rs + server.rs)
| Test | Risk | Description |
|------|------|-------------|
| test_build_lesson_learned_content_with_hotspots | R-09 | JSONL path content generation |
| test_build_lesson_learned_content_with_narratives | R-09 | Structured path content generation |
| test_build_lesson_learned_content_empty_fallback | R-09 | Empty content guard |
| insert_with_audit_sets_embedding_dim | R-03 | embedding_dim = embedding.len() |
| insert_with_audit_empty_embedding_returns_error | R-03 | Empty embedding fails at HNSW |
| correct_with_audit_sets_embedding_dim | R-03 | Correction embedding_dim |

### Component 4: Provenance-Boost (confidence.rs)
| Test | Risk | Description |
|------|------|-------------|
| provenance_boost_value | R-02 | PROVENANCE_BOOST == 0.02 |
| provenance_boost_less_than_coac_max | R-02 | PROVENANCE_BOOST < 0.03 |
| provenance_boost_score_difference | R-02 | Exact score difference |
| provenance_boost_is_additive_tiebreaker | R-02 | Tiebreaker verification |

---

## Existing Test Preservation

- All 1610 workspace tests pass (0 failures, 18 ignored)
- Existing validation.rs tests updated with evidence_limit: None for backward compat
- No existing tests broken by new fields (serde skip_serializing_if)

---

## Smoke Checks

- [x] `cargo build --workspace` — PASS (0 new warnings)
- [x] `cargo test --workspace` — PASS (1610 tests, 0 failures)
- [x] No TODOs or stubs in new code
- [x] No hardcoded magic numbers (PROVENANCE_BOOST imported from confidence.rs)
- [x] "lesson-learned" in CategoryAllowlist INITIAL_CATEGORIES
