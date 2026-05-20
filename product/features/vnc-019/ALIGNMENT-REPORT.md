# Alignment Report: vnc-019

> Reviewed: 2026-05-19
> Artifacts reviewed:
>   - product/features/vnc-019/architecture/ARCHITECTURE.md
>   - product/features/vnc-019/specification/SPECIFICATION.md
>   - product/features/vnc-019/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | subgraph mode directly serves the domain-agnostic, graph-first retrieval path validated in Wave 1B; no strategy drift |
| Milestone Fit | PASS | squarely in W1B-2, correctly scoped as the second of three sequential W1B-2 issues |
| Scope Gaps | WARN | FR-07 max_nodes > 200 behavior left to "architect decision" — SCOPE.md expected this resolved before delivery |
| Scope Additions | PASS | no out-of-scope work introduced; non-goals explicitly enumerate every excluded item |
| Architecture Consistency | PASS | all architectural decisions inherit from or extend vnc-018 ADRs correctly; no contradictions with vision non-negotiables |
| Risk Completeness | PASS | all 7 SCOPE-RISK-ASSESSMENT risks are traced to RISK-TEST-STRATEGY entries; 16 additional implementation risks identified |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SR-02 structured truncation reason | SCOPE.md asked architect to "assess whether `truncated` alone is sufficient or if a structured reason should be returned." Architecture/Spec chose `truncated: bool` only and deferred structured reason to W1B-2c. Rationale documented in ADR-004 and C-09. Acceptable deferral — explicitly scope-bounded. |
| Simplification | SR-05 batch supersession pre-resolution | SCOPE.md asked architect to "consider whether a batch resolution pass before BFS is preferable to inline per-hop resolution." Architecture chose inline per-hop. Rationale documented in SR-05 disposition (50-hop guard + 200-node cap bounds worst case to 200 Store::get calls). Acceptable. |
| Gap | FR-07 max_nodes > 200 behavior | SCOPE.md Constraint 2 states `max_nodes=200` is the hard cap; the specification's FR-07 says "A value above 200 is clamped to 200 (or rejected with a validation error — architect decision)." The behavior is still undecided at the spec level. R-07 test scenarios test both outcomes but do not mandate one. This is a gap: a caller passing max_nodes=201 will get either silent clamping or a validation error, and the tool description does not yet commit to either. Agents cannot predict behavior without reading source code. |
| Gap | SR-01 `graph_rebuilt_at` field | SCOPE-RISK-ASSESSMENT SR-01 recommendation says "Consider whether `depth_reached` + `truncated` are sufficient signals or if a `graph_rebuilt_at` field is needed." Architecture ADR-004 decides against it. This is a resolved question, not a gap — the rationale is documented. Included here for completeness: the decision is that text-only disclosure is sufficient. |

---

## Variances Requiring Approval

None with FAIL status. One item warrants human attention before delivery begins:

### 1. FR-07: max_nodes > 200 behavior is unresolved (WARN)

**What**: FR-07 in the specification states that a `max_nodes` value above 200 should be "clamped to 200 (or rejected with a validation error — architect decision; whichever is chosen, the cap is never exceeded in the response)." The design documents leave this open. R-07 in RISK-TEST-STRATEGY tests for "either behavior" without prescribing which. The tool description in tools.rs has not yet captured this behavior. Delivery will need to pick one, but that choice affects the tool's public contract and is not documented in any ADR.

**Why it matters**: The `GraphParams` struct is a wire contract (ADR-003 vnc-018). Callers that pass `max_nodes=300` today and get silently clamped to 200 will break if a future release changes to reject the value. The product vision's emphasis on a trustworthy, integrity-chained, auditable knowledge engine requires predictable tool contracts. Silent clamping without disclosure violates that spirit. The tool description is the agent's only behavioral reference (ADR-004).

**Recommendation**: Resolve before delivery gate. Preferred: reject with a validation error (consistent with `max_depth` range validation in FR-06) and document in the tool description. Clamping is acceptable only if the tool description explicitly states that values above 200 are clamped. Add the behavior to the staleness/parameter disclosure block in tools.rs.

---

## Detailed Findings

### Vision Alignment

The vision describes Unimatrix as a "workflow-aware, self-learning knowledge engine" where "A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency." Wave 1B is the roadmap section that opens graph traversal to agents. The vision explicitly states: "context_graph enables direct typed-edge traversal: dependency navigation, supersession chains, gap detection, and Goal → Decision → Outcome audit subgraphs in a single query."

