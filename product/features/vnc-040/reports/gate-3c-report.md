# Gate 3c Report: vnc-040

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | All 14 risks / 32 scenarios map to passing tests; RISK-COVERAGE-REPORT.md complete; R-13 accepted residual confirmed single-owner |
| 2. Test coverage completeness | PASS | 34/34 component tests green (verified by re-run); #5172 N=2 model-free harness present; no risk uncovered |
| 3. Specification compliance | PASS | FR-01..FR-16 implemented; AC-01..AC-11 (08 split a/b) all verified PASS |
| 4. Architecture compliance | PASS | Overlay at call-site loop; merge/load/validate reused (only visibility bump, body unchanged); `build_project_server` signature unchanged; ADR-004 registry data-only; both model invariants by construction + `Arc::ptr_eq` |
| Integration: smoke gate | PASS | smoke 24/24 PASS (`pytest -m smoke`) |
| Integration: regression suites | PASS | 294 passed / 0 failed across batches; protocol/tools/lifecycle/confidence |
| Integration: xfail hygiene | PASS | 21 pre-existing xfail markers; NONE added by vnc-040; 2 xpass pre-existing; no vnc-040 bug masked |
| Integration: no deletions | PASS | `git diff main...feature/vnc-040 -- product/test/` is EMPTY — no integration test deleted/commented |
| Multi-slug harness limitation | PASS (acceptable) | Single-server harness has no multi-slug fixture; per-slug proof is Rust-internal (correct); GH issue deferred to Phase 4 Delivery Leader |
| 5. Knowledge stewardship | PASS | Tester report has `## Knowledge Stewardship` with `Queried:` + "nothing novel to store -- {reason}" |
| Code quality (NFR-06) | PASS | No `todo!()`/`unimplemented!()`/`.unwrap()` in new non-test code; new/changed source files ≤500 lines |

## Detailed Findings

