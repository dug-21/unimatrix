# Risk-Based Test Strategy: crt-030

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Rayon offload branch — `PPR_RAYON_OFFLOAD_THRESHOLD` path deferred out of crt-030 scope; branch will not exist in this feature | High | Med | **Deferred** |
| R-02 | `use_fallback = true` guard skips PPR but pool mutation from prior steps makes result non-identical to pre-crt-030 | High | Low | High |
| R-03 | `ppr_blend_weight = 0.0` collapses PPR-only entries to similarity 0.0, leaving them alive in the pool but scoring at floor — silent semantic null | High | Low | High |
| R-04 | Step 6d node-ID sort construct per call: at 100K nodes the O(N log N) allocation is paid on every search, adding to the RayonPool offload budget calculation; if the sort is accidentally placed inside the iteration loop, latency is O(I × N log N) not O(N log N + I × N) | High | Med | High |
| R-05 | Sequential async store fetches for up to 50 PPR-only entries are sequential `.await` calls inside the search handler — a slow storage layer or contention spike turns each fetch into a serialized wait; error from any single fetch is silently skipped with no observability | Med | Med | High |
| R-06 | `ppr_inclusion_threshold` boundary: entries exactly equal to threshold — not greater than — may be included or excluded depending on `>` vs `>=` comparison, violating AC-13 spec wording "exceeds" | Med | Med | High |
| R-07 | NaN propagation: if any `hnsw_score` is 0.0 and `phase_affinity_score` returns `1.0`, the sum is non-zero and normalization is safe; but if all HNSW scores are 0.0 the zero-sum guard fires correctly — the risk is a partial-zero case where some seeds are 0.0 and normalization amplifies the non-zero seeds to values above 1.0 / near-NaN territory in extreme floating-point cases | Med | Low | Med |
| R-08 | PPR-only entries injected into `results_with_scores` bypass the quarantine check at Step 6 (which filtered entries fetched via HNSW); the Step 6d fetch must re-apply the quarantine check or quarantined entries silently appear in results | High | Med | Critical |
| R-09 | `edges_of_type` exclusivity: a developer adding a new edge traversal call in `graph_ppr.rs` might use `.edges_directed()` directly (violating AC-02); no compile-time enforcement exists — test-only coverage | Med | Med | Med |
| R-10 | Phase affinity snapshot construction (ADR-006): the spec says read from the already-cloned snapshot, not call `phase_affinity_score()` directly — an implementer reading FR-06/FR-08 in SPECIFICATION.md without ADR-006 may call the method directly and hit the wrong fallback behavior under concurrent lock ordering | Med | Med | Med |
| R-11 | `ppr_blend_weight` dual-role boundary at 1.0: existing HNSW candidates have their similarity fully replaced by PPR score; PPR-only entries get `initial_sim = ppr_score` (potentially > the HNSW floor) — this can invert the natural HNSW ranking, surfacing PPR-only entries above real HNSW candidates | Med | Low | Med |
| R-12 | Prerequisite edge traversal: #412 has not yet produced Prerequisite edges — the PPR code will include the traversal path but it is never exercised in any real or test graph. The path can silently accumulate an off-by-one in direction semantics that is only caught when #412 ships | Med | Med | Med |
| R-13 | CoAccess edge density assumption: if production co_access table has many entries with count >= 3, CoAccess edge count grows proportional to (popular entries)^2 — the score map before threshold filtering grows O(N) but dense edge traversal cost grows O(E_pos) which could exceed the 1 ms latency budget at 10K without hitting the 100K Rayon threshold | Med | Med | Med |

---

## Risk-to-Scenario Mapping

### R-01: Rayon Offload Branch Coverage — DEFERRED
**Severity**: High
**Likelihood**: Med
**Status**: **Deferred** — the `PPR_RAYON_OFFLOAD_THRESHOLD` offload branch is out of crt-030 scope. The branch will not be implemented in this feature. crt-030 ships inline-only PPR (below-threshold path).

