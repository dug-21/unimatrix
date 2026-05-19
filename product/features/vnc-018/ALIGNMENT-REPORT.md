# Alignment Report: vnc-018

> Reviewed: 2026-05-19
> Artifacts reviewed:
>   - product/features/vnc-018/architecture/ARCHITECTURE.md
>   - product/features/vnc-018/specification/SPECIFICATION.md
>   - product/features/vnc-018/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/vnc-018/SCOPE.md
> Risk assessment: product/features/vnc-018/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly delivers W1B-2a — the graph read surface completing the W1B write-surface investment |
| Milestone Fit | PASS | Correctly scoped to Wave 1B (in-flight); consumes W1B-1 foundation; defers W1B-2b/c appropriately |
| Scope Gaps | WARN | One SCOPE.md OQ remains open in the spec (OQ-01: neighbors non-existent ID behavior); one OQ not carried (OQ-02: depth upper bound) |
| Scope Additions | WARN | SPECIFICATION.md adds two open questions (OQ-01, OQ-02) that SCOPE.md resolved or did not flag; Spec also added AC-10a, AC-15a–AC-15c, AC-03b as explicit sub-criteria beyond SCOPE.md ACs — additions are net-positive |
| Architecture Consistency | PASS | All seven ADRs resolve open questions from SCOPE.md; architecture faithfully implements scope decisions |
| Risk Completeness | PASS | 19 risks identified, all with test scenarios; scope risks traced to architecture risks; security surface analyzed |

**Overall verdict: PASS with two WARNs.** No variances requiring approval. WARNs are open-question tracking gaps that are internal to the spec agent's work, not deviations from scope or vision.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | SCOPE.md OQ-01 resolution not confirmed in SPEC | SCOPE.md OQ-01 (neighbors non-existent ID: error vs. empty) is listed as RESOLVED in SCOPE.md ("return empty — consistent with chain mode"). SPECIFICATION.md OQ-01 re-opens this as open for the architect, which contradicts SCOPE.md. The architecture document does not address it. |
| Gap | SCOPE.md OQ-02 (depth upper bound) not in SPECIFICATION | SCOPE.md does not set an explicit upper bound; SPECIFICATION adds `1..=10` range and OQ-02 asking architect to confirm. Good addition, but the spec leaves it as a question rather than resolving it. |
| Addition | SPECIFICATION.md adds sub-ACs beyond SCOPE.md | AC-03b (per-direction truncation distinguishability), AC-10a (silent exclusion no-warning assertion), AC-15a (Supersedes explicit rejection error message), AC-15b (forward-compat field error-on-misuse), AC-15c (resolve_supersessions rejected on chain mode) — all are elaborations of SCOPE.md intent, not scope additions. Net-positive. |
| Addition | SPECIFICATION.md FR-08 adds explicit error on resolve_supersessions in chain mode | SCOPE.md stated "resolve_supersessions is not a parameter on chain mode" — spec converts this to an active error, not just parameter absence. Stricter than scope required; aligned with intent. |
| Simplification | `subgraph` mode deferred to #597 | SCOPE.md title and roadmap position imply neighbors+subgraph; SCOPE.md Non-Goals explicitly defers subgraph. Spec and architecture correctly reflect this. No issue. |

---

## Variances Requiring Approval

None. Both WARNs are documentation/process gaps within the spec agent's outputs, not architectural deviations from vision or scope additions that require human approval.

---

## Detailed Findings

### Vision Alignment

**PASS.**

The product vision (W1B section) states: "`context_graph` enables direct typed-edge traversal: dependency navigation, supersession chains, gap detection (entries missing expected incoming edges), and Goal → Decision → Outcome audit subgraphs in a single query." vnc-018 delivers the first three traversal modes (`chain`, `current`, `neighbors`) that establish this surface. The feature directly enables the "After Wave 1B" outcome: "agents declare typed relationships between entries at write time — ADR dependency chains, goal traceability, lesson→decision reasoning chains."

The architecture and specification both position this correctly as the read complement to W1B-1's write surface. No shortcut deviates from the vision's intended graph-traversal capability model.

The vision's non-negotiable "single binary" and "in-memory hot path" principles are both honored: the feature adds to the existing binary, and depth>1 BFS correctly uses the `Arc<RwLock<TypedRelationGraph>>` cached structure rather than direct DB reads on the hot path.

The vision's "hash chain integrity" and "immutable audit log" non-negotiables are respected: the new tool is read-only (`Capability::Read`), writes nothing to knowledge.db state, and follows the standard audit ceremony (`capability_used = "read"`, `operation = "context_graph"`).

---

### Milestone Fit

**PASS.**

