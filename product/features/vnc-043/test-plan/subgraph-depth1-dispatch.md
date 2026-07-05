# Test Plan — Component: subgraph depth-1 dispatch + uniform ordering

Component: `handle_subgraph` (`graph_read_subgraph.rs:68`) + reused `subgraph_via_db` (`:395`).
Change under test: exact `max_depth == 1` early return to `subgraph_via_db` inserted after
`resolve_supersessions` (~`:162`), before the lock block; uniform ordering sort applied in BOTH
`subgraph_via_db` and the warm-BFS assembly.

Home suites (extend, do not fork):
- Unit: `crates/unimatrix-server/src/mcp/graph_read_subgraph_bfs_tests.rs` (async, real store + graph state)
- Integration: `crates/unimatrix-server/tests/graph_subgraph_integration.rs`
- MCP wire: `product/test/infra-001/suites/test_tools.py`, `test_lifecycle.py`

Test naming: `test_subgraph_{concept}_{expected}` (unit) / `test_graph_subgraph_{behavior}` (wire).
Arrange/Act/Assert; `#[tokio::test]` for async.

---

## R-01 — Dispatch capture boundary (High) → AC-01, AC-02, AC-12

- **test_subgraph_depth1_routes_live** — Arrange: store 2 entries + 1 edge directly in the DB
  (bypassing tick rebuild) with an EMPTY/warm `TypedGraphState` that does NOT contain the edge.
  Act: `handle_subgraph` with `max_depth == 1`. Assert: the DB-committed edge/node IS returned —
  proves the depth-1 result came from live SQL, not the cache. (Path assertion via observable
  freshness, not mocking.)
- **test_subgraph_depth2_served_from_cache** — Arrange: warm `TypedGraphState` populated; write an
  extra edge to DB only (not in cache). Act: `max_depth == 2`. Assert: the DB-only edge is NOT
  present → depth>1 did not route live. Repeat with `max_depth == 10`.
- **test_subgraph_max_depth_zero_and_absent_boundary** — exact `==1` never captures these: `max_depth==0`
  rejected by existing validation (`test_validate_max_depth_zero_rejected` already covers reject);
  absent → default (3) → cache path. Assert dispatch does not early-return for either.
- Regression: `test_bfs_max_depth_one_only_direct_neighbors` stays green (depth-1 = seed + one hop set).
- **Coverage**: exact `==1` proven live at 1; not-live at 2 and 10; boundary 0/absent unaffected.

## R-02 — Depth>1 cold-start fallback intact (High) → AC-02

- **test_bfs_cold_start_empty_result** (existing) — confirm still green: depth>1 on empty
  `TypedRelationGraph`.
- **test_subgraph_use_fallback_true_with_real_entries_falls_back_to_db** (existing) — REGRESSION guard:
  the early return must be placed BEFORE the lock so this depth>1 `use_fallback` branch still fires.
- **test_subgraph_depth_gt1_empty_graph_falls_back_to_live** (add if not already asserting the SET) —
  depth==2 against an empty cache with real DB entries returns the live neighborhood (not empty),
  proving #4562/#623 regression is not reintroduced by the insertion.
- **Coverage**: cold-start fallback fires at depth>1 post-insertion; warm depth>1 non-regression.

## R-03 — Dual-path SET parity (Critical) → AC-05, AC-08

- **test_subgraph_depth1_set_parity_vs_warm_cache** — Arrange: warm+fresh graph (cache == DB, no
  within-tick writes). Act: run same seed+filter at depth-1 (live) and capture the depth>1/warm-cache
  result for the same one-hop neighborhood. Assert: node SET equal, edge SET equal (order-independent —
  compare `HashSet` of ids / canonical triples). Sub-cases in one parametrized test or three:
  - `edge_types` absent vs `[]` → both = all types except Supersedes, identical on both paths (AC-05).
  - explicit `edge_types:["Advances"]` → only Advances, identical on both paths.
  - `resolve_supersessions` default-true vs explicit-false → identical resolution on depth-1 live (AC-08).
- **Coverage**: set-equality across absent/empty/explicit filter + both supersession modes. Freshness
  is the ONLY permitted difference.

## R-04 — Promoted load-bearing path latent bug (Critical) → (regression on d1)

- **test_bfs_direction_both_no_duplicate_edges** (existing) — confirm it runs ON the depth-1 live path:
  `direction:both` on one physical bidirectional-eligible edge → exactly one `EdgeRecord`, no dup.
- **test_subgraph_depth1_dangling_edge_filtered_under_cap** — Arrange: fan-in that hits `max_nodes`
  mid-hop so some target nodes are dropped. Act: depth-1. Assert: no `EdgeRecord` points at a dropped
  node id (post-cap dangling filter applied). 
- **test_subgraph_depth1_metadata_or_chain_within_max_edges_upper** — high edge count → `fetch_edge_metadata`
  OR-chain stays ≤ `MAX_EDGES_UPPER` (1000); no over-cap query / no panic. (May reuse an existing
  star-topology fixture, e.g. `test_bfs_star_topology_near_cap_edges_within_bound`.)
