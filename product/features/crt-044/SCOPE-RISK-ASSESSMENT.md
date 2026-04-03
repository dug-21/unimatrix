# Scope Risk Assessment: crt-044

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `pairs_written` counter semantics shift (per-pair → per-edge) doubles reported values for S8; monitoring or alerting that keys off this counter may behave unexpectedly | Med | Med | Document the semantic change in the migration PR; spec writer should add an AC asserting the new count is 2× for new pairs |
| SR-02 | `write_graph_edge` returns bool via `rows_affected() > 0` — the new second call per pair in each tick will frequently return false (UNIQUE conflict for already-bidirectional edges post-migration); if the implementation keys budget counters off the second call's return value, counts are understated (entry #4041) | Med | Med | Architect should specify that both direction calls' return values are independently valid; budget counters must be incremented only on true return for each call |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | S1 and S2 both use `relation_type='Informs'` — combined `WHERE source IN ('S1','S2')` migration block is correct, but future edge sources writing Informs edges could be silently excluded if developers copy this filter without updating it (entry #4078 precedent: S8 was missed by crt-035) | Low | Low | Spec writer should add a constraint requiring source-scoped guards in every future migration; the `// SECURITY:` comment pattern in graph_expand is a model for this kind of inline obligation marking |
| SR-04 | The security comment in Phase 3 is documentation-only (no logic change), but its content ("caller MUST apply SecurityGateway::is_quarantined()") creates an implicit contract that tests cannot verify by reading the comment alone — if the comment text diverges from the actual SecurityGateway call site over time, the contract becomes stale silently | Low | Low | Architect should consider whether a `#[doc]` attribute or test assertion is more durable than an inline comment |

## Integration Risks

| Risk ID | Risk | Likelihood | Severity | Recommendation |
|---------|------|------------|----------|----------------|
| SR-05 | The v19→v20 migration back-fills edges into GRAPH_EDGES that the crt-042 `graph_expand` will immediately traverse once `ppr_expander_enabled=true`; if the migration runs against a DB that already has some reverse edges (re-run scenario), the `NOT EXISTS` guard prevents duplicates, but `INSERT OR IGNORE` is the actual safety net — both mechanisms must remain in the spec per C-05 | Med | Low | Spec writer should include an explicit idempotency AC (already AC-07) and a test asserting no duplicate rows after two migration runs |
| SR-06 | Three separate tick functions each require a symmetric second `write_graph_edge` call — the pattern is identical to `co_access_promotion_tick.rs` (two-call pattern), but if any one tick is missed, graph asymmetry reappears only for that source; crt-042 SR-03 gate confirmed the prior 0-bidirectional-pairs state went undetected for multiple feature cycles | High | Med | Architect should specify a post-tick assertion or integration test that verifies bidirectionality per source after a tick run — not just after migration |

## Assumptions

- **SCOPE.md §Proposed Approach / Phase 2**: Assumes `write_graph_edge` is called directly in all three tick functions and that adding a second call is sufficient. This holds only if the function signature accepts the swapped args without type coercion — validated by the co_access_promotion_tick pattern, but the spec should confirm exact argument types.
- **SCOPE.md §Background Research / crt-035 Precedent**: Assumes entry #3889's template SQL is directly portable to S1/S2/S8. Valid for S8 CoAccess; for S1/S2 Informs the `relation_type` value differs — the combined block must use `'Informs'` not `'CoAccess'`. A copy-paste error here is the highest-probability implementation mistake.
- **SCOPE.md §Non-Goals**: Assumes `source='nli'` and `source='cosine_supports'` Informs edges are correctly excluded by the `source IN ('S1','S2')` filter. This is correct but depends on those sources never being renamed.

## Design Recommendations

- **SR-06 / SR-02**: Architect should specify that all three tick tests assert both `(a→b)` and `(b→a)` exist in GRAPH_EDGES after a single tick run against a two-entry fixture — not just that no error was returned. This directly addresses the "zero mandatory tests delivered" pattern seen in crt-042 (entry #4076).
- **SR-01**: Spec writer should add an explicit note to the S8 tick spec that `pairs_written` now counts edges (not logical pairs) and that 2× values are expected and correct.
- **SR-05**: Spec writer should include a two-run idempotency test in the migration test suite — running the v19→v20 block twice must produce the same row count as running it once. This is AC-07 territory but should be an explicit test, not just an assertion.
