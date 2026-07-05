# IMPLEMENTATION-BRIEF — vnc-043

context_graph subgraph: Class-1 doc fix + live depth-1 read. GH Issue [#903](https://github.com/dug-21/unimatrix/issues/903).

> NARROW feature — handler dispatch (~6 lines) + doc text across 4 edit points + a uniform ordering sort + tests. No wire/interface/struct/hot-path change. Single delivery wave.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-043/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-043/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-043/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-043/architecture/ARCHITECTURE.md |
| ADR-001 (depth-1 live dispatch reuse) | product/features/vnc-043/architecture/ADR-001-depth1-live-dispatch-reuse.md |
| ADR-002 (description source-of-truth) | product/features/vnc-043/architecture/ADR-002-description-source-of-truth.md |
| ADR-003 (depth-1 response ordering + truncation) | product/features/vnc-043/architecture/ADR-003-depth1-response-ordering-truncation.md |
| Risk / Test Strategy | product/features/vnc-043/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-043/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-043/ACCEPTANCE-MAP.md |

## Component Map

Pseudocode and test-plan files are produced in Session 2 Stage 3a. This is a single-file code change plus doc edits in two files; the component rows below reflect the change surface from ARCHITECTURE.md § Component Breakdown. Actual file paths filled during delivery.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| subgraph depth-1 dispatch + ordering (`handle_subgraph` / `subgraph_via_db`) | pseudocode/subgraph-depth1-dispatch.md | test-plan/subgraph-depth1-dispatch.md |
| discoverable-contract doc surfaces (schemars docs + twin description literals) | pseudocode/doc-surfaces.md | test-plan/doc-surfaces.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Correct the discoverable `context_graph` contract so agents can find that `edge_types`/`direction` filtering is available on `subgraph` mode (it already ships since vnc-019 #597 — the contract mis-documents it as unavailable, the literal root cause of #903), and route `subgraph` at `max_depth == 1` to the existing live `subgraph_via_db` path so a write committed immediately before the call is visible with no tick lag. Two co-equal Class-1 deliverables — a documentation fix and a dispatch-only code change — with no interface, wire, or hot-path change.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Depth-1 live path implementation | Reuse existing `subgraph_via_db` unconditionally at `max_depth == 1`; no dedicated `subgraph_sql` helper | SCOPE Open Q2, ARCHITECTURE | architecture/ADR-001-depth1-live-dispatch-reuse.md |
| Dispatch insertion point | Exact-match `max_depth == 1` guard after `resolve_supersessions` computed (`graph_read_subgraph.rs:162`), before the lock/snapshot block (`:164`) — depth-1 takes no `TypedGraphState` lock | ARCHITECTURE, SR-07 | architecture/ADR-001-depth1-live-dispatch-reuse.md |
| Description drift prevention | Keep twin-literal + byte-equality guard (`test_graph_tool_attr_description_matches_const`, #869); edit both literals identically; do NOT collapse (rmcp 1.7.0 cannot consume a const in `#[tool(description)]`) | SCOPE Open Q, SR-01 | architecture/ADR-002-description-source-of-truth.md |
| Response ordering | Nodes by ascending `id`; edges by canonical `(source_id, target_id, relation_type)`; applied uniformly to BOTH depth-1 and depth>1 paths | SCOPE Open Q3, SR-03 | architecture/ADR-003-depth1-response-ordering-truncation.md |
| Snapshot pin (Open Q4) | Resolved negative in-repo: NO `.snap`/`insta`/`schema_for` pin on the description string or `GraphParams` schema. Only in-crate substring + byte-equality pins, both handled in-scope | ARCHITECTURE, SR-04 | architecture/ADR-002-description-source-of-truth.md |
| max_nodes / realistic fan-in (Open Q5) | Board caller does NOT raise `max_nodes`; default 200 covers realistic fan-in (depth-1 = seed + one hop, ~199 headroom). "Realistic fan-in" for AC-15 = **≥ 30 incoming `Advances` capabilities**, `truncated == false`; pathological >199 surfaces `truncated == true` | ARCHITECTURE, SR-05 | architecture/ADR-003-depth1-response-ordering-truncation.md |

## Files to Create/Modify

All modifications — no new files (no new crates, no dedicated helper).

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs` | Insert exact `max_depth == 1` → `subgraph_via_db` dispatch after `resolve_supersessions` (~`:162`), before the lock block; add the uniform ordering sort as a final assembly step in BOTH `subgraph_via_db` and `handle_subgraph`'s warm-BFS assembly |
| `crates/unimatrix-server/src/mcp/graph_read.rs` | Edit `direction` schemars doc (`:82`) and `edge_types` schemars doc (`:84`/`:85`) — state both apply to subgraph; drop "neighbors only" |
| `crates/unimatrix-server/src/mcp/tools.rs` | Edit BOTH description literals identically: the `CONTEXT_GRAPH_DESCRIPTION` mirror const (`:76`) and the live `#[tool(description=…)]` literal (`:~3945–3996`) — filter-availability text + depth-1-live staleness carve-out; extend substring assertions (`:6198+`) with the new phrases |

## Data Structures (unchanged — no shape change)

- `GraphParams` — wire-locked (ADR-003 vnc-018). Fields already present: `seed_ids`, `edge_types: Option<Vec<String>>`, `direction: Option<String>`, `max_nodes`, `max_depth: Option<u8>`. Doc-only edits to `direction`/`edge_types` schemars strings.
- `SubgraphResponse` — `{ nodes: Vec<EntryRecord>, edges: Vec<EdgeRecord>, truncated: bool, seed_ids: Vec<u64>, depth_reached: u8 }`. Shape fixed (ADR-004 vnc-019) — no `graph_rebuilt_at`/freshness field.
- `EdgeRecord` — `{ source_id, target_id, relation_type: String, direction: "outgoing", depth: u8, metadata: Option<Value> }`. `direction` filter affects inclusion only, never the canonical `source→target` label.
- `EntryRecord` — hydrated node: id, title, content, status, kind, tags (tags via `load_tags_for_entries`, ADR-006). Depth-1 live must produce an identical field set to the cache path.

## Function Signatures (reused — do not invent new names)

```rust
async fn handle_subgraph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<SubgraphResponse, ErrorData>            // graph_read_subgraph.rs:68

async fn subgraph_via_db(                            // graph_read_subgraph.rs:395 — REUSED depth-1 path
    store: &Store,
    seed_ids: &[u64],
    max_depth: u8,
    max_nodes: u32,
    petgraph_dirs: &[PetgraphDirection],
    edge_types: &[RelationType],
    resolve_supersessions: bool,
) -> Result<SubgraphResponse, ErrorData>
```

Dispatch to insert (`petgraph_dirs` and `edge_types` already resolved above the insertion point):

```rust
if max_depth == 1 {
    return subgraph_via_db(
        store, &seed_ids, max_depth, max_nodes,
        &petgraph_dirs, &edge_types, resolve_supersessions,
    ).await;
}
```

Reused (no change): `query_direct_neighbors` (same edge query `neighbors` depth-1 uses), `fetch_nodes_batch`, `fetch_edge_metadata`, `load_tags_for_entries`, `validate_subgraph_params`.

## Constraints

- `GraphParams` wire layout locked — additive `Option<T>` only; fields already present, no wire change (ADR-003 vnc-018).
- `RelationEdge` and the tick hot path must not be touched (Principle #7). Depth-1 live routing must NOT acquire the `TypedGraphState` lock (A3 / NFR-2).
- Dispatch must be exact `max_depth == 1` match placed BEFORE the `use_fallback` branch, so the depth>1 dispatch path and its cold-start fallback branch (#4562 / GH #623) are unchanged at the SET level — the only addition to depth>1 is the FR-9 presentation-only ordering sort; the returned node/edge SET is unchanged, and prior depth>1 order was arbitrary/undocumented (SR-07, FR-8/FR-9).
- `fetch_edge_metadata` OR-chain stays ≤ `MAX_EDGES_UPPER` (1000) — inherited via `subgraph_via_db`.
- Staleness disclosed in tool-description text only; `SubgraphResponse` shape fixed (ADR-004 vnc-019).
- Both description literals edited identically; `test_graph_tool_attr_description_matches_const` (#869) stays green (SR-01).
- Uniform ordering is presentation-only and set-preserving — runs after the R-05 dangling-edge filter; does not change which nodes/edges are returned.
- File stays under the 500-line limit (dispatch ~6 lines; no split needed).

## Dependencies

- Existing code only — no new crates. `handle_subgraph`, `subgraph_via_db`, `validate_subgraph_params` (`graph_read_subgraph.rs`, `graph_read_validation.rs`); `GraphParams` (`graph_read.rs`); `CONTEXT_GRAPH_DESCRIPTION` + twin literal + byte-equality guard (`tools.rs`); `fetch_nodes_batch`, `fetch_edge_metadata`, `load_tags_for_entries`, `query_direct_neighbors`.
- Precedent mirrored: `handle_neighbors` `depth == 1 → neighbors_sql` dispatch (`graph_read_neighbors.rs:185`).
- ADRs honored: ADR-003 vnc-018 (wire lock), ADR-005 vnc-018 (depth asymmetry), ADR-001/003/004/006 vnc-019 (max_depth, batch hydration, staleness disclosure, tags).

## NOT in Scope

- Accelerating typed-graph currency below the tick window for depth>1 (heal-acceleration / hot path) — reopens ADR-004 vnc-019; separate research spike.
- Depth>1 live read — depth>1 stays cached.
- Adding `edge_types`/`direction` to `chain`/`current`/`neighbors` (neighbors already has both; chain/current are supersession-only).
- Any `RelationEdge` / hot-path struct change or `GraphParams` wire-shape change.
- Adding a `graph_rebuilt_at`/freshness field to `SubgraphResponse` (ADR-004 vnc-019 rejected).
- Re-deriving/re-validating the existing edge_types/direction filter logic beyond confirming the one-shot works and asserting dual-path parity.
- A dedicated depth-1 helper — reuse `subgraph_via_db`.

## Alignment Status

ALIGNMENT-REPORT.md: no FAIL-level or blocking variances. Two WARN items carried for human awareness (both non-blocking):

1. **`goal:self-learning` label overstates strategic contribution.** This is read-surface ergonomics (discoverability + depth-1 freshness for the uni-zero §6 capability-board read), not frontier / capability progress — it closes no capability's `done_when`. The artifact prose is honest (SCOPE, ARCHITECTURE "NARROW feature", SPECIFICATION all frame it accurately); only the GH goal *tag* is the mismatch. Retag to tooling/infra is the human's call — a labeling correction, not an artifact-rework item.

2. **FR-9 / ADR-003 uniform ordering touches the depth>1 path.** SCOPE Goal 3 / AC-02 say depth>1 is "behaviorally unchanged." FR-9 pins a deterministic sort on BOTH depths (the deliberate SR-03 resolution: one ordering contract, not two). **Reconciled as presentation-only: "depth>1 behaviorally unchanged" means the returned SET is unchanged — NOT byte-order-unchanged.** `fetch_nodes_batch` already documented "arbitrary order," so well-written tests compare sets and are unaffected; the tester must sweep existing depth>1 fixed-order assertions and update any as presentation-only.
