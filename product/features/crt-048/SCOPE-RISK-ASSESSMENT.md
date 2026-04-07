# Scope Risk Assessment: crt-048

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | f64 weight constants (0.46/0.31/0.23) must sum to exactly 1.0; floating-point representation may produce 1.0000000000000002 or similar, breaking the `lambda_weight_sum_invariant` test | Med | Med | Use epsilon-tolerance comparison in invariant test, not `==`; verify sum in `DEFAULT_WEIGHTS` with a compile-time or startup assertion |
| SR-02 | `compute_lambda()` signature change removes a positional f64 parameter; any call site added since Background Research was written will silently compile if types match — the compiler cannot detect semantic mis-ordering of identical f64 args | Med | Low | Architect should search all crates for `compute_lambda` before implementation; consider named-field struct args instead of positional |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | `DEFAULT_STALENESS_THRESHOLD_SECS` is explicitly retained for `run_maintenance()` (SCOPE.md §Implementation Notes), but the scope's Goals list (Goal 7) says "remove if no other caller." If the implementer follows Goal 7 literally without reading Implementation Notes, the constant is deleted and `run_maintenance()` silently compiles using a hardcoded literal or fails | High | Med | Spec must express Goal 7 as a conditional: retain with explanatory comment; make the `run_maintenance()` dependency explicit in the acceptance criteria |
| SR-04 | Three output formats (text, markdown, JSON) must all drop `confidence_freshness_score` and `stale_confidence_count`; SCOPE.md §Constraints notes JSON field removal is a breaking change for downstream callers — but no operator migration window is provided | Med | Low | Spec should require release-note language and confirm no live callers in product/test/; SCOPE.md §Resolved Decisions (OQ-2) already confirms zero matches in product/test/ — document this as the justification for clean removal |
| SR-05 | `coherence_by_source` loop calls `confidence_freshness_score` per source (SCOPE.md line ~793-804); removing freshness from this loop is mentioned but the per-source Lambda re-normalization behaviour when embedding is absent must remain correct for 2-of-3 dimensions — not just the main path | Med | Med | Spec must include an explicit AC for the per-source path, not just the main `compute_lambda()` AC |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `mcp/response/mod.rs` has approximately 6 test fixtures that explicitly set `confidence_freshness_score: 1.0` and `stale_confidence_count: 0` (SCOPE.md §Implementation Notes); these produce compile errors, not test failures — the scope correctly identifies them but the count is an estimate ("approximately 12 field removals") | Med | High | Architect should enumerate exact fixture sites before writing pseudocode; a missed fixture causes a build failure that blocks all gate runs |
| SR-07 | ADR-003 (entry #179) currently records the 4-dimension weight rationale; crt-048 supersedes it (AC-12). If the ADR is not superseded before the feature merges, downstream agents will find the old ADR and apply wrong weights | Med | Med | Spec must list ADR supersession (via `context_correct`) as a required delivery step, not an optional knowledge-stewardship step |

## Assumptions

- **SCOPE.md §Resolved Decisions (OQ-2)**: Assumes `grep` of `product/test/` for `confidence_freshness` returns zero matches. If any integration test fixture captured the field as a JSON key, it would be a live test failure, not just an inert research artifact. This assumption has been verified per OQ-2 but should be re-confirmed at delivery start.
- **SCOPE.md §Background Research (~line 123)**: Assumes `confidence_freshness_score()` is a pure function with no I/O. If it was modified between the scope being written and implementation, the removal may have unintended side effects.
- **SCOPE.md §Implementation Notes**: Assumes `run_maintenance()` is the sole surviving caller of `DEFAULT_STALENESS_THRESHOLD_SECS` after the three Lambda call sites are removed. This is a static analysis claim that must be re-verified at implementation time.

## Design Recommendations

- **SR-03 (High)**: The spec must rewrite Goal 7 as a conditional rule, not a simple "remove if unused" instruction. The implementation note about `run_maintenance()` is critical and must be surfaced as an AC, not buried in prose.
- **SR-06 (Med/High likelihood)**: Enumerate all `StatusReport` default-construct sites in `mcp/response/mod.rs` before pseudocode. The ~12-fixture estimate means a partial removal compiles only if all struct fields are removed atomically.
- **SR-01 (Med)**: The ADR recording new weights (AC-12) must specify the exact f64 literals and confirm sum=1.0; the `lambda_weight_sum_invariant` test must use epsilon comparison, not exact equality.