**Impact**: (deferred) At 100K+ entries the offload branch is the only path that keeps the Tokio thread unblocked. If the branch has a logic error, PPR silently returns an empty map at scale.

**Test Scenarios**: None for crt-030. The offload path does not exist in this feature.

**Coverage Requirement**: N/A for crt-030.

**Recommendation**: Open a follow-up issue to scope both the RayonPool offload implementation and its test coverage (both-branches requirement, timeout-graceful-degradation path). That issue should reference R-01 and FM-01 from this assessment.

---

### R-02: use_fallback Guard — Bit-for-Bit Identity
**Severity**: High
**Likelihood**: Low
**Impact**: If Step 6d mutates shared state (e.g., `results_with_scores`) before checking `use_fallback`, or if the guard is placed after any allocation/mutation, search results differ from pre-crt-030 when `use_fallback = true` — violating AC-12.

**Test Scenarios**:
1. Construct a search context with `use_fallback = true` and a non-empty HNSW pool; run Step 6d and assert the pool (entry IDs and scores) is bit-for-bit identical before and after.
2. Confirm that with `use_fallback = true`, zero memory allocations occur in Step 6d (no HashMap created for seed scores or PPR output).

**Coverage Requirement**: AC-12 explicitly requires bit-for-bit identity. Test must compare `Vec` contents by value, not just length.

---

### R-03: ppr_blend_weight = 0.0 Degenerate
**Severity**: High
**Likelihood**: Low
**Impact**: PPR-only entries are injected with `initial_sim = 0.0`. They remain in the pool through co-access prefetch and NLI scoring. If floor scoring (Step 10) does not eliminate them, they appear in results with a fused score of approximately 0.0, displacing valid results or polluting the top-K.

**Test Scenarios**:
1. Run Step 6d with `ppr_blend_weight = 0.0` and confirm PPR-only entries' `initial_sim == 0.0`.
2. Trace a PPR-only entry with `initial_sim = 0.0` through fused scoring — confirm it scores below the lowest real HNSW candidate.
3. Confirm the blend formula for existing candidates reduces to `sim * 1.0 = sim` when `ppr_blend_weight = 0.0` (no PPR contamination).

**Coverage Requirement**: Boundary value 0.0 must be explicitly tested per FR-08 step 6 and AC-14.

---

### R-04: Node-ID Sort Placement — O(I × N log N) Regression Risk
**Severity**: High
**Likelihood**: Med
**Impact**: If the sorted-key Vec is reconstructed inside the iteration loop instead of before it, latency at 10K nodes goes from ~1 ms to ~20 ms (20 iterations × O(N log N) sort), violating NFR-01. This is not caught by correctness tests — only by benchmarks or timing checks.

**Test Scenarios**:
1. Code review gate: assert `all_node_ids.sort_unstable()` appears exactly once in `personalized_pagerank`, outside the iteration loop.
2. Benchmark test (can be a `#[bench]` or a timing assertion in a release test): run `personalized_pagerank` on a graph with 10K nodes, 20 iterations — assert wall time < 5 ms (2× the NFR budget as a regression gate).
3. Unit test: confirm the sorted key list is the same length as `graph.node_index.len()` — detecting cases where the sort was applied to a subset.

**Coverage Requirement**: The sort must be placed before the iteration loop by code review. A timing test at 10K scale provides a regression signal.

---

### R-05: Sequential Store Fetch Observability
**Severity**: Med
**Likelihood**: Med
**Impact**: Fetch errors are silently skipped per AC-13. In a degraded storage scenario, all 50 fetches fail silently, PPR expansion adds zero entries, and the search caller receives no signal that PPR expansion failed. At high QPS, 50 sequential fetches add latency that is invisible in search metrics.

**Test Scenarios**:
1. Mock `entry_store.get()` to return `Err(...)` for every PPR-expansion entry — confirm Step 6d completes normally and `results_with_scores` does not grow.
2. Mock `entry_store.get()` to return a quarantined entry — confirm the entry is silently skipped, pool does not contain it.
3. Timing test: with 50 real SQLite fetches on a test DB, assert Step 6d store-fetch phase completes within 10 ms (NFR-01 ceiling per ADR-008).

