# crt-042: PPR Expander — Test Plan Overview

## Overall Test Strategy

crt-042 adds a BFS graph expansion phase (Phase 0) to the search pipeline, behind a feature
flag. The test strategy is structured in three layers:

1. **Unit tests** (`graph_expand.rs` inline or `graph_expand_tests.rs` split): Pure function
   correctness — traversal direction, degenerate inputs, depth limits, seed exclusion,
   max-candidates cap, determinism. All tests construct minimal `TypedRelationGraph` fixtures
   in-process; no DB required.

2. **Config unit tests** (`infra/config.rs` inline test module): InferenceConfig serde
   defaults, validation ranges, and merge behavior for the three new fields. Grep-verified
   coverage of every `InferenceConfig {` literal.

3. **Integration tests** — two tiers:
   - `infra-001` harness suites (system-level, MCP JSON-RPC interface): flag-off regression
     gate, quarantine safety, cross-category behavioral proof (AC-25).
   - Tracing subscriber test (Phase 0 `debug!` emission): validates instrumentation that
     cannot be observed at the function unit level.

**Test count target**: ~30 new unit tests across all components + 1–2 new integration tests.
All tests are deterministic; no async in `graph_expand` unit tests (pure synchronous function).

---

## Risk-to-Test Mapping

| Risk ID | Risk Description | Test Location | Test Names / Coverage |
|---------|-----------------|---------------|----------------------|
| R-01 | Flag-off regression (bit-identical) | infra-001 tools/lifecycle/edge_cases suites + unit | AC-01: full existing suite passes with `ppr_expander_enabled=false`; dedicated flag-off pool-size invariant test |
| R-02 | S1/S2 single-direction Informs edges (blocking gate) | SQL query (AC-00) + unit | AC-00 prerequisite gate; unit test: one-direction fixture → empty result; back-fill fixture → result present |
| R-03 | Quarantine bypass | integration (infra-001 lifecycle + new test) | AC-13/AC-14: fixture with quarantined graph-reachable entry asserts absence; two-hop scenario |
| R-04 | O(N) latency at full expansion | tracing subscriber unit + eval | AC-24: debug! event emitted with elapsed_ms; eval gate P95 delta <= 50ms baseline |
| R-05 | Combined ceiling overflow (270) | unit (phase0_search) | AC-05 ceiling: Phase 0 caps at max_candidates; Phase 5 check; combined ceiling comment verified |
| R-06 | Back-fill race during eval | procedural | Delivery checklist: snapshot taken after back-fill committed |
| R-07 | Eval gate failure | eval harness | AC-22/AC-23: profile exists; MRR >= 0.2856 recorded |
| R-08 | InferenceConfig hidden test sites | grep + config unit | Grep for `InferenceConfig {` across entire test suite; default value unit tests; merge unit tests |
| R-09 | edges_of_type() boundary violation | grep (AC-16) | `grep -n 'edges_directed\|neighbors_directed' graph_expand.rs` → zero matches |
| R-10 | Timing instrumentation absent/wrong level | tracing subscriber unit | AC-24: assert debug! emitted with all required fields; assert not emitted on flag=false path |
| R-11 | BFS visited-set missing | unit (graph_expand) | Bidirectional CoAccess cycle test; triangular cycle termination test |
| R-12 | Seed exclusion failure | unit (graph_expand) | AC-08: seeds {A,B}, edge A→B; both absent from result; self-loop test |
| R-13 | Determinism failure | unit (graph_expand) | NFR-04: two calls same inputs → identical HashSet; budget-boundary multi-call |
| R-14 | Config validation conditional gap | config unit | AC-18/19/20/21: four validation tests, all with ppr_expander_enabled=false |
| R-15 | get_embedding layer-0 miss | unit (phase0_search) | AC-15: None embedding → silent skip; code inspection of IntoIterator usage |
| R-16 | Phase 0 insertion point wrong | integration + unit | AC-02: after Phase 0 and before Phase 1, results_with_scores contains expanded entries |
| R-17 | S8 CoAccess directionality gap | SQL query + unit | Directionality verification query; S8-style unidirectional fixture test |

---

## Cross-Component Test Dependencies

- `graph_expand` tests are independent: pure function, no DB, no async.
- Phase 0 integration tests depend on `graph_expand` being correct first.
- Config tests depend on `InferenceConfig` struct being complete (all 4 coordinated sites).
- The tracing subscriber test (AC-24) depends on Phase 0 being wired in `search.rs`.
- AC-25 (integration) depends on both `graph_expand` and Phase 0 insertion being correct.

