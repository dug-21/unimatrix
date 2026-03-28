# Scope Risk Assessment: col-031

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `run_single_tick` constructs services directly, bypassing `ServiceLayer`. Past bug (#3216, dsn-001 GH #311): new Arc handles threaded to `ServiceLayer` were silently unused in the tick because it is an independent construction site. `PhaseFreqTableHandle` must be threaded through BOTH paths. | High | High | Make `PhaseFreqTableHandle` a required (non-optional) constructor parameter in every consuming service. Grep for all direct instantiation sites of `SearchService` in `background.rs` before declaring wiring complete. |
| SR-02 | `w_phase_explicit = 0.05` raises the fused scoring total to 1.02 (outside the six-weight sum constraint). The ADR-004 (crt-026) additive exemption means `validate()` is unchanged — but any future weight-sum assertion or documentation that assumes the sum = 1.0 will silently be wrong. | Med | Med | Architect must update the FusionWeights sum-check comment to state 0.95 + 0.02 + 0.05 = 1.02 and confirm `validate()` requires no change. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | AC-12 (regression gate) is explicitly non-separable from AC-16 (eval harness fix). If delivery splits these into separate waves, AC-12 will be declared passing against `current_phase = None` scenarios — a vacuous gate. Entry #3683 tags this risk directly. | High | High | Spec writer must add a hard gate-ordering constraint: AC-12 cannot be validated until AC-16 is present in the same wave and its output is verified to carry non-null `current_phase` values in scenarios. |
| SR-04 | Phase vocabulary is runtime strings with no compile-time enum. A phase rename in the workflow layer silently strands all historical frequency data under the old key; the new key starts cold. No alerting or detection is in scope. | Med | Low | Architect should note that cold-start fallback (`use_fallback = true`) is the only recovery path. Document the silent-cold-start behavior as an explicit operational characteristic, not a bug. |
| SR-05 | `query_log_lookback_days = 30` is a time window with no cycle alignment. High-volume periods compress many cycles into the window; low-volume periods may span only a fraction of one. The signal quality is therefore session-frequency-dependent, not workflow-phase-representative. | Med | Med | Spec writer should note that #409 (cycle-aligned GC) is the correct long-term fix. The 30-day default is an approximation; consider allowing per-environment override in deployment config. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `phase_affinity_score` is the published API contract for #398 (PPR). Two cold-start return values from one method (1.0 for PPR, 0.0 path for fused scoring) is an unusual pattern. If #398 calls through fused scoring instead of calling `phase_affinity_score` directly, the cold-start semantics will be wrong. | Med | Med | Architect must document the two-caller contract explicitly in the method's doc comment: "Returns 1.0 on cold-start — neutral PPR multiplier. Fused scoring must guard on `use_fallback` before calling this method." |
| SR-07 | Lock acquisition order is constrained: `EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`. This order is implicit in `run_single_tick`; no compile-time enforcement exists. A future tick refactor that reorders acquisitions introduces deadlock risk. | Low | Low | Architect should add a code comment at the tick's lock sequence listing the required order by name. |

## Assumptions

- **SCOPE.md §Background Research / query_log Schema**: Assumes col-028 (schema v17, gate-3c PASS 2026-03-26) is merged and `query_log.phase` column is present in all environments. If not merged in a deployment environment, the rebuild SQL returns zero rows and `use_fallback = true` — degraded but not broken.
- **SCOPE.md §Proposed Approach / ServiceLayer Integration**: Assumes `TypedGraphStateHandle` wiring is the exact template. If that pattern has diverged (e.g., generation counter was added), `PhaseFreqTable` must match the current pattern, not the documented one.
- **SCOPE.md §Acceptance Criteria / AC-12**: Assumes col-030 eval baselines (MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340) remain valid. If the baseline was measured with `w_phase_explicit = 0.0` and those scores included phase-aware runs, raising the weight to 0.05 may alter scores for queries with `current_phase` set.

## Design Recommendations

- **SR-01**: Before writing `ServiceLayer::with_rate_config`, run `grep -r "SearchService::new"` across `crates/unimatrix-server/` to enumerate all construction sites. Thread `PhaseFreqTableHandle` as a required (non-optional) argument to each.
- **SR-03**: Spec writer should make AC-16 a delivery prerequisite for the wave that includes AC-12, not a parallel deliverable. Gate 3b must reject any submission where AC-12 is claimed PASS without verified non-null `current_phase` in eval scenario output.
- **SR-06**: Add a `# Integration Contract` doc-comment block to `phase_affinity_score` naming PPR (#398) as the expected direct caller and fused scoring as the guarded path.
