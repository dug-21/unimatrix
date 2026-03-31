# Gate 3b Report: crt-037

> Gate: 3b (Code Review)
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All five components match validated pseudocode; `feature_cycle` adaptation correct |
| Architecture compliance | PASS | ADR decisions followed; component boundaries maintained |
| Interface implementation | PASS | Signatures match ARCHITECTURE.md §Integration Surface exactly |
| Test case alignment | PASS | All AC-13–AC-23 present; AC-05 asserts on specific node index |
| Code quality (no stubs) | PASS | No `todo!()`, `unimplemented!()`, TODO, FIXME, or placeholder functions |
| Code quality (unwrap) | PASS | `.unwrap()` confined to test code only |
| Code quality (line counts) | WARN | 4 files exceed 500 lines — all pre-existing violations, not introduced by this feature |
| Build | PASS | `cargo build --workspace` — zero errors, 15 warnings (pre-existing) |
| Security | PASS | No hardcoded secrets; input flows through config; no path traversal risk in DB layer |
| `cargo audit` | WARN | `cargo-audit` not installed; cannot verify CVE status |
| Knowledge stewardship | PASS | All 4 rust-dev agents have `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |

**Critical checks (spawn prompt):**

| Check # | Description | Status |
|---------|-------------|--------|
| 1 | `NliCandidatePair` is a tagged union enum (not flat struct with Option fields) | PASS |
| 2 | AC-05 PPR test asserts `scores[specific_lesson_node_index] > 0.0` | PASS |
| 3 | Phase 8b routes via pattern matching on `NliCandidatePair::Informs` | PASS |
| 4 | Domain vocabulary absent from `nli_detection_tick.rs` production code | PASS |
| 5 | No `Handle::current()` or `.await` inside rayon closure | PASS |
| 6 | `Direction::Outgoing` used for Informs in both PPR functions | PASS |
| 7 | `query_existing_informs_pairs` directional — no min/max normalization | PASS |
| 8 | `nli_informs_cosine_floor` defaults to 0.45; `nli_informs_ppr_weight` defaults to 0.6 | PASS |
| 9 | Informs edges written with `source = EDGE_SOURCE_NLI ("nli")` | PASS |
| 10 | All 11 AC-13–AC-23 tests present in tick implementation | PASS |

---

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

**Evidence:**

`graph.rs` — `RelationType::Informs` variant added at line 85. `as_str()` returns `"Informs"` (line 97). `from_str("Informs")` returns `Some(RelationType::Informs)` (line 113). Module doc comment updated at line 15 to include `Informs`. Enum doc updated at lines 68–76 to say "Six edge types" with `Informs` description. Matches pseudocode/graph.md exactly.

`graph_ppr.rs` — Fourth `edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing)` call added in both `personalized_pagerank` (line 118) and `positive_out_degree_weight` (line 182). Module doc updated at line 1. Matches pseudocode/graph_ppr.md exactly.

`config.rs` — Three fields added to `InferenceConfig`: `informs_category_pairs`, `nli_informs_cosine_floor`, `nli_informs_ppr_weight` (lines 550–572). Default functions at lines 773–788 yield the four canonical pairs, 0.45, and 0.6 respectively. `validate()` range checks added at lines 1115–1136. Merged config logic added at lines 2406–2430. All match pseudocode/config.md.

`read.rs` — `query_existing_informs_pairs()` at line 1448: SQL `WHERE relation_type = 'Informs' AND bootstrap_only = 0`, returns directional `(source_id, target_id)` (line 1465), no min/max normalization. Matches pseudocode/read.md.

`nli_detection_tick.rs` — `NliCandidatePair` tagged union (lines 53–64), `InformsCandidate` struct with 9 non-Option fields (lines 77–87), `PairOrigin` construction scaffolding (lines 95–102). Phase 4b HNSW scan implemented (lines 259–387). Phase 5 cap with `remaining_capacity = max_cap.saturating_sub(supports.len())` (lines 402–437). Phase 6 text fetch extended for both candidate types (lines 439–508). Phase 7 `pair_origins.into_iter().zip(raw_scores)` construction of `merged_pairs` (lines 567–588). Phase 8 pattern-matches `SupportsContradict` only (lines 593–622). Phase 8b pattern-matches `Informs` only and calls `apply_informs_composite_guard` (lines 625–658). Matches pseudocode/nli_detection_tick.md with one adaptation (see below).

**Pseudocode adaptation — `feature_cycle` type:** The pseudocode specified `EntryRecord.feature_cycle: Option<String>` and directed skipping on `None`. The actual schema (`unimatrix-store/src/schema.rs:86`) defines `feature_cycle: String`. The implementation correctly adapts by checking `.is_empty()` at line 293 (source) and line 763 (inside `phase4b_candidate_passes_guards`). The rust-dev agent documented this deviation and stored it as entry #3945. Semantically equivalent to the pseudocode intent; no behavioral gap.

### Architecture Compliance

**Status**: PASS

**Evidence:**

- ADR-001 (discriminator tag struct): `NliCandidatePair` is a typed enum — SR-08 misrouting is a compile error. No parallel index-matched vecs.
- ADR-002 (combined cap priority): Phase 5 sorts supports first, truncates to `max_graph_inference_per_tick`, then computes `remaining_capacity = max_cap.saturating_sub(supports.len())` before truncating informs. Supports-first guarantee enforced.
- ADR-003 (directional dedup): `query_existing_informs_pairs` returns `(source_id, target_id)` without normalization. Comment on line 1465 explicitly confirms "directional — NOT (a.min(b), a.max(b))".
- AC-02 / `edges_of_type` boundary: All PPR traversal in `graph_ppr.rs` uses `edges_of_type()`. No direct `.edges_directed()` calls in traversal functions.
- W1-2 / C-14 / R-09: Single rayon spawn per tick. Closure body (lines 524–530) is synchronous — only `provider_clone.score_batch(&pairs_ref)` call, no `.await`, no `Handle::current()`. The AC-21 test (`test_ac21_no_handle_current_in_file`) scans non-comment source lines and asserts empty.
- C-12 domain agnosticism: Domain strings (`"lesson-learned"`, `"decision"`, `"pattern"`, `"convention"`) appear only in `default_informs_category_pairs()` in config.rs (lines 773–780). AC-22 test verifies this at runtime by scanning production code.
- EDGE_SOURCE_NLI: `write_nli_edge` SQL (nli_detection.rs:543) hardcodes `'nli'` for source and `'nli'` for created_by. Informs edges inherit this correctly.

### Interface Implementation

**Status**: PASS

**Evidence:**

All interfaces from ARCHITECTURE.md §Integration Surface are implemented:

| Integration Point | Specified | Implemented |
|---|---|---|
| `RelationType::Informs` | enum variant | graph.rs:85 |
| `RelationType::Informs.as_str()` | `"Informs"` | graph.rs:97 |
| `RelationType::from_str("Informs")` | `Some(RelationType::Informs)` | graph.rs:113 |
| `personalized_pagerank` 4th call | `edges_of_type(idx, Informs, Outgoing)` | graph_ppr.rs:118 |
| `positive_out_degree_weight` 4th call | `edges_of_type(idx, Informs, Outgoing)` | graph_ppr.rs:182 |
| `InferenceConfig::informs_category_pairs` | `Vec<[String; 2]>`, default 4 pairs | config.rs:551 |
| `InferenceConfig::nli_informs_cosine_floor` | `f32`, default 0.45 | config.rs:561 |
| `InferenceConfig::nli_informs_ppr_weight` | `f32`, default 0.6 | config.rs:572 |
| `NliCandidatePair` | enum `{ SupportsContradict {...}, Informs { candidate, nli_scores } }` | nli_detection_tick.rs:53 |
| `InformsCandidate` | struct with 9 non-Option fields | nli_detection_tick.rs:77 |
| `Store::query_existing_informs_pairs` | `async fn(&self) -> Result<HashSet<(u64, u64)>>` | read.rs:1448 |

### Test Case Alignment

**Status**: PASS

**Evidence:**

All acceptance criteria from the test plans have corresponding tests:

`graph_tests.rs` (AC-01 through AC-04, AC-24):
- AC-01: `test_relation_type_informs_from_str_returns_some`
- AC-02: `test_relation_type_informs_as_str_returns_string`
- AC-03: `test_build_typed_relation_graph_includes_informs_edge`
- AC-04: `test_build_typed_relation_graph_informs_no_warn_log` (structural assertion: edge present means warn did not fire)
- AC-24 (penalty): `test_graph_penalty_with_informs_only_returns_fallback`
- AC-24 (terminal): `test_find_terminal_active_with_informs_only_returns_empty`

`graph_ppr_tests.rs` (AC-05, AC-06):
- AC-05: `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node` — asserts `scores.get(&1).copied().unwrap_or(0.0) > 0.0` specifically on the lesson node index. Both forward (A→B) and reverse (B→A) edges present per entry #3896 pattern. Correct.
- AC-06: `test_positive_out_degree_weight_includes_informs_edge`

`config.rs` tests (AC-07 through AC-12):
- AC-07: `test_inference_config_default_informs_category_pairs`
- AC-08: `test_inference_config_default_nli_informs_cosine_floor`
- AC-09: `test_inference_config_default_nli_informs_ppr_weight`
- AC-10: `test_validate_nli_informs_cosine_floor_zero_is_error`, `test_validate_nli_informs_cosine_floor_one_is_error`, `test_validate_nli_informs_cosine_floor_valid_value_is_ok`
- AC-11: (tests present for -0.01, 1.01 rejected; 0.0, 1.0 accepted)
- AC-12: `test_inference_config_default_passes_validate`

`read.rs` tests: 7 tests covering empty, directional, reverse-absence, multiple rows, bootstrap exclusion, mixed, and relation-type isolation.

`nli_detection_tick.rs` tests (AC-13–AC-23, R-20 mandate met):
- AC-13: `test_phase8b_writes_informs_edge_when_all_guards_pass` (also covers AC-19 source="nli")
- AC-14: `test_phase8b_no_informs_when_timestamps_equal`, `test_phase8b_no_informs_when_source_newer_than_target`
- AC-15: `test_phase8b_no_informs_when_same_feature_cycle`
- AC-16: `test_phase8b_no_informs_when_category_pair_not_in_config`
- AC-17: `test_phase8b_no_informs_when_cosine_below_floor`
- AC-18: `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`
- AC-20: `test_phase8b_edge_weight_equals_cosine_times_ppr_weight`
- AC-21: `test_ac21_no_handle_current_in_file`
- AC-22: `test_ac22_no_domain_vocab_literals_in_file`
- AC-23: `test_second_tick_does_not_write_duplicate_informs_edge`, `test_second_tick_query_existing_informs_pairs_loads_prior_edge`

**Minor divergence (WARN):** The test plan `graph.md` specifies AC-24 asserts `FALLBACK_PENALTY`. The implementation asserts `DEAD_END_PENALTY` (with explanatory comment that an Active node with no Supersedes successors takes `DEAD_END_PENALTY`, not `FALLBACK_PENALTY` which is used only when cycle detection prevents graph construction). The assertion is functionally correct and _stronger_ than FALLBACK_PENALTY would be. This is a test plan precision gap, not a code defect.

### Code Quality

**Status**: PASS (line count WARN)

**Evidence:**

- No `todo!()`, `unimplemented!()`, TODO, FIXME, or placeholder functions found in any of the five implementation files.
- `.unwrap()` usage in `nli_detection_tick.rs` is entirely within `#[cfg(test)]` block. Non-test code uses proper error handling (`match`, `?`, `tracing::warn` + degrade pattern).
- `debug_assert!(weight.is_finite())` used for weight finitude guard (line 642) — appropriate use of debug assertion for a property that should be structurally guaranteed.