**Execution order for Stage 3c**:
1. `cargo test --workspace` (all unit tests, including graph_expand and config)
2. `pytest -m smoke` (flag-off regression gate)
3. `pytest suites/test_lifecycle.py suites/test_tools.py` (quarantine, search behavior)
4. New integration test for AC-25 (cross-category behavioral proof)

---

## Integration Harness Plan

### Existing suites that cover this feature

| Suite | Relevance | Coverage |
|-------|-----------|---------|
| `smoke` | Mandatory minimum gate | Flag-off regression: entire smoke path exercises search with `ppr_expander_enabled=false` (default). If Phase 0 code leaks onto the flag-false path, smoke tests detect it via result drift. |
| `tools` | Store/retrieval behavior | `test_search_*` tests exercise the search pipeline end-to-end. With flag=false (default), these are the R-01 regression suite. |
| `lifecycle` | Multi-step flows | `test_store_then_search_*`, quarantine chain tests. AC-13/AC-14 quarantine bypass check is lifecycle-style. |
| `security` | Quarantine enforcement | `test_search_excludes_quarantined` directly validates that quarantined entries are absent. Covers the quarantine safety invariant for expanded entries if harness creates graph-reachable quarantined entries. |
| `edge_cases` | Boundary values, empty DB | Degenerate cases (empty graph, no seeds) are partially covered by `edge_cases`. |

**The smoke tests DO cover the flag-off regression path**: all existing smoke tests run with
the server built from crt-042 code; if Phase 0 is not properly gated behind the flag, any
result drift surfaces immediately. This makes `pytest -m smoke` the mandatory minimum gate
for R-01.

### New integration test to write (mandatory)

**AC-25: Cross-Category Behavioral Proof** — this is the core behavioral guarantee of the
feature and cannot be validated by any existing suite test.

File: `product/test/infra-001/suites/test_lifecycle.py` (or a new file
`test_graph_expand.py` if the lifecycle suite is already crowded)

```python
# Naming: test_search_graph_expand_{behavior}
def test_search_graph_expand_surfaces_cross_category_entry(server):
    """AC-25: Entry reachable by positive graph edge appears in results only with
    ppr_expander_enabled=true. With flag=false, the same entry is absent."""
    ...
```

**Fixture**: `server` (fresh DB, no state leakage)
**Setup**: store two entries with dissimilar embeddings (different categories/topics so HNSW
k=20 would not naturally co-return them); store a GRAPH_EDGES row connecting them via a
positive edge type (Supports or CoAccess); run search with flag=true and flag=false; compare
result sets.

**Note**: The MCP tool interface does not expose `ppr_expander_enabled` at query time — it is
a server config field. This integration test may need to use two server instances or a config
override. Consult the fixture design with the delivery agent. If server config overrides per
test are not supported by the harness, this test should be implemented as a unit-level
integration test within `unimatrix-server` crate tests instead of the MCP harness.

**AC-14: Quarantine bypass** — add one targeted test if the existing `test_search_excludes_quarantined`
does not construct a GRAPH_EDGES row pointing to the quarantined entry. The new test should:
1. Store entry A (active seed), store entry Q (then quarantine it).
2. Insert a GRAPH_EDGES row A→Q (positive edge).
3. Enable the expander.
4. Run search with query that surfaces A as a seed.
5. Assert Q is absent from results.

### Triage guidance for Stage 3c

- Any failure in `tools`, `lifecycle`, `security` suites that is **not** in code changed by
  crt-042: file a GH Issue `[infra-001] test_<name>: <description>`, mark `@pytest.mark.xfail`,
  continue. Known pre-existing: GH#303 (import::tests pool timeout), GH#305
  (test_retrospective_baseline_present).
- Any failure in `smoke`: investigate immediately. The smoke gate is mandatory. A smoke
  failure in flag=false behavior is R-01 (Critical) and blocks the PR.
- Any failure specifically in a new test added for AC-25 or AC-14: this is a feature bug,
  fix the code.

---

## AC-00 Prerequisite Gate: SQL Verification Plan

Before any Phase 0 code is written, the delivery agent must execute:

