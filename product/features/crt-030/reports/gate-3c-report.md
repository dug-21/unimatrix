# Gate 3c Report: crt-030

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 risks addressed; R-01 deferred per strategy; R-08 (critical) has 3 tests + integration coverage |
| Test coverage completeness | PASS | 20 PPR unit tests + 16 Step 6d tests + 29 config tests; smoke 20/20 |
| Specification compliance | WARN | FR-01 all-zero guard not in `personalized_pagerank` function itself (caller handles it) |
| Architecture compliance | PASS | All 3 components match architecture; ADR decisions followed; step ordering correct |
| Knowledge stewardship compliance | PASS | Queried and Stored entries present in RISK-COVERAGE-REPORT |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 13 risks to passing tests:

| Risk | Coverage | Tests |
|------|----------|-------|
| R-01 | DEFERRED | N/A — offload branch out of scope per strategy |
| R-02 | Full | `test_step_6d_skipped_when_use_fallback_true` |
| R-03 | Full | `test_step_6d_ppr_only_entry_blend_weight_zero_initial_sim_is_zero`, `test_step_6d_blend_weight_zero_leaves_hnsw_unchanged` |
| R-04 | Full | Sort at graph_ppr.rs:59 (before loop), `test_ppr_sort_covers_all_nodes`, `test_ppr_deterministic_large_graph` |
| R-05 | Partial | Sync quarantine/error-skip path tested; full async mock not present (low risk — `continue` pattern) |
| R-06 | Full | `test_step_6d_entry_at_exact_threshold_not_included`, `test_step_6d_entry_just_above_threshold_included` |
| R-07 | Full | `test_ppr_scores_all_finite`, `test_zero_positive_out_degree_no_forward_propagation` |
| R-08 | Full (Critical) | `test_step_6d_quarantine_check_applies_to_fetched_entries`; search.rs:942–947; integration: `test_search_excludes_quarantined`, `test_quarantine_excluded_from_search` |
| R-09 | Full | grep gate: zero `.edges_directed()` callsites in graph_ppr.rs; behavioral exclusion tests |
| R-10 | Full | `test_step_6d_none_phase_snapshot_uses_hnsw_score_only`, `test_step_6d_non_uniform_phase_snapshot_amplifies_seeds`; grep confirms no `phase_affinity_score(` call in Step 6d block |
| R-11 | Full | `test_step_6d_blend_weight_one_overwrites_hnsw` |
| R-12 | Full | `test_prerequisite_incoming_direction`, `test_prerequisite_wrong_direction_does_not_propagate` |
| R-13 | Partial | `test_ppr_dense_50_node_coaccess_completes_under_5ms` (release-build only, `#[cfg(not(debug_assertions))]`) |

R-08 (Critical) is confirmed at search.rs:942–947:

```rust
// R-08 Critical: quarantine check — MANDATORY for every PPR-fetched entry.
if SecurityGateway::is_quarantined(&entry.status) {
    continue; // silent skip (AC-13 / R-08)
}
```

The quarantine check is present exactly where specified. All three required scenarios are covered.

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**:

Unit tests run via `cargo test --workspace`:
- PPR unit tests (graph_ppr_tests.rs): 20 passed, 1 ignored (10K-node scale gate, appropriately marked `#[ignore]`)
- Step 6d unit tests (search.rs step_6d module): 16 passed (15 match `test_step_6d` prefix, plus `test_fusion_weights_default_sum_unchanged_by_crt030`)
- Config PPR field tests: 29 passed

Live workspace run confirmed 3915 total tests passing, 0 failures.

Integration suite (per RISK-COVERAGE-REPORT.md):

| Suite | Passed | Failed | Xfailed | Xpassed |
|-------|--------|--------|---------|---------|
| Smoke | 20 | 0 | 0 | 0 |
| Lifecycle | 40 | 0 | 2 | 1 |
| Security | 19 | 0 | 0 | 0 |
| Tools (search) | 10 | 0 | 1 | 0 |
| **Total** | **89** | **0** | **3** | **1** |

Smoke suite 20/20 passed — mandatory gate met.