**Line count (pre-existing violations):**

| File | Lines Before crt-037 | Lines After | Status |
|------|----------------------|-------------|--------|
| `nli_detection_tick.rs` | 948 | 2034 | Pre-existing violation (was already 948 > 500) |
| `config.rs` | 6714 | 7050 | Pre-existing violation |
| `read.rs` | 2460 | 2766 | Pre-existing violation |
| `graph.rs` | 595 | 602 | Pre-existing violation |
| `graph_ppr.rs` | — | 203 | PASS |

All over-500-line files were already over the limit before this feature. The additions for crt-037 did not transform a compliant file into a non-compliant one.

### Security

**Status**: PASS

- No hardcoded credentials, API keys, or secrets.
- Category pair strings come from config (not hardcoded in detection logic). AC-22 enforces this with a runtime test.
- SQL uses parameterized binds (`?1`, `?2`, etc.) — no string interpolation in SQL paths. No path traversal risk.
- No shell/process invocations.
- Input validation: `validate()` range-checks the two new f32 fields; the `phase4b_candidate_passes_guards` function validates all guard conditions before constructing `InformsCandidate`.
- `NliScores` deserialization goes through the existing ONNX cross-encoder path — no new deserialization surface.

**`cargo audit` — WARN**: `cargo-audit` is not installed in this environment. CVE status cannot be verified. No new crate dependencies were added by crt-037 (confirmed: no `Cargo.toml` changes; all dependencies are pre-existing).

