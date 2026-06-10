# Scope Risk Assessment: crt-053

Active-only PPR expansion seeds — one surgical filter on `seed_ids` (`services/search.rs` ~`:915`). Risks below are scope/product-level only; architecture risks follow in architecture-risk mode.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **The platform cannot measure search-heuristic effectiveness** (ass-074 #721, Unimatrix #4888). Eval harness scores positive relevance only — no "forbidden-absent" or "rank-below" gate. Correctness of the filter cannot be empirically validated against "better agent results." | High | High (confirmed) | Architect: accept behavior-based acceptance (presence/exclusion of specific IDs) as the only validation lever; do NOT scope an eval-harness gate. Route the trust assertion to the Python integration suite or a post-processor over raw `entries` JSON. |
| SR-02 | **Filter must keep 6b terminal-active heads while dropping deprecated seeds.** At `:915` `results_with_scores` mixes deprecated/superseded entries (`:770`), 6b-injected terminal actives (`:814–821`, already `status==Active`), and HNSW actives. A naive filter could drop legitimate active anchors. | High | Med | Spec: filter predicate is `status == Active` on the seed set only — terminal-active heads pass by construction; superseded entries are excluded. Define the predicate against the `Status` enum, not string compare. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | **Adjacent "gaps" tempt scope creep** — injection-path penalty bypass, vnc-017 50-edge redirect ceiling, #585 edge hygiene, #406 multi-hop test, deprecated-in-Flexible. SCOPE.md locks all of these closed. History shows this exact failure: vnc-018 inverted write-only PPR negative tests without an ADR (Unimatrix #4495), caught only at product-owner review. | High | High | Architect/spec: encode C-01 (single production change) as a hard boundary. No `find_terminal_active` on injected entries, no `penalty_map` extension, no steepness work (Q6/Q8 dropped). Any in-scope item that looks wrong → stop and raise, do not fix. |
| SR-04 | **Default-off path must be bit-for-bit identical** (C-02, `ppr_expander_enabled = false`). The filter sits inside the `if self.ppr_expander_enabled` block — risk is the predicate or a shared helper leaking cost/behavior into the off path. | Med | Low | Spec: assert zero behavior delta when expander off; place the filter strictly inside the enabled branch (post `:914`), touching no shared seed/candidate structures used by the off path. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | **Status-guard correctness is invisible without a supersession false-positive test** (Unimatrix #4536). A deprecated-superseded-by-cited-active chain can silently slip past or over-filter; the guard's effectiveness on that specific case is untestable by default. | Med | Med | Spec: require a fixture with deprecated A (superseded by active B, both with positive out-edges) asserting BFS expands from B's path, not A's — the exact case SCOPE.md Validation calls for. |
| SR-06 | **Graph direction/semantics mis-description** (Unimatrix #4077 — crt-030/crt-042 BFS-vs-PPR prose traps). graph_expand is BFS-forward (seed B, edge B→X surfaces X). A risk doc or test framed in reverse-walk PPR language will mis-assert what the filter excludes. | Med | Med | Architect/spec: describe the seed-filter behavioral contract with concrete edge examples; verify by outcome, never by `Direction::` enum inspection. The filter narrows seeds — it does not change traversal direction. |

## Assumptions

- **(SCOPE.md §"Research provenance", ass-074)** The leak is *latent* today because the prod graph is all Active→Active. Filter value depends on deprecated entries eventually acquiring positive out-edges. If #585 (edge hygiene) keeps the edge graph deprecated-free at write time, the filter is belt-and-suspenders — still correct, lower urgency. Not invalidating, but right-sizes severity.
- **(SCOPE.md §"Disposition", #406)** #406 does not reproduce in the eval-graph rebuild; treated as a test/snapshot artifact, not a retrieval fix. If it *does* reproduce in the delivery fixture, that is a signal the fixture differs from ass-073's — raise, do not patch retrieval.
- **(SCOPE.md §"The change")** `seed_ids` is built from `results_with_scores` post-6a/6b. Assumes that collection is the sole seed source for `graph_expand`. Confirmed at `:915` — the only seed build inside the expander branch.

## Design Recommendations

- **R1 (SR-01):** No eval-harness acceptance gate. Acceptance = behavior assertions on seed inclusion/exclusion in the integration suite. State this in the spec so the tester does not chase an unmeasurable metric (#500/soft-GT P@5 trap).
- **R2 (SR-02, SR-05):** Specify the predicate as `Status::Active` on the seed set, and mandate the deprecated-superseded-by-active fixture as the primary acceptance test.
- **R3 (SR-03):** Carry SCOPE.md's five Locked Decisions verbatim into ARCHITECTURE.md constraints. The #4495 precedent makes this a real, recurring failure mode — make the boundary structurally hard to cross.
- **R4 (SR-04):** Architecture must show the filter lives entirely inside the `ppr_expander_enabled` branch with a stated off-path equivalence guarantee.
