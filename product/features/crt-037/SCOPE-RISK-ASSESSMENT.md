# Scope Risk Assessment: crt-037

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | NLI neutral score is not a reliable `Informs` signal — the neutral band captures unrelated pairs as readily as genuine empirical-to-normative bridges | High | Med | Architect must specify a composite guard: neutral > 0.5 AND cosine ≥ 0.45 AND temporal AND cross-feature. No single predicate is sufficient. |
| SR-02 | Separate HNSW scan at 0.45 doubles candidate fan-out in ticks with large active-entry pools, risking tick duration inflation disproportionate to `Informs` edge value | Med | Med | Architect must bound the Informs candidate slice explicitly before NLI scoring (distinct from the shared cap); spec must include a tick-duration regression gate. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Combined cap with Informs as second-priority (resolved OQ-1) can silently starve Informs detection in high-churn ticks, producing zero Informs edges with no observable signal | Med | Med | Spec must require an observable metric (logged count) for Informs candidates processed vs. cap-dropped per tick. |
| SR-04 | `informs_category_pairs` default config embedding four software-engineering pairs is the only domain coupling surface — but adding a fifth pair is a scope-free extension that risks growing before v1 ships | Low | Low | Spec must explicitly freeze the default pair list at four. Expansion is deferred work. |
| SR-05 | `graph_penalty` / `find_terminal_active` must not traverse `Informs` edges (AC-24 / SR-01 invariant). A future crt-03x that adds a new penalty traversal path could silently include `Informs` without a guard. | Med | Low | Architecture must document the `Supersedes`-only penalty invariant as an explicit boundary, not just an acceptance criterion. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | crt-036 is a logistical dependency (must merge first). If crt-036 slips, crt-037 delivery cannot begin regardless of zero technical blocking in detection logic (SCOPE.md §Constraints). | Med | Low | Track crt-036 merge status at delivery gate-in; do not begin Phase C work until crt-036 is on main. |
| SR-07 | PPR direction semantics: `Informs` edge direction is source→target (past→future). Unimatrix entry #3744 confirms PPR uses `Direction::Outgoing` for reverse walk — correct behavior depends on this being consistent across all four `edges_of_type` calls. A fourth call using wrong direction silently produces no mass flow from lessons to decisions. | High | Low | Architect must confirm the Outgoing-is-reverse-walk contract applies symmetrically to the new `Informs` call; spec must have a PPR propagation AC verifying non-zero score on the lesson node (AC-05 already exists — confirm it tests direction, not just non-zero). |
| SR-08 | Merged Phase 7 rayon batch with discriminator tag (resolved OQ-2): if the tag routing in Phase 8/8b diverges from how Phase 4b metadata is attached, Informs pairs may be silently routed to the Supports write path (entailment threshold) and dropped. | Med | Med | Architect must specify the discriminator tag struct and routing table explicitly; spec must cover the cross-routing failure case. |

## Assumptions

- **SCOPE.md §Background/Codebase State**: Assumes `GRAPH_EDGES.relation_type` is a free-text column with no CHECK constraint. If a constraint was added post-ASS-034 analysis, "Informs" insertion would silently fail or error without a schema migration.
- **SCOPE.md §Proposed Approach Phase C**: Assumes `NliScores.neutral` is populated for all pairs scored by the existing cross-encoder. If the model returns only entailment/contradiction (two-class output), neutral may be `1 - entailment - contradiction` and carry higher noise than expected.
- **SCOPE.md §Background/PPR direction semantics**: The reverse-walk behavior depends on the `Direction::Outgoing` contract holding for `Informs`. Entry #3744 confirms this is the actual (not documented) behavior — the spec must verify this, not just assume it.

## Design Recommendations

- **SR-01 + SR-08**: Define the composite guard as a typed struct passed from Phase 4b through to Phase 8b — not as parallel lists matched by index. Misaligned index is the failure mode. (Entry #1616 pattern: write guard data before marking applied.)
- **SR-02 + SR-03**: Specify the Informs candidate slice cap as a distinct config field (e.g., `max_informs_candidates_per_tick`) subordinate to `max_graph_inference_per_tick`, with logged discard count. Avoids both tick inflation and silent starvation.
- **SR-07**: Spec test AC-05 should explicitly assert the lesson node receives PPR mass — not just that mass is non-zero somewhere. The direction inversion makes this easy to get wrong silently.
