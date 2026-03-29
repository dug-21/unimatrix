# Alignment Report: crt-030

> Reviewed: 2026-03-29
> Artifacts reviewed:
>   - product/features/crt-030/architecture/ARCHITECTURE.md
>   - product/features/crt-030/specification/SPECIFICATION.md
>   - product/features/crt-030/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-030/SCOPE.md
> Scope risk input: product/features/crt-030/SCOPE-RISK-ASSESSMENT.md
> Re-checked: 2026-03-29 (post Option B correction — RayonPool offload deferred)

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | PPR directly advances the Wave 1A intelligence pipeline goal |
| Milestone Fit | PASS | Squarely in Wave 1A — graph-based relevance propagation feeds W3-1 |
| Scope Gaps | PASS | All SCOPE.md goals, ACs, and constraints addressed |
| Scope Additions | PASS | RayonPool offload branch previously flagged (WARN-01) is now consistently deferred across all three source documents |
| Architecture Consistency | PASS | Component breakdown, lock ordering, latency budget, and SR resolutions are internally consistent; ADR-008 unambiguously defers offload |
| Risk Completeness | PASS | R-01 (offload) correctly classified Deferred with zero test scenarios required; all remaining risks have explicit coverage |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | RayonPool offload deferred (Option B applied) | SCOPE.md Constraints noted the offload as a future consideration. All three source documents now consistently defer it: ARCHITECTURE.md ADR-008 row "deferred to follow-up", SPECIFICATION.md NFR-07 "explicitly deferred and out of scope for crt-030", RISK-TEST-STRATEGY R-01 "Deferred — branch will not be implemented in this feature." No residual tension. |
| Simplification | `ppr_inject_weight` not added separately | SCOPE.md SR-04 raised the option of a separate parameter. Architecture ADR-007 and Specification FR-08 explicitly document the dual-role as intentional with a deferred follow-up. Rationale is present and clear. |
| Simplification | SR-08 / #414 integration test deferred | SCOPE.md SR-08 recommends an AC verifying that #414 data is used when available, not just that fallback works. RISK-TEST-STRATEGY R-10 cites AC-16 as covering this. SPECIFICATION AC-16 requires a unit test with non-uniform phase data; full integration verification against live #414 data is deferred post-merge. Documented in ARCHITECTURE.md open questions item 3. |

---

## Variances Requiring Approval

None. All checks are PASS. No VARIANCE or FAIL classifications. WARN-01 from the prior review is resolved — see WARN-01 resolution note below.

### WARN-01 Resolution: RayonPool Offload Branch — RESOLVED

The prior report flagged a tension: SCOPE.md said "does not require the offload path" while ARCHITECTURE.md scoped it for inclusion with a Critical test requirement in RISK-TEST-STRATEGY.

Human approved Option B: offload deferred, not implemented in crt-030.

**Verification across all three source documents:**

- **ARCHITECTURE.md**: ADR-008 table row reads "Inline synchronous path only; offload deferred to follow-up (100K+ scale)". Section "RayonPool Offload (SR-01) — DEFERRED" is unambiguous: "The offload path (`PPR_RAYON_OFFLOAD_THRESHOLD`) is **out of scope for crt-030**. crt-030 ships the inline synchronous call only." The Integration Points table explicitly marks `RayonPool.spawn_with_timeout()` as "Not used by crt-030; offload path deferred to follow-up issue (ADR-008)".

- **SPECIFICATION.md**: NFR-07 states "A Rayon offload branch (`PPR_RAYON_OFFLOAD_THRESHOLD`) is explicitly deferred and out of scope for crt-030." NFR-01 states "The inline synchronous path is the only implementation in crt-030." NFR-09 notes the `rayon` petgraph feature is not needed because the offload branch is deferred.

- **RISK-TEST-STRATEGY.md**: R-01 is classified "Deferred" in the Risk Register. The R-01 section states "Status: Deferred — the `PPR_RAYON_OFFLOAD_THRESHOLD` offload branch is out of crt-030 scope. The branch will not be implemented in this feature. crt-030 ships inline-only PPR." Test scenarios: "None for crt-030." Coverage Summary: "0 — offload branch not in crt-030; follow-up issue required." FM-01 (RayonPool Timeout) also marked DEFERRED: "does not apply to this feature."

All three documents are fully consistent. No residual tension. WARN-01 is closed.

---

## Detailed Findings

### Vision Alignment

The product vision (Wave 1A) identifies the intelligence pipeline's core gap: "Intelligence pipeline is additive boosts, not a learned function" (Critical Gaps table, status "Roadmapped — Wave 1A + W3-1"). The vision also flags "No session-conditioned relevance — every query treated identically" as High severity and roadmapped for Wave 1A.

