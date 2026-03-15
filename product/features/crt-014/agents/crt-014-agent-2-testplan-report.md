# Agent Report: crt-014-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Feature**: crt-014 — Topology-Aware Supersession
**Completed**: 2026-03-15

---

## Deliverables

| File | Lines | Purpose |
|------|-------|---------|
| `/workspaces/unimatrix-crt-014/product/features/crt-014/test-plan/OVERVIEW.md` | 177 | Risk-to-test mapping, cross-component dependencies, integration harness plan |
| `/workspaces/unimatrix-crt-014/product/features/crt-014/test-plan/graph.md` | 431 | Unit test expectations for graph.rs (AC-03 through AC-17, R-01 through R-12) |
| `/workspaces/unimatrix-crt-014/product/features/crt-014/test-plan/search.md` | 394 | T-SP migration specs, AC-12/AC-13/AC-16 integration test specs |
| `/workspaces/unimatrix-crt-014/product/features/crt-014/test-plan/confidence.md` | 161 | Removal plan for 4 constant-value tests, atomic commit requirement, net coverage analysis |

---

## Risk Coverage Summary

| Priority | Risks | Status |
|----------|-------|--------|
| Critical | R-01, R-03, R-04, R-05, R-06 | Full coverage — distinct test per branch |
| High | R-02, R-07, R-08, R-09, R-10, R-11 | Full coverage — unit + code-review checks |
| Medium | R-12, IR-01, IR-02, IR-03, IR-04 | Full coverage — unit + grep + code-review |
| Low | R-13 | Cargo build verification |

---

## Integration Harness Plan Summary

Suites to run: `smoke` (mandatory gate) + `tools` + `lifecycle`.

New integration tests required in `suites/test_lifecycle.py`:
1. `test_search_multihop_injects_terminal_active` — AC-13 (A→B→C, C injected not B)
2. `test_search_single_hop_injection_regression` — R-06 regression guard
3. `test_search_deprecated_entry_visible_with_topology_penalty` — AC-12

AC-16 (cycle fallback) is unit-test-only: cannot inject a supersession cycle through the MCP interface. Verified via `build_supersession_graph` called directly in a `search.rs` unit test.

---

## Key Findings

1. **8 T-SP tests need migration**: Lines 540–678 and 814–869 of `search.rs` reference `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY`. Detailed per-test migration specs are in `search.md` with exact constant substitutions and updated numerical comments.

2. **`penalties_independent_of_confidence_formula` can be retained**: The body of this test does not reference the removed constants — it only checks the weight sum invariant. Rename to `weight_sum_invariant_is_0_92` and keep it in `confidence.rs`. Only 3 tests are truly deleted; 1 is renamed.

3. **crt-018b interaction tests (lines 814–869)**: These tests use `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` as representative values for testing utility delta formula placement. After constant removal, substitute `ORPHAN_PENALTY` and `CLEAN_REPLACEMENT_PENALTY` respectively. The formula verification logic is unaffected.

4. **Ordering semantics inversion is intentional**: T-SP-08 migrated to use topology constants changes the relative ordering of orphan-deprecated vs clean-replacement-superseded entries. The new behavior (orphan at 0.75 is softer than clean-replacement at 0.40) is correct per ADR-004 and must be documented in the updated test.

5. **Atomic commit is load-bearing**: R-05 risk is real. If the 3 constant deletions land before the graph.rs ordering tests, CI will pass with zero penalty coverage for one commit. Stage 3b implementers must be instructed to land these in a single commit.

---

## Open Questions

1. **`make_test_entry` signature**: The existing `make_test_entry` helper in `search.rs` takes `(id, status, superseded_by, confidence, category)`. The graph tests need both `supersedes` and `superseded_by` fields populated. The `graph.rs` helper will need its own constructor that accepts both fields. Confirm the existing `EntryRecord` struct allows this without additional derive macros.

2. **`find_terminal_active` starting-node behavior**: The spec says "return None if starting node not in graph" but is silent on "starting node is itself the terminal." The test plan specifies `Some(node_id)` for this case (consistent with the intent of the function — the caller wants the active terminal, and if the start node is active and not superseded, it is the terminal). Implementation team should confirm this interpretation.

3. **tracing::warn! verification for R-09**: Without `tracing_test` or a subscriber in the test environment, the warn assertion cannot be automated. Plan: document as code-review-only for Stage 3c. If `tracing_test` is already available as a dev-dependency in `unimatrix-engine`, use it — otherwise mark as code-review.

4. **infra-001 test numbering vs crt-014 scope**: The USAGE-PROTOCOL says 157 tests across 8 suites. The spawn prompt references 185 integration tests (as of col-022). Either the count was updated post-protocol or there are more tests than documented. No impact on the test plan — the plan references suites, not counts.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures — MCP tool unavailable in this environment; no results retrieved. Non-blocking per protocol.
- Stored: nothing novel to store — the test migration pattern (behavioral ordering replaces constant-value assertions) is feature-specific to the constant removal decision in crt-014 and is fully captured in `test-plan/confidence.md`. The pattern itself (behavioral over absolute) is a general testing principle already well-understood in the codebase.
