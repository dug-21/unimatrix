# Scope Risk Assessment: crt-031

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `categories.rs` is 453 lines; second `RwLock<HashSet<String>>` + `is_adaptive` + updated constructors will likely breach the 500-line file-size rule | Med | High | Architect plans module split upfront — extract `lifecycle.rs` or companion struct before spec is written |
| SR-02 | `spawn_background_tick` has 22+ parameters; adding `Arc<CategoryAllowlist>` increases merge friction | Med | Med | Architect evaluates bundling into a `BackgroundTickConfig` composite rather than adding a raw parameter |
| SR-07 | `eval/profile/layer.rs` Step 12 has no parsed `UnimatrixConfig` in scope at the `boosted_categories` construction point — OQ-5 resolution requires tracing whether `config_overrides.knowledge.boosted_categories` is reachable there; if not, a threading change is needed | High | Med | Architect traces the `EvalProfile` data flow at Step 12 before spec is written; if `config_overrides` is not accessible a new injection path must be designed |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | `adaptive_categories` and `boosted_categories` share the same default (`["lesson-learned"]`); test fixtures zeroing one without the other will fail `validate_config` with misattributed errors (entry #2312, entry #3771) | High | High | Spec enumerates every `validate_config` fixture using partial `KnowledgeConfig` construction; all must zero both parallel lists together |
| SR-04 | AC-09 summary text exposes only adaptive categories while JSON exposes all — intentional asymmetry risks silent operator audit gaps if the format contract is not documented | Low | Low | Spec documents the asymmetry explicitly; golden-output test locks the summary format |
| SR-08 | Seven test infrastructure sites replace hardcoded literals with a shared helper; `test_support.rs` is the central fixture used by many tests — if the replacement pattern differs across sites or the helper is not importable from all seven without a circular dependency, the change creates partial consistency | Med | Med | Architect confirms a single `default_boosted_categories_set()` helper in `infra/config.rs` is reachable from all seven call sites before spec enumerates them |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `CategoryAllowlist` is `pub`; `server.rs`, both `main.rs` call sites, and tests reference it. `new()` in `server.rs` will carry the legacy policy if the spec omits updating its default field init | Med | Med | Spec traces every `new()` and `from_categories()` call site; compile-level wiring test enforces the policy-aware path (entry #3772 ADR-001 mandate) |
| SR-06 | Maintenance tick stub with `is_adaptive()` gating must survive #409 intact; a bare `if` block will require #409 to refactor the outer guard | Low | Med | Architect designs stub as a deliberate insertion point with a comment referencing #409 — no abstraction, but not a bare conditional |
| SR-09 | `main_tests.rs` line 393 asserts `KnowledgeConfig::default().boosted_categories == ["lesson-learned"]`; after Goal 8 changes `Default` to `[]` this test fails. OQ-6 is resolved but the test update is a scope item that must be spec'd explicitly or it will be missed | Med | High | Spec includes AC-17/AC-18 test rewrites as mandatory deliverables; implementer must grep for any additional test calling `KnowledgeConfig::default()` expecting `["lesson-learned"]` |

## Assumptions

- **SCOPE.md §Background/CategoryAllowlist**: `categories.rs` has a single `RwLock<HashSet<String>>` today. Correct — verify no in-flight feature modifies this.
- **SCOPE.md §Background/maintenance tick**: `maintenance_tick` currently takes no `CategoryAllowlist` parameter. Confirmed — wiring is additive.
- **SCOPE.md §Constraints**: `categories.rs` is under 500 lines (current: 453). Leaves ~47 lines of headroom — less than the new code will likely require. Module split is not optional.
- **SCOPE.md §Proposed Approach §9**: `eval/profile/layer.rs` `EvalProfile` carries `config_overrides: UnimatrixConfig`. Assumed reachable at Step 12 — architect must verify this before the spec is written (SR-07).

## Design Recommendations

- **SR-01 + SR-03 (critical pair)**: Decide module layout before speccing test fixtures. The 500-line ceiling and the parallel-list collision trap interact; test helpers for the new cross-check will themselves add lines to the same file.
- **SR-07 (highest new risk)**: Confirm `config_overrides.knowledge.boosted_categories` is accessible at `layer.rs` Step 12 before spec is finalized. If not accessible, the eval harness fix requires a config-threading change that should be spec'd as a distinct work item, not left to the implementer.
- **SR-09 + SR-03**: The implementer must run a grep for `KnowledgeConfig::default()` and `config_with_categories` across the test suite before starting; AC-21 (remove workaround) and AC-18 (serde test rewrite) are easily missed in a large diff.
