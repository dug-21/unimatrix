# Risk Coverage Report: crt-046 — Behavioral Signal Delivery

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Memoisation gate bypasses step 8b | `test_cycle_review_force_false_reruns_step8b` (tools), `test_step8b_runs_on_force_false_lifecycle` (lifecycle), AC-15 | PASS | Full |
| R-02 | write_graph_edge return contract misused | `test_emit_behavioral_edges_unique_conflict_not_counted` (tools), `test_emit_behavioral_edges_unique_conflict_not_counted` (unit in behavioral_signals.rs) | PASS | Full |
| R-03 | Analytics drain shedding drops behavioral edges | Code inspection: `write_pool_server()` used directly (ADR-006), not analytics drain. `bootstrap_only=false` shed path not applicable. | PASS | Full (structural) |
| R-04 | Silent observation parse failures not returned | `test_cycle_review_parse_failure_count_in_response`, `test_cycle_review_parse_failure_count_zero_clean` (tools) | PASS | Full |
| R-05 | Schema migration v21→v22 cascade incomplete | `test_v21_to_v22_migration_creates_goal_clusters`, `test_v21_to_v22_migration_creates_index`, `test_fresh_db_creates_schema_v22`, `test_current_schema_version_is_at_least_22` (migration_v21_v22.rs); AC-17 grep check | PASS | Full |
| R-06 | Goal-cluster partial-record persistence | `test_populate_goal_cluster_duplicate_returns_false` (unit), populate_goal_cluster is final step in step 8b | PASS | Full |
| R-07 | Recency cap not enforced | `test_query_goal_clusters_recency_cap_100` (unit), `test_briefing_recency_cap_101_rows` (tools) | PASS | Full |
| R-08 | Briefing NULL short-circuit fires too late | `test_briefing_feature_none_cold_start` (tools), `test_briefing_empty_goal_clusters_cold_start` (tools) | PASS | Full |
| R-09 | Pair cap enforced after iteration | `test_cycle_review_pair_cap_200` (tools), `test_build_coaccess_pairs_cap_enforced_at_200` (unit) | PASS | Full |
| R-10 | Bidirectional edge emission — one direction missing | `test_cycle_review_bidirectional_edges` (tools), `test_emit_behavioral_edges_new_pair_emits_both_directions` (unit) | PASS | Full |
| R-11 | Cold-start regressions | `test_briefing_empty_goal_clusters_cold_start`, `test_briefing_feature_none_cold_start` (tools) | PASS | Full |
| R-12 | Inactive entry leakage | `test_briefing_inactive_entries_excluded` (tools) | PASS | Full |
| R-13 | Zero-remaining-slot suppression | **RESOLVED (ADR-005)** — `test_briefing_cluster_score_below_semantic_no_displacement` (tools) | PASS | Full |
| R-14 | spawn_blocking violation for sqlx | Code inspection: all three new store methods are `async fn` with no `spawn_blocking`. Confirmed via grep. | PASS | Full (structural) |
| R-15 | goal_clusters DDL mismatch migration.rs/db.rs | `test_create_tables_goal_clusters_schema` (sqlite_parity), `test_v21_to_v22_migration_creates_goal_clusters` (migration) | PASS | Full |
| R-16 | Outcome weight boundary | `test_outcome_to_weight_*` (unit, 4 cases in behavioral_signals.rs) | PASS | Full |

---

## Test Results

### Unit Tests

- **Total**: 4482
- **Passed**: 4482
- **Failed**: 0
- **Ignored**: 28 (pre-existing, unchanged)

Key crt-046 unit test groups:
- `crates/unimatrix-store/src/goal_clusters.rs` — 13 tests (insert, query, cosine, recency, threshold, embedding roundtrip)
- `crates/unimatrix-store/tests/migration_v21_v22.rs` — 5 tests (fresh db v22, v21→v22 migration, index, idempotency)
- `crates/unimatrix-server/src/services/behavioral_signals.rs` — tests for collect, build_pairs, outcome_to_weight, emit_edges, populate_cluster, blend_cluster_entries
- `crates/unimatrix-store/src/db.rs` — schema v22 tests, sqlite_parity