**Coverage Requirement**: Error skip path (AC-13) and quarantine skip path must both have explicit tests. Timing bound should be asserted.

---

### R-06: Inclusion Threshold Boundary Condition (`>` vs `>=`)
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-13 says "PPR score > ppr_inclusion_threshold" (strictly greater). Using `>=` includes entries at exactly the threshold. This is not a correctness catastrophe but violates the spec contract and can cause unexpected entries to surface in production.

**Test Scenarios**:
1. Unit test: construct a PPR result where one entry scores exactly `ppr_inclusion_threshold` — assert it is NOT included in the expansion set.
2. Unit test: construct a PPR result where one entry scores `ppr_inclusion_threshold + f64::EPSILON` — assert it IS included.
3. Unit test for config validation: assert `ppr_inclusion_threshold = 0.0` is rejected (exclusive lower bound per FR-11).

**Coverage Requirement**: Boundary value at exactly the threshold must be explicitly tested with a `>` (not `>=`) assertion.

---

### R-07: NaN / Infinity in Score Pipeline
**Severity**: Med
**Likelihood**: Low
**Impact**: If floating-point edge weight normalization produces division by zero (node with zero positive out-degree, but only if the zero-out-degree guard is absent), PPR scores become NaN or Infinity. NaN propagates silently through blend and fused scoring, producing corrupt rankings.

**Test Scenarios**:
1. Unit test: graph node with zero positive out-degree (only Supersedes out-edges) — confirm `positive_out_degree = 0` is correctly detected and the node does not propagate forward (receives teleportation mass only, as per FR-05).
2. Unit test: personalization vector with a single entry having score `f64::MIN_POSITIVE` — confirm normalization does not produce NaN.
3. Property: run `personalized_pagerank` on a realistic graph; assert all returned scores are finite and in `[0.0, 1.0]`.

**Coverage Requirement**: AC-07 (zero out-degree nodes do not propagate) directly requires a test. NaN guard must be explicitly verified.

---

### R-08: Quarantine Bypass for PPR-Only Entries
**Severity**: High
**Likelihood**: Med
**Impact**: Step 6 applies quarantine filtering to the HNSW result set. PPR-only entries are fetched in Step 6d and must independently apply the same quarantine check. If the check is omitted, quarantined entries (withdrawn knowledge, poisoned entries) appear in search results — a correctness and safety failure.

**Test Scenarios**:
1. Unit test: `entry_store.get()` returns an entry with `status = Quarantined` — confirm the entry is not appended to `results_with_scores`.
2. Unit test: `entry_store.get()` returns an entry with `status = Active` — confirm it IS appended.
3. Integration test: a quarantined entry that is a PPR neighbor of an HNSW seed must not appear in the final result set after Step 6d.

**Coverage Requirement**: AC-13 explicitly states quarantined entries are silently skipped. This must have a dedicated test, not just implicit coverage. This is the highest-impact gap in the spec's test list — it was called out in AC-13 but no corresponding named test (T-PPR-XX) exists for the quarantine path.

---

### R-09: `edges_of_type` Exclusivity — No Direct `.edges_directed()` Calls
**Severity**: Med
**Likelihood**: Med
**Impact**: Violates AC-02 and the SR-01 single-filter-boundary invariant. A direct `.edges_directed()` call bypasses the `edges_of_type` filter and could accidentally include Supersedes or Contradicts edges in PPR traversal, corrupting relevance propagation.

**Test Scenarios**:
1. Static analysis / CI check: `grep "edges_directed" crates/unimatrix-engine/src/graph_ppr.rs` must return no results (per AC-02 verification method).
2. Unit test: construct a graph where Supersedes edges connect a seed to a non-seed — confirm the non-seed receives zero PPR mass (T-PPR-08 from AC-16).
3. Unit test: construct a graph where Contradicts edges connect a seed to a non-seed — same assertion.

