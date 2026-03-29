# Gate 3a Report: crt-030

> Gate: 3a (Component Design Review) — re-check after rework
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All three components match architecture decomposition; interfaces match ADR contracts |
| Specification coverage | PASS | All FRs and NFRs covered; no scope additions |
| Risk coverage (test plans) | PASS | All 13 risks mapped; R-08 has 3 scenarios; R-12 has dedicated unit test |
| Interface consistency | WARN | Minor spec wording vs. ADR-006 tension — pseudocode correctly resolves in ADR's favor |
| Knowledge stewardship — architect | PASS | `## Knowledge Stewardship` section now present; 9 ADRs listed as stored (#3731–#3739); `Queried:` entries present |
| Knowledge stewardship — risk-strategist | PASS | Has `Queried:` and `Stored:` entries with reason |
| Knowledge stewardship — pseudocode | WARN | Block present; `Queried:` present; storage disposition implied but not in standard `Stored: nothing novel to store -- {reason}` format |
| Knowledge stewardship — test plan | PASS | Has `Queried:` and `Stored:` entries with reason |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

- **Component boundaries**: Three components in pseudocode (`graph_ppr.rs`, `config.rs` extension, `search.rs` Step 6d) exactly match the three components defined in ARCHITECTURE.md §Component Breakdown.

- **`graph_ppr.rs` structure**: Pseudocode declares `#[path = "graph_ppr.rs"] mod graph_ppr;` and `pub use graph_ppr::personalized_pagerank;` in `graph.rs`, matching ADR-001's submodule-of-`graph.rs` pattern (mirrors `graph_suppression.rs`).

- **Interfaces match ADR contracts**:
  - ADR-002 function signature: `fn personalized_pagerank(graph: &TypedRelationGraph, seed_scores: &HashMap<u64, f64>, alpha: f64, iterations: usize) -> HashMap<u64, f64>` — exact match in `graph_ppr.md`.
  - ADR-003 direction semantics: all three edge types use `Direction::Incoming` in the main loop; `positive_out_degree_weight` uses `Direction::Outgoing` (correct for out-degree normalization).
  - ADR-004 sort placement: `all_node_ids.sort_unstable()` appears once before the iteration loop in `graph_ppr.md`.
  - ADR-005 step order: `search_step_6d.md` inserts between Step 6b and Step 6c explicitly.
  - ADR-006 snapshot read: `search_step_6d.md` reads from `phase_snapshot` with no direct `phase_affinity_score()` call — correct.
  - ADR-007 dual role: `config_ppr_fields.md` documents both roles in the `ppr_blend_weight` field doc-comment.

- **Technology choices consistent**: `HashMap<u64, f64>` for score map (no BTreeMap — ADR-004). Sequential async fetches (no Rayon — ADR-008 deferred). No new locks at Step 6d (NFR-04).

- **Lock ordering**: Step 6d uses pre-cloned `typed_graph` and pre-extracted `phase_snapshot`. No new lock acquisitions. Consistent with ARCHITECTURE.md §Component Interactions lock chain.

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

| Requirement | Coverage |
|-------------|----------|
| FR-01: PPR function signature | `graph_ppr.md` exactly matches spec signature |
| FR-02: Power iteration algorithm | `graph_ppr.md` implements formula with correct teleportation + diffusion terms |
| FR-03: Edge type inclusion (edges_of_type only) | `graph_ppr.md` uses only `edges_of_type` calls; no `.edges_directed()` |
| FR-04: Edge direction semantics | All three types use `Direction::Incoming`; out-degree uses `Direction::Outgoing` |
| FR-05: Out-degree normalization | `positive_out_degree_weight` helper correctly counts only positive edges |
| FR-06: Personalization vector construction | `search_step_6d.md` reads from `phase_snapshot`, cold-start → 1.0 |
| FR-07: Step 6d position | `search_step_6d.md` inserts after 6b, before 6c |
| FR-08: Full Step 6d algorithm | All 7 sub-steps (guard → seed → normalize → PPR → blend → candidates → fetch+inject) present |
| FR-09: PPR-only entry score treatment | `initial_sim = ppr_blend_weight * ppr_score` documented |
| FR-10: Module structure | `#[path]` submodule pattern documented in `graph_ppr.md` |
| FR-11: InferenceConfig extension | All 5 fields with correct ranges in `config_ppr_fields.md` |
| FR-12: crt-029 naming disambiguation | Doc-comment note present in `config_ppr_fields.md` |

Non-functional requirements addressed: NFR-01 timing tests planned; NFR-03 determinism via sorted Vec; NFR-04 no new locks; NFR-05 no schema changes confirmed; NFR-06 FusionWeights regression test planned.

No scope additions found — pseudocode implements exactly what is specified, nothing beyond it.

### Check 3: Risk Coverage (Test Plans vs. Risk-Based Test Strategy)

**Status**: PASS

All 13 risks from RISK-TEST-STRATEGY.md are mapped in `test-plan/OVERVIEW.md`:

| Risk | Priority | Plan Coverage | Gap? |
|------|----------|---------------|------|
| R-01 | Deferred | Correctly mapped as deferred; zero scenarios | None |
| R-02 | High | `test_step_6d_skipped_when_use_fallback_true`, `test_step_6d_use_fallback_true_no_allocation` | None |
| R-03 | High | Blend=0.0 leaves HNSW unchanged; PPR-only sim=0.0; blend formula | None |
| R-04 | High | Sort-outside-loop code review gate; sort length == node_index.len(); 10K timing | None |
| R-05 | High | Fetch error skipped; all fetches fail → pool unchanged | None |
| R-06 | High | Exact threshold not included; threshold+epsilon included; threshold=0.0 rejected | None |
| R-07 | Med | Zero out-degree no propagation; MIN_POSITIVE seed no NaN; all scores finite | None |
| R-08 | **Critical** | `test_step_6d_quarantined_entry_not_appended`, `test_step_6d_active_entry_appended`, T-PPR-IT-02 (integration) — **exactly 3 met** | None |
| R-09 | Med | grep gate (no edges_directed); Supersedes/Contradicts excluded behavioral tests | None |
| R-10 | Med | grep gate (no direct phase_affinity_score call); non-uniform snapshot test; cold-start None test | None |
| R-11 | Med | blend_weight=1.0 overwrites HNSW; PPR-only at blend=1.0 ranks above lower HNSW | None |
| R-12 | Med | `test_prerequisite_incoming_direction`, `test_prerequisite_wrong_direction_does_not_propagate` | None |
| R-13 | Med | Dense 50-node CoAccess completes < 1 ms; pre-launch edge-count validation requirement | None |

Integration risks I-01 through I-04 are all addressed. Non-negotiable tests from the strategy are all present.

### Check 4: Interface Consistency

**Status**: WARN

**Evidence**: All shared types in `OVERVIEW.md` are consistently used across component files:
- `TypedRelationGraph` — `&TypedRelationGraph` consistently in all signatures.
- `RelationType` — referenced in all three edge-type call sites.
- `RelationEdge.weight: f32` — cast to `f64` consistently.
- `SecurityGateway::is_quarantined` — applied in `search_step_6d.md`.
- All five PPR field names and types — consistent across pseudocode and test plans.

**Warning**: SPECIFICATION.md FR-06/FR-08 says "call `phase_affinity_score(entry_id)` directly" while ADR-006 says "does NOT call `phase_affinity_score()` directly — reads from the already-cloned snapshot." The pseudocode in `search_step_6d.md` correctly follows ADR-006. The spec's phrasing predates ADR-006 and uses "directly" to mean "without a use_fallback guard." This is a documentation inconsistency in the spec, not a pseudocode defect — the implementation agent must follow the pseudocode (ADR-006), not the spec's literal wording.

### Check 5: Knowledge Stewardship — Architect

**Status**: PASS

**Evidence**: `product/features/crt-030/agents/crt-030-agent-1-architect-report.md` now contains:

```markdown
## Knowledge Stewardship

**Stored:**
- ADR-001 graph_ppr.rs submodule structure → Unimatrix #3731
- ADR-002 personalized_pagerank() function signature → Unimatrix #3732
- ADR-003 edge direction semantics (Incoming for all three types) → Unimatrix #3733
- ADR-004 deterministic accumulation via node-ID-sorted iteration → Unimatrix #3734
- ADR-005 pipeline position Step 6d between 6b and 6c → Unimatrix #3735
- ADR-006 personalization vector construction via pre-cloned snapshot → Unimatrix #3736
- ADR-007 ppr_blend_weight dual role intentional → Unimatrix #3737
- ADR-008 latency budget and RayonPool offload (deferred) → Unimatrix #3738 (corrected → #3741)
- ADR-009 PPR score map memory profile no traversal depth cap → Unimatrix #3739

**Queried:**
- context_search: "graph traversal implementation patterns edges_of_type" — retrieved graph_suppression.rs pattern
- context_search: "crt-030 architectural decisions", category: decision — retrieved prior session ADRs
```

All 9 ADRs are listed as stored with Unimatrix IDs. `Stored:` and `Queried:` entries present. Stewardship obligation satisfied.

### Check 6: Knowledge Stewardship — Risk-Strategist

**Status**: PASS

**Evidence**: The RISK-TEST-STRATEGY.md itself contains the stewardship block with `Queried:` entries (#2800, #1628, #2964, #729) and `Stored: nothing novel to store — crt-030 risks are feature-specific` with a reason. Satisfies requirements.

### Check 7: Knowledge Stewardship — Pseudocode Agent

**Status**: WARN

**Evidence**: `test-plan/graph_ppr.md` contains:
```
## Knowledge Stewardship
- Queried: `context_briefing` — surfaced ADRs #3731-#3740, pattern #3740, pattern #264
- Queried: `context_search` for graph traversal testing patterns — surfaced #1607, #3627
```

`Queried:` entries are present. Storage disposition uses "Deviations from established patterns: none" in the pseudocode report rather than the standard `Stored: nothing novel to store -- {reason}` format. Block is present and shows evidence of querying; non-standard wording is a minor format gap.

### Check 8: Knowledge Stewardship — Test Plan Agent

**Status**: PASS

**Evidence**: The test plan overview contains stewardship entries with `Queried:` and `Stored: nothing novel to store` with a reason. Satisfies requirements.

---

## Mandatory Key Checks (from spawn prompt)

1. **`personalized_pagerank` signature matches ADR-002 exactly**: PASS — exact match in `graph_ppr.md`.
2. **All traversal uses `edges_of_type` only — no `.edges_directed()` in pseudocode**: PASS — verified in `graph_ppr.md` main loop and both helpers (`incoming_contribution`, `positive_out_degree_weight`).
3. **Node-ID sort Vec constructed ONCE before the iteration loop (ADR-004 / R-04)**: PASS — `all_node_ids.sort_unstable()` appears before `FOR _ IN 0..iterations DO` in `graph_ppr.md`; Vec is never reconstructed inside the loop.
4. **Step order: 6b → 6d → 6c → 7**: PASS — OVERVIEW.md pipeline diagram and `search_step_6d.md` insertion point both confirm this order explicitly.
5. **R-08 (Critical): quarantine check on every entry fetched in Step 6d**: PASS — `SecurityGateway::is_quarantined(&entry.status)` applied immediately after every `entry_store.get()` call in `search_step_6d.md`.
6. **All 5 InferenceConfig fields with correct ranges**: PASS — all five fields with exact ranges and types from FR-11 present in `config_ppr_fields.md`.
7. **R-08 has 3+ dedicated test scenarios**: PASS — `test_step_6d_quarantined_entry_not_appended`, `test_step_6d_active_entry_appended`, T-PPR-IT-02 (integration).
8. **R-12 (Prerequisite direction) has a unit test**: PASS — `test_prerequisite_incoming_direction` and `test_prerequisite_wrong_direction_does_not_propagate` present in `test-plan/graph_ppr.md`.
9. **Architect report has Knowledge Stewardship section**: PASS — section present with 9 stored ADRs (Unimatrix #3731–#3739) and 2 `Queried:` entries.

---

## Notes for Implementation Agents

1. **Spec vs. ADR-006 wording gap (WARN)**: SPECIFICATION.md FR-06 and FR-08 say "call `phase_affinity_score` directly." ADR-006 resolves this to mean "read from the pre-cloned `phase_snapshot`." The pseudocode is correct (follows ADR-006). The implementation agent must follow `search_step_6d.md` pseudocode (snapshot read, no method call) and NOT interpret FR-06/FR-08 literally.

2. **Pseudocode stewardship format (WARN)**: The pseudocode agent report uses "Deviations from established patterns: none" instead of the standard `Stored: nothing novel to store -- {reason}` format. Acceptable for this gate; future phases should use the standard format.

3. **T-PPR-IT-01 and T-PPR-IT-02 harness dependency**: Whether these tests live in `infra-001/test_lifecycle.py` or fall back to inline `search_tests.rs` unit tests depends on whether `GRAPH_EDGES` rows can be written in the test harness. The Stage 3c tester must resolve this.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate-3a findings are feature-specific; the previously-identified architect-missing-stewardship pattern is a known validation rule already covered by gate rules.
