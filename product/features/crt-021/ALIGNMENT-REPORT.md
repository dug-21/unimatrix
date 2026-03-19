# Alignment Report: crt-021

> Reviewed: 2026-03-19
> Artifacts reviewed:
>   - product/features/crt-021/architecture/ARCHITECTURE.md
>   - product/features/crt-021/specification/SPECIFICATION.md
>   - product/features/crt-021/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-021/SCOPE.md
> Scope risk source: product/features/crt-021/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly closes three High/Medium vision gaps; all non-negotiables respected |
| Milestone Fit | PASS | W1-1 target; W0-1 (sqlx) prerequisite confirmed complete; no Wave 3 scope pulled in |
| Scope Gaps | PASS | All SCOPE.md goals and constraints addressed in source documents |
| Scope Additions | WARN | `metadata TEXT DEFAULT NULL` column added beyond SCOPE.md; pre-known human decision; forward-compat rationale documented |
| Architecture Consistency | VARIANCE | Supersedes edge direction reversed between ARCHITECTURE.md migration SQL and SPECIFICATION.md FR-08; TypedGraphState field definition differs across ARCHITECTURE §3a and SPECIFICATION FR-16 |
| Risk Completeness | WARN | R-15 in RISK-TEST-STRATEGY.md mischaracterizes FR-09 (spec does use normalized formula, not flat 1.0); risk doc contains a factually incorrect premise that could mislead the implementer |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `metadata TEXT DEFAULT NULL` on GRAPH_EDGES | In ARCHITECTURE.md §2a and SPECIFICATION.md FR-05/AC-04; not in SCOPE.md. Pre-known human decision: forward-compat with W3-1 GNN per-edge features. RISK-TEST-STRATEGY SR-08 documents it as an open human decision, but ARCHITECTURE and SPECIFICATION both include it. Rationale is sound; needs human confirmation that the decision is closed. |
| Simplification | No Contradicts bootstrap from `shadow_evaluations` | SCOPE.md Goal 6 includes "Bootstrap... Contradicts from shadow_evaluations (bootstrap-flagged)." Source docs close AC-08 as a dead path (entry #2404 confirms no entry ID pairs). Intentional per pre-known variance list. |
| Simplification | `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3` | SCOPE.md says "high-count co_access pairs" without specifying threshold. Source documents define 3 as the constant. Reasonable implementation detail per pre-known variance list. |
| Simplification | CoAccess weight is normalized `count/MAX(count)` | SCOPE.md does not specify the weight formula. Architecture §2b and SPECIFICATION FR-09 both use `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`. Pre-known human decision. |

---

## Variances Requiring Approval

### VARIANCE 1: Supersedes edge direction reversed in ARCHITECTURE.md migration SQL

**What**: ARCHITECTURE.md §2b (step 2) bootstrap SQL uses:
```sql
SELECT supersedes AS source_id, id AS target_id
```
SPECIFICATION.md FR-08 states:
> "insert one edge with `source_id = entry.id`, `target_id = entry.supersedes`"

These are opposite directions. The architecture puts the **old** entry as `source_id` and the **new** entry as `target_id`. The spec puts the **new** entry (the correction) as `source_id` and the **old** entry (what was superseded) as `target_id`.

**Why it matters**: `graph_penalty` uses outgoing Supersedes edges from a node to determine whether it has been superseded. If the edge direction is wrong, penalty traversal is inverted — a superseded (old) entry will appear to have an active successor path, and the correcting (new) entry will look like a dead end. Every existing `graph_penalty` test that verifies supersession chains would fail, but only if the test exercises the direction convention. Tests that only check isolated single-edge cases may pass despite the inversion. The SCOPE.md AC-06 verification (asserting `bootstrap_only=0` and `source="bootstrap"` on Supersedes edges) would pass regardless of direction, masking the error.

**Recommendation**: Resolve before implementation begins. Confirm the intended semantic: does an edge point from the correcting entry to the superseded entry, or vice versa? The spec (FR-08) and the existing `entries.supersedes` column semantics (an entry with `supersedes = X` is the successor that replaces X) suggest `source_id = entry.id` (the correcting entry) and `target_id = entry.supersedes` (the replaced entry). The architecture SQL must be corrected to match. The fix is a one-line SQL change.

---

### VARIANCE 2: TypedGraphState field definition inconsistent between ARCHITECTURE and SPECIFICATION

**What**: SPECIFICATION FR-16 defines `TypedGraphState` as holding a pre-built graph:
```rust
pub all_entries: Vec<EntryRecord>
pub typed_graph: TypedRelationGraph   // pre-built in-memory graph
pub use_fallback: bool
```
ARCHITECTURE §3a defines `TypedGraphState` as holding raw edge rows:
```rust
pub all_entries: Vec<EntryRecord>
pub all_edges: Vec<GraphEdgeRow>      // raw rows, not pre-built graph
pub use_fallback: bool
```
The Architecture §3b search path pseudocode then calls `build_typed_relation_graph(&all_entries, &all_edges)` **per search query**, contradicting the SPECIFICATION FR-22 which explicitly states: "The search path reads the pre-built graph from TypedGraphState under a read lock — it does not rebuild the graph on each query (this is the key behavior change from per-query rebuild to tick-rebuild)."

RISK-TEST-STRATEGY §Integration Risks flags this explicitly: "The architecture doc's §3b pseudocode shows a rebuild-per-search pattern that contradicts FR-16 and FR-22 — this is the authoritative discrepancy risk. The spec (FR-22) governs."

**Why it matters**: The implementer faces two incompatible struct definitions. If they follow the architecture, every search query rebuilds the graph from raw rows, negating the performance improvement that is one of the goals of the tick-rebuild pattern. If they follow the spec, the architecture §3b pseudocode is wrong. The risk doc asserts spec governs, but that is a convention stated in the risk doc, not an explicit ruling in either source document. This is a design decision that must be made before implementation.

**Recommendation**: The spec is architecturally correct (pre-built graph in state; no per-query rebuild). Correct ARCHITECTURE.md §3a and §3b to match SPECIFICATION FR-16 and FR-22. The `TypedGraphState` struct should hold `typed_graph: TypedRelationGraph`, not `all_edges: Vec<GraphEdgeRow>`. The risk doc's statement that the spec governs should be elevated to an explicit architectural decision.

---

## Detailed Findings

### Vision Alignment

PASS. crt-021 directly and specifically addresses three gaps listed in the product vision under "Intelligence & Confidence":

- "Only supersession edge type — no typed relationships" (severity: High) — closed by `RelationType` enum with five variants.
- "Graph edges not persisted — lost on restart" (severity: Medium) — closed by `GRAPH_EDGES` table and v12→v13 migration.
- "Co-access and contradiction never formalized as graph edges" (severity: Medium) — closed by CoAccess bootstrap and Contradicts design (empty at migration, runtime via W1-2).

The feature correctly defers NLI (W1-2) and GNN weight learning (W3-1), consistent with Wave 1 scope. No Wave 3 capability is introduced prematurely.

All vision non-negotiables are respected:
- Hash chain integrity: GRAPH_EDGES carries `created_by` attribution; no hash-chain bypass introduced.
- Audit log: edge writes go through analytics queue with attribution. The architecture notes edge writes are not routed through AUDIT_LOG directly (consistent with how `co_access` is handled — analytics-tier data does not get audit log entries).
- ACID storage: sqlx write_pool used for compaction; migration is transactional.
- In-memory hot path: confirmed — graph is rebuilt by tick and read from memory on search path (with the VARIANCE 2 caveat above about which definition governs).
- Single binary: no new services introduced.

The product vision explicitly calls out "ADR #1604 must be explicitly superseded with a new ADR before W1-1 ships." SCOPE.md Goal 9, SPECIFICATION AC-16, and FR-27 all require this. The source documents are consistent on this requirement.

---

### Milestone Fit

PASS. crt-021 is correctly positioned as W1-1. The W0-1 sqlx prerequisite (nxs-011, PR #299) is documented as complete in the product vision. The feature builds on the sqlx dual-pool architecture (write_pool, analytics queue, async store methods) without assuming Wave 2 or Wave 3 infrastructure.

The `metadata TEXT DEFAULT NULL` addition (VARIANCE: scope addition, pre-known) is explicitly a forward-compatibility provision for W3-1 (GNN), not an early implementation of W3-1 behavior. The column stores no data in crt-021. This is consistent with the vision principle of additive, non-behavioral schema extensions.

---

### Architecture Review

The architecture is well-structured and covers all three crates comprehensively. Specific findings:

**Strong areas:**
- `edges_of_type` centralized filter pattern (§1) directly mitigates SR-01 and R-02.
- SR-07 promotion mechanism (§SR-07) is fully designed: DELETE+INSERT with the correct attribution reset rationale. This closes a gap identified in SCOPE-RISK-ASSESSMENT.
- SR-02 write-routing boundary (§2c) is explicitly documented with the W1-2 prescription to use direct `write_pool` rather than the analytics queue for NLI-confirmed edges.
- Data flow diagram (§Data Flow) is accurate and consistent with the tick sequence.
- The `bootstrap_only` structural exclusion in §3b (filtering edges before they enter the graph) is the correct implementation of the safety constraint.

**Concerns:**

1. **Supersedes direction in migration SQL** (VARIANCE 1 above): `SELECT supersedes AS source_id, id AS target_id` is opposite to SPECIFICATION FR-08.

2. **TypedGraphState struct definition** (VARIANCE 2 above): §3a defines `all_edges: Vec<GraphEdgeRow>`; §3b shows per-query `build_typed_relation_graph` calls. Both contradict SPECIFICATION FR-16/FR-22.

3. **`build_typed_relation_graph` Supersedes source note** (Open Question 1 in ARCHITECTURE): The architecture states that Supersedes edges are derived from `entries.supersedes` during graph construction, not from `GRAPH_EDGES` Supersedes rows. This means `GRAPH_EDGES` Supersedes rows exist for persistence/attribution but are not the source of truth for graph construction. This is architecturally defensible but creates a subtle inconsistency: after W1-2 writes new NLI-confirmed `Contradicts` or `Supports` edges to `GRAPH_EDGES` at runtime, the tick rebuild picks them up. But if a new `Supersedes` edge is written to `entries.supersedes` between ticks, the architecture says it is picked up at rebuild time from `entries.supersedes` — not from `GRAPH_EDGES`. This means `GRAPH_EDGES` Supersedes rows are redundant copies, not the authoritative rebuild source. This is a documented open question in the architecture (OQ-1), appropriately flagged for the spec writer. The SPECIFICATION C-14 ("Graph rebuilt from GRAPH_EDGES, not recomputed from canonical sources each tick") appears to contradict this architecture note. Flagged as a WARN — needs resolution before implementation.

---

### Specification Review

The specification is detailed and comprehensive. ACs are testable and specific. Key findings:

**Strong areas:**
- AC-08 is correctly closed as "empty bootstrap, schema ready for W1-2" — resolves SR-04.
- AC-19 (sqlx-data.json regeneration) and NF-08 are present — resolves SR-09 and R-09.
- AC-21 (promotion path integration test) establishes the W1-2 contract in a testable way.
- Constraints section is thorough; C-14 is important (GRAPH_EDGES as rebuild source).
- FR-03 correctly defines `bootstrap_only` on `RelationEdge`.

**Concerns:**

1. **FR-08 vs ARCHITECTURE migration SQL direction** (VARIANCE 1): FR-08 says `source_id = entry.id, target_id = entry.supersedes`. This conflicts with the architecture's migration SQL. One is wrong.

2. **FR-16 vs ARCHITECTURE §3a on TypedGraphState fields** (VARIANCE 2): FR-16 defines `typed_graph: TypedRelationGraph` as the field. The architecture defines `all_edges: Vec<GraphEdgeRow>`. The spec is architecturally correct per FR-22's "pre-built graph" language, but the implementer will see both and must choose.

3. **FR-26 vs ARCHITECTURE SR-07 on W1-2 promotion write path**: FR-26 says the promotion path for W1-2 is "DELETE the existing row and INSERT a new row via `AnalyticsWrite::GraphEdge`." ARCHITECTURE §SR-07 says the two statements "execute in the same transaction on the direct `write_pool` path." These are different write paths. The analytics queue is bounded and shedding; the direct write_pool path is not. Given that ARCHITECTURE §2c explicitly states NLI-confirmed edges must not go through the analytics queue (SR-02), the spec's routing of promotion through `AnalyticsWrite::GraphEdge` contradicts the architecture's routing through `write_pool`. This is a WARN — it is a W1-2 concern, not a W1-1 implementation concern, but the contract must be consistent now.

4. **FR-09 weight formula vs R-15 risk doc claim**: The specification FR-09 correctly specifies the `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)` normalized formula. The RISK-TEST-STRATEGY R-15 states "Spec FR-09 says `weight=1.0`" — this is factually incorrect. The spec never says flat 1.0. This risk is a false alarm. See Risk Strategy section below.

---

### Risk Strategy Review

The risk strategy is thorough: 15 risks, mapped to test scenarios with coverage requirements. Priority classification is appropriate. Critical/High risks (R-01 through R-07, R-09, R-11, R-15) all have concrete test scenarios.

**Strong areas:**
- R-06 (CoAccess NULL weight) is correctly classified Critical/High likelihood and includes mandatory migration test scenarios. This is the highest-probability implementation failure.
- R-03 (bootstrap_only structural exclusion) test scenarios are strong — explicitly tests structural exclusion in `build_typed_relation_graph` rather than conditional checks.
- R-04 (tick sequencing) integration test scenario is well-defined.
- Integration Risks section correctly identifies the architecture/spec discrepancy on TypedGraphState as the authoritative risk.
- Failure Modes table is complete and each failure mode is testable.

**Concerns:**

1. **R-15 false alarm (WARN)**: R-15 states "Spec FR-09 says `weight=1.0`" and classifies this as Med severity / High likelihood. FR-09 in SPECIFICATION.md actually specifies the normalized COALESCE formula. The risk doc is incorrect on this factual premise. An implementer reading R-15 may waste time resolving a discrepancy that does not exist, or may override the correct spec formula with a flat 1.0 because R-15 suggests the spec says that. This needs correction before handoff to the implementer. The risk does not exist as described; it should be removed or rewritten as "verify spec FR-09 and architecture §2b use identical formula" (which they do).

2. **R-15 listed twice in Coverage Summary table**: It appears in both "High" (R-15 at the end of the row) and "Medium" (R-15 at the end of medium risks). This is a copy-paste error in the coverage summary.

3. **R-13 test coverage**: The risk doc states "No test required — the migration path uses direct SQL inserts, not the analytics queue." This is correct — the migration bootstrap uses direct SQL, not `AnalyticsWrite`. However, the risk should note that the `AnalyticsWrite::GraphEdge` variant introduced in W1-1 *is* intended for runtime use (W1-2), and any W1-1 code that mistakenly routes bootstrap writes through the queue would create the R-13 risk. The code inspection check is the right mitigation.

4. **Security coverage is proportionate**: crt-021 adds no new MCP tools and no new external input surface. The security analysis correctly identifies the limited blast radius and focuses on `weight: f32` NaN/Inf (R-07) as the primary persistence-layer risk. The injection analysis (parameterized queries prevent SQL injection) is accurate.

---

## Pre-Known Variances (Confirmed Not Flagged)

These were provided as confirmed human decisions and have been verified in the source documents:

| Variance | Verification |
|----------|-------------|
| No Contradicts bootstrap from `shadow_evaluations` | Confirmed: ARCHITECTURE §AC-08 Status, SPECIFICATION FR-10/AC-08, SCOPE-RISK-ASSESSMENT SR-04 all cite entry #2404. Dead path is consistently documented across all three documents. |
| No separate analytics.db — single-file topology | Confirmed: ARCHITECTURE Constraint #1 and SPECIFICATION C-01 both cite entry #2063. SCOPE-RISK-ASSESSMENT §Assumptions documents the W2-1 apparent conflict with product vision language and confirms single-file is the current implementation. |
| `metadata TEXT DEFAULT NULL` column | Confirmed additive: ARCHITECTURE §2a and SPECIFICATION FR-05/AC-04 both include it. SR-08 documents it as a human decision point; source documents treat it as resolved. Rationale: avoids v14 migration for W3-1 GNN per-edge feature vectors. WARN status retained (see Scope Additions above) pending explicit human confirmation that SR-08 is closed. |
| CoAccess bootstrap weight is normalized `count/MAX(count)` | Confirmed: ARCHITECTURE §2b and SPECIFICATION FR-09 both use the COALESCE formula. Pre-known; not flagged. |

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision alignment patterns scope additions milestone discipline` — results: entry #2298 (config key semantic divergence between TOML and vision), entry #2063 (single-file vs split-file vision language). Entry #2063 directly relevant and confirmed as a resolved pre-known variance.
- Stored: nothing novel to store — the Supersedes edge direction inversion (VARIANCE 1) and the in-state-struct definition conflict (VARIANCE 2) are feature-specific implementation artifacts. The spec-governs-architecture convention for resolving internal document conflicts was already documented by the risk strategy team; not a new cross-feature pattern. R-15 false alarm (risk doc misstating spec content) is potentially recurring but has been seen as a one-off; if it appears again in a future feature, store as a pattern at that point.