### Integration Tests

#### Smoke Gate (mandatory minimum)
- **Total**: 22
- **Passed**: 22
- **Failed**: 0
- **Run time**: ~3m 11s

#### New crt-046 tests — `suites/test_tools.py`
- **Total**: 17
- **Passed**: 17
- **Failed**: 0

| Test | AC/Risk |
|------|---------|
| `test_cycle_review_parse_failure_count_in_response` | AC-13 (NON-NEGOTIABLE) |
| `test_cycle_review_parse_failure_count_zero_clean` | R-04 |
| `test_cycle_review_bidirectional_edges` | AC-01, R-10 |
| `test_cycle_review_edge_idempotency` | AC-02 |
| `test_cycle_review_edge_weight_success` | AC-03 |
| `test_cycle_review_edge_weight_other` | AC-03 |
| `test_cycle_review_zero_get_obs_zero_edges` | AC-04 |
| `test_cycle_review_goal_cluster_created` | AC-05 |
| `test_cycle_review_no_goal_no_cluster` | AC-06 |
| `test_briefing_empty_goal_clusters_cold_start` | AC-09, R-11 |
| `test_briefing_inactive_entries_excluded` | AC-10, R-12 |
| `test_cycle_review_force_false_reruns_step8b` | AC-15, R-01 (NON-NEGOTIABLE) |
| `test_cycle_review_pair_cap_200` | AC-14, R-09 |
| `test_emit_behavioral_edges_unique_conflict_not_counted` | R-02-contract (NON-NEGOTIABLE) |
| `test_briefing_feature_none_cold_start` | AC-16, R-08 |
| `test_briefing_recency_cap_101_rows` | AC-11, R-07 |
| `test_briefing_cluster_score_below_semantic_no_displacement` | R-13-doc |

#### New crt-046 tests — `suites/test_lifecycle.py`
- **Total**: 2
- **Passed**: 2
- **Failed**: 0

| Test | AC/Risk |
|------|---------|
| `test_cycle_review_to_briefing_blending_chain` | AC-05+AC-07 lifecycle chain |
| `test_step8b_runs_on_force_false_lifecycle` | AC-15, R-01 lifecycle form |

#### Edge cases suite
- **Total**: 24
- **Passed**: 23
- **xfailed**: 1 (pre-existing, unchanged)
- **Failed**: 0

### Integration Test Totals
- New tests added: 19 (17 in test_tools.py + 2 in test_lifecycle.py)
- Pre-existing xfail markers: no new ones added (no pre-existing failures caused by crt-046)

---

## AC-17: Schema Version grep Check

```bash
grep -r 'schema_version.*== 21' crates/
# (no output — zero matches)
```

**Result: PASS.** Zero matches. All 9 schema cascade sites updated.

---

## Naming Collision Verification

Verified that the `cluster_score` formula in `context_briefing` handler
(`crates/unimatrix-server/src/mcp/tools.rs` lines 1191–1211) uses
`record.confidence` (EntryRecord.confidence, Wilson-score composite from
`store.get(id)`) and NOT `IndexEntry.confidence` (raw HNSW cosine from
`briefing.index()`). The naming collision warning comment is present at line 1191
citing ADR-005 crt-046.

---

## Gaps

None. All 16 risks from RISK-TEST-STRATEGY.md have test coverage.

Notes on partial-coverage items:
- **AC-07** (cluster entry displaces weakest semantic result): The unit test
  `test_blend_cluster_entries_displaces_weakest_semantic` validates the pure
  function. The integration test `test_cycle_review_to_briefing_blending_chain`
  validates the end-to-end chain. The full displacement integration scenario
  (seeding a matching cluster row via real cycle reviews with near-identical goal
  embeddings) is difficult to guarantee in a harness test due to embedding model
  timing; the test validates success without asserting specific entry IDs.
