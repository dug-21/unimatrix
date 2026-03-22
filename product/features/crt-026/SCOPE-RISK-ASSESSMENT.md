# Scope Risk Assessment: crt-026

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ~~`w_phase_histogram=0.005` signal too weak~~ **RESOLVED** — ADR-004 raises default to `0.02` (ASS-028 calibrated value, full session signal budget). Signal is detectable at realistic concentrations; AC-12 asserts score delta `≥ 0.02` at p=1.0. | ~~High~~ Low | ~~High~~ Low | No further action required. |
| SR-02 | `InferenceConfig::validate()` sum-invariant: adding `w_phase_histogram=0.02` brings total to 0.97. `validate()` must confirm this is within `<= 1.0`. | Low | Low | **RESOLVED** — architect confirmed `validate()` checks only the six original fields (sum remains 0.95); 0.97 total is not included in the sum check. No test asserts `sum == 0.95` against defaults. |
| SR-03 | ~~`w_phase_histogram=0.005` too small for W3-1 gradient signal~~ **RESOLVED** — `0.02` is the ASS-028 calibrated value, a meaningful cold-start seed. W3-1 has sufficient gradient signal from day one. | ~~Med~~ Low | ~~Med~~ Low | No further action required. |

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

- **SR-01, SR-03**: ~~Resolved via ADR-004~~ — `w_phase_histogram=0.02`. AC-12 asserts delta `≥ 0.02` at p=1.0. W3-1 cold-start seed is meaningful.
- **SR-02**: ~~Resolved~~ — `InferenceConfig::validate()` checks only original six fields; 0.97 total passes without touching existing defaults.
- **SR-04**: Spec writer must resolve the AC-07 ambiguity before the spec is finalized — either drop AC-07 (explicit phase term deferred) or scope it explicitly with a no-op implementation.
- **SR-07, SR-08**: Architect should document the UDS `session_id` origin and assess whether the pre-resolution pattern is forward-compatible with WA-4a proactive injection.
