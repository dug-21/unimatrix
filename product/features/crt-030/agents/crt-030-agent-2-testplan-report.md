# Agent Report: crt-030-agent-2-testplan (Stage 3a — Test Plan Design)

## Output Files

- `/workspaces/unimatrix/product/features/crt-030/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-030/test-plan/graph_ppr.md`
- `/workspaces/unimatrix/product/features/crt-030/test-plan/config_ppr_fields.md`
- `/workspaces/unimatrix/product/features/crt-030/test-plan/search_step_6d.md`

---

## Risk Coverage Mapping

| Risk ID | Priority | Covered By | Test Plan File(s) |
|---------|----------|------------|-------------------|
| R-01 | Deferred | N/A — no scenarios | N/A |
| R-02 | High | `test_step_6d_skipped_when_use_fallback_true`, `test_step_6d_use_fallback_true_no_allocation` | search_step_6d.md |
| R-03 | High | `test_step_6d_blend_weight_zero_leaves_hnsw_unchanged`, PPR-only sim=0.0 test | search_step_6d.md |
| R-04 | High | `test_ppr_sort_covers_all_nodes`, `test_ppr_10k_node_completes_within_budget`, sort placement code review gate | graph_ppr.md |
| R-05 | High | `test_step_6d_fetch_error_silently_skipped`, `test_step_6d_all_fetches_fail_pool_unchanged` | search_step_6d.md |
| R-06 | High | `test_step_6d_entry_at_exact_threshold_not_included`, `test_step_6d_entry_just_above_threshold_included`, `test_ppr_inclusion_threshold_zero_rejected` | search_step_6d.md, config_ppr_fields.md |
| R-07 | Med | `test_ppr_scores_all_finite`, `test_zero_positive_out_degree_no_forward_propagation`, `test_ppr_single_min_positive_seed_no_nan` | graph_ppr.md |
| R-08 | Critical | `test_step_6d_quarantined_entry_not_appended`, `test_step_6d_active_entry_appended`, T-PPR-IT-02 (integration) | search_step_6d.md, OVERVIEW.md |
| R-09 | Med | grep gate (no edges_directed), `test_supersedes_edge_excluded_from_ppr`, `test_contradicts_edge_excluded_from_ppr` | graph_ppr.md |
| R-10 | Med | `test_step_6d_no_phase_affinity_score_direct_call_in_step_6d` (grep gate), `test_step_6d_non_uniform_phase_snapshot_changes_seeds` | search_step_6d.md |
| R-11 | Med | `test_step_6d_blend_weight_one_overwrites_hnsw`, `test_step_6d_blend_weight_one_ppr_only_entry_gets_ppr_score`, `test_ppr_blend_weight_one_valid` | search_step_6d.md, config_ppr_fields.md |
| R-12 | Med | `test_prerequisite_incoming_direction`, `test_prerequisite_wrong_direction_does_not_propagate` | graph_ppr.md |
| R-13 | Med | `test_ppr_dense_50_node_coaccess_completes_under_1ms` | graph_ppr.md |

---

## Integration Suite Plan

Suites to run in Stage 3c:

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory minimum gate |
| `lifecycle` | New `store→search` flow via PPR expansion; T-PPR-IT-01 and T-PPR-IT-02 go here |
| `tools` | `context_search` is a modified tool |
| `security` | R-08 quarantine bypass is security-relevant |

New integration tests planned (T-PPR-IT-01, T-PPR-IT-02, T-PPR-IT-03) are described in
OVERVIEW.md with a dependency condition: if the infra-001 harness cannot write GRAPH_EDGES
rows directly, T-PPR-IT-01 and T-PPR-IT-02 fall back to inline `search_tests.rs` unit tests.
The Stage 3c tester must resolve this at execution time.

---

## Open Questions

1. Can the infra-001 harness write GRAPH_EDGES rows (e.g., via `context_store` with an edge
   type parameter or via a pre-seeded fixture DB)? This determines whether T-PPR-IT-01 and
   T-PPR-IT-02 are infra-001 integration tests or `search_tests.rs` unit tests.
2. Does `populated_server` fixture include any `GRAPH_EDGES` rows? Check `suites/conftest.py`.
3. The R-04 timing test (`test_ppr_10k_node_completes_within_budget`) should be gated with
   `#[ignore]` and run via `cargo test -- --ignored` in release mode. Confirm CI config supports this.

---

## Self-Check

- [x] OVERVIEW.md maps all 13 risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan — suites to run, two new tests specified
- [x] Per-component plans match architecture component boundaries (graph_ppr.rs, config.rs, search.rs Step 6d)
- [x] All high-priority and critical risks have at least two specific test expectations each
- [x] R-08 (critical) has three explicit scenarios plus integration test reference
- [x] Integration tests defined for Step 6d → co-access boundary (AC-11 / I-02)
- [x] All output files within `product/features/crt-030/test-plan/`
- [x] R-01 (Deferred) correctly has zero test scenarios

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced all 9 crt-030 ADRs (#3731–#3740) plus
  relevant patterns (#3740 graph traversal submodule, #264/#315 test gateway pattern, #1607
  SupersessionGraph DAG testing). Results were directly applicable.
- Queried: `context_search` "crt-030 architectural decisions" — confirmed ADR entries #3731–#3739.
- Queried: `context_search` "graph traversal unit testing patterns" — surfaced #3627 (edges_of_type
  boundary) and #1607 (graph test patterns).
- Stored: nothing novel to store — the test patterns in this plan (threshold boundary testing,
  quarantine bypass guard, determinism via sorted-key Vec) are feature-specific combinations.
  If the quarantine-bypass-for-injected-entries pattern generalizes after crt-030 ships, store
  it as a pattern entry at that point (flagged in RISK-TEST-STRATEGY.md knowledge stewardship note).