- **Coverage**: dedup, dangling-filter, metadata-cap each exercised on the depth-1 live path.

## R-05 — Hydration / tag parity (Medium) → AC-04

- **test_subgraph_depth1_entryrecord_field_and_tag_parity** — Arrange: a tagged fixture node. Act:
  depth-1 live hydration. Assert: returned `EntryRecord` carries id, title, content, status, kind, tags —
  field-for-field equal to the cache-path hydration of the same node (tags via `load_tags_for_entries`).
- Wire regression: `test_graph_subgraph_node_shape_matches_entry_record` (test_tools.py) stays green.
- **Coverage**: full `EntryRecord` field + tag parity on a tagged node.

## R-06 — Uniform ordering, both depths (High) → AC-14

- **test_subgraph_depth1_node_and_edge_ordering** — depth-1 result: `nodes` strictly ascending by `id`;
  `edges` ascending by `(source_id, target_id, relation_type)`. Assert on a fixture with ≥3 nodes /
  ≥3 edges whose insertion order differs from sorted order.
- **test_subgraph_depth_gt1_same_ordering_keys** — depth>1 warm result carries the SAME ordering keys
  (one contract across paths).
- **test_subgraph_dod_oneshot_deterministic** — run the DoD one-shot twice; assert byte-identical
  serialized `nodes`/`edges` order (NFR-4, no flake).
- **SWEEP (mandatory, 3c)**: audit `graph_read_subgraph_bfs_tests.rs`, `graph_subgraph_integration.rs`,
  and the python `test_graph_subgraph_*` for index-based order assertions; reframe any as set-level.
  Most already use set membership.
- **Coverage**: deterministic order proven on both depths; no surviving fixed-order assertion the sort flips.

## R-08 — No lock on depth-1 (Medium) → AC-10

- **Structural review checklist (documented in RISK-COVERAGE-REPORT)**: the `max_depth == 1` early
  return precedes the lock/snapshot block; no `.read()` on `TypedGraphState` on the depth-1 path.
  Verify by reading the final dispatch source; no runtime test asserts absence of a lock cleanly, so
  this is a review gate item (A3/NFR-2/AC-10).

## R-09 — Truncation surfaced (Medium) → AC-15

- **test_subgraph_depth1_truncated_false_realistic_fanin** — ≥30 incoming `Advances` on the seed →
  all present, `truncated == false`.
- **test_bfs_seed_saturation_sets_truncated** (existing) / **test_bfs_not_truncated_under_cap** (existing) —
  confirm the >cap path still sets `truncated == true`; add a depth-1 >199-neighbor variant if not
  already exercised on the live path.
- **Coverage**: `truncated` asserted both false (≥30 realistic) and true (>199 over-cap).

## R-10 — Freshness both ways (High) → AC-01, AC-11

- **test_subgraph_depth1_write_then_read_visible** (integration / wire, one module) — write edge →
  immediately query depth-1 → edge appears (AC-11 forward, AC-01). Best placed at the wire level
  (test_lifecycle.py) so it exercises the real committed-before-call path with no tick dependency.
- **test_subgraph_depth_gt1_within_tick_write_not_visible** (unit) — Arrange: WARM
  `TypedRelationGraph` built WITHOUT the new edge; write the edge to DB only; Act: depth>2 query;
  Assert: the new edge does NOT appear (served from stale warm cache, staleness preserved). NB: must
  use a warm (non-empty) cache — on an EMPTY cache depth>1 falls back to live and would show the write,
  masking the assertion.
- **Coverage**: both directions asserted in one test module (mandatory, ADR-005 precedent).

## R-11 — Direction label invariant (Medium) → AC-06

- **test_subgraph_depth1_direction_label_invariant** — for `direction` in {incoming, outgoing, both}
  at depth-1: inclusion SET changes as expected, but every `EdgeRecord` keeps canonical
  `source_id → target_id` with `direction:"outgoing"` label (never rewritten to reflect the filter).
- Reuse pattern from existing `test_bfs_edge_direction_always_outgoing`; extend to incoming/both.
- Wire regression: `test_graph_subgraph_direction_outgoing_on_all_edge_records` (test_tools.py) green.
- **Coverage**: label invariant across all three direction values at depth-1.

## Edge cases (from Risk Strategy)

- Empty `seed_ids` → validation error, no panic (`test_subgraph_params_empty_seed_ids_is_invalid` /
  `test_validate_seed_ids_empty_returns_error` — existing, confirm on d1 path).
- Non-existent seed id → empty neighborhood, not error (`test_graph_subgraph_unknown_seed_empty_result`).
- `max_nodes == 0` → truncation/empty consistent with `subgraph_via_db` today.
- Self-loop / duplicate `seed_ids` → dedup yields no duplicate nodes/edges.
- Unknown `edge_types` value → still rejected on d1 (`test_validate_unknown_edge_type_rejected`).

## AC coverage from this component

AC-01, AC-02, AC-03, AC-04, AC-05, AC-06, AC-07, AC-08, AC-10 (lock review), AC-11, AC-12, AC-14, AC-15.
(AC-09, AC-13 doc surfaces → `doc-surfaces.md`.)