```sql
-- Count total Informs edges
SELECT COUNT(*) AS total_informs FROM GRAPH_EDGES WHERE relation_type = 'Informs';

-- Sample to check for symmetric pairs
SELECT e1.source_id, e1.target_id,
       CASE WHEN e2.source_id IS NOT NULL THEN 'BIDIRECTIONAL' ELSE 'SINGLE-DIRECTION' END AS dir
FROM GRAPH_EDGES e1
LEFT JOIN GRAPH_EDGES e2
  ON e1.target_id = e2.source_id AND e1.source_id = e2.target_id
  AND e2.relation_type = 'Informs'
WHERE e1.relation_type = 'Informs'
LIMIT 20;

-- Count CoAccess pairs and check bidirectionality
SELECT COUNT(*) AS total_coaccess FROM GRAPH_EDGES WHERE relation_type = 'CoAccess';
SELECT COUNT(*) AS bidirectional_coaccess
FROM GRAPH_EDGES e1
JOIN GRAPH_EDGES e2
  ON e1.target_id = e2.source_id AND e1.source_id = e2.target_id
  AND e2.relation_type = 'CoAccess'
WHERE e1.relation_type = 'CoAccess' AND e1.source_id < e1.target_id;
```

Expected outcomes:
- If Informs edges are bidirectional: proceed with Phase 0 implementation.
- If single-direction only: file back-fill issue before writing Phase 0 code. Document the
  result as the AC-00 verification artifact in the PR description.

---

## R-08: InferenceConfig Hidden Test Sites — Grep Plan

After adding the three new fields, the tester MUST run:

```bash
grep -rn 'InferenceConfig {' crates/unimatrix-server/src/ --include='*.rs'
```

Every match must either:
1. Include all three new fields (`ppr_expander_enabled`, `expansion_depth`,
   `max_expansion_candidates`), OR
2. Use `..InferenceConfig::default()` / `..Default::default()` spread syntax.

Any literal that is missing the new fields and does not use spread syntax is a hidden site
bug that will cause silent defaults divergence. The tester must flag these as failures and
return them to the implementation agent for correction before Stage 3c can pass.

---

## R-04 / AC-24: Tracing Instrumentation Test Design

The `debug!` trace emission test cannot be satisfied by a log-level inspection alone. It
requires a tracing test subscriber that captures events at `TRACE` or `DEBUG` level and
asserts field presence.

**Recommended approach**: use the `tracing-test` crate (`#[traced_test]` macro) or a manual
`tracing_subscriber::fmt()` subscriber with a `TestWriter` capture buffer. The test should:

1. Configure `ppr_expander_enabled = true` in the `SearchService` under test.
2. Execute a search with a real or mock graph containing graph-reachable entries.
3. Capture tracing output at DEBUG level.
4. Assert the captured output contains the string `"Phase 0 (graph_expand) complete"` (or
   the exact message from the implementation).
5. Assert field presence: `expanded_count`, `fetched_count`, `elapsed_ms`,
   `expansion_depth`, `max_expansion_candidates`.
6. Assert the trace is NOT emitted when `ppr_expander_enabled = false`.

**Do not defer this test.** Entry #3935 documents a gate failure where tracing tests were
deferred to a follow-up; this is a non-negotiable test per the Risk Strategy.

---

## AC-25: Cross-Category Regression Test Design

AC-25 is the behavioral proof that the architecture delivers its core promise: cross-category
entries previously invisible (outside HNSW k=20) become reachable via graph topology.

**Design**:
- Entry S (seed): embedding close to query Q. Will appear in HNSW k=20.
- Entry E (expanded): embedding far from Q (e.g., different category, different vocabulary).
  Would NOT appear in HNSW k=20.
- Edge: `S → E` of type `CoAccess` or `Supports`.
- Search with Q, `ppr_expander_enabled = true`: assert E is in results.
- Search with Q, `ppr_expander_enabled = false`: assert E is absent from results.

**This test must be in the test suite regardless of eval gate outcome.** It is the behavioral
proof that cannot be replaced by eval metrics alone.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — entries #3740 (submodule pattern), #3754 (direction semantics lesson), #4044/#2730 (InferenceConfig hidden test sites), #4049–#4054 (all crt-042 ADRs), #3806/#3386/#3579 (Gate 3b failure patterns), #3631 (inline test pattern).
- Queried: `context_search` × 4 — confirmed graph fixture pattern (#3650), InferenceConfig hidden sites (#4044, #2730), direction semantics (#3754), tracing search (#2929, #3461 — no perfect match for tracing subscriber test pattern, but PPR test fixture pattern in #3740 is directly applicable).
- Entry #3754 (traversal direction semantics lesson) is critical for test design: all traversal correctness tests must be behavioral (observable outcome), not enum-value assertions.