vnc-018 is the W1B-2a delivery item, correctly positioned in the product roadmap. The dependency graph is respected: W1B-1 (`vnc-015`, PR #600) is a hard prerequisite. The feature defers W1B-2b (subgraph, #597) and W1B-2c (inverse, path, filter, #598) explicitly. No Wave 2 or Wave 3 capabilities are built or implied.

The forward-compat `GraphParams` fields (`seed_ids`, `from_id`, `to_id`, `max_nodes`) are explicitly designed to anticipate #597 and #598 struct shapes — this is milestone discipline, not scope addition. The fields validate-to-error on misuse, ensuring no unintended behavior ships.

The schema migration (v26 → v27, index-only) is correctly scoped: four indexes that serve both current modes and future W1B-2c modes. This is legitimate infrastructure amortization, not premature future-milestone capability.

---

### Architecture Review

**PASS.**

The architecture document is internally consistent and resolves all six SCOPE.md open questions (OQ-01 through OQ-06) via ADRs:

- **ADR-001**: SQL CTE mandatory for chain/current (not in-memory graph) — resolves SR-07 concern.
- **ADR-002**: `Truncated { forward: bool, backward: bool }` struct — resolves SR-05 scope ambiguity.
- **ADR-003**: `GraphParams` struct layout locked with centralized validation — resolves SR-03 forward-compat contract.
- **ADR-004**: `EdgeRecord` defined in `graph_read.rs`, re-exported for #597/#598 — clean forward-compat boundary.
- **ADR-005**: depth=1 SQL, depth>1 in-memory BFS split — resolves OQ-01 from SCOPE.md.
- **ADR-006**: Advances/Motivates added to PPR/BFS — completes W1B-1 deferral.
- **ADR-007**: Schema migration v26→v27, index-only, with full cascade checklist.

The module structure (`mcp/graph_read.rs`, `tools.rs` dispatch-only) follows the `edge_write.rs` precedent from vnc-015 (ADR-005). The 500-line module constraint is acknowledged and a split strategy is provided if needed. All component interactions are diagrammed clearly.

One informational concern flagged in ARCHITECTURE.md: `node_index` on `TypedRelationGraph` is `pub(crate)` within `unimatrix-engine`, but `graph_read.rs` is in `unimatrix-server`. The architecture correctly flags this as an implementation-time decision (accessor vs. engine-side BFS), notes both paths are valid, and defers the decision. This is appropriate for a design document — it is not an open question blocking delivery, but it IS a high-severity risk (R-07 in the risk strategy) and must be resolved at delivery start.

---

### Specification Review

**PASS with WARN on two open questions.**

The specification faithfully translates all SCOPE.md goals into functional requirements (FR-01 through FR-14) and acceptance criteria (AC-01 through AC-20). The spec adds sub-ACs (AC-03b, AC-10a, AC-15a–c) that sharpen the SCOPE.md AC set — all are legitimate elaborations, not scope changes.

**WARN item 1 — OQ-01 conflict**: SCOPE.md resolves OQ-01 as "return empty for non-existent neighbors ID — consistent with chain mode." SPECIFICATION.md OQ-01 re-opens this question and says "Recommend: return empty — but architect should confirm." This is an internal inconsistency. SCOPE.md already resolved it. The spec agent should have cited SCOPE.md OQ-01's resolution rather than re-opening the question. The delivery agent must follow SCOPE.md's resolution (return empty, consistent with AC-04), not wait for additional architect input.

**WARN item 2 — OQ-02 depth upper bound**: SPECIFICATION.md adds `1..=10` as the valid range for `depth` (NFR-06, Constraints) and then marks OQ-02 as open asking whether the architect should confirm. The spec has already specified the constraint (`1..=10`) in NFR and Safety Constraints sections — the OQ is redundant and creates ambiguity about whether `1..=10` is authoritative. The delivery agent should treat the NFR/Constraints specification as authoritative and close OQ-02 without additional input.

The domain model section (GraphParams, EdgeRecord, ChainResponse, TruncationStatus, CurrentResponse, NeighborsResponse, Ubiquitous Language) is comprehensive and well-specified. The five user workflows are concrete and trace directly to the three mode ACs. The NOT In Scope section (12 items) is thorough and prevents scope creep at delivery.

---

### Risk Strategy Review

**PASS.**

The risk strategy identifies 19 risks across Critical/High/Medium priority tiers. All SCOPE-RISK-ASSESSMENT.md scope risks (SR-01 through SR-09) are traced to corresponding implementation risks (R-01 through R-19) with a traceability table. This is thorough cross-document coverage.

Critical risks are well-chosen: the three that could cause silent wrong behavior (R-01 in-memory vs. SQL, R-02 truncated wire shape, R-07 node_index cross-crate compile failure) correctly carry the highest severity. R-03 (depth=1/depth>1 staleness) is correctly labeled High likelihood because it will manifest in testing if not proactively tested.

R-07 (node_index visibility) is the highest-probability Critical risk. It is a compile-time failure, not a runtime behavior issue. The risk strategy correctly notes this must be resolved before any depth>1 test can run, and flags it as an "architectural decision" to be recorded in a Unimatrix ADR if the delivery agent chooses the engine-side BFS path. This is appropriate tracking.

The security risk analysis correctly identifies the attack surface as low-risk (all read-only, parameterized SQL, enum validation for edge types, u8 depth bounded at deserialization). The `depth` parameter resource exhaustion concern is noted.

The four non-negotiable tests listed at the end of the coverage summary (AC-16 P-03, AC-19 four indexes, AC-03b per-direction truncation, R-03 staleness test) are correctly identified as gate failures from prior features. This is knowledge stewardship in practice.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found #2298 (config key semantic divergence), #3337 (architecture diagram header divergence), #3426 (formatter section-order risk), #3746 (pre-loop extraction gotcha), #3771 (KnowledgeConfig parallel list defaults). None of the top results directly matched the vision alignment review patterns being sought (scope additions, milestone discipline, infrastructure feature N/A classification). The query confirmed no accumulated cross-feature misalignment patterns that would change this review's findings.
- Stored: nothing novel to store — the variances found (open question re-opening, redundant OQ) are spec-agent process gaps specific to this feature. They do not generalize to a recurring cross-feature pattern. The well-executed scope traceability (SR → R mapping, SCOPE.md → ADR chain) is positive reinforcement but not a novel pattern entry — it reflects existing protocols working correctly.
