# Alignment Report: crt-014 — Topology-Aware Supersession

> Reviewed: 2026-03-14
> Artifacts reviewed:
>   - product/features/crt-014/architecture/ARCHITECTURE.md
>   - product/features/crt-014/specification/SPECIFICATION.md
>   - product/features/crt-014/RISK-TEST-STRATEGY.md
> Scope: product/features/crt-014/SCOPE.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly executes the Graph Enablement milestone, Phase 1 |
| Milestone Fit | PASS | Supersedes ADR-003/ADR-005 as called out in vision; scoped to supersession graph only (Phase 1 of 3) |
| Scope Gaps | PASS | All 6 SCOPE.md goals addressed in architecture + specification |
| Scope Additions | WARN | SPECIFICATION.md adds AC-17 and AC-18 beyond SCOPE.md's AC-01 through AC-16; additions are defensive and non-expanding |
| Architecture Consistency | PASS | Component boundaries, ADR set, and integration surface are internally consistent |
| Risk Completeness | PASS | 13 risks identified; all SR-XX scope risks traced; security blast radius assessed |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | AC-17: Dangling `supersedes` reference test | SCOPE.md does not enumerate AC-17 explicitly; SPECIFICATION.md adds this as a defensive correctness criterion. Acceptable — it operationalizes FR-03's `tracing::warn!` behavior which is stated in SCOPE.md's proposed approach. |
| Addition | AC-18: Workspace builds clean with no new warnings | SCOPE.md does not enumerate AC-18 explicitly; SPECIFICATION.md adds this as a workspace hygiene criterion. Acceptable — it is a pass/fail build gate with no scope impact. |
| Simplification | `context_status` cycle surface | SCOPE.md/OQ-4 answer says "surface in context_status output." Architecture resolves this as log-only (`tracing::error!`) with no struct change. ADR-005 documents this as a v1 scope boundary. Rationale: status service struct change is out of scope for crt-014. |

---

## Variances Requiring Approval

None. The simplification of `context_status` cycle surfacing (log-only vs. struct field) was explicitly resolved by the human in the OQ-4 answer and is documented in ADR-005. The two specification additions (AC-17, AC-18) are defensive and do not expand scope.

---

## Detailed Findings

### Vision Alignment

The product vision (line 79–81) names the Graph Enablement milestone explicitly:

> "Introduce petgraph for topology-derived scoring and multi-hop traversal. Research complete (ASS-017). Replace hardcoded deprecation/supersession penalty constants (0.7x/0.5x) with graph-topology-derived scoring. Enables multi-hop supersession traversal... Three-phase rollout: supersession graph → co-access graph → unified knowledge graph."

crt-014 implements Phase 1 of this three-phase rollout exactly as described:
- `petgraph` added with `stable_graph` feature (ADR-001)
- Hardcoded 0.7x/0.5x constants removed and replaced with `graph_penalty` (ADR-004)
- Multi-hop traversal enabled via `find_terminal_active` (ADR-003)
- Cycle detection via `is_cyclic_directed` (FR-03)

Phases 2 and 3 (co-access graph, unified knowledge graph) are explicitly excluded from crt-014 scope and listed under NOT In Scope in SPECIFICATION.md. This is the correct milestone discipline.

The vision's "auditable knowledge lifecycle" and "trustworthy, correctable" principles are served: topology-derived penalties mean deprecated entries are penalized according to their actual position in the knowledge graph, not a uniform scalar. This directly improves retrieval trustworthiness.

### Milestone Fit

crt-014 is correctly sequenced. The vision positions Graph Enablement as depending on Intelligence Sharpening. This design session runs on the worktree branch, consistent with parallel feature development. The feature does not pull in any future-milestone capabilities (no Graphviz export, no RwLock cache, no schema changes). All Phase 2/3 capabilities are explicitly deferred in SPECIFICATION.md's NOT In Scope section.

The vision notes crt-014 is a "prerequisite for crt-017" (Contradiction Cluster Detection). ARCHITECTURE.md's crt-017 forward compatibility section documents how `SupersessionGraph` as a named opaque struct enables future edge type extension — this is proportionate preparation, not premature optimization.

### Architecture Review

The architecture is internally consistent across all components:

1. **Component boundaries are clean**: `graph.rs` is pure sync with no I/O; `search.rs` wraps it via `spawn_blocking` (consistent with existing engine patterns). No async in `graph.rs` (NFR-05 / SCOPE.md constraint).

2. **ADR set is complete**: Six ADRs cover all major decisions. ADR-003 and ADR-004 correctly reference the system ADRs they supersede. The feature ADR numbering does not conflict with system ADRs (feature ADRs are local files; system ADRs are Unimatrix entries).

3. **Integration surface is fully enumerated**: 14-row integration surface table covers all functions, constants, and removals. `Store::query(QueryFilter::default())` is identified as the correct full-store read path (SR-02 concern resolved).

4. **Cycle fallback is sound**: `use_fallback = true` path preserves search availability (ADR-005). The fallback constant (`FALLBACK_PENALTY = 0.70`) lives in `graph.rs`, not `confidence.rs` — clean removal as required by SR-05.

5. **crt-017 forward compatibility**: `SupersessionGraph` is a named struct, not a type alias. Edge type `()` can be upgraded to `EdgeKind` enum in crt-017 without changing the public API. This is proportionate and correct.

One minor consistency note: ARCHITECTURE.md lists ADR-003 file as `ADR-003-supersede-prior-adr-003.md` in the Technology Decisions section, but the actual file is named `ADR-003-supersede-system-adr-003-multi-hop.md`. This is a documentation-only inconsistency with no behavioral impact.

### Specification Review

The specification is complete against all SCOPE.md goals:

| SCOPE.md Goal | Specification Coverage |
|--------------|----------------------|
| Goal 1: Add petgraph | FR-01, AC-01 |
| Goal 2: Implement graph.rs | FR-02, FR-03, AC-02 through AC-04 |
| Goal 3: Replace constants with graph_penalty | FR-04, FR-09, FR-10, AC-05 through AC-08, AC-14, AC-15 |
| Goal 4: Multi-hop successor resolution | FR-05, FR-07, AC-09 through AC-11, AC-13 |
| Goal 5: Cycle detection integrity check | FR-03, FR-06, FR-08, AC-03, AC-16 |
| Goal 6: Supersede ADR-003 and ADR-005 | ARCHITECTURE.md ADR-003/ADR-004 files |

Non-functional requirements (NFR-01 through NFR-07) add latency, purity, depth cap, no-unsafe, no-async, no-schema-change, and cumulative-test constraints — all consistent with SCOPE.md constraints.

The AC-17 and AC-18 additions are warranted: dangling reference handling is mentioned in the proposed approach but not enumerated as an AC; workspace build hygiene is a universal gate.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is complete:

- 13 risks identified (5 Critical, 6 High, 2 Medium/Low)
- All SR-XX scope risks traced in the traceability table with resolution status
- Integration risks cover the key boundary conditions (QueryFilter behavior, unified penalty guard, async executor blocking, thiserror dependency)
- Edge cases cover empty input, single entry, all-Active, and boundary u64 values
- Security assessment confirms no external untrusted input surfaces in `graph.rs`; cycle and depth cap defenses bound malformed-data blast radius
- Failure modes table maps each failure scenario to expected behavior and verifiability

The risk strategy correctly identifies R-04 (edge direction reversal) and R-05 (test migration window) as Critical — these are the two failure modes most likely to produce silent behavioral regressions.
