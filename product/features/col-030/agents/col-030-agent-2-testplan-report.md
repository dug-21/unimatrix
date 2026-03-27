# Agent Report: col-030-agent-2-testplan

## Phase: Test Plan Design (Stage 3a)

## Output Files

- `/workspaces/unimatrix/product/features/col-030/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/col-030/test-plan/graph_suppression.md`
- `/workspaces/unimatrix/product/features/col-030/test-plan/search_step10b.md`

---

## Risk Coverage Mapping

| Risk ID | Priority | Coverage Type | Test Identifier(s) |
|---------|----------|---------------|--------------------|
| R-01 | Critical | Placement rule | Tests in `graph_suppression.rs` `#[cfg(test)]` by design; file-size check at gate-3b |
| R-02 | High | Compile gate | Any test importing via `graph.rs` re-export validates this |
| R-03 | High | Unit + combo test | T-SC-09: asserts `results[1].final_score == F_C` (catches silent shadow omission) |
| R-04 | High | Unit test | T-GS-02: constructs edge using `RelationType::Contradicts.as_str()` explicitly |
| R-05 | High | Unit test (mandatory) | T-GS-06: Incoming direction edge — gate blocker for AC-03 |
| R-06 | High | All 8 unit tests | Every test asserts `mask.len() == result_ids.len()`; T-GS-01 covers empty input |
| R-07 | High | Combo test | T-SC-09: floors + suppression in same call; verifies `aligned_len` and `final_score` values |
| R-08 | Med | Compile gate | `cargo build --workspace` fails if `mod graph_suppression` missing from `graph.rs` |
| R-09 | Med | Code review | `grep "pub mod graph_suppression" lib.rs` returns 0 |
| R-10 | Med | Code review | `debug!` field check for both `suppressed_entry_id` and `contradicting_entry_id` |
| R-11 | Med | Existing tests + code review | Cold-start tests in `search.rs` pass; `if !use_fallback` guard form confirmed |
| R-12 | Med | Test helper restriction | T-SC-08 uses `build_typed_relation_graph` only; grep gate on `create_graph_edges_table` |
| R-13 | Med | Explicit gate listing | T-SC-08 listed as mandatory positive gate separate from eval gate in gate-3b checklist |

---

## Integration Suite Plan

**Run at Stage 3c:**

| Suite | Command | Purpose |
|-------|---------|---------|
| Smoke (mandatory) | `pytest suites/ -v -m smoke --timeout=60` | Minimum gate |
| tools | `pytest suites/test_tools.py -v --timeout=60` | `context_search` regression |
| lifecycle | `pytest suites/test_lifecycle.py -v --timeout=60` | Multi-step search flow regression |
| contradiction | `pytest suites/test_contradiction.py -v --timeout=60` | NLI detection path unaffected |

**No new infra-001 Python tests planned.** Suppression behavior is not MCP-contract-visible
(response shape unchanged; suppressed entries simply absent). Positive validation is done
through the `search.rs` `#[cfg(test)]` integration tests (T-SC-08, T-SC-09) which call
`SearchService::search` directly with controlled in-memory fixtures.

---

## Key Design Decisions

1. **T-GS-06 is the most critical test.** An Outgoing-only implementation passes T-GS-01
   through T-GS-05 and T-GS-07. Only T-GS-06 (Incoming direction) catches the missing
   bidirectional query. It is explicitly flagged as a gate blocker for AC-03.

2. **T-SC-09 is the only test that catches the `final_scores` shadow omission (R-03) and
   the `aligned_len` bug (R-07).** A test without both a similarity floor AND a suppression
   event in the same call cannot distinguish correct behavior from the silent misalignment
   scenarios.

3. **Local helpers required in `graph_suppression.rs` `#[cfg(test)]`.** The `make_entry` and
   `make_edge_row` helpers in `graph_tests.rs` are not accessible from the new module.
   The test plan specifies the exact helper signatures to implement.

4. **`bootstrap_only: false` is non-negotiable** on all `GraphEdgeRow` test fixtures. Edges
   with `bootstrap_only=true` are excluded by `build_typed_relation_graph` Pass 2b — tests
   using `true` would silently construct an empty graph and give false passes.

---

## Open Questions

None. All open questions from SCOPE.md and SPECIFICATION.md are resolved in IMPLEMENTATION-BRIEF.md:
- OQ-01 (file placement): resolved — `graph_suppression.rs`
- Edge direction: resolved — both Outgoing and Incoming required (ADR-003)
- `use_fallback` atomicity: resolved — read-lock clone at Step 6 (ADR-005, SR-08)
- Eval scenario coverage: deferred to post-#412 — integration test in `search.rs` is sufficient

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3630, #3626, #3627, #3624,
  #3616 directly relevant. Entry #3624 confirmed mandatory positive integration test as a
  non-optional gate for suppression features. Entry #3616 confirmed Step 10b insertion pattern
  and `use_fallback` guard. Entries #3628, #3629 (ADR-003, ADR-004) confirmed bidirectional
  query and single-pass mask requirements.
- Queried: `context_search` for col-030 ADRs (topic filter) — returned all 5 col-030 ADRs
  by ID.
- Stored: entry #3631 "Inline #[cfg(test)] in new sibling module when parent test file is
  already oversized" via `/uni-store-pattern` — captures the discovery that a module split
  creates a test isolation boundary: the new module cannot access helpers from the parent's
  test file, requiring its own local helpers inside `#[cfg(test)]`. This is distinct from the
  existing entry #3568 (which covers mod.rs declaration patterns for splitting test files,
  not the helper inaccessibility trap).
