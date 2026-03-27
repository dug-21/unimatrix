# col-030: Test Plan Overview — Contradicts Collision Suppression

## Overall Test Strategy

col-030 delivers a pure post-scoring filter. The implementation has two distinct surfaces that
require distinct test approaches:

1. **`suppress_contradicts` (graph_suppression.rs)** — pure function, no I/O, no async.
   Fully unit-testable by constructing `TypedRelationGraph` instances in-process via
   `build_typed_relation_graph` with hand-crafted `GraphEdgeRow` slices. 8 required cases
   (FR-13).

2. **Step 10b insertion (search.rs)** — wires `suppress_contradicts` into the live search
   pipeline. Must be tested via the existing `search.rs` `#[cfg(test)]` module. Two tests:
   - Positive integration test (AC-07, FR-14): end-to-end collision suppression visible through
     the search pipeline.
   - Floor + suppression combo test (R-07): exercises both Step 10 `retain` and Step 10b mask
     in the same call; asserts surviving entry identity and `final_score` values.

3. **Compile-time / code review gates** — R-08, R-09, R-02, AC-10, AC-12 are not runtime
   testable. They are gated by `cargo build` and explicit code review checks listed in each
   component plan.

4. **Regression gate (AC-06)** — Eval harness run (zero-regression profile). Passes because
   existing eval scenarios have no `Contradicts` edges. This gate does NOT validate suppression
   correctness (R-13). It is a necessary but not sufficient gate.

---

## Risk-to-Test Mapping

| Risk ID | Severity | Mapped Test(s) |
|---------|----------|----------------|
| R-01 | Critical | Placement: unit tests in `graph_suppression.rs` `#[cfg(test)]`, NOT `graph_tests.rs`. File size checked before gate-3b. |
| R-02 | High | Compile gate: any test that imports `suppress_contradicts` via the `graph.rs` re-export path validates visibility. |
| R-03 | High | Test case T-SC-09 (combo): asserts `ScoredEntry.final_score` values are from the surviving entries, not the suppressed ones. Code review: `let final_scores = new_fs` shadow present. |
| R-04 | High | Test case T-GS-02 (outgoing Contradicts): constructs edge with `relation_type: "Contradicts"` string explicitly. |
| R-05 | High | Test case T-GS-06 (incoming direction): edge written as rank-1 → rank-0. **Non-negotiable gate blocker (AC-03).** |
| R-06 | High | All 8 unit tests assert `result.len() == result_ids.len()`. T-GS-01 (empty input) explicitly checks zero-length return. |
| R-07 | High | Test case T-SC-09 (combo): both `similarity_floor` set AND `Contradicts` edge present in same call; asserts surviving entry count and `final_score`. |
| R-08 | Med | Compile gate: `cargo build --workspace` will fail if `mod graph_suppression` is missing from `graph.rs`. |
| R-09 | Med | Code review: `grep -n "pub mod graph_suppression" crates/unimatrix-engine/src/lib.rs` must return no matches. |
| R-10 | Med | Code review: `debug!` call confirmed to contain both `suppressed_entry_id` and `contradicting_entry_id`. |
| R-11 | Med | AC-05: existing cold-start tests in `search.rs` continue to pass; guard form `if !use_fallback` confirmed in code review. |
| R-12 | Med | Test case T-SC-08 uses `build_typed_relation_graph` with `bootstrap_only=false`. Code review: grep for `create_graph_edges_table` in new test code returns no matches. |
| R-13 | Med | T-SC-08 is the mandatory positive gate (FR-14). Listed separately from the eval gate in gate-3b checklist. |

---

## Cross-Component Test Dependencies

- T-SC-08 and T-SC-09 (search.rs) both call `suppress_contradicts` indirectly through the
  full `SearchService::search` pipeline. They depend on `graph_suppression.rs` being compiled
  and wired. If `graph_suppression.rs` is missing, these tests fail to compile (not at runtime).

