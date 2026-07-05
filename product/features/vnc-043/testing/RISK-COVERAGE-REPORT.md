# Risk Coverage Report: vnc-043

context_graph subgraph — Class-1 doc fix + live depth-1 read via `subgraph_via_db` reuse (GH #903).
Stage 3c execution. NARROW feature: dispatch (~6 lines) + four-point doc edit + uniform ordering sort + tests.
No wire/struct/hot-path change.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Dispatch capture error (exact `==1`, before lock; `>1` not routed live) | `test_subgraph_depth1_routes_live`, `test_subgraph_depth2_served_from_cache` (depth 2+10), `test_bfs_max_depth_one_only_direct_neighbors`, `test_validate_max_depth_zero_rejected`, `test_validate_max_depth_boundary_values_accepted`; wire `test_graph_subgraph_depth1_write_then_read_visible` | PASS | Full |
| R-02 | Depth>1 cold-start `use_fallback` fallback broken/bypassed | `test_bfs_cold_start_empty_result`, `test_subgraph_use_fallback_true_with_real_entries_falls_back_to_db` (#623 guard), `test_subgraph_use_fallback_true_direction_incoming_forwarded`, `test_subgraph_depth2_served_from_cache` | PASS | Full |
| R-03 | Dual-path SET divergence (absent/`[]`/explicit edge_types; supersession modes) | `test_subgraph_depth1_set_parity_vs_warm_cache` | PASS | Full |
| R-04 | Promoted load-bearing path latent bug (dedup / dangling / metadata cap) | `test_bfs_direction_both_no_duplicate_edges`, `test_subgraph_depth1_dangling_edge_filtered_under_cap`, `test_bfs_star_topology_near_cap_edges_within_bound` (MAX_EDGES_UPPER); wire `test_graph_subgraph_direction_both_dedup`, `test_graph_subgraph_topology_traversal` | PASS | Full |
| R-05 | Hydration/tag parity break | `test_subgraph_depth1_entryrecord_field_and_tag_parity`; wire `test_graph_subgraph_node_shape_matches_entry_record`, `test_graph_subgraph_edge_record_fields` | PASS | Full |
| R-06 | Ordering non-uniform / breaks fixed-order test | `test_subgraph_depth1_node_and_edge_ordering`, `test_subgraph_depth_gt1_same_ordering_keys`, `test_subgraph_dod_oneshot_deterministic`; wire `test_graph_subgraph_depth1_ordering_deterministic`; + R-06 SWEEP (clean — see below) | PASS | Full |
| R-07 | Four-point doc drift (2 literals byte-guarded + 2 schemars docs) | `test_graph_tool_attr_description_matches_const` (#869), `test_context_graph_description_contains_staleness_text` (extended), `test_graphparams_schemars_docs_state_subgraph_applies` | PASS | Full |
| R-08 | Depth-1 acquires `TypedGraphState` lock | Structural review (see below) — `max_depth==1` early return at `graph_read_subgraph.rs:171` precedes the lock at `:189` | PASS (review) | Full |
| R-09 | Silent truncation at high fan-in | `test_subgraph_depth1_truncated_false_realistic_fanin` (≥30, false), `test_bfs_seed_saturation_sets_truncated` (true), `test_subgraph_depth1_dangling_edge_filtered_under_cap` (true on live path), `test_bfs_not_truncated_under_cap`; wire `test_graph_subgraph_depth1_truncated_false_at_realistic_fanin`, `test_graph_subgraph_truncation_depth_reached` | PASS | Full |
| R-10 | Freshness split not tested both ways | Forward (d1 visible): `test_subgraph_depth1_routes_live` + wire `test_graph_subgraph_depth1_write_then_read_visible`; Reverse (d>1 within-tick NOT visible): `test_subgraph_depth2_served_from_cache` | PASS | Full |
| R-11 | Direction label leak | `test_subgraph_depth1_direction_label_invariant` (incoming/outgoing/both); wire `test_graph_subgraph_direction_outgoing_on_all_edge_records` | PASS | Full |

All 11 risks have full coverage and PASS. No new failures introduced by this feature.

## R-06 Fixed-Order Sweep (mandatory, Stage 3c)

Swept the three sources named in test-plan/OVERVIEW.md §Sweep for index-based (`nodes[0]`, `edges[1]`) order assertions the new uniform sort could flip:

- `crates/unimatrix-server/src/mcp/graph_read_subgraph_bfs_tests.rs` — no depth>1 fixed-order index pin. The `resp.edges[0]` reads in `test_bfs_traverses_supports_edge` and `test_bfs_max_depth_one_only_direct_neighbors` are on **single-edge** fixtures (exactly one edge possible), so index `[0]` is set-safe, not order-dependent. Depth>1 tests (`test_bfs_two_hop_chain_depth_reached_2`, star topology) assert counts + membership, not order.
- `crates/unimatrix-server/tests/graph_subgraph_integration.rs` — all three tests (`test_subgraph_single_hop_five_entries`, `test_subgraph_two_hop_linear_chain`, `test_subgraph_default_direction_both`) use set-membership (`node_ids.contains`, `edge_triples.contains`). No index pin.
- `product/test/infra-001/suites/test_tools.py` + `test_lifecycle.py` (`test_graph_subgraph_*`) — all set-based (triple sets, `node_id_set` membership). No index pin.

**Outcome: sweep clean — zero fixed-order assertions required reframing.** The developer's tests were written set-level from the start; the presentation-only sort (FR-9) changes no returned SET. New ordering-determinism tests were added at both unit and wire level to positively assert the sort.

## R-08 Structural Review (lock-free depth-1)

`handle_subgraph` (`graph_read_subgraph.rs`): the `if max_depth == 1 { return subgraph_via_db(...) }` early return is at `:171`, computed after all filter args resolve and **before** the `typed_graph_state.read()` snapshot block at `:189`. The depth-1 path takes zero `TypedGraphState` lock (A3/NFR-2/AC-10). `subgraph_via_db` reads only `store.read_pool_server()` / `store.write_pool_server()` — no graph-state lock. Confirmed.

Snapshot-pin absence (FR-5/Open Q4) re-confirmed: no `insta`/`assert_snapshot`/`.snap`/`schema_for`-snapshot pins the description string or `GraphParams` schema under `crates/unimatrix-server/`.

## Test Results

### Unit Tests (cargo)
- `unimatrix-server --lib`: **4373 passed, 0 failed, 1 ignored** (includes all `graph_read_subgraph_bfs_tests` Section D depth-1 dispatch/ordering/parity tests, #623 cold-start fallback guards, and all R-07 doc-surface tests). Re-confirmed green after Python-only test additions (no Rust source changed).
- `unimatrix-server --test graph_subgraph_integration`: **3 passed, 0 failed**.
- Full-workspace LINK smoke (`infra-002/check-workspace-link-smoke.sh`, #878 guard): **PASS** — link holds at configured parallelism.

### Integration Tests (infra-001 MCP wire, release binary)
Release binary built (`target/release/unimatrix`); `ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so`.

| Suite | Passed | xfailed | xpassed | Failed | Notes |
|-------|--------|---------|---------|--------|-------|
| smoke (`-m smoke`, MANDATORY gate) | 28 | 0 | 0 | 0 | cross-suite critical paths |
| protocol | 13 | 0 | 0 | 0 | handshake/discovery regression |
| tools | 198 | 1 | 0 | 0 | xfail = pre-existing GH#405 |
| lifecycle | 85 | 6 | 1 | 0 | xfails pre-existing (see below) |
| edge_cases | 23 | 1 | 0 | 0 | xfail = pre-existing GH#111 |

Suites selected per harness plan (feature touches server tool logic + store/retrieval + schema-visible behavior). `confidence`/`contradiction`/`security`/`volume` not run standalone (no such behavior touched); their smoke members ran and passed via the smoke gate.

### New Integration Tests Added (3, all PASS)
1. `test_lifecycle.py::test_graph_subgraph_depth1_write_then_read_visible` — DoD one-shot write-then-read at depth-1, no tick wait (AC-07/AC-01/AC-11 forward).
2. `test_lifecycle.py::test_graph_subgraph_depth1_truncated_false_at_realistic_fanin` — ≥30 incoming `Advances`, `truncated == false`, all present (AC-15).
3. `test_tools.py::test_graph_subgraph_depth1_ordering_deterministic` — depth-1 order (nodes asc id, edges canonical triple), run-twice byte-identical (AC-14).

Fixture note: these strict tests store multiple linked entries. Content strings were drawn from a cross-domain distinct-subject pool to clear the server's `DUPLICATE_THRESHOLD = 0.92` cosine near-duplicate collapse (`services/store_ops.rs`) — near-identical fixtures collapse to one entry id and make `context_edge` self-referential.

## xfail / xpass Triage

No integration failure was caused by this feature. All xfails are **pre-existing, unrelated, with existing GH Issue refs** — no marker added, changed, or removed by this PR (per USAGE-PROTOCOL.md: never fix unrelated failures in a feature PR):

| Test / marker | GH ref | Cause |
|---------------|--------|-------|
| `test_edge_cases.py` (rapid sequential stores) | GH#111 | rate-limit blocks rapid stores |
| `test_tools.py` (deprecated confidence) | GH#405 | deprecated confidence can exceed active (background scoring timing) |
| `test_lifecycle.py` (find_terminal_active multi-hop) | GH#406 | multi-hop terminal traversal not implemented |
| `test_lifecycle.py` tick-interval markers (`:565`, `:1579`, `:2140`) + related | — | tick interval not drivable without `UNIMATRIX_TICK_INTERVAL_SECONDS` |

**1 xpass** in `test_lifecycle.py` chunk 1: a pre-existing non-strict (`strict=False`) tick/timing-dependent xfail marker that incidentally passed this run. Not feature-related; left as-is (removing a pre-existing marker is out of scope for this feature PR — flagged here for the validator/human). **No new GH Issues filed.**

## Gaps

None. Every risk R-01..R-11 in RISK-TEST-STRATEGY.md maps to at least one executed, passing test. The pathological >199-neighbor `truncated == true` wire variant (plan-marked optional) is covered structurally by the unit seed-saturation truncation test and the depth-1 capped dangling-filter test (`truncated == true` on the live path), so no gap remains.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_subgraph_depth1_routes_live`; wire `test_graph_subgraph_depth1_write_then_read_visible` |
| AC-02 | PASS | `test_subgraph_depth2_served_from_cache` (SET-level, depth 2+10 not live); cold-start `test_subgraph_use_fallback_true_*`; depth>1 warm regression `test_bfs_two_hop_chain_depth_reached_2` |
| AC-03 | PASS | depth-1 reuses `subgraph_via_db` → single `query_direct_neighbors` per hop (no per-edge round-trips); `test_subgraph_use_fallback_true_direction_incoming_forwarded` |
| AC-04 | PASS | `test_subgraph_depth1_entryrecord_field_and_tag_parity`; wire node/edge shape tests |
| AC-05 | PASS | `test_subgraph_depth1_set_parity_vs_warm_cache` (absent/`[]`/explicit edge_types) |
| AC-06 | PASS | `test_subgraph_depth1_direction_label_invariant`; wire `test_graph_subgraph_direction_outgoing_on_all_edge_records` |
| AC-07 | PASS | wire `test_graph_subgraph_depth1_write_then_read_visible` (DoD one-shot, incoming Advances, write committed before) |
| AC-08 | PASS | `test_subgraph_depth1_set_parity_vs_warm_cache` (supersession default vs explicit-false parity); `test_validate_unknown_edge_type_rejected` |
| AC-09 | PASS | `test_context_graph_description_contains_staleness_text` (subgraph depth-1-live / depth>1-cache carve-out substrings) |
| AC-10 | PASS | R-08 structural review — no `.read()` on `TypedGraphState` on depth-1 path; no wire/struct change; no snapshot pin |
| AC-11 | PASS | Both ways: forward `test_graph_subgraph_depth1_write_then_read_visible`; reverse `test_subgraph_depth2_served_from_cache` |
| AC-12 | PASS | chain/current/neighbors dispatch regression green (`test_graph_subgraph_chain_mode_rejects_*`, `test_graph_subgraph_neighbors_mode_rejects_max_depth`, boundary validation) |
| AC-13 | PASS | `test_graph_tool_attr_description_matches_const` (#869) + extended substrings + `test_graphparams_schemars_docs_state_subgraph_applies` — all four edit points |
| AC-14 | PASS | `test_subgraph_depth1_node_and_edge_ordering`, `test_subgraph_dod_oneshot_deterministic`; wire `test_graph_subgraph_depth1_ordering_deterministic` |
| AC-15 | PASS | `test_subgraph_depth1_truncated_false_realistic_fanin` (≥30, false) + `test_bfs_seed_saturation_sets_truncated` (true); wire `test_graph_subgraph_depth1_truncated_false_at_realistic_fanin` |
