# Scope Risk Assessment: crt-018b

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `EffectivenessState` is in-memory only; a server restart resets `consecutive_bad_cycles`, deferring auto-quarantine by N×15 min post-restart. Under frequent restart conditions (deploys, crashes), the N-cycle guard may never fire. | Medium | Medium | Architect should document the restart-reset semantic explicitly in the implementation and ensure the env-var default (3 cycles) reflects this. Consider whether zero-persistence is acceptable for the initial rollout. |
| SR-02 | HashMap clone on every `search()` call. SCOPE states ~32KB for 500 entries and must stay under 1ms. As entry count grows toward thousands, this clone cost grows linearly and the budget assumption becomes stale. | Medium | Low | Architect should introduce a snapshot version counter or generation tag so readers can skip the clone when nothing changed since their last snapshot, rather than always cloning. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Auto-quarantine is irreversible without manual intervention (`context_quarantine` has no undo in the happy path — restore requires a separate operation). If the effectiveness classifier produces a false positive (e.g., a temporarily under-voted Effective entry cycles through Noisy), the entry is silently removed from retrieval. | High | Medium | Spec writer must add an AC requiring the audit event to include enough context (category history, cycle count, entry title) for operators to diagnose and restore. The scope should clarify whether `UNIMATRIX_AUTO_QUARANTINE_CYCLES=0` is the recommended safe default for new deployments. |
| SR-04 | `UTILITY_BOOST`/`UTILITY_PENALTY` constants are defined in `unimatrix-engine::effectiveness` but the SCOPE does not address how they interact with the adaptive confidence weight from crt-019 (`clamp(spread * 1.25, 0.15, 0.25)`). At low spread the confidence term is weak; the fixed ±0.05 utility delta will dominate relative to confidence. The interaction is implicit, not explicit. | Medium | High | Spec writer should add a constraint asserting the final score formula with all active signals shown together, not piecewise. Verify the ±0.05 magnitude assumption holds across the full crt-019 spread range (0.15–0.25 confidence weight). |
| SR-05 | The Settled category receives +0.01 but SCOPE section "Non-Goals 3" states classification logic is unchanged. If a knowledge base has many Settled entries (historically common at product maturity), the Settled boost becomes the dominant differentiator between entries of equal confidence, which may not be the intended signal. | Low | Medium | Spec writer should add an AC or note clarifying that SETTLED_BOOST is intentionally smaller than co-access boost (0.03) so it doesn't overwhelm the signal hierarchy. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `BriefingService` will receive `EffectivenessStateHandle` via constructor, but the SCOPE also states semantic search in briefing already delegates to `SearchService` (which benefits from Change 2). The injection-history and convention sort paths bypass `SearchService` entirely, meaning those two paths get effectiveness only if the constructor wiring is done. If the constructor wiring is missed or deferred, the briefing semantic path gains effectiveness but the injection-history path regresses silently. | Medium | Medium | Architect should specify that `BriefingService::new()` takes `EffectivenessStateHandle` as a required parameter (not optional), making incomplete wiring a compile error rather than a silent omission. |
| SR-07 | Background tick error recovery: if `compute_report()` returns an error (e.g., transient SQLite lock), `EffectivenessState` is not updated. The old classifications remain stale for another 15 minutes. If bad classifications linger, `consecutive_bad_cycles` keeps incrementing and auto-quarantine may trigger on stale data. Pattern #1366 identified this class of bug in tick loops. | High | Medium | Architect must specify the failure semantics: on `compute_report()` error, should `consecutive_bad_cycles` increment, hold, or reset? Recommend: hold (do not increment) on tick error, and emit a structured audit event when a tick is skipped. |
| SR-08 | crt-019 is listed as a hard dependency (confidence spread established before adding utility signal). crt-019 is marked fully merged in the spawn prompt, but the SCOPE does not verify that crt-019's adaptive weight is enabled and active in the environment where crt-018b will be tested. | Low | Low | Spec writer should add an integration test prerequisite check that crt-019's confidence spread is non-zero in the test fixture, confirming the dependency is exercised. |

## Assumptions

- **SCOPE §Background Research / ConfidenceState pattern**: Assumes the `Arc<RwLock<_>>` pattern from `ConfidenceState` is directly reusable without modification. Risk: if `ConfidenceState` wiring was added to `UnimatrixServer` in a way that makes adding a second handle non-trivial (e.g., field ordering, service initialization order), the architect needs to verify constructor order.
- **SCOPE §Change 4 / spawn_blocking**: Assumes the auto-quarantine SQLite write can complete within the `spawn_blocking` budget of the maintenance tick. If quarantine involves multiple entries in one tick (e.g., a bulk auto-quarantine event on first deployment post-upgrade), the tick may run long.
- **SCOPE §Non-Goals 8**: Assumes no retroactive quarantine on first deployment. This is a safe assumption IF the consecutive counter starts at 0 on startup. If there is any path where the counter could be initialized from existing classification data, this assumption is violated.

## Design Recommendations

- **SR-07 (Critical)**: Define and document the tick error semantics for `consecutive_bad_cycles` before implementation. A silent increment on error is the most dangerous failure mode.
- **SR-03 (High)**: Make the auto-quarantine audit event rich enough for operator recovery. Consider a status report field listing recent auto-quarantines (the SCOPE already proposes `auto_quarantined_this_cycle` — confirm it survives into spec).
- **SR-04 (Medium)**: Add a combined scoring formula to the spec with all active signals shown simultaneously so test authors can verify the full magnitude range, not just individual deltas.
- **SR-06 (Medium)**: Require `EffectivenessStateHandle` as a non-optional constructor parameter on `BriefingService` to make incomplete wiring a compile error.
