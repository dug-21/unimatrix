# Risk-Based Test Strategy: col-030

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `graph_tests.rs` is 1068 lines — adding 6+ new unit tests will further violate the 500-line limit, triggering gate-3b rejection (entry #3580) | High | High | Critical |
| R-02 | `graph_suppression.rs` visibility: items declared `pub(super)` instead of `pub` will silently prevent the `pub use` re-export in `graph.rs` from compiling (entry #3602) | High | Med | High |
| R-03 | `final_scores` is a `let` (immutable) at line 893 — implementation agent introduces `let mut final_scores` at line 893 instead of shadowing at Step 10b, or forgets the shadow entirely, causing a compile error or silently operating on the pre-suppression Vec | High | Med | High |
| R-04 | `edges_of_type` has never been called with `RelationType::Contradicts` — if the string comparison in `edges_of_type` uses a wrong case or the stored edge string differs from `RelationType::Contradicts.as_str()`, suppression silently returns all-true mask | High | Low | High |
| R-05 | Bidirectional query omission: implementation agent calls only `Direction::Outgoing`, missing Incoming edges — suppression works for half of all Contradicts edge orientations, with no test or compile error to catch it | High | Med | High |
| R-06 | `keep_mask[i]` index out of bounds if `suppress_contradicts` returns a mask shorter than `results_with_scores.len()` (e.g., off-by-one in mask construction) — panics in production | High | Low | High |
| R-07 | Step 10b inserted after Step 10 `retain` calls but `final_scores` is not `retain`-filtered — aligned-prefix assumption is correct, but implementation agent uses `final_scores.len()` instead of `results_with_scores.len()` as `aligned_len`, producing silent misalignment | High | Med | High |
| R-08 | `graph_suppression.rs` not wired into `graph.rs` via `mod graph_suppression; pub use graph_suppression::suppress_contradicts;` — the file exists but `suppress_contradicts` is unreachable from `search.rs`, causing a compile error | Med | Med | High |
| R-09 | `lib.rs` incorrectly receives a new `pub mod graph_suppression;` entry — contradicts ADR-001 ("lib.rs does NOT need a new top-level pub mod entry") — exposes the submodule as a separate public crate path alongside the re-export | Med | Med | Med |
| R-10 | DEBUG log (FR-09) emits only the suppressed entry ID but not the contradicting entry ID — NFR-05 requires both; this is a silent partial compliance that passes code review unless explicitly verified | Med | Med | Med |
| R-11 | Cold-start guard (`if !use_fallback`) omitted or inverted — suppression runs on the empty cold-start graph, which is a natural no-op but omits the documented intent guard; no test catches the missing guard | Med | Med | Med |
| R-12 | Integration test uses `create_graph_edges_table` helper (pre-v13 schema) instead of `build_typed_relation_graph` with in-memory fixtures — test fails against production schema on first run (SR-07, entry #3600) | Med | Med | Med |
| R-13 | Zero-regression eval gate passes even if suppression is entirely broken (no-op for all existing scenarios) — delivery agent treats gate passage as proof of correctness and skips the mandatory positive integration test (SR-05) | Med | High | Med |

---

## Risk-to-Scenario Mapping

### R-01: graph_tests.rs line-count violation
**Severity**: High
**Likelihood**: High
**Impact**: Gate-3b rejection, rework wave required to split test modules before the feature can merge. Historically (entry #3580) this was the primary source of rework in nan-009.

**Test Scenarios**:
1. Before writing any tests, run `wc -l crates/unimatrix-engine/src/graph_tests.rs` — if current count (1068) plus estimated new tests (~80 lines for 6 cases) will exceed 500, a module split must happen first.
2. Gate-3b validator checks `wc -l` on all modified files; gate must fail if any file exceeds 500 lines.

**Coverage Requirement**: Implementation brief must call out the graph_tests.rs line count as a split-candidate file. New `suppress_contradicts` tests must live in a separate `graph_suppression_tests.rs` (or inline in `graph_suppression.rs` under `#[cfg(test)]`), not appended to the existing 1068-line file.

---

### R-02: graph_suppression.rs visibility
**Severity**: High
**Likelihood**: Med
**Impact**: Compile error `E0364/E0365 private, cannot be re-exported` — caught at build time, but causes a rework cycle if not anticipated.

**Test Scenarios**:
1. Confirm `suppress_contradicts` is declared `pub fn`, not `pub(super) fn` or `fn` in `graph_suppression.rs`.
2. Confirm `graph.rs` contains `mod graph_suppression; pub use graph_suppression::suppress_contradicts;`.
3. Confirm `suppress_contradicts` is importable as `unimatrix_engine::graph::suppress_contradicts` in a `search.rs` test.

**Coverage Requirement**: Compile-time verification — any integration test that calls `suppress_contradicts` via the re-export path implicitly validates this.

---

### R-03: final_scores immutable binding
**Severity**: High
**Likelihood**: Med
**Impact**: If the implementation agent adds `let mut` to line 893 instead of using a shadow at Step 10b, the intent of ADR-004 is violated and the existing comment at line 909 ("zip stops at shorter") may become incorrect. If shadowing is forgotten entirely, `final_scores` post-suppression retains the pre-suppression entries — Step 11 zip silently includes scores for suppressed entries.

**Test Scenarios**:
1. Positive integration test (FR-14): verify result count equals `expected_k - suppressed_count` — if the final_scores shadow is missing, `zip` silently pairs wrong scores but count may still look correct.
2. Code review: confirm `final_scores` at line 893 is `let` (not `let mut`), and the Step 10b assignment uses `let final_scores = new_fs;` shadow.
3. Unit test asserting `ScoredEntry.final_score` values are correct after suppression (not scores of the suppressed entries).

**Coverage Requirement**: At least one test must assert both the identity of retained entries AND their `final_score` values, not just their presence.

---

### R-04: edges_of_type correctness for Contradicts
**Severity**: High
**Likelihood**: Low
**Impact**: `edges_of_type` filters by `e.weight().relation_type == type_str` where `type_str = relation_type.as_str()`. If the stored edge string in a test fixture uses a different capitalization (e.g., `"contradicts"` vs `"Contradicts"`), the filter returns an empty iterator — suppression is silently a no-op and the positive test fails. Conversely, if NLI ever writes a differently-cased string, production suppression silently breaks.

**Test Scenarios**:
1. Unit test: hand-construct a `TypedRelationGraph` with a `Contradicts` edge (string exactly `"Contradicts"`), call `suppress_contradicts`, assert the lower-ranked entry is suppressed.
2. Unit test: hand-construct with a `"contradicts"` (wrong case) edge and assert all entries are kept — confirming the filter is case-sensitive and no false positives.
3. Integration test: seed edge via `build_typed_relation_graph` using a `GraphEdgeRow` with `relation_type: "Contradicts".to_string()` — same string as `RelationType::Contradicts.as_str()`.

**Coverage Requirement**: At least one test must construct the edge using `RelationType::Contradicts.as_str()` explicitly to confirm string identity.

---

### R-05: Bidirectional Contradicts query omission
**Severity**: High
**Likelihood**: Med
**Impact**: If only `Direction::Outgoing` is queried, a contradiction detected when entry B (lower-ranked) was stored first — producing edge `B → A` — will be missed. Entry A ranks higher, but the edge is Outgoing from B. The collision passes through unsuppressed. No compile error; only the Incoming unit test catches this.

**Test Scenarios**:
1. Unit test (FR-13, AC-03): Contradicts edge written as rank-1 → rank-0 (Incoming to rank-0's perspective) — assert rank-1 is suppressed.
2. This is the single most important unit test: the Outgoing case can pass with a partial implementation; only the Incoming case reveals missing bidirectional handling.

**Coverage Requirement**: The Incoming direction unit test (AC-03 case) is non-negotiable. It must be present and passing before gate-3b.

---

### R-06: Mask length mismatch panic
**Severity**: High
**Likelihood**: Low
**Impact**: If `suppress_contradicts` returns a `Vec<bool>` shorter than `result_ids.len()` (off-by-one in initialization), the `keep_mask[i]` access in the Step 10b loop panics. In production this surfaces as a search request failure.

**Test Scenarios**:
1. AC-01: assert `suppress_contradicts(result_ids, graph).len() == result_ids.len()` for all test cases including the empty-graph and zero-result cases.
2. Edge case: `result_ids` is empty — assert an empty `Vec<bool>` is returned without panic.
3. Edge case: single-entry result set — assert a one-element `Vec<bool>` is returned.

**Coverage Requirement**: Every unit test must implicitly check mask length by destructuring the return value. An explicit length assertion on the empty-graph case is required.

---

### R-07: aligned_len computed from wrong Vec
**Severity**: High
**Likelihood**: Med
**Impact**: If the implementation uses `final_scores.len()` instead of `results_with_scores.len()` as `aligned_len`, the `final_scores[..aligned_len]` slice includes floor-removed entries. The zip then pairs surviving `results_with_scores` entries with wrong `final_scores` indices — silently wrong scores in output.

**Test Scenarios**:
1. Integration test with both a similarity_floor and a Contradicts edge: some entries are removed by the floor, some by suppression — assert surviving entry count and assert `ScoredEntry.final_score` values match expected (not the suppressed or floor-removed entries' scores).
2. Code review: confirm `aligned_len = results_with_scores.len()` (not `final_scores.len()`).

**Coverage Requirement**: A test exercising both Step 10 floors AND Step 10b suppression in the same search call is required to catch this class of bug.

---

### R-08: graph_suppression.rs not wired into graph.rs
**Severity**: Med
**Likelihood**: Med
**Impact**: The file `graph_suppression.rs` exists in the filesystem but `graph.rs` does not declare `mod graph_suppression;` — Rust ignores the file entirely. `suppress_contradicts` is unreachable; `search.rs` import fails at compile time. Caught immediately but avoidable.

**Test Scenarios**:
1. Any `cargo build` that touches `search.rs` will catch the missing `mod` declaration.
2. Code review checklist: confirm `graph.rs` contains both `mod graph_suppression;` and `pub use graph_suppression::suppress_contradicts;`.

**Coverage Requirement**: Compile-time — no dedicated test needed, but the implementation brief must include this as an explicit step with exact lines.

---

### R-09: lib.rs polluted with graph_suppression entry
**Severity**: Med
**Likelihood**: Med
**Impact**: Adding `pub mod graph_suppression;` to `lib.rs` exposes `unimatrix_engine::graph_suppression::suppress_contradicts` as a second public path alongside `unimatrix_engine::graph::suppress_contradicts`. Future callers may import the wrong path; PPR (#398) may depend on the wrong import and need fixup.

**Test Scenarios**:
1. Code review: confirm `lib.rs` has no new entries after this feature.
2. Confirm `suppress_contradicts` is only importable via `unimatrix_engine::graph::suppress_contradicts`.

**Coverage Requirement**: Code review only — no runtime test needed.

---

### R-10: DEBUG log missing contradicting_id
**Severity**: Med
**Likelihood**: Med
**Impact**: NFR-05 requires both the suppressed entry ID and the contradicting (retaining) entry ID in the log line. If only the suppressed ID is logged, operators cannot identify which pair triggered suppression — defeating the minimum audit trail before #412 ships.

**Test Scenarios**:
1. Code review: confirm `debug!` call contains both `suppressed_entry_id` and `contradicting_entry_id` fields (or equivalent).
2. Manual test: run a search with a known Contradicts pair under `RUST_LOG=debug` and verify both IDs appear in output.

**Coverage Requirement**: Code review is the primary gate. ADR-005 leaves the exact signature to pseudocode but specifies "at least the suppressed entry ID must appear" — this is a floor, not a ceiling. FR-09 requires both IDs.

---

### R-11: Cold-start guard missing or inverted
**Severity**: Med
**Likelihood**: Med
**Impact**: Without the guard, suppression runs on the empty cold-start graph — a natural no-op, but the intent is undocumented. More critically, an inverted guard (`if use_fallback`) skips suppression when the graph IS built and runs it only on cold-start, making suppression permanently ineffective in production.

**Test Scenarios**:
1. AC-05 verification: confirm `use_fallback = true` path returns all results unchanged — existing cold-start tests in search.rs must continue to pass.
2. Code review: confirm `if !use_fallback { ... suppress_contradicts call ... }` is the guard form.

**Coverage Requirement**: Existing cold-start tests serve as the regression check. No new test required, but the implementation brief must flag the inversion risk.

---

### R-12: create_graph_edges_table helper in integration test
**Severity**: Med
**Likelihood**: Med
**Impact**: `create_graph_edges_table` reflects pre-v13 schema — missing columns added in schema v13+. Integration test using it will fail with a schema mismatch error on the `GRAPH_EDGES` table, not with a meaningful assertion failure. Causes test infrastructure confusion.

**Test Scenarios**:
1. FR-14 / FR-15: integration test must seed Contradicts edges via `build_typed_relation_graph` with a hand-constructed `GraphEdgeRow` slice, not via `create_graph_edges_table`.
2. Code review: grep for `create_graph_edges_table` in any new test code — must return no matches.

**Coverage Requirement**: FR-15 is already a spec constraint. This risk is a delivery-agent discipline failure; the implementation brief must cite FR-15 explicitly.

---

### R-13: eval gate passage mistaken for suppression correctness proof
**Severity**: Med
**Likelihood**: High
**Impact**: The zero-regression gate passes even if `suppress_contradicts` returns all-true unconditionally (or is never called). A delivery agent may treat gate passage as sufficient and skip FR-14's mandatory positive integration test. The feature ships with broken suppression logic that is invisible until a Contradicts edge exists in a production search result.

**Test Scenarios**:
1. The positive integration test (FR-14, AC-07) is the only gate that validates suppression correctness. It is a mandatory, non-substitutable gate per SR-05.
2. Gate-3b validator must explicitly check for the presence of the FR-14 integration test (test name or assertion pattern) — not just eval gate passage.

**Coverage Requirement**: The implementation brief and gate-3b checklist must list FR-14 as a separate mandatory gate distinct from the eval gate.

---

## Integration Risks

**Step 10b insertion stability**: Step 10b is inserted between two existing steps whose data structures are well-defined at that point (`results_with_scores` floor-filtered, `final_scores` unfiltered). The risk is that a future feature (e.g., PPR #398) inserts a new step between floors and ScoredEntry construction and assumes the Vecs have their pre-col-030 semantics. The Step 10b comment block must be explicit about what it consumes and produces so PPR's architect can reason about ordering.

**`typed_graph` clone scope**: The `typed_graph` clone at Step 6 (lines 611–622) is already in scope at Step 10b — no new lock acquisition is needed. The risk is an implementation agent who adds a second read-lock acquisition at Step 10b (redundant but not wrong) or who attempts to access `self.typed_graph_handle` directly instead of using the already-cloned `typed_graph`. Either would compile, but the redundant lock acquisition wastes time on the hot path.

**`build_typed_relation_graph` edge filtering**: Pass 2b in `build_typed_relation_graph` skips `bootstrap_only=true` edges structurally. A test that seeds edges with `bootstrap_only=true` will produce a TypedRelationGraph with no Contradicts edges — suppression will be a no-op and the test will give a false pass. Integration tests must use `bootstrap_only=false` (or the default, if that is `false`).

---

## Edge Cases

**Empty result set**: `suppress_contradicts(&[], graph)` must return `vec![]` without panic. Step 10b with zero results must be a no-op.

**Single-entry result set**: No pairs possible; mask is `[true]`. No suppression regardless of graph edges.

**All entries suppressed**: If rank-0 contradicts rank-1 and rank-0 also contradicts rank-2, the result is `[true, false, false]`. The result set has 1 entry. This is valid behavior — no minimum result count is guaranteed.

**Contradicts chain not transitive**: rank-0 contradicts rank-2; rank-2 contradicts rank-3. But rank-0 does NOT contradict rank-3. Correct output: rank-2 suppressed by rank-0; rank-3 suppressed by rank-2 (already suppressed at the time rank-3 is checked). This requires that the sweep processes in rank order and skips already-suppressed entries when collecting neighbors — if suppressed entries are still used to trigger downstream suppression, the behavior is wrong.

**Non-Contradicts edge types**: CoAccess edges (bidirectional A↔B) are common in the graph. The suppression function must not confuse them with Contradicts edges. This is guarded by `edges_of_type(RelationType::Contradicts, ...)`, but the unit test for non-Contradicts edges (FR-13 last case) is the verification gate.

**Entry present in graph but absent from result set**: The function only suppresses entries present in `result_ids`. An entry with Contradicts edges that did not make the top-k result set is irrelevant — this is correct behavior and requires no special handling.

**Entry not present in TypedRelationGraph node_index**: If `result_ids` contains an ID that has no corresponding node in the graph (entry stored after the last tick rebuild), `node_index.get(&id)` returns `None`. The function must skip the entry rather than panic. This is a valid production scenario during the window between entry storage and the next graph tick.

---

## Security Risks

**Untrusted input surface**: `suppress_contradicts` takes `result_ids: &[u64]` derived from `results_with_scores`, which is itself derived from the HNSW vector index query. Entry IDs are `u64` — no string injection, no path traversal. The function is pure with no I/O. No meaningful attack surface.

**TypedRelationGraph poisoning**: `Contradicts` edges are written by the NLI path. A sufficiently crafted store payload could produce a false-positive NLI score, injecting a spurious `Contradicts` edge and causing legitimate results to be suppressed. This is not a new attack surface introduced by col-030 — the NLI write path already existed. The blast radius is: one result dropped from one search response per poisoned edge. The DEBUG log (FR-09) is the minimum observable trace.

**Denial of result via edge flood**: If an adversary can write many NLI-triggered `Contradicts` edges from a high-ranked entry, all lower-ranked results could be suppressed, returning a 1-entry result for every search. This requires the ability to write many entries that pass the NLI contradiction threshold against a target entry. No new capability is introduced by col-030; the NLI threshold is the only mitigation and is out of scope for this feature.

---

## Failure Modes

**Graph not yet built (cold-start)**: `use_fallback = true` → suppression skipped → full result set returned. Documented, expected, and required per AC-05. No user-visible error.

**suppress_contradicts returns wrong-length mask**: Panics at `keep_mask[i]` in the mask application loop. Surfaces as a `SearchError` or unhandled panic in the search service. Mitigation: AC-01 unit test catches length invariant before delivery.

**NLI false-positive Contradicts edge**: A legitimate result is silently suppressed. Observable only via DEBUG log (FR-09) until #412 ships. No runtime error — by design (ADR-005).

**Tick rebuild race (mitigated)**: `use_fallback` and `typed_graph` are cloned atomically under the read lock at Step 6. No torn-read is possible. SR-08 is resolved.

**`graph_suppression.rs` import fails**: Compile error caught immediately — not a runtime failure mode.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: `edges_of_type` unverified for Contradicts | R-04 | ADR-002 mandates `edges_of_type` as sole boundary. Unit tests for both Outgoing and Incoming directions on a hand-constructed Contradicts graph verify the method for this edge type for the first time. |
| SR-02: Parallel Vec mask application silent misalignment | R-03, R-07 | ADR-004 mandates single indexed zip-and-rebuild pass. `aligned_len = results_with_scores.len()` is the explicit contract. R-03 covers the `final_scores` binding risk; R-07 covers the wrong-length slice risk. |
| SR-03: graph.rs 500-line budget | — | Resolved by architecture: `graph_suppression.rs` split mandated by ADR-001. graph.rs stays at ~587 lines. |
| SR-04: No operator escape hatch | R-10 | Addressed by FR-09/NFR-05: DEBUG log with both suppressed and contradicting entry IDs. R-10 flags the risk that only one ID is logged. |
| SR-05: Zero-regression gate does not validate suppression correctness | R-13 | Architecture mandates FR-14 mandatory positive integration test as a separate non-substitutable gate. R-13 flags the delivery-agent discipline risk. |
| SR-06: File placement unresolved at scope time | — | Resolved by ADR-001 before implementation begins. |
| SR-07: create_graph_edges_table pre-v13 schema helper | R-12 | Architecture mandates FR-15: integration tests use `build_typed_relation_graph` with in-memory fixtures only. R-12 is the delivery-agent risk. |
| SR-08: use_fallback atomicity | — | Resolved: `use_fallback` and `typed_graph` cloned under same read lock at Step 6. No partial-graph race window. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | graph_tests.rs split before new tests; gate-3b file-size check |
| High | 7 (R-02 through R-08) | Compile verification, bidirectional unit test, length invariant test, floor+suppression combo test |
| Medium | 5 (R-09 through R-13) | Code review gates, FR-14 positive integration test, lib.rs check |

## Knowledge Stewardship
- Queried: /uni-knowledge-search for `lesson-learned failures gate rejection` — found entries #3579 (test omission at gate-3b), #3580 (file-size violation at gate-3b), both directly applicable
- Queried: /uni-knowledge-search for `parallel Vec alignment silent misalignment` — found entry #3616 (col-030 pattern, confirms SR-02 risk)
- Queried: /uni-knowledge-search for `module re-export pub use sibling wiring` — found entry #3602 (Rust submodule visibility trap, directly informs R-02)
- Stored: nothing novel to store — R-01 (graph_tests.rs line count as a split-candidate) is col-030-specific; the general file-size pattern is already in entry #3580
