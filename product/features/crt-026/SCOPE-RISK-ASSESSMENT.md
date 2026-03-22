# Scope Risk Assessment: crt-026

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `w_phase_histogram=0.005` is so small it may produce imperceptible ranking changes in production — signal too weak to validate correctness or detect regressions without synthetic test fixtures that manufacture ideal conditions | High | High | Architect must specify minimum histogram concentration (e.g., one category at ≥60% of stores) required before boost is detectable; AC-12 must define the exact score gap, not just "ranks higher" |
| SR-02 | OQ-01 RESOLVED to integrate inside `compute_fused_score`, but the shipped `FusionWeights` sum-invariant enforces `sum <= 1.0`; adding `w_phase_histogram=0.005` brings the sum to 0.955 — valid, but `InferenceConfig::validate()` must be confirmed to permit this without any default weight adjustment | Med | Med | Architect must confirm `validate()` accepts 0.955 cleanly; if existing tests assert `sum == 0.95` exactly, they will break |
| SR-03 | `w_phase_histogram=0.005` becomes W3-1's cold-start initialization weight for this dimension; if the value is too small to produce a useful gradient signal during GNN training, W3-1 will under-weight this dimension from the outset | Med | Med | Product vision names this as W3-1 initialization — spec writer should note W3-1 dependency and document minimum detectable effect |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | OQ-03 RESOLVED to defer `w_phase_explicit` at `0.0` until W3-1, but `phase_category_weight(category, phase)` is still referenced in AC-07 and SCOPE.md Component 5 — spec writer must clarify whether AC-07 is in scope for crt-026 or is already deferred | High | Med | Spec writer must explicitly mark AC-07 as deferred (w_phase_explicit=0.0 means boost is always 0.0) or remove it; ambiguity here will cause gate failure |
| SR-05 | SCOPE.md Non-Goals explicitly exclude `context_briefing`, but WA-4b (product vision) uses category affinity boost in briefing. If crt-026 scopes `SearchService` changes only to `context_search`, WA-4b will require re-opening this code path — scope boundary coupling risk | Low | Low | Architect should confirm affinity boost entry point is generic enough for WA-4b reuse; no code change needed in crt-026 |
| SR-06 | Duplicate-store guard at AC-02 (histogram not incremented on duplicate) relies on `insert_result.duplicate_of.is_some()` — this is the correct gate, but the OQ-02 snapshot pattern means the handler reads session state before the insert; a concurrent duplicate in the same session could be double-counted if two stores race on the same entry | Low | Low | Spec writer should note the fire-and-synchronous lock hold makes this window effectively zero; confirm in test |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | OQ-02 RESOLVED: histogram pre-resolved in handler before `ServiceSearchParams` construction. But WA-4a (proactive injection) resolves candidates WITHOUT a user query — its session context access path is different. If the histogram pre-resolution pattern is handler-specific, WA-4a cannot reuse it and will need `Arc<SessionRegistry>` on `SearchService` anyway | Med | Med | Architect should confirm the pre-resolution pattern is sufficient for WA-4a or flag that WA-4a will re-open this decision |
| SR-08 | UDS search path (`handle_context_search` in `uds/listener.rs`) must also pass `session_id` per OQ-04. The UDS path has no `audit_ctx` equivalent — architect must identify where `session_id` originates in that path | Med | Med | Architect must trace the UDS session_id source; this is a non-trivial integration point not fully described in SCOPE.md |
| SR-09 | Historical evidence (#2964): additive boosts applied as post-pipeline passes caused NLI override in WA-0. OQ-01 resolves this by integrating inside `compute_fused_score` — but the `status_penalty` multiplier is applied AFTER `compute_fused_score`. If `status_penalty` is applied before the affinity boost, a penalty could reduce a relevance boost intended to surface a borderline entry | Low | Low | Spec writer must clarify the exact application order: `fused_score * status_penalty + affinity_boost` vs. `(fused_score + affinity_boost) * status_penalty` |

## Assumptions

- **SCOPE.md §Background/What Already Exists**: Assumes `session_id` is reliably present on all `context_search` calls that should receive the boost. Sessions that call search without `session_id` silently receive no boost — acceptable, but means the feature is invisible to unregistered callers.
- **SCOPE.md §Constraints/Cold-start**: Assumes empty histogram = zero boost = exact parity with current behavior. This holds only if the boost application path short-circuits on empty histogram before touching `FusedScoreInputs` — spec must enforce this.
- **SCOPE.md §Resolved Decisions/OQ-03**: Assumes `w_phase_explicit=0.0` fully disables the explicit phase term with no code path reaching `phase_category_weight`. If the function is called with weight=0.0, it is a no-op, but the mapping table coupling concern (SCOPE.md §Proposed Scope/Affinity Boost Formula) still requires a resolution — an empty or no-op mapping table or a guard before calling the function.

## Design Recommendations

- **SR-01, SR-03**: Spec writer should define AC-12 with a concrete score delta (e.g., "boost delta ≥ `w_phase_histogram * 1.0 = 0.005`") and note that this weight is W3-1's initialization value for this dimension.
- **SR-02**: Architect must confirm `InferenceConfig::validate()` permits `sum=0.955`; if any existing test asserts the exact pre-WA-2 sum, it will need updating.
- **SR-04**: Spec writer must resolve the AC-07 ambiguity before the spec is finalized — either drop AC-07 (explicit phase term deferred) or scope it explicitly with a no-op implementation.
- **SR-07, SR-08**: Architect should document the UDS `session_id` origin and assess whether the pre-resolution pattern is forward-compatible with WA-4a proactive injection.