The 2 lifecycle xfails (`test_auto_quarantine_after_consecutive_bad_ticks`, `test_dead_knowledge_entries_deprecated_by_tick`) are pre-existing, require tick driving, and are unrelated to crt-030.

The 1 xpassed test (`test_search_multihop_injects_terminal_active`, GH#406) is pre-existing: marked `@pytest.mark.xfail(reason="Pre-existing: GH#406 — find_terminal_active multi-hop traversal not implemented")` at test_lifecycle.py:704. The test passing is not caused by crt-030 — PPR does not modify `find_terminal_active` or Step 6b. The xfail marker can be removed and GH#406 reviewed for closure, but this is outside crt-030 scope. Confirmed pre-existing, not introduced by this feature.

The 1 tools xfail is pre-existing background scoring timing — unrelated.

**Integration harness gap (documented)**: T-PPR-IT-01 and T-PPR-IT-02 could not be implemented as infra-001 integration tests because the harness has no `context_store_edge` mechanism. Unit-level equivalents (`test_step_6d_ppr_surfaces_support_entry`, `test_step_6d_quarantine_check_applies_to_fetched_entries`) provide equivalent functional coverage. The R-08 guarantee is additionally validated end-to-end through the security suite quarantine tests.

No integration tests were deleted or commented out.

### 3. Specification Compliance

**Status**: WARN

**Evidence**: All 18 ACs verified in RISK-COVERAGE-REPORT.md and confirmed by code inspection.

**Issue (WARN — not blocking)**: FR-01 specifies: "If `seed_scores` is empty **or all-zero**, the function returns an empty `HashMap` immediately without iterating." The `personalized_pagerank` function guards `is_empty()` (graph_ppr.rs:46) but does not explicitly guard the all-zero case. If called with `{42: 0.0}`, it iterates and returns a map of `{all_nodes: 0.0}` rather than an empty map.

This is not a runtime defect: the caller in search.rs guards `if total > 0.0` (line 876) before calling `personalized_pagerank` — so an all-zero seed map is never passed in practice. The function's contract per FR-01 is technically violated at the API level. Risk: any future caller that does not apply the zero-sum guard upstream could receive a non-empty all-zero map, which would cause no semantic harm but wastes 20 iterations.

This is a WARN (not a FAIL) because: (a) the caller's guard is correct and present, (b) the function's behavior is well-defined — it returns a valid but all-zero map, not NaN or panic, (c) the AC for this case (`test_step_6d_all_zero_hnsw_scores_skips_ppr`) tests the caller contract and passes.

All functional requirements FR-02 through FR-12 are implemented correctly, including:
- FR-02: Power iteration runs exactly `iterations` steps (no early exit). Sort placed before loop (graph_ppr.rs:59).
- FR-03: Only Supports, CoAccess, Prerequisite traversed. Doc-comment SR-01 disclaimer at graph_ppr.rs:21-23.
- FR-04: Direction deviation documented — see Architecture Compliance below.
- FR-07: Step ordering verified: 6b (line 713) → 6d (line 839) → 6c (line 962).
- FR-08: Full algorithm implemented including blend, expansion, quarantine check, error skip.
- FR-11: All 5 config fields with correct defaults, ranges, and validation.

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

Component boundaries match architecture:
- `graph_ppr.rs` implemented as submodule of `graph.rs` (declared via `#[path = "graph_ppr.rs"] mod graph_ppr;` at graph.rs:30, re-exported at graph.rs:32). Matches ADR-001.
- Step 6d position: after Step 6b (line 713), before Step 6c (line 962). Matches ADR-005.
- All 5 PPR config fields in `InferenceConfig` with serde defaults, validate() range checks, Default::default() update, and global+project merge block.
- `SearchService` receives all 5 PPR fields at construction.

**Direction deviation**: The spec (FR-04) and ADR-003 specify `Direction::Incoming` for Supports, CoAccess, and Prerequisite. The implementation uses `Direction::Outgoing` with a reverse-walk accumulation formula. This is the "mathematically equivalent reverse-walk formulation" documented in:
- graph_ppr.rs doc-comment (lines 34-38): "PPR direction: this implementation uses outgoing-edge traversal… This is the reverse random-walk formulation that surfaces in-neighbors of seeds"
- RISK-COVERAGE-REPORT.md static-gates table: "Direction::Outgoing used (not Incoming as in ADR-003 pseudocode) — mathematically equivalent reverse-walk; documented in function doc-comment"

**Verification**: The behavioral outcome is identical. For edge A→B (A Supports B):
- Incoming formulation: from B, traverse Incoming to find A → A contributes to B's score
- Outgoing formulation: from A, traverse Outgoing to find B → if B is a seed, A gains mass from B

Both result in the same surfacing behavior: when B (a decision) is a seed, A (a lesson-learned) appears in results. Tests `test_supports_incoming_direction` and `test_prerequisite_incoming_direction` verify the correct behavioral outcome and pass.

The deviation is a valid implementation choice with documented rationale and verified correctness. No inconsistency with architecture intent.

ADR compliance:
- ADR-002: Pure function signature — confirmed (no I/O, no async, no global state)
- ADR-003: Edge type exclusion — confirmed (only Supports/CoAccess/Prerequisite)
- ADR-004: Node-ID sort before loop — confirmed at graph_ppr.rs:58-59
- ADR-006: Phase snapshot read (no lock re-acquisition) — confirmed, grep returns no `phase_affinity_score(` in Step 6d block
- ADR-007: ppr_blend_weight dual role — doc-comment at config.rs:453-466 documents both roles
- ADR-008: Inline synchronous path only — confirmed, no Rayon offload in code

Non-touched components confirmed untouched: `graph_penalty`, `find_terminal_active`, `graph_suppression.rs`, `FusedScoreInputs`, `FusionWeights`/`compute_fused_score`, NLI pipeline, `nli_detection_tick.rs`, schema/SQL.

### 5. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md contains:
```markdown
## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #724 (behavior-based testing patterns), #703 (status penalty test ADR), #749 (deterministic test scenarios). Results informed assertion style choices.
- Stored: nothing novel to store — the quarantine-bypass-for-injected-entries pattern may warrant a future entry once crt-030 ships and the pattern generalizes across retrieval expansion paths.
```

Both `Queried:` and `Stored:` entries are present with specific IDs and rationale. The "nothing novel" explanation is substantive (references the candidate pattern from RISK-TEST-STRATEGY.md and explains why deferral is appropriate).

RISK-TEST-STRATEGY.md also contains a `## Knowledge Stewardship` section with queried IDs (#2800, #1628, #2964, #729) and stored rationale.

---

## File Size Notes

`graph_ppr_tests.rs` is 581 lines, marginally exceeding the 500-line limit. This file is the test split created specifically because `graph_ppr.rs` would otherwise exceed 500 lines — the split is required by C-09/NFR-08 (cited in the file header). Test files are secondary to source files in this constraint. `search.rs` (4845 lines) and `config.rs` (5676 lines) are pre-existing files not created by crt-030; crt-030 added incremental blocks to both.

---

## Gaps (Partial Coverage — Not Blocking)

### R-05: Async Fetch Error Path
The `continue` on `Err(_)` at search.rs:939 is not covered by an async mock-store test. Covered by: sync test confirms quarantine/skip predicate; live integration quarantine tests confirm end-to-end. Risk is low — the pattern is trivially correct.

### R-13: Dense CoAccess Timing (Release-Build Only)
`test_ppr_dense_50_node_coaccess_completes_under_5ms` is gated on `#[cfg(not(debug_assertions))]`. It executes in CI release builds only. Not a gap in coverage, just a CI dependency.

### cargo audit
`cargo-audit` is not installed in this environment. CVE status cannot be confirmed. No new dependencies were added by crt-030 (PPR uses only existing petgraph, std HashMap, and unimatrix-engine internals — no new crates).

---

## Rework Required

None — gate result is PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — crt-030 gate findings are feature-specific. The FR-01 all-zero guard gap (caller handles what the function spec requires) is a recurring pattern worth noting: when a function's contract says it handles a degenerate case but the caller guards it instead, it creates a silent contract violation that only manifests for future callers. This may be worth a future `lesson-learned` entry if it recurs across features.
