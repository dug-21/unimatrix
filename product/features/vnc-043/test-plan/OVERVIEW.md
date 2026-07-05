# Test Plan Overview — vnc-043

context_graph subgraph: Class-1 doc fix + live depth-1 read via `subgraph_via_db` reuse (GH #903).
NARROW feature. Test effort is proportionate — concentrated on the two structural hazards
(promoted load-bearing depth-1 path, four-point doc drift), NOT on re-validating already-shipped
`edge_types`/`direction` filter logic.

## Test Strategy

Three layers, mapped to where each risk is observable:

| Layer | Where | Covers |
|-------|-------|--------|
| Unit (in-crate, `#[tokio::test]`) | `crates/unimatrix-server/src/mcp/graph_read_subgraph_bfs_tests.rs`, `..._tests.rs`, `tools.rs` `#[cfg(test)]`, `graph_read_tests.rs` | dispatch boundary, dedup/dangling/hydration/truncation on the depth-1 live path, ordering keys, doc substrings + byte-equality, schemars-doc presence, depth>1 warm-cache staleness |
| Integration (in-crate `tests/`) | `crates/unimatrix-server/tests/graph_subgraph_integration.rs` | end-to-end `handle_subgraph` through a real store: depth-1 vs depth>1 SET parity, cold-start fallback, DoD one-shot write-then-read |
| Integration (MCP harness, infra-001) | `product/test/infra-001/suites/test_tools.py`, `test_lifecycle.py` | subgraph behavior through the JSON-RPC wire: filter/direction/dedup/hydration shape, depth-1 freshness (write→read visible), truncation |

Guiding rule: extend the existing subgraph test modules — do NOT create parallel scaffolding
(CLAUDE.md test-infra-cumulative rule). The unit-level suites `graph_read_subgraph_bfs_tests.rs`
and the integration file `graph_subgraph_integration.rs` already exercise `subgraph_via_db`; new
cases are added there.

## Component ↔ Test-Plan Map

| Component (pseudocode) | Test-plan file |
|------------------------|----------------|
| `handle_subgraph` / `subgraph_via_db` depth-1 dispatch + uniform ordering | `subgraph-depth1-dispatch.md` |
| schemars field docs + twin description literals + byte-equality guard | `doc-surfaces.md` |

## Risk → Test Mapping (from RISK-TEST-STRATEGY.md)

| Risk | Priority | Primary test(s) | Layer | Plan file |
|------|----------|-----------------|-------|-----------|
| R-01 dispatch capture (exact `==1`, before lock; `>1` not routed live) | High | `test_subgraph_depth1_routes_live`, `test_subgraph_depth2_and_10_served_from_cache`, boundary `==0`/absent | unit + integration | dispatch |
| R-02 depth>1 cold-start fallback intact | High | `test_bfs_cold_start_empty_result`, `test_subgraph_use_fallback_true_with_real_entries_falls_back_to_db` (regression) + new depth>1-on-empty-graph fallback assertion | unit | dispatch |
| R-03 dual-path SET divergence | Critical | `test_subgraph_depth1_set_parity_vs_warm_cache` (absent/`[]`/explicit edge_types; supersession default vs false) | unit/integration | dispatch |
| R-04 promoted load-bearing path latent bug | Critical | `test_bfs_direction_both_no_duplicate_edges` (dedup, ON d1), dangling-filter under `max_nodes` cap, `MAX_EDGES_UPPER` metadata cap | unit | dispatch |
| R-05 hydration/tag parity | Medium | `test_subgraph_depth1_entryrecord_field_and_tag_parity` (tagged fixture) | unit/integration | dispatch |
| R-06 ordering non-uniform / breaks fixed-order test | High | `test_subgraph_depth1_node_and_edge_ordering`, depth>1 same keys, DoD run-twice determinism; SWEEP existing fixed-order asserts | unit + integration | dispatch |
| R-07 four-point doc drift | Critical | `test_graph_tool_attr_description_matches_const` (#869), extended `test_context_graph_description_contains_staleness_text`, new schemars-doc presence check | unit | doc-surfaces |
| R-08 depth-1 acquires lock | Medium | structural/path review checklist: no `.read()` on `TypedGraphState` before the early return | review | dispatch |
| R-09 silent truncation | Medium | `truncated==false` at ≥30 fan-in; `truncated==true` at >199 fixture | unit + integration | dispatch |
| R-10 freshness both ways | High | `test_subgraph_depth1_write_then_read_visible` (forward) + `test_subgraph_depth_gt1_within_tick_write_not_visible` (warm cache) | integration + unit | dispatch |
| R-11 direction label leak | Medium | `test_bfs_edge_direction_always_outgoing` extended: incoming/outgoing/both keep canonical `source→target`, `direction:"outgoing"` | unit | dispatch |

Every AC-01..AC-15 in ACCEPTANCE-MAP.md maps to at least one row above; per-AC detail lives in the
two component files.

## Cross-Component Test Dependencies

- Ordering (R-06/FR-9) is applied in BOTH `subgraph_via_db` and `handle_subgraph`'s warm-BFS
  assembly; the ordering test must assert the SAME keys on the depth-1 live result AND a depth>1
  warm result — one contract, two call sites.
- Doc-surface tests (doc-surfaces.md) are independent of code dispatch but gate the same PR; the
  byte-equality guard must stay green after BOTH literals are edited identically.
- SET-parity (R-03) depends on a warm+fresh `TypedRelationGraph` fixture (no within-tick writes) so
  the only permitted depth-1-vs-cache difference is freshness.

## Integration Harness Plan (infra-001)

### Suites that apply (per suite-selection table — feature touches server tool logic + store/retrieval + schema-visible behavior)

| Suite | Run? | Why |
|-------|------|-----|
| `smoke` (`-m smoke`) | MANDATORY gate | minimum path coverage |
| `test_tools.py` | YES | all existing `test_graph_subgraph_*` cases must stay green through the wire (dispatch/ordering must not change subgraph SET or record shape); new depth-1 freshness + truncation cases land here or in lifecycle |
| `test_lifecycle.py` | YES | multi-step write→read; depth-1 freshness (AC-01/AC-11 forward) and `test_graph_subgraph_*` topology/truncation live here |
| `protocol` | YES | any server tool logic change — handshake/tool-discovery unaffected but run as regression |
| `edge_cases` | YES | empty/unknown seed, `max_nodes==0`, unicode-tagged hydration on depth-1 path |
| `confidence`, `contradiction`, `security`, `volume` | NO (unless smoke touches) | feature adds no confidence/contradiction/security/volume behavior; read-only, `require_cap(Read)` unchanged |

### Existing coverage (no new test needed — regression only)

- `test_graph_subgraph_direction_both_dedup` (test_tools.py:4171) → R-04 dedup through the wire.
- `test_graph_subgraph_direction_outgoing_on_all_edge_records` (:4197) → R-11 label invariant.
- `test_graph_subgraph_node_shape_matches_entry_record` (:4052) / `test_graph_subgraph_edge_record_fields` (:4076) → R-05 hydration shape.
- `test_graph_subgraph_unknown_seed_empty_result` (:4223), `..._max_depth_boundary_0/11_rejected`, `..._unknown_edge_type_rejected` → validation unchanged on the depth-1 path.
- `test_graph_subgraph_topology_traversal` / `..._depth_reached_accuracy` / `..._truncation_depth_reached` (test_lifecycle.py) → topology + truncation.

### Gaps — new MCP-level tests to ADD in Stage 3c

1. **`test_graph_subgraph_depth1_write_then_read_visible`** (test_lifecycle.py) — store two entries, `context_edge` an `Advances` edge, then IMMEDIATELY `context_graph subgraph seed_ids:[target] max_depth:1 direction:incoming edge_types:["Advances"]`; assert the just-written edge + source node present with NO tick wait. This is the AC-07 DoD one-shot + AC-01/AC-11-forward, only observable through the wire (no tick sleep). (Existing subgraph wire tests do not write-then-read at depth-1.)
2. **`test_graph_subgraph_depth1_truncated_false_at_realistic_fanin`** (test_lifecycle.py) — seed a goal with ≥30 incoming `Advances` capabilities; assert all present and `truncated == false` (AC-15 realistic). Pathological >199 truncation is unit-covered (`test_bfs_seed_saturation_sets_truncated`); a wire >199 variant is optional if a fixture is cheap.
3. **`test_graph_subgraph_depth1_ordering_deterministic`** (test_tools.py) — run the DoD one-shot twice, assert byte-identical `nodes`/`edges` order (nodes asc by id, edges by `(source,target,relation_type)`) — AC-14 through the wire.

### xfail / triage note

No integration failure is expected from this change (SET + record shape are preserved). If an
existing `test_graph_subgraph_*` red-bars, triage per USAGE-PROTOCOL.md: (a) caused by this feature
(ordering changed an assertion that pinned arbitrary order) → the assertion was set-safe or must be
reframed set-level, fix in-scope; (b) pre-existing/unrelated → file GH Issue + `@pytest.mark.xfail`,
do NOT fix in this PR.

## Sweep Requirement (R-06 / AC-02 — MANDATORY in 3c)

The uniform ordering (FR-9) touches the depth>1 path. Before execution, sweep these for a FIXED
output-order assertion (index-based `nodes[0].id == …` / `edges[1] == …`) and reframe any hit as
set-level (membership), documenting each as presentation-only:
- `crates/unimatrix-server/src/mcp/graph_read_subgraph_bfs_tests.rs` (all `test_bfs_*`)
- `crates/unimatrix-server/tests/graph_subgraph_integration.rs` (`test_subgraph_single_hop_five_entries`, `test_subgraph_two_hop_linear_chain`, `test_subgraph_default_direction_both`)
- `product/test/infra-001/suites/test_tools.py` / `test_lifecycle.py` (`test_graph_subgraph_*`)
Most already compare sets ("must be in nodes"); confirm and reframe only genuine index pins.

## Knowledge Stewardship

- Queried: `context_briefing` + `context_search` — ADR-001/002/003 vnc-043 (#5448/#5449/#5450),
  ADR-005 vnc-018 depth-asymmetry (#4479), ADR-004 vnc-019 staleness-in-text (#4493); lessons #4562
  (handle_subgraph `use_fallback` cold-start) and #4526 (context_edge no rebuild → stale BFS). #4562
  and #4526 directly informed the R-02/R-10 warm-vs-cold fixture design.
- Stored: nothing novel — the dual-path parity, byte-equality drift-guard, and depth-asymmetry
  freshness patterns are already recorded (#5448/#5449/#5450, #4479, #5396); no new 2+-feature
  pattern emerged from test-plan design alone.