### Knowledge Stewardship Compliance

**Status**: PASS

All four rust-dev agent reports contain `## Knowledge Stewardship` sections with substantive entries:

- `crt-037-agent-3-graph-report.md`: Queried briefing (entries #2429, #3650, #3731, #3740, #2451); Stored entry #3944 "Adding a RelationType variant requires three coordinated updates..."
- `crt-037-agent-4-config-report.md`: Queried briefing (entries #3817, #3937); "nothing novel to store" with specific reason.
- `crt-037-agent-5-read-report.md`: Queried briefing (ADR-003 #3940, #3659); "nothing novel to store" with specific reason.
- `crt-037-agent-6-graph-ppr-report.md`: Queried briefing (entries #3896, #3744, #3892); attempted to supersede #3896 (blocked, no Write capability) — "nothing novel to store" with specific reason.
- `crt-037-agent-7-tick-report.md`: Queried briefing (5 entries); Stored entry #3945 "EntryRecord.feature_cycle is String not Option<String>" — documenting the pseudocode/schema mismatch gotcha.

---

## Rework Required

None. All checks pass (or WARN at pre-existing violations).

---

## Knowledge Stewardship

- Queried: pre-loaded context from spawn prompt — no additional queries needed; all source documents read directly.
- Stored: nothing novel to store — the gate-3b checks for crt-037 found no new recurring failure patterns. The `feature_cycle: String` vs `Option<String>` adaptation pattern was already stored by agent-7 as entry #3945. Pre-existing 500-line violations in this codebase are not a new finding.
