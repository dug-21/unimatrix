# Security Review: crt-030-security-reviewer

## Risk Level: medium

## Summary

PR #443 introduces Personalized PageRank (PPR) as Step 6d in the `context_search` pipeline. The change is well-contained: no schema changes, no new dependencies, no hardcoded secrets, and all five new config fields are validated at startup with bounds checking. The quarantine bypass risk (R-08) — the highest-severity security concern identified in the pre-implementation risk assessment — is correctly mitigated in the production code. One medium-severity finding requires attention before merge: the implementation uses `Direction::Outgoing` throughout `graph_ppr.rs` while ADR-003 (Unimatrix entry #3733), ARCHITECTURE.md, ACCEPTANCE-MAP AC-08, and RISK-TEST-STRATEGY R-12 all specify `Direction::Incoming`. The implementation contains a mathematical justification claiming equivalence ("reverse random-walk formulation"), but this claim is not mathematically correct — the two formulations produce different score distributions. All tests pass with the Outgoing implementation, which means the tests were written to match the code rather than to verify the ADR contract.

---

## Findings

### Finding 1: Direction Discrepancy — Outgoing vs Incoming (ADR-003 Violation)

- **Severity**: medium
- **Location**: `crates/unimatrix-engine/src/graph_ppr.rs:98, 104, 110, 164, 167, 170`
- **Description**: ADR-003 (Unimatrix entry #3733) specifies `Direction::Incoming` for all three positive edge types in `personalized_pagerank`. The ARCHITECTURE.md component diagram at lines 92-94 shows `Incoming`. ACCEPTANCE-MAP AC-08 states "`Supports` and `Prerequisite` edges traverse `Direction::Incoming`". RISK-TEST-STRATEGY R-12 describes the defect mode as "accidentally coded as `Direction::Outgoing`". The code uses `Direction::Outgoing` for all three edge types and justifies this as a "reverse random-walk formulation" that surfaces "in-neighbors of seeds". The mathematical claim in the doc-comment is: node u's score accumulates v's score when there is an edge u→v and v is a high-scoring seed. This is the **transpose** of the standard PPR update (where mass flows from u to v). The two formulations are not equivalent: they produce different score distributions, different out-degree normalizations, and different convergence properties. The tests in `graph_ppr_tests.rs` verify behavioral outcomes (A supports B, seed on B, A surfaces) that happen to be satisfied by the Outgoing formulation — but the tests were written to match the current code, not to verify the Incoming ADR contract. The RISK-TEST-STRATEGY itself (R-12 scenario 1) says: "construct a graph with a Prerequisite edge `A→B`, seed PPR with `{B: 1.0}`, assert `result[A] > 0.0` (Incoming direction on B finds A)." This test passes with the Outgoing formulation as well — making the direction test insufficient to distinguish the two implementations.
- **Recommendation**: The team must explicitly decide: (a) the ADR-003/ARCHITECTURE.md/AC-08 `Incoming` spec was the intended design, in which case the code has the wrong direction and must be corrected, and ADR-003 must be updated to document the correction; or (b) the developer made a deliberate decision to use the Outgoing/transpose formulation and found it gives better results, in which case ADR-003, ARCHITECTURE.md, AC-08, and RISK-TEST-STRATEGY R-12 must all be updated to document the actual implementation, and additional tests must be added that distinguish Incoming from Outgoing behavior (e.g., a test that fails with Incoming but passes with Outgoing, proving the direction matters). Option (b) is acceptable if documented — but the current state has four specification artifacts and one ADR contradicting the code with no update trail.
- **Blocking**: No — the code is internally consistent, all tests pass, and the behavioral outcome (surfacing supporters of seeds) is achieved. However, the undocumented divergence from ADR-003 is a knowledge-base integrity issue that must be resolved before the ADR is considered trustworthy for future implementers.

---

### Finding 2: Architecture Doc Says `Incoming` — Code Comment Claims Equivalence Without Proof

- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/graph_ppr.rs:34-38` (doc-comment on `personalized_pagerank`)
- **Description**: The doc-comment states "This is the reverse random-walk formulation that surfaces in-neighbors of seeds, consistent with ADR-003's goal." The phrase "consistent with ADR-003's goal" is misleading — ADR-003 does not specify a goal-only; it specifies `Direction::Incoming` as the mechanism. The doc-comment implicitly acknowledges the divergence without explicitly calling it out, and does not update the ADR reference to reflect that a different mechanism was chosen.
- **Recommendation**: Either correct the code direction to `Incoming` (Finding 1 path a), or update the doc-comment to state explicitly: "ADR-003 specified Incoming traversal; this implementation uses Outgoing (transpose formulation) because [reason]. ADR-003 has been updated to reflect this decision."
- **Blocking**: No.

---

### Finding 3: Quarantine Bypass for PPR-Only Entries — R-08 Critical Risk CORRECTLY MITIGATED

- **Severity**: low (risk mitigated)
- **Location**: `crates/unimatrix-server/src/services/search.rs:946`
- **Description**: R-08 (Critical) identified that PPR-only entries bypass the Step 6 HNSW quarantine filter. The code correctly applies `SecurityGateway::is_quarantined(&entry.status)` at line 946 before appending any PPR-fetched entry to `results_with_scores`. The check is identical to the one used at Step 6 for HNSW candidates (line 647). A sync test (`test_step_6d_quarantine_check_applies_to_fetched_entries`) verifies `is_quarantined` returns `true` for `Status::Quarantined` and `false` for `Status::Active`. Note: the test is a sync proxy test for the logic correctness — it does not exercise the full async Step 6d path end-to-end with a mocked store that returns a quarantined entry. This is acceptable given the code path is simple and the logic test is direct.
- **Recommendation**: No action required. The mitigation is correct. Consider adding a full async integration test as a follow-up (as noted in the RISK-TEST-STRATEGY R-08 coverage requirement) to guard the end-to-end path.
- **Blocking**: No.

---

### Finding 4: `Deprecated` Entries Allowed Through PPR Expansion Path

- **Severity**: low (by design, consistent with existing behavior)
- **Location**: `crates/unimatrix-server/src/services/search.rs:946`
- **Description**: `SecurityGateway::is_quarantined` returns `false` for `Status::Deprecated` entries. This means `Deprecated` entries fetched via PPR expansion are injected into `results_with_scores` with no graph penalty at initial injection. In the HNSW path, `Deprecated` entries are also allowed through Step 6 and receive graph penalties in the fused scoring pass (penalty at line 1140 via `penalty_map`). Verified: the penalty map is computed over `all_entries`, which includes PPR-injected entries — so Deprecated PPR entries will receive the correct penalty at scoring time. This is consistent behavior.
- **Recommendation**: No action required. The behavior is correct by design.
- **Blocking**: No.

---

### Finding 5: Config Validation — All Five Fields Correctly Bounded

- **Severity**: low (no issue, confirmation only)
- **Location**: `crates/unimatrix-server/src/infra/config.rs:914-961`
- **Description**: All five PPR config fields are validated at startup before the server accepts requests. Bounds: `ppr_alpha` in `(0.0, 1.0)` exclusive; `ppr_iterations` in `[1, 100]` inclusive; `ppr_inclusion_threshold` in `(0.0, 1.0)` exclusive; `ppr_blend_weight` in `[0.0, 1.0]` inclusive; `ppr_max_expand` in `[1, 500]` inclusive. Maximum DoS surface at `ppr_max_expand=500` and minimal `ppr_inclusion_threshold`: 500 sequential async fetches per search. At sub-millisecond SQLite latency this is ~50ms added per search. This is the documented worst-case (ADR-008, SR-02) and the cap at 500 is a deliberate DoS mitigation. Config is read-only at runtime; no hot-reload path exists.
- **Recommendation**: No action required.
- **Blocking**: No.

---

### Finding 6: No New Dependencies Introduced

- **Severity**: low (no issue)
- **Location**: No `Cargo.toml` changes in the diff
- **Description**: The diff contains zero `Cargo.toml` changes. PPR uses only `std::collections::HashMap`, `petgraph` (pre-existing in `unimatrix-engine`), and project-internal types. No new crates, no new petgraph features. `cargo audit` is not installed in this environment, but the absence of new dependencies means the existing dependency CVE surface is unchanged.
- **Recommendation**: No action required.
- **Blocking**: No.

---

### Finding 7: No Hardcoded Secrets

- **Severity**: low (no issue)
- **Location**: All modified files
- **Description**: Full diff scan found no API keys, tokens, passwords, or credentials in any modified file.
- **Recommendation**: No action required.
- **Blocking**: No.

---

## Blast Radius Assessment

**Worst case if the Outgoing direction is subtly wrong**: PPR scores are computed using the transpose of the intended graph — nodes that point TO seeds gain mass rather than nodes that seeds point TO. In the Supports use case (A supports B, seed on B), the Outgoing formulation surfaces A (A→B, and B is a seed) — which is the correct behavioral outcome. For the Prerequisite use case (A is prerequisite of B: edge A→B, seed on B), the Outgoing formulation also surfaces A — same outcome. For CoAccess (bidirectional storage), both formulations are equivalent. The practical blast radius of the direction choice is therefore low in the current edge set — the wrong algorithm produces the right answers for the current topology. The risk materializes when edge semantics diverge from the symmetric case (particularly for Prerequisite edges once #412 ships).

**Worst case if PPR scores are corrupted (NaN/Infinity)**: NaN propagates through the blend formula `(1.0 - w) * sim + w * ppr_score`, corrupting HNSW candidates' similarity scores. The scoring pass would then produce NaN fused scores, and the sort (`partial_cmp(...).unwrap_or(Ordering::Equal)`) would treat NaN entries as equal — they would appear in results in arbitrary order. This is a result quality degradation, not a data corruption or privilege escalation. The zero-out-degree guard and normalized seed scores mitigate NaN production. Tests (`test_ppr_scores_all_finite`, `test_ppr_single_min_positive_seed_no_nan`) cover this path.

**Worst case if quarantine check were absent**: Quarantined (withdrawn/poisoned) entries could appear in search results. This is a correctness and trust failure — agents could retrieve quarantined knowledge as though it were authoritative. The mitigation is correctly implemented and verified.

---

## Regression Risk

**Low-to-medium overall.** Specific risks:

1. `use_fallback = true` guard: confirmed correct — the entire Step 6d block is inside `if !use_fallback { ... }`. Pre-crt-030 behavior is preserved bit-for-bit when `use_fallback = true`.

2. Step ordering `6b → 6d → 6c → 7`: confirmed correct. The co-access prefetch (Step 6c) has been moved after Step 6d in the diff — PPR expansion happens before co-access anchoring, so PPR-surfaced entries participate in co-access boosts (intended).

3. `FusionWeights` sum invariant: confirmed unchanged. Test `test_fusion_weights_default_sum_unchanged_by_crt030` guards this explicitly.

4. Scoring pass regression: PPR-only entries enter with `initial_sim = ppr_blend_weight * ppr_score`. At default `ppr_blend_weight = 0.15`, initial_sim is at most 0.15 — below typical HNSW cosine similarity scores. They will rank below HNSW candidates unless NLI/confidence scores elevate them, which is intended behavior.

5. Lock ordering: confirmed no new lock acquisitions in Step 6d. The `typed_graph` and `phase_snapshot` are already-cloned values.

---

## PR Comments

Posted via `gh pr review --comment` below.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the direction-discrepancy-between-ADR-and-implementation pattern is feature-specific to crt-030 and requires resolution before it becomes a generalizable anti-pattern. If the Outgoing formulation is retained and documented, a lesson-learned about "transpose PPR vs standard PPR — verify ADR matches implementation direction" would be worth storing post-merge.
