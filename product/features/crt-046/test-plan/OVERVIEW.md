# Test Plan Overview: crt-046 — Behavioral Signal Delivery

## Overall Test Strategy

crt-046 spans two crates (unimatrix-store, unimatrix-server) and touches four
distinct execution paths: schema migration, behavioral signal extraction, step
8b insertion in context_cycle_review, and goal-conditioned blending in
context_briefing. The testing strategy reflects these boundaries:

- **Unit tests** cover all pure and near-pure logic in `behavioral_signals.rs`
  and the three new store methods. These tests run without a full server stack
  and form the primary coverage layer for business logic.
- **Store-layer tests** (in-process, `#[tokio::test]`) cover schema migration,
  sqlite_parity, `insert_goal_cluster`, `query_goal_clusters_by_embedding`, and
  `get_cycle_start_goal_embedding`. These exercise real SQLite via `SqlxStore`.
- **Integration tests** (infra-001 harness) cover MCP-visible behavior: the
  `parse_failure_count` field in context_cycle_review responses, step 8b running
  on force=false calls, briefing blending, drain flush timing, and cold-start
  path equivalence through the full binary.

All integration tests that query `graph_edges` MUST flush the analytics drain
before asserting. The flush mechanism is `store.close().await` in store-layer
tests, which sends the drain shutdown signal and awaits drain completion. In the
infra-001 harness, the drain flush is achieved via the
`enqueue_analytics_and_flush` helper (entry #2148, pattern #4114) — either a
server restart or a dedicated force-flush MCP call must be issued before
asserting `graph_edges` row counts.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Primary Tests | Component |
|---------|----------|---------------|-----------|
| R-01 | Critical | AC-15 integration: call twice, assert graph_edges stable | cycle-review-step-8b |
| R-02 | Critical | R-02-contract unit: UNIQUE conflict → edges_enqueued == 0 | behavioral-signals |
| R-03 | Critical | Drain flush before every graph_edges assertion (I-02) | all integration tests |
| R-04 | Critical | AC-13 integration: malformed row → parse_failure_count ≥ 1 in response | cycle-review-step-8b |
| R-05 | Critical | AC-12 migration test; AC-17 grep check; sqlite_parity | store-v22 |
| R-06 | High | R-06: populate_goal_cluster called only after entry_ids assembled | behavioral-signals |
| R-07 | High | AC-11: 101-row boundary recency cap | briefing-blending |
| R-08 | High | AC-16 unit: feature=None → no DB call; guard-B unit: embedding=None → no cluster query | briefing-blending |
| R-09 | High | AC-14: 21 observations → edge count ≤ 400; build_coaccess_pairs unit cap | behavioral-signals |
| R-10 | High | AC-01: both A→B and B→A present; emit_behavioral_edges unit: 2 enqueue calls | behavioral-signals |
| R-11 | High | AC-08, AC-09 cold-start equivalence; below-threshold cold-start | briefing-blending |
| R-12 | High | AC-10: deprecated/quarantined IDs excluded from briefing | briefing-blending |
| R-14 | Med | Code review: all three new store methods are async fn called with .await | store-v22 |
| R-15 | Med | sqlite_parity: test_create_tables_goal_clusters_schema; migration test column count | store-v22 |
| R-16 | Low | outcome_to_weight table-driven unit test | behavioral-signals |

---

## Cross-Component Test Dependencies

1. **AC-15 (R-01)** requires both store-v22 (insert_goal_cluster working) and
   cycle-review-step-8b (memoisation gate position) to be correct simultaneously.
   Integration test is the only form that validates both.

2. **AC-13 (R-04)** requires the parse_failure_count field to be wired from
   collect_coaccess_entry_ids through the handler to the JSON response. The unit
   test validates extraction logic; the integration test validates the wire.

3. **AC-11 (R-07)** requires store-v22 (query_goal_clusters_by_embedding LIMIT
   clause) and briefing-blending (handler calls with correct recency_limit=100).
   Integration test covers the end-to-end path.

4. **AC-07 (displacement)** requires behavioral-signals (blend_cluster_entries),
   briefing-blending (handler blending sequence), and store-v22
   (query_goal_clusters_by_embedding returning rows above threshold). Integration
   test is required.

5. **Drain flush rule (R-03/I-02)** applies to AC-01, AC-02, AC-03, AC-04,
   AC-13, AC-14, AC-15. Every test touching graph_edges must call close() or the
   harness flush helper before asserting row counts.

---

## Integration Harness Plan

### Suites to Run

crt-046 touches server tool logic (context_cycle_review, context_briefing),
store/retrieval behavior (goal_clusters, graph_edges), and schema changes. Per
the suite selection table:

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory minimum gate — always |
| `tools` | context_cycle_review and context_briefing are modified tools |
| `lifecycle` | Multi-step flows: cycle review → briefing blending chain |
| `edge_cases` | Cold-start, empty tables, boundary values |

Optional (run if resources permit): `confidence` (briefing re-ranking
interaction with cluster scores).

### Existing Suite Coverage

The following existing tests in infra-001 provide pre-existing coverage of code
paths touched by crt-046 but not specifically testing new behavior:
- `test_tools.py::test_briefing_*` — verifies briefing returns content and
  handles empty DB. These become regression tests for cold-start (R-11).
- `test_lifecycle.py::test_store_search_find_flow` — smoke gate for basic
  store→search pipeline (unchanged by this feature).
- `test_tools.py::test_cycle_review_*` — existing context_cycle_review tests
  become regression tests for R-01 (memoisation) and R-04 (parse_failure_count).

### New Integration Tests Required

The following new tests must be added to the infra-001 harness in Stage 3c.
Each is a behavior only verifiable through the MCP interface.

#### `suites/test_tools.py` — new tests

| Test name | AC/Risk | Fixture | Description |
|-----------|---------|---------|-------------|
| `test_cycle_review_parse_failure_count_in_response` | AC-13, R-04 | `server` | Seed malformed observation; call review; assert top-level `parse_failure_count >= 1` in JSON response |
| `test_cycle_review_parse_failure_count_zero_clean` | R-04 | `server` | All-valid observations; assert `parse_failure_count == 0` in response (not absent) |
| `test_cycle_review_bidirectional_edges` | AC-01, R-10 | `server` | Two context_get obs; review; flush drain; assert both A→B and B→A in graph_edges |
| `test_cycle_review_edge_idempotency` | AC-02 | `server` | Two review calls; flush after each; assert graph_edges COUNT identical |
| `test_cycle_review_edge_weight_success` | AC-03 | `server` | outcome=success; flush; assert weight=1.0 in graph_edges |
| `test_cycle_review_edge_weight_other` | AC-03 | `server` | outcome=None; flush; assert weight=0.5 in graph_edges |
| `test_cycle_review_zero_get_obs_zero_edges` | AC-04 | `server` | No context_get observations; review; flush; assert 0 behavioral edges |
| `test_cycle_review_goal_cluster_created` | AC-05 | `server` | Cycle with goal → goal_clusters row created with correct fields |
| `test_cycle_review_no_goal_no_cluster` | AC-06 | `server` | No goal → goal_clusters empty for that feature_cycle |
| `test_briefing_cluster_displaces_weak_semantic` | AC-07 | `populated_server` | Matching cluster entry with cluster_score > weakest semantic → appears in top-20 |
| `test_briefing_null_embedding_cold_start` | AC-08 | `server` | NULL goal embedding → result identical to pure-semantic baseline |
| `test_briefing_empty_goal_clusters_cold_start` | AC-09 | `server` | Empty goal_clusters table → result identical to pure-semantic baseline |
| `test_briefing_inactive_entries_excluded` | AC-10 | `server` | Deprecated/quarantined entry IDs in cluster → not in briefing output |
| `test_briefing_recency_cap_101_rows` | AC-11, R-07 | `server` | 101 goal_clusters rows; oldest has best cosine; assert its IDs absent from output |
| `test_briefing_feature_none_cold_start` | AC-16, R-08 | `server` | feature=None → same as pure-semantic; no cluster query issued |
| `test_cycle_review_force_false_reruns_step8b` | AC-15, R-01 | `server` | Call review twice; assert graph_edges count unchanged after second call |
| `test_cycle_review_pair_cap_200` | AC-14, R-09 | `server` | 21 distinct context_get obs → edge count ≤ 400 |
| `test_briefing_cluster_score_below_semantic_no_displacement` | R-13-doc | `populated_server` | Cluster entry with low cluster_score → not in top-20 when all semantic > cluster |

#### `suites/test_lifecycle.py` — new tests

| Test name | AC/Risk | Fixture | Description |
|-----------|---------|---------|-------------|
| `test_cycle_review_to_briefing_blending_chain` | AC-05, AC-07 | `server` | Full chain: cycle with goal + context_get obs → review → briefing with same goal → cluster entry in result |
| `test_step8b_runs_on_force_false_lifecycle` | AC-15, R-01 | `server` | Full lifecycle: first review (force=true or cache-miss), second review force=false; verify edges exist after both |

### Drain Flush Protocol for New Tests

Every new test that queries `graph_edges` must use the flush pattern. In the
infra-001 harness this is done by calling the server's `enqueue_analytics` flush
endpoint or by restarting the server between review and assertion. The exact
mechanism must be confirmed from entry #2148 before Stage 3c begins. If the
harness does not expose a flush endpoint, a short sleep (≥ 600ms to exceed the
500ms DRAIN_FLUSH_INTERVAL) is an acceptable fallback, clearly documented.

### AC-12 Migration Test — Fixture DB Required

AC-12 requires opening a v21 fixture database and running migration to v22.
There are no pre-existing .db fixture files in the codebase (confirmed: no *.db
files under crates/). The implementation agent must create a v21 fixture
programmatically within the test: open a fresh store, run the v21 DDL manually
via a raw connection, set schema_version=21, then close it before running
migration. This pattern mirrors the v20→v21 migration test from crt-043.

---

## Test File Locations

| Test type | File |
|-----------|------|
| store-v22 unit + migration | `crates/unimatrix-store/src/db.rs` (schema tests), `crates/unimatrix-store/src/goal_clusters.rs` (goal_clusters tests), `crates/unimatrix-store/src/migration.rs` (migration tests) |
| behavioral_signals unit | `crates/unimatrix-server/src/services/behavioral_signals.rs` |
| cycle-review-step-8b unit | `crates/unimatrix-server/src/mcp/tools.rs` or dedicated `#[cfg(test)]` submodule |
| briefing-blending unit | `crates/unimatrix-server/src/services/behavioral_signals.rs` (blend_cluster_entries) |
| infra-001 integration | `product/test/infra-001/suites/test_tools.py`, `product/test/infra-001/suites/test_lifecycle.py` |