**Coverage Requirement**: The no-`.edges_directed()` invariant must be verified by both static check and behavioral test.

---

### R-10: Phase Affinity Snapshot vs Direct Method Call
**Severity**: Med
**Likelihood**: Med
**Impact**: ADR-006 specifies that Step 6d reads from the already-cloned `phase_snapshot` rather than calling `phase_affinity_score()` directly. An implementer calling the method directly would need to re-acquire the `PhaseFreqTableHandle` lock, violating NFR-04 (no new lock acquisitions in Step 6d) and potentially deadlocking under concurrent search load.

**Test Scenarios**:
1. Code review gate: no `phase_affinity_score(` call appears inside the Step 6d block in `search.rs` — the snapshot read pattern is used instead.
2. Unit test with a `phase_snapshot` containing non-uniform entries — confirm personalization vector values differ from the pure-HNSW baseline (AC-16 / SR-08).
3. Unit test with `phase_snapshot = None` — confirm all personalization values are `hnsw_score × 1.0` (neutral, cold-start).

**Coverage Requirement**: AC-16 requires a test for #414 data used when available. Code review must confirm no lock re-acquisition in Step 6d.

---

### R-11: ppr_blend_weight = 1.0 Score Inversion
**Severity**: Med
**Likelihood**: Low
**Impact**: At `ppr_blend_weight = 1.0`, HNSW candidates' similarity is fully overwritten by their PPR score. PPR-only entries get `initial_sim = ppr_score` (up to 1.0). In a dense graph where PPR-only entries have high PPR scores, they can rank above every HNSW candidate in the pre-fusion pool — an extreme but valid configuration that operators tuning the field must understand.

**Test Scenarios**:
1. Unit test with `ppr_blend_weight = 1.0`: an HNSW candidate with `sim = 0.9` and PPR score `0.2` should have final `sim = 0.2` — confirm the formula overwrite is correct.
2. Unit test: a PPR-only entry with `ppr_score = 0.8` and `ppr_blend_weight = 1.0` gets `initial_sim = 0.8` — confirm it ranks above HNSW candidates with lower blended similarity.
3. Confirm config validation accepts 1.0 (`[0.0, 1.0]` inclusive per FR-11) and does not reject it.

**Coverage Requirement**: Boundary value 1.0 must be tested to confirm the inclusive upper bound and the resulting blend behavior.

---

### R-12: Prerequisite Edge Direction — Silent Off-by-One Until #412 Ships
**Severity**: Med
**Likelihood**: Med
**Impact**: Prerequisite traversal uses `Direction::Incoming` per ADR-003. If the direction is accidentally coded as `Direction::Outgoing`, the function traverses `B → X` edges (where B is a seed) and surfaces what B is a prerequisite FOR — the opposite semantic. This error is undetectable until #412 begins producing Prerequisite edges in the real graph, at which point it causes a correctness regression silently.

**Test Scenarios**:
1. Unit test (synthetic graph): construct a graph with a `Prerequisite` edge `A→B`. Seed PPR with `{B: 1.0}`. Assert `result[A] > 0.0` (Incoming direction on B finds A).
2. Unit test: construct a `Prerequisite` edge `A→B`. Seed PPR with `{A: 1.0}`. Assert `result[B]` is either 0.0 or strictly less than `result[A]` (A is the seed; B has no Incoming edge from A — A only has Outgoing to B, so B should NOT receive direct propagation via Prerequisite).
3. This test must be added even though no production Prerequisite edges currently exist — it validates the direction constant used in the code.

**Coverage Requirement**: A specific Prerequisite direction test must exist regardless of #412 status. This is the only way to detect a direction regression before #412 ships.

---

