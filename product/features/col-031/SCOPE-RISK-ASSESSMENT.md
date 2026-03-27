# Scope Risk Assessment: col-031

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `json_each` expansion of `result_entry_ids` (JSON integer array) may behave differently than expected — integer vs. string element typing in SQLite's `json_each` is a known portability trap. Existing usage in `knowledge_reuse.rs` must be confirmed to match this new call site exactly. | High | Med | Architect must verify the `json_each.value` cast form against a live `query_log` row before finalizing the store query; do not rely on assumption from SCOPE.md §Open Question 2. |
| SR-02 | `w_phase_explicit = 0.05` default is calibrated by judgment, not a research spike. Prior lesson (#3208) shows weight defaults validated only against vision prose (not a spike) required rework in crt-026. ASS-032 is cited for the decision but the SCOPE.md §Background Research does not cite a numerically derived value from ASS-032 for 0.05 specifically. | Med | Med | Architect/spec writer should retrieve ASS-032 from Unimatrix and confirm 0.05 has empirical grounding; if not, document the accepted risk explicitly in the ADR. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The eval harness fix (`extract.rs` adding `current_phase`) is in scope but is in a different crate/subsystem than the feature's primary implementation. Scope coupling between the eval tooling fix and the scoring feature creates a risk that one can be declared done while the other is incomplete, making AC-12 a vacuous gate. | High | Med | Spec writer must gate AC-12 explicitly on AC-16 (eval harness fix) being complete first; they are not independently shippable. Delivery protocol should treat them as a single wave. |
| SR-04 | Phase vocabulary is runtime strings with no compile-time validation. A phase name mismatch between what `current_phase` supplies and what `query_log.phase` stored (e.g. case differences, renamed phases) silently degrades to cold-start rather than erroring — operator has no visibility into mismatch. | Med | Low | Architect should consider whether a diagnostic log line or status field is warranted; at minimum the spec should document the silent-degradation contract explicitly. |
| SR-05 | PPR wire-up (AC-07, AC-08) is conditional on #398 not being shipped. If #398 ships concurrently or just before col-031, the integration boundary could produce merge conflicts or duplicated work at `phase_affinity_score` call sites. | Low | Low | Track #398 status at delivery start; confirm wire-up AC applicability before implementation wave. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `PhaseFreqTableHandle` adds a third `Arc<RwLock<_>>` to the hot path in `SearchService` (alongside `TypedGraphStateHandle` and `EffectivenessStateHandle`). Lock contention risk under concurrent search load is low given read-dominance, but the SCOPE.md sole-writer contract must be enforced — tick holds write lock while background maintenance also holds other locks. Lock ordering must be audited at design time to prevent deadlock. | High | Low | Architect must document lock acquisition order for all three handles in the architecture; ensure background tick never holds `PhaseFreqTable` write lock while acquiring any other write lock. |
| SR-07 | Background tick TICK_TIMEOUT is shared across all rebuild tasks. The frequency table SQL aggregation is claimed to be <5ms at 20K rows, but that estimate assumes a warm SQLite page cache. On first tick after server start (cold cache, large `query_log`), the query may be significantly slower and consume a material fraction of TICK_TIMEOUT. | Med | Low | Architect should add a note to instrument tick timing for the rebuild step; spec should include a non-regression constraint that the rebuild does not materially increase tick wall time at representative `query_log` sizes. |

## Assumptions

- **§Constraints "col-028 must be shipped"**: Assumes schema v17 with `query_log.phase` is present. Confirmed shipped (gate-3c PASS 2026-03-26). If this assumption is wrong, no data exists and the feature is entirely cold-start — which is safe by design but makes AC-12 a noise check.
- **§Background Research "At 20K query_log rows completes in <5ms"**: Assumes warm SQLite page cache and a representative data distribution. Not benchmarked against a production-size `query_log` with many phases and categories.
- **§Open Questions #2**: Assumes `json_each` expansion works correctly for integer-typed JSON arrays — explicitly flagged as unverified in the SCOPE.md itself.
- **§Proposed Approach "phase_affinity_score returns 1.0 (neutral) for absent entries"**: Assumes 1.0 neutral is correct for PPR personalization vector semantics. If PPR expects 0.0 for unknown entries rather than 1.0, the integration contract produces wrong behavior silently.

## Design Recommendations

- **SR-01**: The store query method design must pin the exact `json_each` SQL form to a verified working example. This is the single highest implementation-surprise risk.
- **SR-02**: Retrieve ASS-032 research spike before finalizing architecture; confirm 0.05 is empirically grounded or explicitly accept and document the calibration risk in the ADR (per #3208).
- **SR-03 + SR-06**: Architecture doc must define the tick lock acquisition sequence and treat eval harness fix as a non-separable deliverable from the scoring activation.