### 1. Risk Mitigation Proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §Coverage Summary maps all 14 risks (R-01..R-14, 32 scenarios) to named tests with PASS results. Verified the highest-priority risks by re-running the suite and confirming the named tests exist and pass:
- R-01 (Critical, #3905): `test_resolve_merged_violation_fails_loud_naming_slug__fusion_weight_sum`, `test_per_file_validation_alone_does_not_catch_merged_violation__fusion_weight_sum`, `test_resolve_runs_post_merge_validate_inside_helper_after_merge` — all present and green. Code confirms post-merge `validate_config(&merged, &path)` runs inside `resolve_slug_config` after `merge_configs`, before return (`http_provision.rs:350`).
- R-03/R-04 (High): `test_no_file_arm_ptr_eq_on_three_global_handles`, `test_n2_exactly_one_nli_and_one_embed_handle_resident`, `test_fields_0_2_cloned_unconditionally_on_file_present_arm` — present and green. Code at `main.rs:~1098` clones the 3 handles unconditionally outside the overlay branch.
- R-05 (High): `merge_configs` retains `embedding_model_sha256`/`nli_model_sha256` global-wins with `tracing::warn` (`config.rs:~3915`).
- R-14 (High proof obligation): `test_classification_drift_guard_every_entry_matches_merge_configs` + `test_classification_registry_exhaustive_vs_seam_field_set` — present and green, machine-pinning the registry to `merge_configs` behavior.
- R-13 (accepted residual): `test_classification_is_single_source_of_truth`; no runtime warn by design — consistent with spec Known Limitation.

### 2. Test Coverage Completeness
**Status**: PASS
**Evidence**: Re-ran the component suite (did not trust the report alone):
- `--lib slug_config_classification_tests`: 11 passed, 0 failed.
- `--bin unimatrix slug_config_tests`: 13 passed, 0 failed.
- `--bin unimatrix per_slug_loop_tests`: 10 passed, 0 failed.
Total 34/34 — exactly matching RISK-COVERAGE-REPORT.md. The #5172 N=2 model-free harness scenarios (`test_n2_*`) are present and green, covering the cross-slug isolation risks (R-04, R-12, AC-01/AC-04/AC-10) that the model-bearing harness cannot reach.

### 3. Specification Compliance
**Status**: PASS
**Evidence**: FR-01..FR-16 trace to implementation:
- FR-01/FR-02/FR-07: `resolve_slug_config` order load → per-file validate → merge → post-merge validate (`http_provision.rs:330-354`), reusing existing functions.
- FR-04/FR-05/FR-15: fields 0–2 + `permissive` threaded unconditionally from global, never `resolved` (`main.rs` loop).
- FR-14: `instructions` relocated from `main.rs:687` hoist to `resolved.server.instructions` in the loop.
- FR-16/ADR-004: `PER_SLUG_CONFIG_CLASSIFICATION` registry is data-only (`config.rs:4456`), `is_per_slug_overlayable` predicate present; verdict table renders from it.
All AC-01..AC-11 (AC-08 split a/b) verified PASS in RISK-COVERAGE-REPORT.md §Acceptance Criteria Verification; cross-checked against named tests above.

### 4. Architecture Compliance
**Status**: PASS
**Evidence**:
- Overlay lives at the `build_project_server` call-site loop, NOT `load_config` — confirmed (`main.rs` per-slug loop calls `resolve_slug_config`).
- `merge_configs` / `load_single_config` / `validate_config` reused: the only diff to these is a visibility bump (`fn` → `pub fn`); the `merge_configs` body (incl. the explicit-field `InferenceConfig {…}` arm and hash-pin global-wins) is unchanged. SR-02/#4070 audit confirmed: the inference arm enumerates every field explicitly (no `..default()` spread).
- `build_project_server` signature UNCHANGED (vnc-040 changes only caller-derived values).
- ADR-004 registry data-only: const slice of `ConfigKeyClass { key, disposition }`, no merge logic.
- Both model invariants by construction with `Arc::ptr_eq`: handles cloned outside the merge branch; `test_no_file_arm_ptr_eq_on_three_global_handles` machine-checks pointer identity on the fallthrough arm.

**Minor note (not a defect)**: ARCHITECTURE §9 documents `merge_configs(global: &…, project: &…)` (borrowed), but the live signature is `(global: UnimatrixConfig, project: UnimatrixConfig)` (owned, pre-existing crt-056-era). `resolve_slug_config` correctly adapts by cloning the borrowed global once (`global.clone()`, startup-only, negligible). The architecture's `&`-reference rendering is a documentation imprecision, not a code deviation; the reuse-unchanged invariant holds.

### Integration Test Validation (MANDATORY)
**Status**: PASS
- **Smoke 24/24 PASS** (`pytest -m smoke`) — reported and consistent with the harness (smoke markers present across suites).
- **Regression**: 294 passed / 0 failed across the belt-and-suspenders batched re-run (batch A protocol/confidence/lifecycle 100 passed/6 xfail/2 xpass; batch B tools 194 passed/1 xfail). The initial combined run RC=124 was a wall-clock ceiling kill (environment limit, 0 failures observed), correctly NOT treated as a test failure and re-run in batches for exact tallies.
- **xfail hygiene**: 21 pre-existing xfail markers verified in the harness (`test_lifecycle.py:7`, `test_tools.py:1`, etc.); NONE added by vnc-040. The 2 xpass are pre-existing stale markers, not vnc-040 tests — flagged for harness owner, not a vnc-040 concern.
- **No deletions**: `git diff main...feature/vnc-040 -- product/test/` is empty. No integration test deleted or commented out.
- **RISK-COVERAGE-REPORT.md** includes integration counts (§Integration Tests table). Confirmed.
- **Multi-slug harness limitation**: ASSESSED ACCEPTABLE. The infra-001 harness is single-server / single `--project-dir` with no multi-slug fixture, so vnc-040's per-slug overlay (Arc::ptr_eq, N=2 invariants, drift-guard, post-merge validate) is Rust-internal/startup-time and not MCP-reachable. Per-slug behavioral coverage is correctly the in-crate #5172 N=2 model-free harness + Rust unit/construction tests (all PASS). Adding single-server per-slug infra-001 tests would be vacuous. This is a sound coverage approach, not a gap. The multi-slug harness uplift GH issue is correctly deferred to the Phase 4 Delivery Leader.

### 5. Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md contains a `## Knowledge Stewardship` section with:
- `Queried:` — `context_briefing` surfacing ADR-004 (#5210), #5212, #5172, crt-031 lesson.
- `Stored: nothing novel to store —` with a reason (the patterns exercised are already captured; the lib/bin split caveat is flagged with a defer-unless-recurs reason).
Both required entries present with reasons. No WARN.

## Rework Required

None.

## Scope Concerns

None.