vnc-019's `subgraph` mode directly implements "Goal → Decision → Outcome audit subgraphs in a single query." The research domain workflows W1, W2, W3 in the specification (Goal evidence graph, Thesis evidence chain, Contradiction surface) are the exact traversal patterns validated in ASS-057 and cited in the Wave 1B section of the product vision.

The vision's non-negotiable "In-memory hot path" rule states: "all analytics-derived search data (graph, weights, co-access, GNN scores) cached in Arc<RwLock<_>> rebuilt by tick — never read from the database directly at query time." The architecture correctly uses the in-memory `TypedRelationGraph` for BFS hop enumeration and issues SQL only post-BFS for node hydration and metadata. This pattern is consistent with the vision constraint.

The vision's "Single binary" non-negotiable is respected: no new crate, no new tool, no new table.

**Finding**: Full vision alignment. No drift detected.

---

### Milestone Fit

vnc-019 is W1B-2b (#597) in the product roadmap. The roadmap sequences W1B-2 into three issues: W1B-2a (neighbors+subgraph is revised; this document indicates the split is chain+current → neighbors → subgraph), W1B-2b (chain+current — or subgraph, per the current feature naming), W1B-2c (inverse+path+filter). The product vision states W1B-2 effort is 14–17 days total.

The feature correctly does not build any W1B-2c capabilities (inverse, path, filter are explicitly in the non-goals list). It does not attempt W2 or W3 capabilities. The dependency on vnc-018 (W1B-2a) merging first is documented as a hard constraint (C-01, SR-06).

**Finding**: Correct milestone. No future milestone capabilities pulled forward.

---

### Architecture Review

The architecture document is internally consistent and well-reasoned. Key observations:

1. **ADR coverage is complete**: All seven SCOPE-RISK-ASSESSMENT risks have explicit disposition entries in the architecture's SR Disposition table. All four architecture-specific ADRs (#4490–4493) have Unimatrix IDs confirming they were stored.

2. **Lock discipline is correct**: The `std::sync::RwLock` pattern (acquire once, clone, release before async) matches the established pattern in `graph_read_neighbors.rs`. The architecture explicitly identifies this as "identical to `neighbors_bfs`."

3. **`resolve_supersessions` ordering is correct**: The BFS pseudocode in the architecture document shows substitution BEFORE the visited-set check, preventing double-enqueuing of the same terminal node. This resolves R-01 at design time.

4. **Post-BFS metadata strategy is sound**: The OR-chain approach is O(1) round-trips regardless of edge count, bounded by the 200-node cap. The architecture identifies that SQLite does not support tuple IN syntax and chooses the OR-chain approach explicitly.

5. **Minor observation — `all_non_supersedes_types` scope**: The architecture references `all_non_supersedes_types` from `graph_read_neighbors.rs` as "re-used" but does not resolve the visibility question. Integration Risks IR-02 in RISK-TEST-STRATEGY surfaces this; the architecture's Component 3 section says "pub(super) or pub(crate) visibility change enables the import." This is a delivery-time decision, not a design gap.

6. **`depth_reached` computation in BFS pseudocode (step 8)**: Computed as `collected_edges.iter().map(|e| e.depth).max().unwrap_or(0)`. This is correct. However, the BFS pseudocode uses `depth` on each edge record set to `current_depth + 1`, which means `depth_reached` will correctly reflect actual traversal depth under truncation.

**Finding**: Architecture is sound. No contradictions with vision or with inherited ADRs from vnc-018.

---

### Specification Review

The specification translates the architecture faithfully into 23 functional requirements, 8 non-functional requirements, and 19 acceptance criteria. Observations:

1. **AC coverage is complete for all SCOPE.md acceptance criteria**: SCOPE.md AC-01 through AC-15 all have corresponding Specification ACs. Two new ACs are added (AC-16 for `max_depth` on non-subgraph modes, AC-17 for missing-seed behavior, AC-18 for metadata populated, AC-19 for empty-edge guard) that address risks identified in SCOPE-RISK-ASSESSMENT.

2. **FR-04 `edge_types` empty list behavior**: The specification states "When absent or empty, all 16 recognized RelationType variants are traversed." RISK-TEST-STRATEGY R-14 scenario 3 asks "Call with edge_types=[] (empty list). Assert: behavior identical to absent edge_types (defaults to all types). Verify this is consistent with FR-04." This is internally consistent.

3. **FR-07 gap (described in Scope Alignment above)**: The specification defers the clamp-vs-reject decision to the architect. This is the only unresolved behavioral choice in the specification.

4. **NFR-02 "bounded at 3" SQL round-trips**: The specification states "The total number of SQL round-trips for a single subgraph call is bounded at 3." This is accurate for the non-supersession case: (1) batch node hydration, (2) metadata batch query. However, when `resolve_supersessions=true`, `follow_to_current` issues up to one `Store::get()` per deprecated node (bounded by max_nodes=200). NFR-02's parenthetical "(3) optional per-deprecated-node follow_to_current calls (capped at max_nodes=200)" correctly adjusts the bound, but calling 200 sequential DB reads "1 additional round-trip type" slightly understates the worst-case I/O. This is a documentation imprecision, not a behavioral defect.

5. **Tool description text (FR-19)**: The exact text mandated by FR-19 is present in the specification. It covers all four required disclosure categories from AC-13. The absence of a `graph_rebuilt_at` field (C-08) is consistent with ADR-004.

**Finding**: Specification is complete and internally consistent. One behavioral gap (FR-07 max_nodes > 200) should be resolved before delivery.

---

### Risk Strategy Review

The RISK-TEST-STRATEGY document demonstrates thorough risk decomposition. Observations:

1. **All SCOPE-RISK-ASSESSMENT risks traced**: The Scope Risk Traceability table at the bottom maps each SR-01 through SR-07 to at least one RTS risk entry. Coverage is complete.

2. **Critical risks correctly elevated**: R-01 (supersession/visited-set ordering), R-02 (direction dedup canonical direction), and R-03 (seed count at cap) are all classified Critical — appropriate given their potential to produce silently wrong results that pass superficial testing.

3. **Lesson #4077 cited in R-02**: The risk document references Unimatrix entry #4077, which documents that direction semantics bugs survive review when the code uses the opposite enum value from what the spec describes. This is exactly the class of bug most likely to occur in subgraph mode's direction="both" dedup logic. The test scenarios for R-02 (scenario 4: "Verify edge_key construction in code review") appropriately add a code-review gate on top of behavioral tests.

4. **R-09 (batch hydration with missing ENTRIES rows)**: The test strategy correctly notes that the behavior depends on whether `get_many` returns partial results or errors on missing IDs. The strategy calls for reviewing the `get_many` implementation. This is the correct approach — the behavior should be confirmed during delivery, not assumed.

5. **Security section is appropriately brief**: SEC-01 through SEC-05 cover the relevant surfaces (SQL injection via bind parameters, enum gating on edge_types, OR-chain dynamic construction, resource exhaustion, metadata deserialization). All five confirm no amplification vectors exist. The security analysis is proportional for a read-only graph traversal feature.

6. **R-16 (truncated bool — no structured reason)**: Classified Low/Low/Low. This is consistent with the product vision's pragmatism on deferral — the vision deferred WA-3 (MissedRetrieval) for similar reasons of "insufficient signal justification at this stage." The deferral to W1B-2c is appropriate.

7. **One observation on R-12 (edge depth non-determinism)**: The test strategy asserts "first-discovery-wins behavior" from BFS FIFO ordering (VecDeque). This is architecturally sound — petgraph edge traversal order within a single node is deterministic for a given graph state, and BFS FIFO ensures shallowest discovery first. The test scenarios are correct.

**Finding**: Risk strategy is complete, well-traced, and proportionate. No significant gaps.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — results returned 5 entries. Entry #2298 (Config key semantic divergence, dsn-001) and #3337 (Architecture diagram headers diverge from spec) are patterns from prior features. Neither applies directly to vnc-019: this feature's architecture and spec are tightly aligned. Entry #3337 is a reminder to verify that tool description text in tools.rs matches the staleness disclosure mandated by FR-19 and AC-13 — covered by the R-11 test scenario. No new recurring pattern identified that generalizes beyond vnc-019 specifically. The FR-07 "behavioral choice deferred to delivery" pattern is worth watching across W1B-2c — if the same "architect decides at delivery time" language appears there for bounded parameters, it would warrant storing as a pattern. For now, this is a single instance.
- Stored: nothing novel to store — the FR-07 deferral is a single-feature observation, not yet a recurring pattern.
