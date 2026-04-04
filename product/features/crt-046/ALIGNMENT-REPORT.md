# Alignment Report: crt-046

> Reviewed: 2026-04-04
> Artifacts reviewed:
>   - product/features/crt-046/architecture/ARCHITECTURE.md
>   - product/features/crt-046/specification/SPECIFICATION.md
>   - product/features/crt-046/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Agent ID: crt-046-vision-guardian

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly implements the W3-1 behavioral signal collection required by Wave 1A / Group 6 roadmap |
| Milestone Fit | PASS | crt-046 is the correct Wave 1A / post-WA-4 deliverable; no future milestone capabilities are anticipated |
| Scope Gaps | PASS | All four SCOPE.md goals (Items 1–3 + cold-start guarantee) are addressed in all three source documents |
| Scope Additions | PASS | Architecture updated to match specification: parse_failure_count returned as top-level MCP response field (outside CycleReviewRecord) AND logged at warn!. FR-03/AC-13 satisfied. Variance V-01 closed. |
| Architecture Consistency | PASS | Architecture is internally coherent; SR-04 (INSERT OR IGNORE vs INSERT OR REPLACE) is resolved consistently; two design amendments verified as present |
| Risk Completeness | PASS | All 16 risks are mapped to test scenarios; all scope risks from SCOPE-RISK-ASSESSMENT.md are traced in the risk register |

**Overall verdict: ALIGNED** — V-01 closed. Architecture corrected to match specification. All checks pass.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SR-04 (INSERT OR REPLACE for `force=true`) | SCOPE.md body mentioned INSERT OR REPLACE; Constraints section said INSERT OR IGNORE. Architecture ADR-002 resolves in favour of INSERT OR IGNORE throughout. Specification Constraint 2 adopts the same position. Consistent resolution; no additional approval needed. |
| Simplification | SR-01 parse-failure observability | SCOPE-RISK-ASSESSMENT recommends a "per-cycle parse-failure counter surfaced in the review result." Architecture says `warn!` log level only, citing scope creep risk from extending `CycleReviewRecord`. Specification adds FR-03 and AC-13 requiring the counter in the MCP response. The specification overrules the architecture on this point. See Variance V-01. |
| Addition (minor) | Architecture §write_graph_edge Return Contract table | SCOPE.md does not specify pseudocode gate requirements; architecture adds a mandatory "lead pseudocode with this table" requirement for delivery agents, citing pattern #4041. This is a delivery guardrail, not a scope addition to the feature itself. Appropriate and consistent with project conventions. |

---

## Variances Requiring Approval

### V-01 (WARN): Architecture and Specification disagree on parse-failure observability surface

**What**: ARCHITECTURE.md §Component 3 step 10 states: "Log `parse_failures` at `warn!` level (SR-01 observability)." ARCHITECTURE.md §Technology Decisions elaborates that the count "is not added to `CycleReviewRecord` — that struct is a serialized `RetrospectiveReport` JSON field, and extending it requires a `SUMMARY_SCHEMA_VERSION` bump." By contrast, SPECIFICATION.md FR-03 states: "A per-cycle parse-failure counter shall be tracked and returned in the `context_cycle_review` result." SPECIFICATION.md Ubiquitous Language defines "parse-failure count" as "Surfaced in the review result, not only server logs." AC-13 validates this as a non-negotiable test: "assert the returned result exposes a count ≥ 1 for parse failures."

**Why it matters**: If the delivery agent follows the architecture prose, they will implement `warn!` log-only and the AC-13 test will fail at Gate 3a, causing rework. If the delivery agent follows the specification FR-03, they will need to extend the `CycleReviewRecord` (or the MCP response wrapper) — the architecture warns this requires a `SUMMARY_SCHEMA_VERSION` bump. One of these paths must be canonically authoritative before implementation begins.

SCOPE-RISK-ASSESSMENT.md SR-01 recommendation explicitly asks for the counter in the review result, and the specification adopted this recommendation. The architecture's reasoning (schema version bump risk) is a legitimate concern, but it is an implementation detail, not a reason to omit the feature. The `SUMMARY_SCHEMA_VERSION` bump or an equivalent mechanism to surface the counter needs to be confirmed in scope.

**Recommendation**: Confirm the specification position as authoritative — FR-03 and AC-13 stand. The delivery brief should explicitly note that `CycleReviewRecord` extension (or an equivalent MCP response wrapper field) is required, and that a `SUMMARY_SCHEMA_VERSION` bump is in scope for this feature if the struct is the chosen surface. If the architectural concern about schema version is accepted, an alternative — adding the count as a top-level field on the MCP response JSON rather than inside `RetrospectiveReport` — should be approved before delivery begins.

---

## Detailed Findings

### Vision Alignment