### R-13: CoAccess Edge Density Latency Cliff
**Severity**: Med
**Likelihood**: Med
**Impact**: If the co_access table has O(K^2) pairs where K is the number of popular entries, CoAccess edge count can grow quadratically relative to popular-entry count — not linearly with total entry count. PPR iteration cost is O(I × E_pos), and E_pos could be 10–100× the estimated O(N) if the popular-entry co-access graph is dense. This puts the 1 ms budget at 10K nodes at risk without triggering the 100K Rayon threshold.

**Test Scenarios**:
1. Unit test with a dense CoAccess graph (50 nodes, each connected to every other — 2450 edges): assert `personalized_pagerank` completes within 1 ms (timing assertion).
2. Pre-launch validation: query the production co_access table (or staging) for total edge count and average degree — confirm E_pos / N_pos is within the 5–10× assumption from the latency budget table.
3. NFR-02 explicitly requires monitoring CoAccess edge counts from crt-029 data as a pre-launch validation requirement — this should be a named checklist item in the delivery gate.

**Coverage Requirement**: At least one dense-graph timing test. Pre-launch crt-029 edge-count check should be a named validation step.

---

## Integration Risks

### I-01: Step 6d Inserts Between 6b and 6c — Step Number Comments in search.rs
The pipeline step numbering in `search.rs` comments must be updated to reflect Step 6d insertion. If comments are not updated, future contributors will misread the step order and insert new steps in the wrong position. Verification: code review confirms `// Step 6d` comment appears between `// Step 6b` and `// Step 6c`.

### I-02: Co-Access Anchor Selection After PPR Expansion
Step 6c computes co-access boosts using anchor IDs from the top entries of `results_with_scores`. After PPR blending, the top entries may differ from the pre-PPR HNSW top entries if `ppr_blend_weight` is non-trivial. This is expected behavior, but it means the co-access boost is anchored to a PPR-adjusted ranking — a second-order interaction that should be validated. Test: confirm that when PPR surfaces an entry with high PPR score that blends to the top position, co-access prefetch uses that entry as an anchor.

### I-03: FusionWeights Sum Invariant After PPR Entry Injection
PPR-only entries enter the pool with synthetic similarity in `[0.0, ppr_blend_weight]`. These values are passed to `compute_fused_score` alongside real HNSW scores. Since `FusionWeights` are normalized to `<= 1.0` and the similarity component is just an input field (not a weight), there is no arithmetic violation — but a test should confirm no `FusionWeights` field was accidentally modified by the crt-030 implementation. Verification: assert `FusionWeights::default()` sum matches the pre-crt-030 value.

### I-04: NLI Scoring Coverage of PPR-Only Entries
Step 7 NLI scores all entries in `results_with_scores` including PPR-only entries. NLI scoring reads `entry.content` (or equivalent) — if any PPR-only entry has a missing or empty content field (e.g., fetched but partially populated), NLI scoring may produce a zero or anomalous entailment score. Verification: the integration test T-PPR-IT-01 (AC-17) must confirm the PPR-surfaced entry passes through NLI scoring without error.

---

## Edge Cases

### E-01: Empty Graph
`personalized_pagerank` on an empty `TypedRelationGraph` (zero nodes) — must return empty `HashMap` immediately. Specified in FR-01 ("If `seed_scores` is empty or all-zero, return empty map").

### E-02: Graph With No Positive Edges (All Supersedes/Contradicts)
A graph where all edges are `Supersedes` or `Contradicts` — PPR traversal finds no positive edges, no mass propagates beyond the personalization vector. Result: output map contains only the seed nodes with their teleportation mass. The zero-out-degree guard applies to every node. Test: AC-07 / T-PPR-05.

### E-03: Single-Entry Graph
One node, one seed, zero PPR neighbors — score map returns `{seed_id: ~1.0}` after iteration (teleportation only). Verify normalization does not divide by zero.

### E-04: ppr_max_expand = 1
With `ppr_max_expand = 1`, only the single highest-scoring PPR-only entry above threshold is fetched. Confirm the sort-and-cap logic correctly identifies the top-1 entry even when the PPR map contains hundreds of above-threshold entries.

