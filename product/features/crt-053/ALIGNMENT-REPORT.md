# Alignment Report: crt-053

> Reviewed: 2026-06-10
> Artifacts reviewed:
>   - product/features/crt-053/architecture/ARCHITECTURE.md
>   - product/features/crt-053/specification/SPECIFICATION.md
>   - product/features/crt-053/RISK-TEST-STRATEGY.md
> Scope source: product/features/crt-053/SCOPE.md (LOCKED, rescoped 2026-06-10 after ass-073 #720, ass-074 #721)
> Scope risk basis: product/features/crt-053/SCOPE-RISK-ASSESSMENT.md (SR-01..SR-06)
> Vision source: product/PRODUCT-VISION.md
> Strategic goals: #4677 (self-learning), #4673 (proactive-delivery)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances self-learning intelligence goal; honors graceful-degradation, in-memory hot-path, capability-gate principles |
| Milestone Fit | PASS | Cortical (crt) search-quality correctness; no future-milestone capability built |
| Scope Gaps | PASS | All SCOPE.md deliverables (the one filter + 3 validation arms) addressed; nothing dropped |
| Scope Additions | PASS | Zero additions; every locked exclusion carried verbatim into all three docs |
| Architecture Consistency | PASS | ARCHITECTURE, SPECIFICATION, RISK-TEST-STRATEGY mutually consistent; single edit site, off-path equivalence, enum predicate all concordant |
| Risk Completeness | PASS | SR-01..SR-06 fully traced to R-01..R-12; vacuous-pass and direction-semantics traps covered with differential control arms |

Status counts: PASS 6, WARN 0, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | SCOPE.md's single deliverable — active-only `seed_ids` filter at `search.rs:~915` inside the `ppr_expander_enabled` branch — is specified in FR-01/FR-08, designed in ARCHITECTURE "The Change", and covered by AC-01/AC-04/AC-05. All 3 SCOPE Validation items map to acceptance criteria. |
| Addition | (none) | No source document adds behavior beyond the seed filter. The five Locked Decisions and the four-issue disposition (#704/#406/#585/#405) are carried verbatim. SPECIFICATION "NOT in Scope" and ARCHITECTURE "Explicitly NOT designed" both enumerate the excluded paths. |
| Simplification | Behavior-based validation only (no eval-harness gate) | Rationale (SR-01, ass-074 #721, Unimatrix #4888): the platform cannot measure graph-relational search-heuristic effectiveness. NFR-05 and AC verification routes assert ID-level presence/absence, not P@5/MRR. This is a SCOPE-mandated simplification, not an unexplained omission. |

## Variances Requiring Approval

None. No VARIANCE or FAIL findings. The feature is a deliberately minimal, human-judged surgical change; the source documents implement exactly that intent and add nothing.

## Detailed Findings

### Vision Alignment

The feature directly advances the **Self-learning intelligence** goal (#4677): "every deployment improves its own retrieval quality." crt-053 corrects a latent defect in the graph-relational layer of the relevance function — PPR expansion seeding from deprecated entries — so expansion anchors on current knowledge. This is squarely the "intelligence pipeline" surface the vision names as the core (PRODUCT-VISION.md lines 24-25).

It is consistent with the vision's distinction between dynamic knowledge (decisions/patterns/lessons that evolve) and static tooling: anchoring expansion on `Status::Active` entries is exactly "managing the dynamic layer" (line 13). It also reinforces the **Proactive knowledge delivery** goal (#4673) indirectly: PPR injection feeds the candidate pool that briefings/injection draw from, so cleaner seeds improve proactive surfacing without changing delivery mechanics.

Architectural principles check:
- **Graceful degradation (principle 5):** honored. FR-07/AC-02 guarantee the `ppr_expander_enabled = false` path is bit-for-bit identical — absent/disabled capability = previous behavior.
- **In-memory hot path (principle 7):** honored. NFR-04 confirms the predicate reads the already-loaded `EntryRecord.status` in-memory; no DB read at query time.
- **Capability checks at service layer (principle 3) / quarantine:** honored and explicitly protected. RISK-TEST-STRATEGY R-11 guards the `:950` quarantine gate against conflation with the seed filter; the seed predicate dropping Quarantined seeds is documented as defense-in-depth, not a replacement.
- **Typed relationship graph (principle 4):** the feature operates on `graph_expand` (forward BFS over positive edges) and leaves traversal semantics, direction, depth, and excluded edge types unchanged (FR-05).

No principle is contradicted, and none is improperly skipped.

### Milestone Fit

Correct phase. This is Cortical (`crt`) work — "Learning & drift," the search-quality/relevance surface. SCOPE.md frames it as "the bread-and-butter of the platform... search-result quality is the single most critical element." No future-milestone capability is pulled forward: ARCHITECTURE "No new components, modules, structs, traits, config flags, or files. No new function." The deliberately-deferred items (#585 edge hygiene, vnc-017 ceiling, injection-side penalty from #4887) are tracked separately rather than built early — this is milestone discipline, not a variance.

### Architecture Review

ARCHITECTURE.md is tightly scoped and internally consistent with both SCOPE.md and the risk assessment:
- The change is a single predicate (`e.status == Status::Active`) on the `seed_ids` build, shown with before/after at the exact site (`:915`), inside the `ppr_expander_enabled` branch.
- The Component Breakdown marks every other component (`graph_expand`, `Status` enum, `penalty_map`/Step 7, 6b injection) as **Unchanged**, matching C-01.
- The "Off-Path Equivalence Guarantee" is structural (lexical scope), not disciplinary — the strongest possible form of the C-02 guarantee, and it correctly addresses SR-04/R-05.
- The five Locked Decisions are carried verbatim as binding constraints with the #4495 precedent cited — the structural defense against the SR-03 scope-creep failure mode.
- Predicate Design correctly handles the SR-02 trap: 6b terminal-active heads pass "by construction" because `:814` already guards `status == Active`; the filter cannot drop a legitimate active anchor.

One worth noting (not a variance): Unimatrix pattern #4886 (the crt-053 scoping reconciliation) warns that the live scoring path is rewritten frequently and spike premises rot. The architecture pins concrete line numbers (`:814`, `:915`, `:950`) and signatures; OQ-2 in the spec correctly flags that `results_with_scores` being the sole seed source must be confirmed at delivery. This is appropriately handled as an open question, not assumed.

### Specification Review

SPECIFICATION.md FR-01..FR-08 and AC-01..AC-05 fully cover SCOPE.md's deliverable and all three Validation items, with no behavior beyond the seed filter:
- FR-01/FR-02 = the predicate (enum-based, SR-02).
- FR-03 = terminal-active retention (SR-02).
- FR-04/FR-05/FR-06 = downstream, traversal, and HNSW ranking unchanged (C-01/C-03).
- FR-07/NFR-01 = default-off equivalence (C-02).
- The **anti-AC** (no test may assert deprecated *absence* from Flexible) directly enforces Locked Decision 1 / C-03 and is the precise guard against the two-mode-design violation.
- The "NOT in Scope" section enumerates all five exclusions plus the four-issue disposition — no scope addition slips through.

The ubiquitous-language table correctly distinguishes "superseded-but-still-Active" (retained — discriminator is `status`, not `superseded_by`) from non-active statuses (dropped). This is the exact misread Unimatrix #4536 / SR-05 warn about, and it is handled correctly.

### Risk Strategy Review

RISK-TEST-STRATEGY.md is complete and proportionate to a one-line change in the most sensitive code in the system. Every scope risk SR-01..SR-06 is traced to architecture risks R-01..R-12 in the Scope Risk Traceability table. Notable strengths:
- **R-04 (vacuous-pass, #4902):** mandates a differential/control arm — with the filter removed, the deprecated-only neighbor MUST reappear — so the absence assertion is provably filter-caused. This is the single most important test-design defense for this feature and it is present.
- **R-01 (unmeasurability, #4888):** explicitly forbids any eval-harness metric gate as acceptance, matching SR-01/NFR-05 and the #500 soft-GT trap — gating on P@5 would reject a correct change.
- **R-03 (scope creep, #4495):** diff-scope gate treats "any of the five exclusions touched" as automatic fail, with the vnc-018 precedent cited.
- **R-07 (direction semantics, #4077/#3744):** fixtures must state edge direction concretely and assert on neighbor IDs, never on `Direction::` enum — closing SR-06.
- Edge cases (all-seeds-deprecated empty set, Proposed/Quarantined dropped, superseded-but-Active retained, accepted >50-edge residual) are enumerated, including the explicit "assert nothing here" for the knowingly-accepted residual.

Coverage matches the vision's architectural principles: the security section confirms no new attack surface and protects the quarantine gate (principle 3); the determinism NFRs preserve the in-memory hot-path contract (principle 7).

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns -- found #2298 (config/vision semantic-divergence, not applicable here), #4886 (crt-053 scoping reconciliation — research-spike premises rot against live scoring path; directly relevant, confirmed the docs pin concrete line numbers and defer seed-source confirmation to OQ-2), #3746 (search.rs step-ordering gotcha — informs delivery, not alignment).
- Stored: nothing novel to store -- the alignment-relevant patterns (research-spike drift #4886, scope-creep-without-ADR #4495, unmeasurable-heuristic #4888, vacuous-pass #4902) are already captured. The crt-053 variances are feature-specific (this is a deliberately surgical, fully-locked feature with zero scope additions), so no new generalizable vision-alignment pattern emerged. The reusable meta-observation — "a SCOPE.md that pre-locks exclusions verbatim produces source docs with zero scope additions" — is already implied by the #4495 lesson and the existing locked-decision discipline.
