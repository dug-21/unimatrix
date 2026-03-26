# Scope Risk Assessment: nan-008

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Dual independent type copies (`runner/output.rs` and `report/mod.rs`) — missing a field update in either copy produces silent zero-valued metrics with no compile error (confirmed pattern #3512) | High | High | Architect must specify a single checklist or test guard that enforces both copies are updated in sync; consider a golden-output integration test on the report JSON |
| SR-02 | `configured_categories` empty-list edge case: CC@k guard returns 0.0, but a profile TOML that omits `[knowledge] categories` falls back to compiled defaults — it is not empty; risk is that a test profile with an explicit empty list silently produces meaningless CC@k = 0.0 with no diagnostic | Med | Med | Spec should require a warning log or assertion when `configured_categories` is empty at metric call time |
| SR-03 | ICD uses raw Shannon entropy with unbounded range `[0, log(n)]`; the scope explicitly declines normalization — downstream consumers comparing ICD values across profiles with different `n` categories will misread them as comparable | Med | Med | Spec must include interpretation guidance and label the ICD column with its max value in the report |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Baseline recording requires running `eval run` against the current snapshot; scope (open question 3) leaves open whether a snapshot already exists — if none exists, the delivery agent must produce one, expanding scope | Med | Med | Scope decision: "if no snapshot, approved to create one" — architect must specify the exact creation steps to avoid ambiguity at delivery |
| SR-05 | `ScoredEntry.category` added to both result copies increases per-entry output size; scope notes this is negligible (~132 KB) but the 1,761-scenario baseline run size was not verified — if scenario count has grown, the estimate may be stale | Low | Low | Confirm current scenario count before delivery; no design change needed |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `report/render.rs` adds a new section 6 (Distribution Analysis) to an existing ordered report — section-order regressions in formatter-style changes are a documented recurring pattern (#3426); existing tests may not catch a misplaced or duplicate section | High | Med | Spec must require a golden-output test or snapshot test for the full report markdown; do not rely on unit tests of individual render functions alone |
| SR-07 | `replay.rs` must pass `configured_categories` down to `compute_cc_at_k`; if the call site in `replay_scenario` passes `profile.config_overrides.knowledge.categories` by reference through a chain where the profile is consumed or moved before use, a borrow lifetime error could block compilation in a non-obvious way | Low | Low | Architect should trace the ownership chain in `replay.rs` → `run_single_profile` before spec is finalised |

## Assumptions

- **SCOPE.md §"KnowledgeConfig.categories as denominator"**: Assumes `profile.config_overrides.knowledge.categories` is always populated at the point `run_single_profile` is called. If profiles loaded from TOML omit the `[knowledge]` section entirely, this field may be an empty `Vec` rather than the compiled defaults — the guard in SR-02 applies.
- **SCOPE.md §"Proposed Approach / Metric formulas"**: Assumes natural log (`f64::ln`) is the correct and agreed log base. If the issue author expected log base 2 (common in information theory), computed ICD values will differ and the baseline entry will be wrong.
- **SCOPE.md §"Background Research / eval-baselines/log.jsonl format"**: Assumes the append-only log entry can be hand-crafted or produced by running `eval run` — no tooling exists to auto-generate it. Delivery agent must run the binary manually.

## Design Recommendations

- **SR-01 + SR-06**: The highest-risk combination is missing a type field (SR-01) AND a broken section order (SR-06) both producing silent failures. The architect should require at minimum one integration test that round-trips `eval run` output through `eval report` and asserts on the presence of `cc_at_k`, `icd`, and section 6 in the rendered output. This single test would catch both failure modes.
- **SR-02**: Add a `debug_assert!` or `tracing::warn!` in `compute_cc_at_k` when the input slice is empty, so test runs surface the misconfiguration rather than silently returning 0.0.
- **SR-04**: Spec should make baseline recording a named acceptance criterion step with an explicit command, not an implied post-run action, to prevent delivery agents from skipping it.