### E-05: All HNSW Scores Equal (Flat Personalization Vector Before Phase Weighting)
When all HNSW scores are identical and phase affinity is 1.0 (cold-start), the personalization vector pre-normalization is a flat distribution. After normalization each seed has equal weight `1/K`. This is correct — PPR should still propagate from all seeds equally. Test: confirm non-NaN, non-zero scores are returned for positive-edge neighbors.

### E-06: ppr_alpha Extremes
`ppr_alpha` approaching `0.0` (almost pure teleportation): PPR scores converge to the personalization distribution. `ppr_alpha` approaching `1.0` (almost pure diffusion): scores spread maximally through the graph, suppressing the personalization prior. Both are valid configurations. Config validation allows `(0.0, 1.0)` exclusive — neither extreme is reachable. Test: assert config rejects `ppr_alpha = 0.0` and `ppr_alpha = 1.0` with `ConfigError`.

### E-07: PPR Score Map Contains Only Seed Entries (Dense Isolated Cluster)
When all HNSW seeds are in a disconnected subgraph with no positive edges to outside nodes, the PPR score map contains only the seed entries. No entries are added to the pool. `results_with_scores` is unchanged except for score blending. This is the zero-expansion case — verify no panic and no empty-vector access.

---

## Security Risks

### S-01: Node ID Injection via seed_scores HashMap
`seed_scores` is built from HNSW candidate entry IDs — these are `u64` integers from the database. The IDs themselves are not attacker-controlled; they come from the HNSW index over `TypedRelationGraph`. Blast radius if IDs were tampered: the score map can reference any node ID in the graph, but all lookups are bounds-checked via `HashMap::get`. No buffer overflows, no out-of-bounds array access. Risk is low.

### S-02: Entry Fetch for PPR-Only Entries
`entry_store.get(entry_id)` is called with IDs from the PPR score map. These IDs came from the pre-built `TypedRelationGraph`, which was populated from the database at tick time. An attacker who can write to `GRAPH_EDGES` could influence which entries are fetched. However, the PPR fetch path is subject to the same quarantine check as HNSW fetches — quarantined entries are skipped. An attacker writing a malicious entry to `GRAPH_EDGES` cannot force it into search results if it is quarantined. Risk is mitigated by the quarantine check requirement (R-08).

### S-03: Config Field Injection via TOML
All five PPR config fields are validated in `validate()` before the server serves requests. Out-of-range values cause `ConfigError` at startup — the server does not start. No runtime config reload path exists. An attacker supplying a malformed config (e.g., `ppr_iterations = 999999`) cannot cause runtime damage because validation rejects the config before any search is served. Risk is low.

### S-04: Memory Pressure via High ppr_max_expand or Low ppr_inclusion_threshold
`ppr_max_expand` is capped at 500 (config validation). With `ppr_max_expand = 500` and `ppr_inclusion_threshold = 0.001`, up to 500 async store fetches are issued sequentially. At sub-millisecond SQLite latency this is ~500 ms of added latency per search call — a de facto DoS on the search handler for any caller with access to override config. Mitigation: `ppr_max_expand` max of 500 and `ppr_inclusion_threshold` min floor `(0.0, ...)` exclusive are the existing config guards. No additional risk beyond operator misconfiguration.

---

## Failure Modes

### FM-01: RayonPool Timeout at 100K Nodes — DEFERRED
The Rayon offload path (`PPR_RAYON_OFFLOAD_THRESHOLD`) is out of crt-030 scope (R-01 deferred). This failure mode does not apply to this feature. The graceful-degradation behavior (`unwrap_or_else(|_| HashMap::new())` + `tracing::warn!`) should be designed and tested in the follow-up issue that scopes R-01.

### FM-02: All Store Fetches Fail
If every `entry_store.get()` call in the expansion phase returns `Err`, no PPR-only entries are added. The pool remains the HNSW set plus blended scores only. Correct behavior per AC-13. No error surface.