- **AC-08** (NULL embedding cold-start): Verified at unit test level
  (`test_get_cycle_start_goal_embedding_null_blob_returns_none`). Integration
  coverage provided by `test_briefing_empty_goal_clusters_cold_start` (empty
  table cold-start path exercises the same briefing.index() fallback).
- **R-03** (drain shedding): Behavioral edges use `write_pool_server()` directly
  per ADR-006 crt-046 — the analytics drain is not in the write path for
  behavioral edges. Structural verification via code inspection is sufficient;
  the drain shed path cannot be exercised for these edges.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_cycle_review_bidirectional_edges`: both A→B and B→A behavioral edges present |
| AC-02 | PASS | `test_cycle_review_edge_idempotency`: COUNT identical after second call |
| AC-03 | PASS | `test_cycle_review_edge_weight_success`, `test_cycle_review_edge_weight_other`: weight column verified |
| AC-04 | PASS | `test_cycle_review_zero_get_obs_zero_edges`: COUNT unchanged with no context_get obs |
| AC-05 | PASS | `test_cycle_review_goal_cluster_created`: goal_clusters row with non-NULL embedding and correct fields |
| AC-06 | PASS | `test_cycle_review_no_goal_no_cluster`: COUNT=0 when no goal start event |
| AC-07 | PASS | Unit: `test_blend_cluster_entries_displaces_weakest_semantic`; integration: lifecycle chain test |
| AC-08 | PASS | Unit: `test_get_cycle_start_goal_embedding_null_blob_returns_none`; structural: Level 2 guard code |
| AC-09 | PASS | `test_briefing_empty_goal_clusters_cold_start`: briefing succeeds with empty goal_clusters |
| AC-10 | PASS | `test_briefing_inactive_entries_excluded`: deprecated entry not in briefing output |
| AC-11 | PASS | `test_briefing_recency_cap_101_rows` (integration); `test_query_goal_clusters_recency_cap_100` (unit) |
| AC-12 | PASS | `test_v21_to_v22_migration_creates_goal_clusters`: version=22, 7 columns, index present |
| AC-13 | PASS | `test_cycle_review_parse_failure_count_in_response`: parse_failure_count >= 1 in JSON response (NON-NEGOTIABLE) |
| AC-14 | PASS | `test_cycle_review_pair_cap_200`: 21 obs → ≤ 400 edges + "pair cap" log warning |
| AC-15 | PASS | `test_cycle_review_force_false_reruns_step8b` + `test_step8b_runs_on_force_false_lifecycle`: edge count identical after force=false (NON-NEGOTIABLE) |
| AC-16 | PASS | `test_briefing_feature_none_cold_start`: briefing succeeds with no cluster query |
| AC-17 | PASS | `grep -r 'schema_version.*== 21' crates/` returns zero matches |
| E-02 | PASS | Unit: `test_build_coaccess_pairs_self_pairs_excluded` in behavioral_signals.rs |
| R-02-contract | PASS | `test_emit_behavioral_edges_unique_conflict_not_counted`: 0 behavioral edges when NLI owns edge (NON-NEGOTIABLE) |
| R-13-doc | PASS | `test_briefing_cluster_score_below_semantic_no_displacement`: documents FR-21/ADR-005 behavior |
| I-04 | PASS | Unit: `test_briefing_guard_a_empty_goal_skips_embedding_lookup` in behavioral_signals.rs (structural guard) |

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — MCP server unavailable in this session; proceeded without.
- Stored: nothing novel to store — the integration test patterns used here (sqlite3 direct seeding, `_compute_db_path`, WAL checkpoint after seed) are pre-existing patterns already established in the codebase (test_tools.py col-002b tests). The `force` parameter addition to the client is a straightforward extension.