crt-030 directly addresses both. PPR propagates relevance mass through the typed knowledge graph (W1-1 infrastructure), activating graph edges that are "currently dead weight in retrieval." The phase-affinity weighting in the personalization vector (from col-031/#414) begins conditioning queries on session state. The SCOPE.md problem statement precisely matches the vision's articulation: lesson-learned and outcome entries that support popular decision entries are never retrieved, confidence stays low, access imbalance compounds.

The feature does not attempt to build the learned function (W3-1) — it explicitly excludes GNN, training loops, and model changes. This is correct milestone discipline: PPR is a computable, deterministic step that moves toward session-conditioned relevance without requiring W3-1 infrastructure. PPR-surfaced entries generate new co-access signal, confidence exposure, and FEATURE_ENTRIES annotations that become W3-1 training data — the feature is W3-1-compatible by construction.

**Finding**: Vision alignment is strong. crt-030 is positioned correctly as a Wave 1A intelligence pipeline enhancement — not a Wave 3 ML feature and not a Wave 0 infrastructure feature.

### Milestone Fit

Wave 1A is the current active wave. All prerequisite wave items consumed by crt-030 are complete:
- WA-0 (signal fusion) — COMPLETE (crt-024)
- WA-1 (phase signal) — COMPLETE (crt-025)
- WA-4 (proactive delivery) — COMPLETE (crt-027)
- W1-1 (typed relationship graph) — COMPLETE (crt-021)
- W1-2 (RayonPool) — COMPLETE

col-031 (phase affinity frequency table, the #414 dependency) is a current Wave 1A deliverable; crt-030 gracefully degrades on cold-start. No Wave 2 or Wave 3 capability is pulled forward. The five PPR config fields follow the W0-3 externalization pattern — no hardcoding. All infrastructure consumed is wave-appropriate.

**Finding**: Milestone fit is correct.

### Architecture Review

The architecture document is thorough and internally consistent. Key findings:

- **Component decomposition**: Three components (`graph_ppr.rs`, `search.rs` Step 6d, `config.rs`) are clearly bounded and follow existing patterns (`graph_suppression.rs`, `InferenceConfig` extension, the col-031 pipeline integration model).

- **Lock ordering**: The lock chain at Step 6d is correctly documented as an extension of the col-031 chain. No new lock acquisitions in Step 6d. PPR runs entirely on already-cloned state.

- **SR resolutions**: All seven scope risks (SR-01 through SR-07) are resolved with ADR references. The step-order contradiction (SR-03) is unambiguously resolved: `6b → 6d → 6c → 7`.

- **Latency budget**: The scale table (1K/10K/100K) with explicit wall-time estimates is appropriate for a hot-path insertion. The 100K row correctly marks "Follow-up issue (deferred)" in the Offload column, consistent with the deferred scope decision.

- **WARN-01 resolved**: ADR-008 in the Technology Decisions table, the "RayonPool Offload (SR-01) — DEFERRED" section, the code block showing only the inline call, and the Integration Points table marking `RayonPool.spawn_with_timeout()` as "Not used by crt-030" — all are consistent. No residual offload branch content remains in the architecture document.

- **Minor note**: Component 2 description says "Build the personalization vector from HNSW candidate scores weighted by `phase_affinity_score` (called directly — no `use_fallback` guard, per ADR-003)." This is correct and consistent with SR-06 resolution and Specification FR-06. No issue.

### Specification Review

The specification is complete and well-structured. Key findings:

- **FR-01 through FR-12** map directly to SCOPE.md Goals 1–10 and all 18 Acceptance Criteria. No goal is unaddressed.

- **FR-07 (step order)**: Unambiguously states `6b → 6d → 6c → 7` with an explicit note that the Background Research section of SCOPE.md contains stale text. The spec corrects the contradiction rather than carrying it forward.

- **FR-06 (phase affinity contract)**: Correctly cites ADR-003 col-031 and Unimatrix #3687. The no-`use_fallback`-guard requirement is explicit and correct.

- **FR-08 step 6 (quarantine check)**: Explicitly requires silently skipping quarantined entries (AC-13). This is the SR-07 / R-08 mitigation.

- **FR-11 (config fields)**: Table matches SCOPE.md ACs AC-09/AC-10 exactly — five fields, correct types, correct ranges, correct error type.

- **NFR-07 (synchronous execution)**: "A Rayon offload branch (`PPR_RAYON_OFFLOAD_THRESHOLD`) is explicitly deferred and out of scope for crt-030." NFR-09 notes the `rayon` petgraph feature is not needed. Both NFRs are aligned with Option B.

- **No orphaned ACs**: All 18 SCOPE.md ACs are traceable to FR or NFR entries.

### Risk Strategy Review

The risk-test strategy is internally consistent with the deferred offload decision. Key findings:

- **R-01 (Rayon offload) — Deferred**: Classified "Deferred" in the Risk Register. Zero test scenarios required. FM-01 also marked DEFERRED. The strategy correctly notes "Open a follow-up issue to scope both the RayonPool offload implementation and its test coverage." This is the correct forward-defensive posture.

- **R-08 (quarantine bypass)** is correctly classified Critical with three explicit test scenarios. The strategy flags it was "called out in AC-13 but no corresponding named test (T-PPR-XX) exists for the quarantine path" — this gap is surfaced rather than papered over.

- **R-04 (node-ID sort placement)**: Classified High. The strategy correctly identifies this as a performance correctness risk not caught by correctness tests, requiring a timing benchmark. Mature risk identification.

- **R-12 (Prerequisite edge direction)**: Requires a direction unit test even though no production Prerequisite edges currently exist. Correct forward-defensive testing approach.

- **I-01 through I-04 (integration risks)**: All four are reasonable and traceable to spec sections. I-02 (co-access anchor selection after PPR expansion) correctly flags a non-obvious second-order interaction.

- **Security section (S-01 through S-04)**: Proportionate and accurate. S-02 correctly identifies that the quarantine check (R-08) is the primary mitigation for the GRAPH_EDGES write path.

- **Coverage Summary**: The "Deferred: 1 (R-01), 0 test scenarios" row is now the correct representation of the offload risk. No ambiguity remains between the risk document and the scope decision.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — results: entry #2298 (config key semantic divergence, not applicable to crt-030), entry #3337 (architecture diagram header divergence from spec, not applicable to crt-030). No prior patterns matched the specific scope/architecture offload-branch tension type.
- Stored: entry #3742 "Optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral" via `/uni-store-pattern` — the three-document consistency requirement for Option B deferral is a reusable alignment pattern (topic: vision, category: pattern).