- The `build_typed_relation_graph` function is shared by both the unit tests in
  `graph_suppression.rs` and the integration test in `search.rs`. Both call it the same way
  (entries slice + GraphEdgeRow slice with `bootstrap_only=false`). No shared test fixture
  file is needed — each test constructs its own minimal graph.

- The 8 unit tests in `graph_suppression.rs` must not touch `graph_tests.rs`. The `make_entry`
  and `make_edge_row` helpers in `graph_tests.rs` are NOT accessible from `graph_suppression.rs`
  (different module). Each must define its own local `make_entry` / `make_edge_row` helpers
  inside `#[cfg(test)]`.

---

## Integration Harness Plan (infra-001)

### Existing suites to run for regression

col-030 modifies `search.rs` (the tool implementation layer) and adds a new pure-function
module in `unimatrix-engine`. The applicable suites per the selection table are:

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory minimum gate for any change. |
| `tools` | `context_search` and `context_briefing` route through `SearchService::search`. |
| `lifecycle` | Multi-step flows involving store→search verify suppression is a no-op on normal entries. |
| `contradiction` | Exercises NLI contradiction detection path; regression check that suppression does not interfere with edge writes. |

### Gap analysis: new infra-001 integration tests

The mandatory positive test (AC-07, FR-14) and the floor+suppression combo (R-07) are implemented
as `#[cfg(test)]` tests inside `search.rs`, using `build_typed_relation_graph` with in-memory
fixtures. They do NOT go through the MCP JSON-RPC protocol layer; they call `SearchService::search`
directly.

**No new infra-001 Python tests are planned.** Reasons:

1. `suppress_contradicts` is a pure in-memory function with no MCP-visible contract change. The
   search tool's response shape is unchanged; suppressed entries simply do not appear. Testing
   absence of a specific entry through MCP requires knowing exact IDs, which is fragile in the
   Python harness.
2. The infra-001 `test_contradiction.py` suite exercises NLI detection (edge writes), not
   suppression (edge reads). These are orthogonal concerns; no new contradiction suite additions
   are needed for col-030.
3. The existing `tools` and `lifecycle` suites already exercise `context_search` end-to-end
   on databases with no Contradicts edges — zero-regression coverage.

If a future integration test needs to exercise suppression through MCP, the correct location
would be a new `test_suppression.py` scenario in `suites/`. That is deferred to post-#412 when
the audit log surfaces suppression counts (eval JSONL scenario coverage deferred per SCOPE.md).

### Zero-regression check command

```bash
# Build binary first
cargo build --release

# Minimum gate: smoke
cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60

# Full regression suites for col-030
cd product/test/infra-001 && python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_contradiction.py -v --timeout=60
```

---

## Acceptance Criteria Coverage Summary

| AC-ID | Test Type | Test Location | Test Identifier |
|-------|-----------|---------------|-----------------|
| AC-01 | Unit | `graph_suppression.rs` `#[cfg(test)]` | T-GS-01 (empty), T-GS-05 (no-Contradicts) |
| AC-02 | Unit | `graph_suppression.rs` `#[cfg(test)]` | T-GS-02 (outgoing rank-0→rank-1) |
| AC-03 | Unit | `graph_suppression.rs` `#[cfg(test)]` | T-GS-06 (incoming rank-1→rank-0) |
| AC-04 | Unit | `graph_suppression.rs` `#[cfg(test)]` | T-GS-05 (non-Contradicts edges) |
| AC-05 | Existing tests | `search.rs` existing cold-start tests | pass unchanged |
| AC-06 | Shell | eval harness | `--distribution_change false` |
| AC-07 | Integration | `search.rs` `#[cfg(test)]` | T-SC-08 |
| AC-08 | Code review | `graph_suppression.rs`, `graph.rs` | file presence + re-export check |
| AC-09 | `cargo test` | workspace-wide | no removals, no ignores |
| AC-10 | Grep | `graph_suppression.rs` | `grep "edges_directed\|neighbors_directed"` returns 0 |
| AC-11 | Code review | `search.rs` Step 10b | `debug!` field check |
| AC-12 | Code review | `search.rs` Step 10b | single-pass loop, shadow binding |
