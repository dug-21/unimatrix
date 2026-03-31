# Risk-Based Test Strategy: crt-037

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `GRAPH_EDGES.relation_type` has a CHECK constraint — inserting `"Informs"` silently fails or errors with no migration path (OQ-S1) | High | Low | Critical |
| R-02 | PPR direction for `Informs` uses wrong `Direction` enum value — no mass flows from lesson nodes to decision seeds | High | Med | Critical |
| R-03 | Phase 8b composite guard partially applied — one predicate missing causes spurious `Informs` edges (same-cycle, same-timestamp, or wrong-category pairs written) | High | Med | Critical |
| R-04 | `NliCandidatePair` origin routing cross-contaminates write paths — Informs pairs evaluated against entailment threshold, SupportsContradict pairs evaluated against neutral threshold | High | Low | High |
| R-05 | Phase 4b → Phase 7 metadata survival failure — `InformsCandidate` fields (`source_created_at`, `feature_cycle`, etc.) not populated correctly, causing Phase 8b guard re-verification to pass vacuously | High | Med | High |
| R-06 | Cap priority sequencing incorrect — Informs candidates processed before Supports exhausts cap, starving Supports/Contradicts detection | High | Low | High |
| R-07 | `NliScores.neutral` is a residual `(1 - entailment - contradiction)` rather than a direct model logit — neutral > 0.5 threshold carries higher noise than assumed (OQ-S2) | Med | Med | High |
| R-08 | Phase 4b source-candidate category check bypassed — domain strings (`"lesson-learned"` etc.) leak into `nli_detection_tick.rs` or the category filter is never applied | Med | Med | High |
| R-09 | `query_existing_informs_pairs` accidentally normalizes `(min, max)` instead of directional `(source, target)` — dedup suppresses valid edges on re-runs | Med | Med | High |
| R-10 | `graph_penalty` / `find_terminal_active` traverses `Informs` edges after the change — penalty contamination corrupts supersession logic | High | Low | High |
| R-11 | Cap accounting math incorrect — `remaining_capacity = max_cap - supports_pairs.len()` computed before `supports_pairs` is truncated, allowing combined batch to exceed `max_graph_inference_per_tick` | Med | Med | Medium |
| R-12 | Silent Informs starvation — Supports fills cap every tick, zero Informs candidates processed, no log signal distinguishing starvation from empty candidate pool (SR-03 partial) | Med | Med | Medium |
| R-13 | `select_source_candidates` does not return category metadata — Phase 4b requires a secondary DB lookup per source entry, adding O(N) latency not accounted for in NF-01 budget (OQ-S3) | Med | Med | Medium |
| R-14 | Rayon closure async contamination — new Phase 7 merged batch path introduces `tokio::runtime::Handle::current()` or `.await` inside the closure (C-14 / R-09 / NF-04) | Med | Low | Medium |
| R-15 | `Informs` edge weight is NaN or ±Inf — `similarity = 0.0` or `nli_informs_ppr_weight = 0.0` produces `0.0` (acceptable), but denormalized f32 input produces non-finite weight (C-13 / NF-08) | Low | Low | Low |
| R-16 | Zero-regression: existing `Supports`/`Contradicts` detection behavior altered by merged batch refactor — edge write count changes for previously-passing test fixtures | High | Low | High |
| R-17 | `Informs` edge pair written on first tick, written again on second tick — dedup pre-filter or `INSERT OR IGNORE` not applied, causing duplicate `GRAPH_EDGES` rows | Med | Low | Medium |
| R-18 | Config validation boundary errors — `nli_informs_cosine_floor = 0.0` or `= 1.0` accepted (exclusive bound), or `nli_informs_ppr_weight = -0.01` accepted (inclusive bound) | Low | Low | Low |
| R-19 | Gap-2 (FR-11) mutual exclusion: same pair satisfies both `entailment > supports_edge_threshold` AND `neutral > 0.5` — pair written as both `Supports` and `Informs` edge, corrupting graph semantics | Med | Low | Medium |
| R-20 | Test wave omits mandatory tick integration tests (AC-13 through AC-23) — entire Phase 4b/8b path untested at gate submission (entry #3579 pattern) | High | Med | Critical |

---

## Risk-to-Scenario Mapping

### R-01: CHECK Constraint on `GRAPH_EDGES.relation_type`

**Severity**: High | **Likelihood**: Low
**Impact**: Every `write_nli_edge("Informs", ...)` call silently drops rows or raises a DB error. No `Informs` edges are written. The feature delivers zero functional value with no compiler or test signal.

**Test Scenarios**:
1. Pre-delivery DDL inspection: read the `CREATE TABLE graph_edges` statement and assert `relation_type` has no `CHECK` clause. This is a delivery gate prerequisite (C-01 / OQ-S1), not a runtime test.
2. Integration test: call `write_nli_edge` with `edge_type = "Informs"` against a real SQLite fixture and assert the row is present with no error.
3. Integration test: query `GRAPH_EDGES` after the write and assert `relation_type = "Informs"` is retrievable verbatim.

**Coverage Requirement**: DDL inspection must pass before any Phase C implementation begins. Runtime insertion test must be in the store integration suite.

---

### R-02: PPR Direction Wrong for `Informs`

**Severity**: High | **Likelihood**: Med
**Impact**: Lesson nodes receive zero PPR mass when decision nodes are seeded. The core feature value — surfacing past lessons when decisions are queried — is silently absent. AC-05 would catch this if the assertion tests the *specific lesson node*, not just any non-zero score.

Historical evidence: entry #3754 documents that crt-030 shipped a spec that described `Direction::Incoming` while the correct implementation uses `Direction::Outgoing`. The direction error survived two gate checks and was found post-merge. The risk here is the inverse: the spec correctly specifies `Direction::Outgoing`, but a test that only asserts `scores.values().any(|&v| v > 0.0)` rather than `scores[lesson_node_id] > 0.0` provides false assurance.

**Test Scenarios**:
1. Unit test (graph_ppr.rs): construct a two-node graph with nodes `A` (lesson-learned) and `B` (decision). Add a single `Informs` edge `A → B`. Seed PPR at `B`. Assert `scores[A] > 0.0` — explicitly by the lesson node index, not by any node.
2. Negative direction test: the same graph with `Direction::Incoming` substituted (manual) should produce `scores[A] = 0.0`. Document this as a comment to prevent future confusion (entry #3754 lesson).
3. Unit test: `positive_out_degree_weight` called on node `A` with one `Informs` edge to `B` returns `weight > 0.0`. With `Direction::Incoming`, returns `0.0` — verify the correct value.
4. CI grep gate: `grep -n 'Direction::Incoming' graph_ppr.rs` returns empty after the change — no accidental reversion.

**Coverage Requirement**: AC-05 test must assert the specific lesson-node score, not aggregate non-zero. Direction regression grep must be a CI check.

---

### R-03: Phase 8b Composite Guard Partially Applied

**Severity**: High | **Likelihood**: Med
**Impact**: Spurious `Informs` edges written for same-cycle pairs, temporally reversed pairs, or category-mismatched pairs. Contaminates the graph with false institutional memory bridges. Downstream PPR traversal surfaces unrelated content.

**Test Scenarios**:
1. Integration test (AC-14): pair where `source.created_at = target.created_at` — assert no `Informs` row written.
2. Integration test (AC-14b): pair where `source.created_at > target.created_at` (reversed) — assert no `Informs` row written.
3. Integration test (AC-15): pair with identical `feature_cycle` strings — assert no `Informs` row written.
4. Integration test (AC-16): pair with category `("decision", "decision")` not in `informs_category_pairs` — assert no `Informs` row written.
5. Integration test (AC-17): pair with cosine `0.44` against default floor `0.45` — assert no `Informs` row written.
6. Unit test: `nli.neutral = 0.5` (not strictly greater) — assert no edge written (boundary: strictly `> 0.5`).
7. Positive integration test (AC-13): all guards pass simultaneously — assert exactly one `Informs` row written with correct fields.
8. Combined boundary test: one pair passes all guards, a second pair fails one guard — assert exactly one row written, not two.

**Coverage Requirement**: Each guard must have an independent negative test. No guard can be validated only through the AC-13 happy path.

---

### R-04: `NliCandidatePair` Origin Routing Cross-Contamination

**Severity**: High | **Likelihood**: Low
**Impact**: `Informs` pairs routed to Phase 8 (entailment threshold) — dropped because `neutral > 0.5` pairs rarely exceed `entailment > 0.6`. `SupportsContradict` pairs routed to Phase 8b — written as `Informs` edges when `neutral > 0.5` incidentally holds. Both silent failures. SR-08 addresses this via the discriminator struct; the risk is that the routing filter condition (`origin == ...`) is inverted or omitted.

**Test Scenarios**:
1. Unit test: construct a `Vec<NliCandidatePair>` with one `SupportsContradict` pair (high entailment, low neutral) and one `Informs` pair (high neutral, low entailment). Run Phase 8 logic only. Assert only a `Supports` edge is written — no `Informs` edge.
2. Unit test (mirror): run Phase 8b logic only on the same vec. Assert only an `Informs` edge is written — no `Supports` edge.
3. Cross-route test: an `Informs` pair with `entailment > supports_edge_threshold` — assert Phase 8 does NOT write it as `Supports` (origin guard takes priority over score).
4. Length-mismatch guard test: `nli_scores.len() != merged_pairs.len()` — assert the existing length-mismatch guard fires and no writes occur.

**Coverage Requirement**: Both cross-route failure modes (Informs→Supports write path, SupportsContradict→Informs write path) must be tested explicitly.

---

### R-05: Phase 4b → Phase 7 Metadata Survival

**Severity**: High | **Likelihood**: Med
**Impact**: `InformsCandidate` fields set to `None` in `NliCandidatePair` when constructed in Phase 4b. Phase 8b re-verification reads `None` values; if guard logic short-circuits on `None` as "pass", spurious edges are written. If it short-circuits as "fail", all Informs edges are silently suppressed.

**Test Scenarios**:
1. Unit test: construct `NliCandidatePair::Informs` with fully populated `InformsCandidate` metadata. Pass through a simulated Phase 7 (nli_scores populated by index). Verify Phase 8b reads correct field values — not defaults, not `None`-unwrap panics.
2. Panic-safety test: `source_created_at = None` in an `Informs` variant — assert Phase 8b does not panic and does not write an edge (graceful guard failure).
3. Data-flow integration test: run a full tick with a qualifying pair. Assert that the `Informs` edge `weight` in `GRAPH_EDGES` equals `cosine * nli_informs_ppr_weight` — not zero, not 1.0, not an uninitialized value. This verifies that `similarity` from Phase 4b survived to the Phase 8b write call.
4. Feature-cycle propagation test: run tick with a pair from different feature cycles. Assert the written edge has no null/empty source or target metadata that would indicate metadata loss.

**Coverage Requirement**: The `similarity` value and all three guard fields (`source_created_at`, `feature_cycle` values, category) must be verified to survive Phase 4b → Phase 8b without corruption.

---

### R-06: Cap Priority Sequencing Incorrect

**Severity**: High | **Likelihood**: Low
**Impact**: In a tick where `informs_candidate_count + supports_candidate_count > max_graph_inference_per_tick`, Informs candidates displace Supports candidates. Higher-precision Supports/Contradicts signal is dropped. This is the primary correctness property of ADR-002 (OQ-1 resolution).

**Test Scenarios**:
1. Unit test: `supports_pairs.len() = max_cap`, `informs_pairs.len() = 5`. After Phase 5, assert `merged_pairs.len() = max_cap` and `informs_pairs_accepted = 0`.
2. Unit test: `supports_pairs.len() = max_cap - 3`, `informs_pairs.len() = 10`. Assert `merged_pairs.len() = max_cap` and the last 3 elements have `origin = Informs`.
3. Unit test: `supports_pairs.len() = 0`, `informs_pairs.len() = max_cap + 5`. Assert `merged_pairs.len() = max_cap`, all with `origin = Informs`.
4. Combined invariant test: `merged_pairs.len() <= max_graph_inference_per_tick` always holds regardless of input sizes — fuzz with varied counts.
5. Log assertion test: when Informs candidates are dropped, the debug log records non-zero `informs_candidates_dropped`. When none are dropped, records `0`.

**Coverage Requirement**: The sequential reservation invariant (`merged <= cap` by construction) must be tested as a property, not just with one example.

---

### R-07: `NliScores.neutral` Reliability

**Severity**: Med | **Likelihood**: Med
**Impact**: If `neutral = 1 - entailment - contradiction` (residual), high-entailment or high-contradiction pairs can produce high neutral scores incidentally. The composite guard (neutral > 0.5 AND entailment < threshold AND contradiction < threshold — FR-11) partially mitigates this, but if FR-11's mutual exclusion check is not implemented, precision degrades silently with no test failure.

**Test Scenarios**:
1. Inspection test (OQ-S2 resolution): confirm `NliScores` field population path in `score_batch` — is `neutral` a third logit or computed as residual? Document in code comment.
2. FR-11 guard test: pair where `nli.neutral > 0.5` AND `nli.entailment > supports_edge_threshold` — assert no `Informs` edge written (Gap-2 exclusion). This is R-19 territory — see R-19 scenarios.
3. Boundary unit test: `neutral = 0.5000001` (just above threshold) — assert edge written. `neutral = 0.5` — assert edge not written (strict `>`).
4. High-contradiction rejection test: pair where `neutral > 0.5` but `contradiction > contradicts_edge_threshold` — assert no `Informs` edge (if FR-11 requires this exclusion; verify spec intent).

**Coverage Requirement**: OQ-S2 must be resolved before Phase C implementation. The neutral computation path must be documented in code; if it is a residual, the risk note must be recorded in the test plan for future threshold tuning.

---

### R-08: Category Filter Not Applied / Domain String Leakage

**Severity**: Med | **Likelihood**: Med
**Impact**: Phase 4b scans all entries rather than only LHS-category entries, causing O(N) HNSW scans where N = all active entries. Tick duration inflates proportionally. Alternatively, domain strings appear in `nli_detection_tick.rs`, violating AC-22 and C-12, causing future detection logic to be coupled to the software-engineering domain.

**Test Scenarios**:
1. CI grep gate (AC-22): `grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' nli_detection_tick.rs` returns empty after the change.
2. Unit test: configure `informs_category_pairs = [["lesson-learned", "decision"]]`. Inject two entries with categories `("convention", "decision")`. Assert Phase 4b produces zero `NliCandidatePair::Informs` elements for this source (category not in LHS set).
3. Unit test: empty `informs_category_pairs`. Assert Phase 4b produces zero candidates (detection disabled, no panic).
4. Latency regression test (NF-01): with `informs_category_pairs` configured to match zero source categories in a 1,000-entry graph, Phase 4b overhead must be < 5 ms.

**Coverage Requirement**: AC-22 CI grep is a non-negotiable gate check. The LHS filter must have a unit test with a non-matching category.

---

### R-09: Directional Dedup Accidentally Normalized

**Severity**: Med | **Likelihood**: Med
**Impact**: `query_existing_informs_pairs` applies `(min(a,b), max(a,b))` normalization (copied from `query_existing_supports_pairs`). The reverse pair `(target, source)` — which would fail temporal detection — is treated as already-written. Dedup suppresses valid new edges on ticks after the first. Silent write stall (entry #3675 shuffle/stall pattern applies here).

**Test Scenarios**:
1. Unit test (ADR-003): insert one row `(source_id=100, target_id=200, relation_type="Informs")`. Call `query_existing_informs_pairs`. Assert `HashSet` contains `(100, 200)` but NOT `(200, 100)`.
2. Unit test: insert same row. Check `set.contains(&(200, 100))` returns `false`. This verifies non-normalization — if normalization were present, this would return `true`.
3. Unit test: `bootstrap_only = 1` row — assert not returned.
4. Unit test: empty table — assert empty `HashSet` returned without error.
5. Integration test: run tick twice with one qualifying pair. After second tick, assert `GRAPH_EDGES` contains exactly one `Informs` row (pre-filter working). Verify dedup metric in log shows `1` skipped on second run.

**Coverage Requirement**: The non-normalization property must be verified by the reverse-lookup test (scenario 2). ADR-003 mandates this test explicitly.

---

### R-10: `graph_penalty` / `find_terminal_active` Traverse `Informs`

**Severity**: High | **Likelihood**: Low
**Impact**: An entry that Informs a decision also accumulates penalty mass. Penalty logic marks empirical knowledge as deprecated or terminal. High-value lessons silently disappear from active traversal.

**Test Scenarios**:
1. Unit test (AC-24): construct a graph with only a single `Informs` edge between two nodes. Call `graph_penalty` for the source node. Assert the return value equals `FALLBACK_PENALTY` (no contribution from `Informs` edge).
2. Unit test (AC-24b): same graph, call `find_terminal_active`. Assert result is empty.
3. Regression test: existing `graph_penalty` tests pass unchanged after adding `Informs` to `RelationType`.
4. Structural enforcement check: `graph_penalty` and `find_terminal_active` call `edges_of_type(_, RelationType::Supersedes, _)` only — verify by code inspection that no `RelationType::Informs` call is added.

**Coverage Requirement**: AC-24 test must explicitly use a graph with only `Informs` edges (no `Supersedes`), not a mixed graph where a passing test could mask traversal of `Informs`.

---

### R-11: Cap Accounting Math Off-by-One

**Severity**: Med | **Likelihood**: Med
**Impact**: `merged_pairs.len()` exceeds `max_graph_inference_per_tick` by up to `informs_accepted_count`. NLI batch sent to rayon exceeds budget. Tick duration inflates. W1-2 contract is technically satisfied (one spawn) but the NF-01 tick latency budget is violated.

**Test Scenarios**:
1. Invariant unit test: for all integer combinations of `supports_len ∈ [0..cap]` and `informs_len ∈ [0..cap*2]`, assert `min(supports_len, cap) + min(informs_len, max(0, cap - min(supports_len, cap))) <= cap`.
2. Edge case: `supports_len = cap`. Assert `informs_accepted = 0`, `merged.len() = cap`.
3. Edge case: `cap = 0`. Assert `merged.len() = 0` (no panic, no division by zero in remaining computation).
4. Integration test: run tick with `max_graph_inference_per_tick = 5` and 10 qualifying pairs of each type. Assert `GRAPH_EDGES` row count after tick does not exceed 5.

**Coverage Requirement**: The `merged.len() <= cap` invariant must be tested as a property with varied inputs, not just one nominal case.

---

### R-12: Silent Informs Starvation with No Log Signal

**Severity**: Med | **Likelihood**: Med
**Impact**: In a high-churn deployment, Supports fills the cap every tick. Informs edges are never written. An operator has no signal to distinguish "no qualifying Informs candidates exist" from "all Informs candidates are cap-dropped." Without observability, the feature appears to work but produces zero institutional memory bridges indefinitely.

**Test Scenarios**:
1. Log assertion test: configure `max_graph_inference_per_tick = N`. Inject exactly `N` qualifying Supports pairs and 5 qualifying Informs pairs. Run tick. Assert debug log contains `informs_candidates_dropped = 5`.
2. Log assertion test (inverse): inject 0 Supports pairs and 5 qualifying Informs pairs. Assert log contains `informs_candidates_accepted = 5`, `informs_candidates_dropped = 0`.
3. Zero-candidate log test: inject no pairs matching `informs_category_pairs`. Assert log still emits the cap-accounting line with `informs_candidates_total = 0` (not a missing log line).

**Coverage Requirement**: All three log fields (`accepted`, `total`, `dropped`) must be exercised in tests. SR-03 compliance is verified through log assertion, not just edge-write count.

---

### R-13: `select_source_candidates` Missing Category Metadata (OQ-S3)

**Severity**: Med | **Likelihood**: Med
**Impact**: Phase 4b must determine source category before HNSW scan. If `select_source_candidates` returns only IDs, Phase 4b requires a secondary `entry_meta` lookup per source. This lookup must be built from the `all_active` query already in Phase 2 — if it is instead a new per-entry DB read, tick latency inflates by O(N) DB calls, violating NF-01.

**Test Scenarios**:
1. OQ-S3 resolution test: confirm `select_source_candidates` return type before implementation. If category is not present, verify that `entry_meta: HashMap<u64, &EntryRecord>` is built from `all_active` (already fetched in Phase 2) — not from new DB calls.
2. Integration test: run tick with 500 active entries, 50 with qualifying LHS category. Assert tick completes within NF-01 budget (p95 <= baseline + 50 ms). Category lookup must use in-memory map, not DB query.
3. Correctness test: entry in `source_candidates` but absent from `entry_meta` (edge case: entry deleted between Phase 2 and Phase 4b) — assert graceful skip, no panic, no spurious Informs pair produced.

**Coverage Requirement**: OQ-S3 must be resolved at architecture-confirm time. The latency test (scenario 2) must be included in the NF-01 gate check.

---

### R-14: Rayon Closure Async Contamination

**Severity**: Med | **Likelihood**: Low
**Impact**: `tokio::runtime::Handle::current()` or `.await` inside the merged Phase 7 rayon closure. Violates C-14 / R-09 / NF-04. Causes deadlock or panic when the Tokio runtime is accessed from a non-Tokio thread. Intermittent in test environments with small entry sets (rayon may not spawn threads); deterministic under load.

**Test Scenarios**:
1. CI grep gate (AC-21 / NF-04): `grep -n 'Handle::current' nli_detection_tick.rs` returns empty. This is a mandatory CI check.
2. CI grep gate: `grep -n '\.await' nli_detection_tick.rs` filtered to the rayon closure body returns empty.
3. Integration test: run tick on a graph requiring Phase 7 NLI scoring. Assert completion without panic or deadlock (indirectly validates sync-only closure).

**Coverage Requirement**: The CI grep gate is the primary enforcement mechanism per AC-21. It must be a hard CI failure, not a warning.

---

### R-16: Zero-Regression on Existing Supports/Contradicts Detection

**Severity**: High | **Likelihood**: Low
**Impact**: Merging the candidate batch type from `Vec<(u64, u64, ...)>` to `Vec<NliCandidatePair>` changes the Phase 8 iteration. If the `SupportsContradict` filter in Phase 8 is misapplied, existing Supports edges stop being written. This silently regresses graph inference quality for the live system.

**Test Scenarios**:
1. Regression test suite: all existing `nli_detection_tick.rs` tests pass unchanged (NF-03). No test count decrease.
2. Specific regression: existing integration test that asserts a `Supports` edge is written for a qualifying pair — must still pass with the merged batch type.
3. `write_inferred_edges_with_cap` signature compatibility: if Phase 8 builds a `write_pairs` slice from `SupportsContradict` elements, verify the slice construction does not silently skip elements due to origin filter logic errors.
4. Side-by-side count test: run tick with only Supports-qualifying pairs (no Informs categories in graph). Assert Supports edge count is identical to pre-refactor baseline.

**Coverage Requirement**: NF-03 is a gate hard-stop. Zero regression on Supports detection is verified by running the existing test suite against the refactored batch type.

---

### R-17: Duplicate `Informs` Edge on Subsequent Ticks

**Severity**: Med | **Likelihood**: Low
**Impact**: Two `Informs` rows for the same `(source_id, target_id)` pair in `GRAPH_EDGES`. Corrupts PPR weight computation (double-counts edge weight). `INSERT OR IGNORE` is the backstop but `UNIQUE(source_id, target_id, relation_type)` must be confirmed on the index.

**Test Scenarios**:
1. Integration test (AC-23): run tick twice with one qualifying pair. Assert `SELECT COUNT(*) FROM graph_edges WHERE relation_type = "Informs"` returns exactly `1`.
2. Pre-filter coverage test: on the second tick, assert `query_existing_informs_pairs` returns the previously-written pair — verifying the pre-filter is loaded and effective.
3. Schema verification: confirm `UNIQUE(source_id, target_id, relation_type)` index exists on `GRAPH_EDGES` before Phase C begins.

**Coverage Requirement**: AC-23 integration test is mandatory. Schema index verification is a delivery gate prerequisite.

---

### R-18: Config Validation Boundary Errors

**Severity**: Low | **Likelihood**: Low
**Impact**: `nli_informs_cosine_floor = 0.0` or `= 1.0` passes validation. Floor of 0.0 means all pairs enter Phase 4b (massive fan-out). Floor of 1.0 means no pairs ever qualify. Both silently produce unexpected behavior.

**Test Scenarios**:
1. Unit test (AC-10): `validate()` with `nli_informs_cosine_floor = 0.0` → error.
2. Unit test (AC-10): `validate()` with `nli_informs_cosine_floor = 1.0` → error.
3. Unit test (AC-10): `validate()` with `nli_informs_cosine_floor = 0.45` → ok.
4. Unit test (AC-11): `validate()` with `nli_informs_ppr_weight = 0.0` → ok (inclusive). `= -0.01` → error. `= 1.0` → ok. `= 1.01` → error.

**Coverage Requirement**: Boundary values (exactly 0.0, exactly 1.0) must be tested explicitly for both fields. f32 floating-point comparison is exact here — use literal values, not computed ones.

---

### R-19: Gap-2 Mutual Exclusion — Same Pair Written as Both `Supports` and `Informs`

**Severity**: Med | **Likelihood**: Low
**Impact**: A pair with `entailment > 0.6` AND `neutral > 0.5` receives both a `Supports` and an `Informs` edge. `GRAPH_EDGES` has two rows for the same `(source, target)` pair with different `relation_type` values. PPR accumulates mass from both — inflated score for entries that happen to be in this overlap zone.

The UNIQUE index is on `(source_id, target_id, relation_type)` — two rows with different `relation_type` values are not duplicates by the index definition and both persist.

**Test Scenarios**:
1. Integration test (FR-11 Gap-2 guard): pair where `entailment > supports_edge_threshold` AND `neutral > 0.5`. Assert only a `Supports` edge is written — no `Informs` edge.
2. Phase 8b guard unit test: `NliScores { entailment: 0.7, neutral: 0.6, contradiction: 0.1 }` with `supports_edge_threshold = 0.6`. Assert Phase 8b composite guard rejects this pair (entailment exclusion).
3. Negative test: confirm FR-11 implementation includes the entailment exclusion check — not just the neutral threshold. Code inspection plus unit test with borderline values.

**Coverage Requirement**: FR-11 entailment exclusion must have an explicit test. Gap-2 cannot be validated only through the AC-13 happy-path scenario where all scores are well-separated.

---

### R-20: Missing Mandatory Tick Integration Tests at Gate

**Severity**: High | **Likelihood**: Med
**Impact**: AC-13 through AC-23 cover Phase 4b/8b detection path. If the implementation wave treats these as "can add later," gate submission will be REWORKABLE FAIL. Entry #3579 documents this exact pattern from nan-009: production code correct, zero Phase-specific tests, gate required creating two test modules from scratch.

**Test Scenarios** (delivery process — not runtime scenarios):
1. Gate check: AC-13 integration test present and passing (positive Informs write).
2. Gate check: AC-14, AC-15, AC-16, AC-17 integration tests each present (four negative guard tests).
3. Gate check: AC-23 dedup integration test present (two-tick run).
4. Gate check: AC-21, AC-22 CI grep gates configured (not just documented).
5. Gate check: AC-05 PPR mass propagation test asserts *lesson node specifically* — not any non-zero score.

**Coverage Requirement**: All 11 tick integration tests (AC-13 through AC-23) must be delivered in the same implementation wave as Phase 4b/8b code. These are not post-gate optional additions.

---

## Integration Risks

### Three-Crate Change Surface

| Boundary | Risk | Test |
|----------|------|------|
| `engine → server`: `RelationType::Informs.as_str()` must equal `"Informs"` string used in `write_nli_edge` call | String mismatch causes R-10 guard to fire silently (AC-03/AC-04 cover this) | Integration test: write row with `"Informs"`, build graph, assert edge present with no warn |
| `server → store`: `query_existing_informs_pairs` called in Phase 2 must return directional set (ADR-003) | Wrong normalization causes silent stall after first tick | Unit test: reverse lookup returns false (R-09 scenario 2) |
| `server (Phase 4b metadata) → server (Phase 8b guard)`: `NliCandidatePair::Informs` fields must be populated at construction and readable at write time | `None` fields cause vacuous guard pass or silent skip | Data-flow integration test verifying weight = `cosine * ppr_weight` (R-05 scenario 3) |
| `config.rs → nli_detection_tick.rs`: `informs_category_pairs` passed as runtime value, never hardcoded | Domain string leakage into tick logic (AC-22 CI grep gate) | grep gate + unit test with non-default category pair (R-08 scenario 2) |
| `graph_ppr.rs`: fourth `edges_of_type` call uses same direction as other three | Direction mismatch causes zero mass flow (R-02) | PPR unit test asserting specific lesson node score (AC-05) |

### Phase 4b → Phase 7 Batch Merge Flow

The metadata path is the highest-integration risk. The `InformsCandidate` record is constructed in Phase 4b, wrapped in `NliCandidatePair::Informs`, merged with `SupportsContradict` pairs in Phase 5, passed to Phase 6 (text fetch), routed through Phase 7 (rayon score_batch), and consumed in Phase 8b. Any step in this chain that drops or corrupts metadata causes a silent failure — no panic, no compile error, just wrong behavior.

Specific failure modes to test:
- Phase 6 text fetch fails for an Informs pair source/target — assert graceful skip, not panic
- Phase 7 produces `nli_scores.len() != merged_pairs.len()` — assert existing length-mismatch guard fires for both `SupportsContradict` and `Informs` elements in merged vec
- Phase 8b reads index `i` into `nli_scores` after Phase 8 has iterated the same vec — assert read is by absolute index, not by a counter reset between phases

---

## Edge Cases

| Edge Case | Risk | Scenario |
|-----------|------|----------|
| All active entries have the same `feature_cycle` | Cross-feature guard eliminates all Informs candidates — zero edges, no stall | Verify tick completes normally, log shows `informs_candidates_total = 0` |
| Single entry in the graph | Phase 4b HNSW search returns self as neighbor — self-skip guard must fire | Verify no `(X, X)` Informs edge written |
| `created_at` ties (two entries at same Unix second) | Temporal guard uses strict `<`, equal timestamps excluded | AC-14 boundary test with `source.created_at = target.created_at` |
| `feature_cycle = None` on one or both entries | Cross-feature guard must exclude null-cycle pairs (per FR-09) | Integration test: one entry with `feature_cycle = null` — no Informs edge |
| `informs_category_pairs` is empty (operator override) | Phase 4b produces zero candidates — no panic | Unit test: empty pairs config, run tick, assert no Informs edges, no error |
| `max_graph_inference_per_tick = 0` | `remaining_capacity = 0` immediately — no candidates enter batch | Verify no divide-by-zero, no panic, empty merged vec |
| `nli_informs_ppr_weight = 0.0` | All Informs edges written with `weight = 0.0` | Verify write proceeds (zero weight is valid); PPR gives zero contribution (acceptable) |
| HNSW returns neighbor with missing embedding vector | Phase 4b get_embedding returns `None` — skip guard must fire | Verify no panic, no Informs pair produced for missing-embedding neighbor |
| Informs pair qualifies but `write_nli_edge` returns error | Error must not abort tick — existing infallible-tick pattern must hold | Verify other edges still written after one write error |

---

## Security Risks

**Untrusted input surface for crt-037:** The new config fields (`informs_category_pairs`, `nli_informs_cosine_floor`, `nli_informs_ppr_weight`) are read from operator-controlled TOML files. The category pair strings are passed from config into Phase 4b detection logic as runtime values.

| Component | Untrusted Input | Damage Potential | Blast Radius |
|-----------|----------------|-----------------|--------------|
| `InferenceConfig::informs_category_pairs` | Operator-supplied category strings | Strings passed as `HashSet<&str>` keys for O(1) lookup — no SQL interpolation, no injection path. Long strings inflate memory marginally. | Config only; no user data path |
| `nli_informs_cosine_floor` | Operator-supplied f32 | Validated by `validate()` range check before use; out-of-range values rejected at startup. No arithmetic on user-supplied input in hot path. | Config only |
| `GRAPH_EDGES.relation_type` | Written by internal logic only (`"Informs"` string literal) | Not a user input; not exposed via MCP tool input. No injection path. | Internal only |
| `write_nli_edge` call site | `source_id`, `target_id` from HNSW index (internal) | Not user-supplied; internal entry IDs from `all_active` query. No external origin. | Internal only |
| `NliCandidatePair` construction | Category strings from `entry_meta` (from DB) | Existing entries' category strings are stored values, not live user input. No interpolation into SQL; used only for `HashSet` membership check. | Stored data, not live injection surface |

**Assessment:** crt-037 introduces no new external input surface. The risk is operator misconfiguration (e.g., extremely low cosine floor causes massive HNSW fan-out), which is mitigated by `validate()` range checks and the Phase 5 cap truncation. No SQL injection, path traversal, or deserialization risks are introduced.

---

## Failure Modes

| Failure | Expected System Behavior | Test |
|---------|-------------------------|------|
| All Phase 4b candidates cap-dropped (Supports fills budget) | Tick completes normally; zero Informs edges written this tick; debug log records `informs_candidates_dropped = N` | R-12 scenario 1 |
| No entries match `informs_category_pairs` LHS | Phase 4b produces empty candidate vec; tick proceeds normally with Supports-only batch | R-08 scenario 3 |
| `query_existing_informs_pairs` DB error | Tick must fail gracefully with an error log, not panic. Existing Supports path error handling pattern applies. | Error injection test |
| `write_nli_edge` returns error for one Informs pair | Skip this pair, continue writing remaining pairs; tick does not abort | R-05 scenario 4 |
| Length-mismatch between `nli_scores` and `merged_pairs` | Existing length-mismatch guard fires; no writes occur; error logged | R-04 scenario 4 |
| `nli_informs_cosine_floor` out of range at startup | `validate()` returns error; server fails to start with clear config error message (not a runtime panic) | R-18 scenario 1 |
| `Informs` edge string does not match `RelationType::from_str` | R-10 guard fires in `build_typed_relation_graph`; edge silently excluded from PPR; `warn` log emitted (AC-04 detects this) | R-01 scenario 2 |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (neutral band unreliable) | R-03, R-07, R-19 | Composite guard (5 predicates) via `NliCandidatePair` struct; FR-11 entailment exclusion; fixed neutral floor 0.5 |
| SR-02 (second HNSW scan tick inflation) | R-08, R-11, R-13 | Phase 5 sequential cap bounds total batch; Phase 4b LHS filter limits HNSW calls; NF-01 latency gate required |
| SR-03 (silent Informs starvation) | R-12 | Debug log per tick: `informs_candidates_accepted/dropped/total`; R-12 log assertion tests |
| SR-04 (default pair list scope creep) | — | Frozen at four in `InferenceConfig` defaults; expansion explicitly deferred (C-10) |
| SR-05 (penalty invariant future breakage) | R-10 | `edges_of_type(Supersedes)` enforced structurally; AC-24 unit test with Informs-only graph; penalty invariant documented in `graph.rs` module header |
| SR-06 (crt-036 logistical dependency) | — | Delivery gate check: confirm crt-036 on main before Phase C begins (C-15) |
| SR-07 (PPR direction contract for Informs) | R-02 | AC-05 asserts lesson node specifically receives mass; entry #3754 lesson incorporated: direction test must be behavioral, not enum-value inspection |
| SR-08 (discriminator tag routing) | R-04, R-05 | `NliCandidatePair` typed union with `PairOrigin` enum; `origin == Informs` filter in Phase 8b; `origin == SupportsContradict` filter in Phase 8; compile-time exhaustive matching (ADR-001) |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-03, R-20, R-02) | R-01: DDL inspection + 2 runtime tests; R-02: 4 unit tests + 1 CI grep; R-03: 8 negative+positive tests; R-20: 5 gate checks |
| High | 8 (R-04, R-05, R-06, R-07, R-08, R-09, R-10, R-16) | Minimum 3 tests each; R-08 and R-10 include mandatory CI grep gates |
| Medium | 6 (R-11, R-12, R-13, R-14, R-17, R-19) | Minimum 2 tests each; R-14 includes CI grep gate |
| Low | 2 (R-15, R-18) | Minimum 1 boundary test each |

**Delivery gate structure (per OQ-5 resolution):**
- Functional correctness: AC-13–AC-24 integration and unit tests all passing
- Zero regression: NF-03 — existing test count does not decrease, all existing tests pass
- CI grep gates: AC-21 (`Handle::current`), AC-22 (domain strings), AC-24 direction checks — hard CI failures
- Post-delivery tracking (not gate): ICD delta at first tick, ~3-tick accumulation

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection NLI graph inference` — found entry #2800 (cap logic must be testable as extracted function; informed R-20 and R-06 scenarios) and entry #3579 (gate-3b missing-tests pattern; directly informs R-20 severity elevation to Critical).
- Queried: `/uni-knowledge-search` for `risk pattern graph edge type detection tick rayon batch` — found entry #3675 (tick source-candidate bound/shuffle/embedding-filter pattern; informed R-13 secondary-lookup risk and edge-case table).
- Queried: `/uni-knowledge-search` for `PPR direction semantics reverse walk outgoing edges traversal` — found entry #3754 (direction semantics lesson; elevated R-02 likelihood to Med and hardened AC-05 scenario to require specific lesson-node assertion); found entry #3744 (PPR Direction::Outgoing pattern; confirmed correct contract).
- Queried: `/uni-knowledge-search` for `SQLite relation_type CHECK constraint DDL schema` — no directly applicable constraint-risk entry found; risk R-01 sourced from OQ-S1 and SCOPE.md assumption.
- Stored: nothing novel to store — R-20 pattern (cap logic testability) is already entry #2800; direction semantics lesson is already entry #3754; merged-batch discriminator struct pattern is specific to crt-037.
