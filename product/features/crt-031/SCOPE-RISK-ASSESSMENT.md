# Scope Risk Assessment: crt-031

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `categories.rs` is 453 lines; adding a second `RwLock<HashSet<String>>` field plus `is_adaptive`, updated constructors, and tests will likely breach the 500-line file-size rule | Med | High | Architect should plan the module split upfront — either extract a `lifecycle.rs` submodule or move the adaptive set into a companion struct |
| SR-02 | `spawn_background_tick` already has 22 parameters; adding `Arc<CategoryAllowlist>` pushes it further past the `#[allow(clippy::too_many_arguments)]` suppression and is a merge-friction risk | Med | Med | Architect should decide whether to bundle `CategoryAllowlist` into an existing `Arc` composite (e.g. alongside `ConfidenceParams`) or accept the parameter addition with explicit justification |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The `adaptive_categories` default (`["lesson-learned"]`) conflicts with the existing `boosted_categories` default (also `["lesson-learned"]`); test helpers that set one but not the other will hit the cross-check and fail with confusing error attribution — exactly the trap documented in entry #2312 for `boosted_categories` | High | High | Spec must enumerate all `validate_config` test fixtures that use partial `KnowledgeConfig` construction and require explicit `adaptive_categories: vec![]` alongside `boosted_categories: vec![]` in the empty-categories test cases |
| SR-04 | AC-09 requires both summary text and JSON to expose lifecycle data, but the locked design decision limits summary text to adaptive categories only while JSON includes all — this divergence could silently omit pinned categories from operator audits done via text parsing | Low | Low | Spec should document the intentional asymmetry and add a golden-output test for the summary format so future formatters do not accidentally include pinned categories in text or strip them from JSON (see pattern #3426) |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `CategoryAllowlist` is `pub` and referenced in `server.rs`, both `main.rs` call sites, and tests. The locked constructor design (`from_categories_with_policy` + `from_categories` delegates) avoids callsite breakage — but `server.rs` constructs `CategoryAllowlist` using `new()` as a default; if the spec omits updating the `server.rs` default field init the `Arc<CategoryAllowlist>` passed to maintenance will carry only the legacy policy | Med | Med | Spec must trace every `CategoryAllowlist::new()` and `from_categories()` call site and verify each gets the policy-aware path; add a compile-level test analogous to the `PhaseFreqTableHandle` wiring test (R-14 precedent) |
| SR-06 | The maintenance tick stub adds `is_adaptive()` gating logic that must survive future #409 implementation unchanged. If the stub is written as a bare `if` block with no trait abstraction, #409 will need to refactor it — creating rework risk documented as scope-creep in the non-goals | Low | Med | Architect should design the stub with a clear comment and a `todo!`-free placeholder that #409 can fill in without touching the outer guard — not a full abstraction, but a deliberate insertion point |

## Assumptions

- **SCOPE.md §Background/CategoryAllowlist**: Assumes `categories.rs` has a single `RwLock<HashSet<String>>` field today. Confirmed. Risk: if a prior feature added a second field before this feature, the struct layout changes — verify no in-flight feature touches `CategoryAllowlist`.
- **SCOPE.md §Background/maintenance tick**: Assumes `maintenance_tick` currently takes no `CategoryAllowlist` parameter. Confirmed — the 22-parameter signature in `spawn_background_tick` does not include it. Wiring is additive, not a replacement.
- **SCOPE.md §Constraints**: Assumes `categories.rs` is under 500 lines. Current count is 453. This assumption is correct but leaves only 47 lines of headroom — less than the ~60 lines the new field + method + tests will likely require.

## Design Recommendations

- **SR-01 + SR-03 (critical pair)**: Plan the module layout before speccing test fixtures. The 500-line ceiling and the `boosted_categories` default trap interact: test code for the new `adaptive_categories` cross-check will need dedicated helper constructors that zero-out both `boosted_categories` and `adaptive_categories`, and that helper code adds lines to an already-tight file.
- **SR-02**: Evaluate whether `CategoryAllowlist` belongs in a new `BackgroundTickConfig` struct that groups the three config-derived Arcs (`ConfidenceParams`, `InferenceConfig`, `CategoryAllowlist`). This would reduce the parameter count rather than increase it.
- **SR-05**: Make the `from_categories_with_policy` constructor the canonical path and have `new()` and `from_categories()` both call it. A compile-level test on `server.rs` field initialization ensures the policy wire is never silently dropped.
