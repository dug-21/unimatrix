# Risk Coverage Report: nan-018

**Feature**: nan-018 (GH #716) — Eval Harness Strategic Upgrade (the *instrument*).
**Stage**: 3c Test Execution. **Branch**: `feature/nan-018`.
**Lens**: silent-wrongness over loud-failure — every test proves the instrument *measures*, not merely *executes*.

All unit tests pass; integration smoke gate passes; the three non-negotiable Wave-1 backstop tests pass (R-09 corpus audit, R-04 hash-sensitivity matrix, R-15 non-vacuous AC-14). No nan-018-caused failures. No new GH Issues filed (the only red unit test is the documented pre-existing flaky; all integration xfails are pre-existing with documented reasons).

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Missed config site silently shifts default penalty (bit-for-bit) | `test_graph_penalty_with_default_equals_graph_penalty_*` / `_named_const_*` (per-shape + clamp_floor, engine); `test_enumerated_penalty_sites_route_through_config`, `test_background_rs_not_a_penalty_site`, `test_with_rate_config_default_resolves_to_const_params` (search); `test_config_omits_graph_penalty_section_deserializes_to_defaults`, `test_dual_default_divergence_empty_toml_vs_default_impl` (config); AC-14 cond.4 (`test_ac14_correlated_sweep_non_vacuous`) | PASS | Full |
| R-02 | Dual-default divergence (serde fn ↔ Default ↔ const) | `test_graph_penalty_config_dual_default_{orphan,clean_replacement,hop_decay,partial_supersession,dead_end,fallback,max_traversal_depth}_triangulates` (7 levers); `test_graph_penalty_params_default_references_consts` | PASS | Full |
| R-03 | Retrieval-shape hash non-determinism | `test_shape_hash_stable_n100`, `test_shape_hash_permuted_input_order_unchanged`, `test_shape_hash_permuted_const_source_unchanged`, `test_shape_hash_cross_process_equal`, `test_shape_hash_float_format_fixed` | PASS | Full |
| R-04 | Incomplete manifest → silent staleness | sensitivity matrix: `test_shape_hash_sensitive_to_each_entry_column`, `_entry_column_added_removed`, `_edge_type_set`, `_confidence_dimension`, `_embedding_dim`, `_manifest_version`, `_model_id`; negative: `test_shape_hash_insensitive_to_display_only_column`; `test_migration_number_not_hashed` | PASS | Full (declared set) — **named human column-manifest completeness gate is a separate delivery obligation, see Gaps** |
| R-05 | Embed-model-dependence regression | `test_shape_hash_sensitive_to_model_id`, `test_shape_hash_reads_embed_model_live_not_literal`, `test_shape_module_has_no_model_id_literal`, `test_embed_sha256_participates_when_set` | PASS | Full |
| R-06 | Mismatch path under-tested (guard never fires) | `test_drift_guard_passes_on_match`, `test_drift_guard_fires_on_mismatch_primary_aborts`, `test_drift_guard_warns_on_mismatch_snapshot_continues`, `test_drift_guard_message_names_diverged_dimension`, `test_diverged_dimensions_attributes_correct_class`, `test_unknown_manifest_version_errors_not_silent`; AC-14 cond.5 | PASS | Full |
| R-07 | Token-proxy infidelity (k-weighted vs token-weighted) | `test_cost_same_k_different_token_load_differs`, `test_token_proxy_monotonic_on_length`, `test_cost_is_sum_of_token_proxy`, `test_payload_includes_title_and_content`, `test_fallback_tier_word_times_1_3_deterministic` | PASS | Full |
| R-08 | Token-proxy non-determinism | `test_token_proxy_deterministic`, `test_cost_deterministic_across_runs`, `test_cost_empty_set_is_zero`, `test_token_proxy_empty_payload_is_zero` | PASS | Full |
| R-09 | Assertions regress to literal-ID/null `expected` | **`test_primary_corpus_audit_zero_literal_id_zero_null` (Wave-1 backstop)**; `test_loader_rejects_literal_id_expected_primary`, `test_loader_rejects_null_expected_primary`, `test_loader_rejects_empty_assertions_as_null`; `test_raw_scenario_literal_expected_detected`, `test_raw_scenario_null_ground_truth_detected` | PASS | Full |
| R-10 | Alias→id resolution breaks on re-snapshot | `test_alias_renumber_survival`, `test_alias_duplicate_rejected`, `test_alias_missing_in_assertion_is_hard_load_error`, `test_alias_missing_in_superseded_by_is_hard_load_error`, `test_property_anchor_resolves_chain_head` | PASS | Full |
| R-11 | Vacuous-pass in rank-below / redirect-to-head | full truth tables: `test_rank_below_both_present_a_after_b_pass`, `_a_before_b_fail`, `_a_absent_pass`, **`test_rank_below_b_absent_fail`** (asymmetric), `_both_absent_pass`; `test_redirect_to_head_*` (5 cases incl. no-valid-head defined failure); `test_absence_*`; `test_unresolvable_absence_alias_fails_not_vacuous`, `test_unresolvable_rank_below_alias_fails` | PASS | Full |
| R-12 | Trust not OR-folded into find_regressions | `test_trust_flip_registers_regression`, `test_trust_no_flip_no_regression`, `test_trust_repair_is_not_a_regression`, `test_or_composition_trust_holds_mrr_regresses_flagged`, `test_or_composition_relevance_holds_trust_flips_flagged` | PASS | Full |
| R-13 | Multiplier precedence / shape-param scaling | `test_with_rate_config_multiplier_scales_severities_only`, `_per_field_override_wins_over_multiplier`, `_multiplier_none_is_noop`; `test_resolve_params_multiplier_scales_severities_not_shape`, `_per_field_override_wins_over_multiplier`, `_multiplier_none_no_scaling`; `test_graph_penalty_with_scaled_severities_change_output` | PASS | Full |
| R-14 | Wave-1/Wave-2 entanglement | Wave-1 unit + AC-14 suites pass with zero Wave-2 code coupling (Wave-2 = docs + recommendation doc + Unimatrix entries only; no Rust import path from Wave-1 to any Wave-2 artifact). `test_ac14_correlated_sweep_non_vacuous` runs against the in-repo corpus alone. | PASS | Full |
| R-15 | AC-14 proof-by-use passes trivially | **`test_ac14_correlated_sweep_non_vacuous` (Wave-1 backstop)** — asserts all 5 conditions against NON-EMPTY result sets (cond.1 requires a `rank_below(A,B)` with BOTH anchors present; cond.2 each-shape evaluated; cond.3 observable lever delta; cond.4 bit-for-bit baseline + steep≠baseline; cond.5 live drift guard hard-aborts) | PASS | Full |
| R-16 | Boundary breach (protocol edit / gate wiring) | Mechanical `git diff --name-only origin/main...feature/nan-018 -- .claude/protocols/` = empty (verified, see AC-13); no eval-as-standing-gate wiring added | PASS | Full |
| R-17 | Regression-gate exit-semantics regression | `test_eval_report_exit_code_unchanged_with_trust_regression`, `test_eval_report_exit_code_unchanged_with_cost_growth`, `test_cost_growth_blocks_nothing_advisory_only` | PASS | Full |
| R-18 | 500-line file rule breach | Line-count audit (see below): all NEW nan-018 production-code submodules ≤500 production lines (inline `#[cfg(test)]` convention); pre-existing large files extended additively | PASS | Full |

---

## Test Results

### Unit Tests
- **Command**: hardened workspace convention — `setsid -w timeout 600 cargo test --workspace` (`CARGO_BUILD_JOBS=2` per sandbox OOM note).
- **Total**: 3881 (workspace `unimatrix-server` lib slice: 3880 + 1 ignored)
- **Passed**: 3879
- **Failed**: 1 — `http::token::tests::test_concurrent_creation_no_corruption` (**KNOWN pre-existing flaky**: concurrency race; passes in isolation — verified `1 passed` on `--exact` re-run; `git diff origin/main...feature/nan-018 -- .../http/token.rs` is empty, file untouched by nan-018; last modified by #664/#661 pre-nan-018). **Not attributed to nan-018. No GH Issue filed (pre-existing known-flaky baseline).**
- **Ignored**: 1 (pre-existing engine `#[ignore]`)

nan-018-specific unit groups (explicit re-runs):
| Group | Result |
|-------|--------|
| `eval::` (server lib) | 274 passed, 0 failed |
| engine `graph` (`graph_penalty_params_tests` etc.) | 188 passed, 0 failed, 1 ignored |
| `config::tests` (incl. `graph_penalty_config_tests`) | 441 passed, 0 failed |
| **Backstop** `test_primary_corpus_audit_zero_literal_id_zero_null` (R-09) | 1 passed |
| **Backstop** sensitivity matrix `test_shape_hash_sensitive*` + `_insensitive_to_display_only_column` (R-04) | 8 passed |
| **Backstop** `test_ac14_correlated_sweep_non_vacuous` (R-15) | 1 passed |

### Integration Tests (infra-001 MCP harness)
Built `target/release/unimatrix` (`CARGO_BUILD_JOBS=2`, exit 0). Per OVERVIEW §4: nan-018 adds **no MCP tool/parameter/client surface**; suites are a **regression backstop** proving default-config search is unperturbed (the MCP face of AC-01 bit-for-bit).

| Suite | Passed | xfailed | xpassed | Failed |
|-------|--------|---------|---------|--------|
| smoke (mandatory gate, `-m smoke`) | 23 | 0 | 0 | 0 |
| protocol + lifecycle (regression backstop) | 77 | 5 | 2 | 0 |
| tools (MCP face of AC-01) | 189 | 1 | 0 | 0 |

- **Backstop total** (protocol + lifecycle + tools, full files): **266 passed, 6 xfailed, 2 xpassed, 0 failed.** (Smoke's 23 are a cross-suite subset, not additive.)
- **xfailed (6)**: all pre-existing, documented reasons unrelated to nan-018 — GH#406 (`find_terminal_active` multi-hop traversal not implemented) and environment-constraint markers (no short tick interval / no ONNX embedding model in this sandbox: tick-driven Path-C edge-write tests). None introduced by nan-018; none require a new GH Issue.
- **xpassed (2)**: environment-dependent tick-liveness tests that incidentally passed despite their pre-existing xfail markers; not nan-018-related. Flagged for the marker owners to re-baseline (not a nan-018 obligation).
- **No category-1 failure** (a default-config MCP search-result shift would be the R-01 failure surfacing at MCP level — none observed; default-config retrieval is bit-for-bit unperturbed).

### R-18 Line-Count Audit (production code; inline `#[cfg(test)]` excluded)
| File | Total lines | Production lines | ≤500? |
|------|-------------|------------------|-------|
| `eval/runner/trust.rs` (new) | 582 | 248 (tests 249–582) | yes |
| `eval/corpus/loader.rs` (new) | 551 | 81 (tests 82–551) | yes |
| `eval/runner/cost.rs` (new) | 382 | ~197 (+tests) | yes |
| `eval/corpus/assertions.rs` (new) | 234 | ~167 (+tests) | yes |
| `eval/shape/{hash,manifest,guard,mod}.rs` (new) | 70/153/187/38 | all ≤500 | yes |
| `eval/runner/sweep.rs` (new) | 181 | ≤500 | yes |
| `eval/report/aggregate/{mod,regression}.rs` | 401/169 | ≤500 | yes |
| `engine/graph.rs`, `infra/config.rs`, `services/search.rs` | 805/11435/5884 | pre-existing large files, **extended additively** by nan-018 (not new focused files) | n/a — pre-existing |

All NEW nan-018 production-code submodules keep production code well under 500 lines via the inline-test convention. The three large files (graph.rs/config.rs/search.rs) pre-date nan-018 and were modified additively per the architecture; the 500-line new-focused-file rule does not retroactively apply to them (pre-existing condition).

---

## Gaps

**No risk lacks automated test coverage** within what tests can prove. Two items are residuals that — by design (RISK-TEST-STRATEGY §R-04 item 3, OVERVIEW §5) — no test can close and are flagged for the leader/human, not coverage gaps:

1. **R-04 named human column-manifest completeness gate** (ARCHITECTURE §7.3, LOCKED; IMPLEMENTATION-BRIEF "R-04 Named Human Delivery Gate"). The sensitivity matrix proves the hash is sensitive to every *declared* manifest input and insensitive to display-only columns. It **cannot** prove the *declared set is itself complete* — that no retrieval/ranking-path column was mis-classified as display-only and silently omitted (the silent-staleness path). A **named human reviewer** must certify, before delivery acceptance, that the manifest's `entries` column list covers every column the live retrieval/ranking path reads. This is a distinct **delivery gate**, separate from automated tests and routine code review. **Status: OUTSTANDING — owned by the leader/human, not closable by the tester.**

2. **NFR-08 cost-proxy error-bar documentation** (R-07 item 3). The Wave-1 obligation (the ADR-003 statement that the proxy is token-weighted with stated error characteristics, labeled a proxy) is satisfied in the ADR. The Band-2 config-knob-reference doc-review checklist item is a **Wave-2** artifact (deferrable; does not gate the Wave-1 exit).

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | (a) engine default-equivalence per-shape+clamp (`test_graph_penalty_with_default_equals_*`); (b) two-profile sweep shows penalty delta (`test_ac14_correlated_sweep_non_vacuous` cond.3); (c) enumerated-site grep guard (`test_enumerated_penalty_sites_route_through_config` + `test_background_rs_not_a_penalty_site`); empty-TOML byte-identity (`test_config_omits_graph_penalty_section_deserializes_to_defaults`, `test_dual_default_divergence_empty_toml_vs_default_impl`). MCP-level backstop: `tools` suite 189/189 — default-config retrieval unperturbed. |
| AC-02 | PASS | absence assertion surfaced + regression-counted: `test_absence_forbidden_not_in_topk_pass`/`_present_fail`; `test_trust_flip_registers_regression`. |
| AC-03 | PASS | rank-below truth table incl. asymmetric `test_rank_below_b_absent_fail`; surfaced + OR-folded (`test_or_composition_relevance_holds_trust_flips_flagged`). |
| AC-04 | PASS | one correlated run reports trust + P@5/MRR for same scenarios: `test_ac14_correlated_sweep_non_vacuous` cond.1; `test_report_surfaces_correlated_trust_relevance_cost`. |
| AC-05 | PASS | fixture graph authored/loaded/searched; property assertions resolve; loader rejects null + literal-ID (`test_loader_rejects_literal_id_expected_primary`, `_null_expected_primary`); `test_property_anchor_resolves_chain_head`. |
| AC-06 | PASS | in-repo corpus at `eval/corpus/fixtures/` covers the four required shapes + optional 5th (`multi_correction_chain`, `dangling_deprecated`, `superseded_active`, `deprecated_connected`, `dead_end_chain`); AC-14 cond.2 asserts each loads + yields ≥1 evaluated assertion. |
| AC-07 | PASS (Wave-1 plumbing) | snapshot path still yields P@5/MRR (existing `eval::report`/`runner` suites green, 274 passed). Two-corpus docs are Wave-2. |
| AC-08 | PASS | (a) hash over ordered versioned manifest (`shape::tests`); (b) deliberate-mismatch fail-loud-primary/warn-snapshot + message names dimension; (c) determinism N≥100/permuted/cross-process; (d) branch-(b) live-source (`test_shape_hash_reads_embed_model_live_not_literal`). |
| AC-09 | PASS | `cost = Σ token_proxy` (`test_cost_is_sum_of_token_proxy`); same-k/different-load differs (`test_cost_same_k_different_token_load_differs`); deterministic; exit-code unchanged (`test_eval_report_exit_code_unchanged_with_cost_growth`). |
| AC-10 | PASS (Wave-2) | `docs/testing/eval-harness.md` updated (commit 78498d1a); ADRs stored in Unimatrix (#4893–#4897). Doc-review is a Wave-2 manual gate. |
| AC-11 | PASS (Wave-2) | Band-2 guides shipped (commit 78498d1a). Sufficiency is a Wave-2 manual doc-review gate. |
| AC-12 | PASS (Wave-2) | `RECOMMENDATION-band3-protocol.md` exists (commit 872a3321); `git diff` shows zero `.claude/protocols/` changes (see AC-13); convention/procedure entries are Wave-2. |
| AC-13 | PASS (HARD GATE) | **`git diff --name-only origin/main...feature/nan-018 -- .claude/protocols/` = EMPTY** (verified). No eval-execution-as-gate wiring added (NOT-in-scope item 1 honored). |
| AC-14 | PASS (Wave-1 EXIT GATE) | **`test_ac14_correlated_sweep_non_vacuous`** — non-vacuous: all 5 conditions asserted against NON-EMPTY result sets. cond.1 requires a `rank_below(A,B)` with BOTH anchors present (not the vacuous A-absent arm); cond.2 each of 4 shapes yields ≥1 evaluated assertion; cond.3 observable non-zero lever delta on a shared deprecated entry; cond.4 baseline reproduces bit-for-bit + steep≠baseline; cond.5 live drift guard hard-aborts on stamp mismatch. |

**AC-13 boundary re-verification** (mechanical, run at report time):
```
git diff --name-only origin/main...feature/nan-018 -- .claude/protocols/   → (empty)
```

---

## Triage Summary

| Failure | Category | Action |
|---------|----------|--------|
| `http::token::test_concurrent_creation_no_corruption` (unit) | Pre-existing flaky (concurrency); passes in isolation; file untouched by nan-018 | Not fixed, not attributed to nan-018, no GH Issue (documented known-flaky baseline per spawn prompt) |
| 6 integration `xfailed` | Pre-existing, documented reasons (GH#406; sandbox tick/ONNX constraints) | Left as-is (already marked + reasoned); no nan-018 obligation |
| 2 integration `xpassed` | Pre-existing markers incidentally passing; environment-dependent tick tests; not nan-018 | Flagged for marker owners to re-baseline; not nan-018's to remove |
| `cargo audit` rsa Marvin-Attack advisory | Pre-existing transitive, no fix, baseline | Per spawn prompt — not attributed to nan-018 |

**No nan-018-caused defect. No new GH Issue required. No integration test deleted, commented out, or newly xfailed.**

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced delivery-process lessons on "test named in plan but not implemented" (#4202, #3935, #4515, #2656), the assert-the-value-not-the-path gap (#3548), and nan-018 ADR-001 (#4897). Each confirmed the Stage-3c discipline: cross-check that every risk-named assertion appears literally in a passing test body, and that named-but-unimplemented tests are caught — here all named backstop tests exist and pass.
- Stored: nothing novel to store — the test patterns exercised (multi-site bit-for-bit triangulation, hash determinism, property-assertion truth tables, non-vacuous proof-by-use) are single-feature instances of already-recorded entries (#4070, #2610, #703, #3548). The candidate cross-feature "instrument-measures-not-executes" test lens remains a one-instance observation per the OVERVIEW stewardship note; reassess at retro if ass-073/crt-053 reuse it.