### FM-03: TypedGraphState.use_fallback Flips to True During Search
The read lock is held briefly to clone state; after release, if `use_fallback` becomes true (a subsequent tick detects a Supersedes cycle), the current in-progress search runs with `use_fallback = false` already cloned. This is by design — the search operates on a snapshot. PPR runs normally with the snapshot. No inconsistency.

### FM-04: Phase Snapshot Missing (None) at Step 6d
`phase_snapshot = None` means no col-031 data is available. Personalization vector falls back to `hnsw_score × 1.0` for all seeds. PPR runs with uniform phase weighting. Search proceeds normally — reduced quality, correct behavior. This is Workflow 3 in the specification.

### FM-05: Zero-Sum Personalization Vector
All HNSW candidates have `sim = 0.0` (degenerate — HNSW normally returns scores in [0.3, 1.0]). The zero-sum guard fires: PPR is skipped, Step 6d exits, `results_with_scores` is unchanged. No panic, no empty-pool error.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (PPR latency budget at scale) | R-04, R-13 (R-01 deferred) | Architecture defines explicit budgets (ADR-008): < 1 ms at 10K inline. Rayon offload path (100K) is deferred out of crt-030 — R-01 has no coverage requirement for this feature. Node-ID sort placed once outside the iteration loop (R-04). |
| SR-02 (sequential fetch latency) | R-05 | Architecture accepts sequential fetches as v1 with 10 ms ceiling (ADR-008). Batch fetch deferred. Test must assert timing bound. |
| SR-03 (step order contradiction) | — | Resolved. SPECIFICATION.md FR-07 is unambiguous: `6b → 6d → 6c → 7`. Background Research stale text corrected in Architecture and Spec. |
| SR-04 (ppr_blend_weight dual role) | R-03, R-11 | Intentional per ADR-007. Both boundary values (0.0 and 1.0) require explicit tests. Doc-comment on the field must document both roles. |
| SR-05 (PPR score map O(N) memory) | — | Resolved per ADR-009: O(N) is acceptable at all realistic scale points. No traversal depth cap needed. Score map is short-lived per search call. |
| SR-06 (phase_affinity_score without use_fallback guard) | R-10 | Resolved per ADR-006: Step 6d reads from the already-cloned `phase_snapshot` (no method call, no lock re-acquisition). Code review must confirm no direct `phase_affinity_score()` call in Step 6d. |
| SR-07 (PPR-only entries with synthetic similarity) | R-08, I-04 | Resolved per FR-09: fused scorer and NLI make no provenance assumptions. R-08 (quarantine bypass) is the highest-residual risk — explicit quarantine test required. |
| SR-08 (#414 phase data path) | R-10, AC-16 | AC-16 requires a test with non-uniform phase data producing a different personalization vector than the uniform baseline. Cold-start path tested via `phase_snapshot = None`. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Deferred | 1 (R-01) | 0 — offload branch not in crt-030; follow-up issue required |
| Critical | 1 (R-08) | 3 scenarios minimum |
| High | 4 (R-02, R-03, R-04, R-05) | 9 scenarios minimum |
| Med | 7 (R-06 through R-13) | 14 scenarios minimum |
| Low | — | — |

**Non-negotiable tests** (must exist by gate or the risk is unmitigated):
- Quarantine skip for PPR-only entries (R-08) — not in AC-16 test list, added by this assessment
- Prerequisite direction unit test (R-12) — required before #412 ships
- `use_fallback = true` bit-for-bit identity (R-02 / AC-12)
- Inclusion threshold strictly-greater boundary (R-06)
- Zero out-degree node does not propagate (R-07 / AC-07)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned, risk patterns, pipeline integration, sequential fetch, determinism — results: #2800 (circuit-breaker cap logic gate failure), #1628 (per-query store reads MCP instability), #2964 (signal fusion sequential sort NLI override), #729 (cross-crate integration test pattern)
- Stored: nothing novel to store — crt-030 risks are feature-specific; the quarantine-bypass-for-injected-entries pattern may warrant a future pattern entry once crt-030 ships and the pattern generalizes