crt-046 directly implements the behavioral signal infrastructure described in the product vision under Wave 1A. The vision (§Intelligence & Confidence Critical Gaps) identifies "No session-conditioned relevance — every query treated identically" as a High-severity gap and marks it as "Roadmapped — Wave 1A + W3-1." The roadmap entry for WA-4 (§WA-4b) explicitly describes `context_briefing` becoming "goal-conditioned" at phase transitions — crt-046 delivers the prerequisite infrastructure: behavioral `Informs` edge emission and `goal_clusters` population at cycle review, plus goal-conditioned blending at briefing time.

The W3-1 training signal specification (§W3-1: GNN) lists three signal sources. Source 2 is "Implicit behavioral (W1-5): retrieval → successful phase completion = positive." crt-046's Informs edge emission from co-access pairs is the graph-layer representation of co-retrieval success within a cycle — a prerequisite for W3-1 GNN edge feature inputs (vision §W1-4: "NLI confidence score stored in metadata for W3-1 GNN edge features"). The `goal_clusters` table provides the goal-conditioned lookup that enables Mode 2 (Comprehensive / `context_briefing`) of the W3-1 session context vector.

The feature is correctly scoped to Wave 1A. It does not attempt any W3-1 GNN training, weight learning, or session context vector construction — those belong to Wave 3. No future milestone capabilities are built here.

### Milestone Fit

The vision places crt-046's work at Wave 1A (Group 6 per SCOPE.md §Background Research). Wave 1A "runs after W1 foundation; completes before Wave 2 deployment begins." SCOPE.md confirms the Group 5 prerequisite (crt-043) is live at schema v21. crt-046 bumps to v22, consistent with the W1-4 → W1-5 → WA-0 → WA-1 → WA-2 → WA-4 → crt-046 sequence the roadmap implies. The vision does not assign a dedicated milestone entry to Group 6 (it predates the ASS-040 roadmap grouping), but the work is clearly bounded within Wave 1A and does not reach into W3-1 territory.

### Architecture Review

The architecture is well-structured and internally coherent. Specific findings:

**Design amendment 1 verified (context_search exclusion)**: ARCHITECTURE.md §Component 4 scopes blending exclusively to `context_briefing` handler and `IndexBriefingService::index()`. The architecture explicitly states "never in `SearchService::search()`." The component interaction diagram shows only the `context_briefing` handler invoking `blend_cluster_entries`. `context_search` is not modified.

**Design amendment 2 verified (configurable threshold)**: ARCHITECTURE §Integration Surface table row for "Cosine threshold" reads: "`InferenceConfig.goal_cluster_similarity_threshold: f32` (default 0.80). Not a constant; passed to `query_goal_clusters_by_embedding`." This is correctly a config field, not a hardcoded value in `behavioral_signals.rs`.

**SR-04 resolution**: Architecture ADR-002 resolves the INSERT OR IGNORE vs INSERT OR REPLACE contradiction definitively. The additive-only invariant is applied uniformly. No INSERT OR REPLACE path exists. This is consistent with the roadmap spec ("Removing or downweighting existing Informs edges when a cycle outcome is negative. Edges are additive only").

**SR-09 mitigation**: Architecture adds a `created_at DESC` index on `goal_clusters` and caps the cosine scan at 100 rows (ADR-003). This was absent from SCOPE.md and correctly added by the architect in response to the scope risk assessment.

**SR-01 / parse-failure counter**: Architecture resolves this as `warn!` log only (citing `SUMMARY_SCHEMA_VERSION` bump risk). This diverges from the specification. See V-01.

**OQ-1, OQ-2, OQ-3 (open questions left for delivery)**: The architecture leaves three open questions for the delivery agent. These are low-risk (store method name lookup, cold-start edge case confirmation, drain semantics verification). None affects correctness of the architecture design. Appropriate to defer.

**Memoisation gate correctness**: Architecture §Component 3 §Memoisation gate behaviour states the `force=false` early return path "returns before step 8b." The full pipeline path "always runs step 8b." This conflicts with SPECIFICATION.md FR-09 which requires step 8b to run "on every `context_cycle_review` call — including `force=false` cache-hit returns." Risk R-01 in the risk strategy captures this contradiction as a Critical risk with test AC-15. The specification is the authority here: step 8b must run even on cache hits. The architecture's prose is the source of R-01 — the delivery brief must flag this explicitly so the implementer does not follow the architecture prose and skip step 8b on cache hits.

This is documented as a delivery concern, not a new variance, because the specification and risk strategy already cover it with AC-15 as a gate-blocking test. However, the architecture should be read with care at this point.

### Specification Review

The specification is complete and correctly traces all acceptance criteria to functional requirements. All SCOPE.md goals are covered:

- Goal 1 (behavioral edge emission at cycle review) → FR-01–FR-09, AC-01–AC-04, AC-14, AC-15
- Goal 2 (goal_clusters population) → FR-10–FR-15, AC-05–AC-06
- Goal 3 (goal-conditioned blending in `context_briefing`) → FR-16–FR-22, AC-07–AC-11
- Goal 4 (cold-start correctness) → NFR-02, AC-08, AC-09, AC-16

The specification adds FR-03 (parse-failure counter in review result) and AC-13 which the SCOPE.md risk assessment recommended. This is a legitimate scope tightening in response to SR-01 and is aligned with the vision principle of observable system behaviour. The specification position is the correct one; see V-01 for the architecture conflict.

Specification Constraint 12 explicitly forbids blending in `context_search`: "The blending logic must be placed in `IndexBriefingService::index()` only — never in `SearchService::search()`." This is consistent with design amendment 1.

Specification Constraint 13 explicitly requires the threshold to be a config field: "`goal_cluster_similarity_threshold` is a config field ... not a hardcoded constant in `behavioral_signals.rs`." This is consistent with design amendment 2.

The two design amendments are consistently reflected in both architecture and specification. No gap found.

### Risk Strategy Review

The risk strategy is thorough and proportional. All 16 risks in the register are mapped to test scenarios with coverage requirements. All scope risks from SCOPE-RISK-ASSESSMENT.md appear in the traceability table (§Scope Risk Traceability).

Five Critical-priority risks are correctly identified (R-01 through R-05). The gate-blocking test list at the bottom of the document is appropriate. The risk strategy adds AC-11, AC-13, AC-15, AC-17, the R-02 contract test, and the drain flush requirement as non-negotiable tests not explicitly listed in SCOPE.md — these are all correct escalations from the scope risk analysis.

I-04 (`session_state.feature` vs `session_state.current_goal` independence) correctly identifies that FR-16 requires both fields for the blending path, and that an empty `current_goal` should activate cold-start before the DB call. This is a legitimate integration edge case not covered in the architecture or specification; the risk strategy is the sole document to flag it. The delivery brief should carry this forward.

E-02 (self-pair A,A) is flagged as a spec gap ("Spec is silent"). The risk strategy recommends skipping self-pairs and adds a test assertion. This is correct and consistent with the SCOPE.md non-goal of introducing graph noise. The delivery brief should include this as a resolved decision.

---

## Human-Approved Resolutions (2026-04-04)

All items below are resolved and must be carried into the implementation brief verbatim.

**V-01 — parse_failure_count surface (Option 2 approved)**
Add `parse_failure_count: u32` as a top-level field in the `context_cycle_review` JSON response, outside the serialized `CycleReviewRecord`. FR-03 and AC-13 are satisfied. No `SUMMARY_SCHEMA_VERSION` bump required. `CycleReviewRecord` struct is not extended.

**DN-1 — Memoisation gate / step 8b placement**
Architecture §Component 3 step sequence prose is **wrong** — it implies the `force=false` early-return exits before step 8b. FR-09 is the authority: step 8b runs on every call, cache-hit or miss. The brief must explicitly override the architecture prose and direct implementers to FR-09. The memoisation early-return must appear **after** step 8b in code. AC-15 is the gate test.

**DN-2 — Empty `current_goal` → cold-start**
FR-16 reads "when `session_state.current_goal` is non-empty." Treat empty string identical to absent: no `get_cycle_start_goal_embedding` call, no blending, cold-start activates immediately. The write side (crt-043) already handles this (embedding only stored for non-empty goal), so the cold-start path is doubly safe. No spec change needed; brief must make this explicit for implementers.

**DN-3 — Self-pair (A, A) exclusion**
Add `filter(|(a, b)| a != b)` in `build_coaccess_pairs` before deduplication. `Informs(A→A)` is a meaningless self-loop with no traversal value. Resolved for the brief; E-02 test assertion belongs in the pair-building unit tests.

**SR-08 — Zero remaining slots: silent suppression confirmed**
When semantic search fills all k=20 slots, cluster entries are silently suppressed. No slot expansion, no log line, no result field. This is the confirmed intended behavior. Implementers must not add unexpected slot-expansion logic.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #3742 (optional future branch / deferred scope warn pattern), #3158 (deferred scope resolution leaves AC references live), #2298 (config key semantic divergence). Entry #3742 informed the check for architectural additions not in scope. Entry #2298 informed the config threshold alignment check (design amendment 2). No prior pattern matched the architecture-vs-specification parse-failure surface divergence exactly — this is partially novel.
- Stored: nothing novel to store — V-01 (architecture and specification disagreeing on where a counter is surfaced) is a variant of a known pattern but is feature-specific in its mechanism. The existing "deferred scope resolution leaves AC references live" pattern (#3158) partially covers it; no new generalizable pattern is established by this single occurrence.
