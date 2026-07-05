# context_graph subgraph — live depth-1 read (edge-filtered, hydrated one-shot)

> GH Issue #903. Feature ID: vnc-043.

## Problem Statement

The uni-zero §6 capability-board / frontier query wants a single graph read that is simultaneously **edge-type-filtered**, **node-hydrated**, and **live** (reflects writes committed before the call). A sibling Unimatrix deployment hit a context overflow assembling this by hand during uni-zero orientation: neighbor-queries + bulk lookups + a client-side id-join.

`subgraph` mode is the right home — it already does multi-hop BFS (`max_depth` 1..=10, ADR-001 vnc-019), batch node hydration (ADR-003 vnc-019), and — verified against the code — **already honors `edge_types` and `direction` filtering** (delivered in vnc-019 #597, not a gap as the issue states; see Background Research). The one remaining gap is **freshness at depth-1**: subgraph always reads the in-memory tick-cache (rebuilt every 30–60s; ADR-004 vnc-019 accepted this), so a curator who writes a capability status and immediately re-queries the board inside the tick window sees stale status. `neighbors` mode solved exactly this by reading the live DB at depth=1 and the cache at depth>1 (ADR-005 vnc-018). This feature extends that established asymmetry into subgraph.

## Goals

**Two co-equal Class-1 (primary) deliverables — a documentation fix and a code change:**

1. **(DOC — closes the origin problem)** Correct the discoverable contract so agents can find that `edge_types`/`direction` filtering is available on subgraph. The root cause of #903 is that the *contract mis-documents the filter as unavailable* even though the code already honors it. Fix all three discoverable surfaces:
   - `edge_types` schemars doc in `GraphParams` (`graph_read.rs:85`) — currently "neighbors only: …"; must state it also applies to subgraph.
   - `direction` schemars doc (`graph_read.rs:82`) — currently lists chain + neighbors only; must include subgraph.
   - The subgraph section of `CONTEXT_GRAPH_DESCRIPTION` (`tools.rs`) **and its mirror-const duplicate** (the zero-reviewer flagged the two copies) — state that edge_types/direction filtering is honored on subgraph.
2. **(CODE)** At `max_depth == 1`, subgraph resolves the neighborhood from the **live DB** — reflecting every write committed before the call — preserving edge_types/direction filtering and batch node hydration. Implemented by **reusing the existing `subgraph_via_db` path** (no dedicated helper), routing `max_depth == 1` to it unconditionally, mirroring `handle_neighbors`' `depth == 1` dispatch. **No interface/wire change** — `GraphParams` already carries every field; this is dispatch + doc only.
3. Keep `max_depth > 1` exactly as today: cached BFS over the in-memory `TypedRelationGraph` (with the existing `use_fallback` → live-DB cold-start path).
4. Update the subgraph tool-description **staleness** text (distinct from the filter-availability fix in Goal 1) to state the depth-1 = live / depth>1 = cache split.
5. Deliver and test the DoD one-shot: `subgraph, seed=[goal], max_depth=1, edge_types=["Advances"], direction="incoming"` returns exactly the incoming `Advances` capabilities, hydrated, live, in a **stable documented order**, with `truncated == false` at realistic goal fan-in — no client-side filter, no tick lag.

## Non-Goals

- **NOT** accelerating typed-graph currency below the tick window for depth>1 (heal-acceleration / the hot path). That reopens ADR-004 vnc-019 and carries hot-path blast radius — tracked as its own research spike.
- **NOT** a depth>1 live read. Live 10-hop BFS per query is intrinsically the cache's job; depth>1 stays cached.
- **NOT** adding `edge_types`/`direction` filtering to `chain` / `current` / `neighbors`. (`neighbors` already has both; chain/current are supersession-only.)
- **NOT** changing `RelationEdge` or any hot-path struct (Principle #7), and **NOT** any `GraphParams` wire-shape change (ADR-003 vnc-018 forward-compat lock — fields already present).
- **NOT** adding a `graph_rebuilt_at` / freshness field to `SubgraphResponse` (ADR-004 vnc-019 rejected this; unchanged here).
- **NOT** re-deriving or re-validating the existing edge_types/direction filter logic beyond confirming the one-shot works (it is already implemented and covered).

## Background Research

Grounded in the current code, not assumptions:

**Gap 1 in the issue (edge_types/direction filtering on subgraph) is already delivered.** `handle_subgraph` (`crates/unimatrix-server/src/mcp/graph_read_subgraph.rs`) parses `direction` (incoming/outgoing/both) and `edge_types` (validated against `RelationType`, absent/`[]` = all non-Supersedes) and applies both in the BFS traversal loop and in the live-DB fallback. `validate_subgraph_params` (`graph_read_validation.rs`) does **not** reject either field. Git blame: both were added in the original vnc-019 delivery (commit `24d0794`, #597, 2026-05-20). The tool description already advertises the filter. Therefore the "client-side filter" the issue wants to remove is *already* unnecessary today via the cache path — the only residual problem is the depth-1 freshness lag. The DoD one-shot's filter half works now; its **live** half does not.

**The live path already exists as a function.** `subgraph_via_db` (same file) is a full live-SQL BFS: it issues `query_direct_neighbors` (the same edge query `neighbors_sql` uses at depth=1), applies `edge_types`/`direction`, dedups by canonical triple (R-02), runs the post-BFS dangling-edge filter (R-05), and hydrates via `fetch_nodes_batch` + `fetch_edge_metadata` (ADR-003). It is currently invoked **only** when `use_fallback == true` (cold-start / cycle-detected, GH #623). Routing `max_depth == 1` to this same live path — regardless of `use_fallback` — yields live + filtered + hydrated depth-1 with no new SQL shape and no hot-path touch. This is the direct analog of `handle_neighbors`'s `depth == 1 → neighbors_sql` dispatch.

**The asymmetry is an established convention, not a new contract.** ADR-005 vnc-018 (#4479): neighbors depth=1 = live SQL (point-lookup freshness), depth>1 = in-memory BFS (tick-window staleness). "The asymmetry goes in the expected direction — depth=1 must be fresh; depth>1 tolerates a tick window." vnc-043 extends the identical rule into subgraph.

**Staleness is disclosed via tool-description text only.** ADR-004 vnc-019 (#4493) deliberately kept freshness out of `SubgraphResponse` (no `graph_rebuilt_at`). This feature only edits the description text; it does not reopen that decision. Current subgraph description (`tools.rs` `CONTEXT_GRAPH_DESCRIPTION`, ~lines 89–100) says "subgraph mode uses the in-memory graph cache for BFS traversal … same staleness contract as neighbors mode at depth>1" — this text must gain the depth-1 = live carve-out.

**Hydration contract to preserve.** `fetch_nodes_batch` returns id, title, content, status, kind, tags (tags via `load_tags_for_entries`, ADR-006). Depth-1 live path must produce the identical hydrated `EntryRecord` set.

**GraphParams is wire-locked but already carries the fields.** `edge_types: Option<Vec<String>>`, `direction: Option<String>`, `seed_ids`, `max_nodes`, `max_depth: Option<u8>` are all present (`graph_read.rs`). No field add/remove/retype — this is handler dispatch wiring plus tests plus doc text.

## Proposed Approach

**Doc fix (Goal 1).** Correct the three discoverable surfaces so `edge_types`/`direction` read as subgraph-supported: the two schemars field docs in `GraphParams` and the subgraph section of `CONTEXT_GRAPH_DESCRIPTION` plus its mirror-const duplicate. Text-only; no schema shape change. Verify Open Q4 (snapshot pinning) before editing the description string.

**Code (Goal 2).** Mirror the `neighbors` dispatch inside `handle_subgraph`: after parameter validation, when `max_depth == 1`, delegate to the existing `subgraph_via_db` **unconditionally** — before the `use_fallback` branch (Open Q2 resolved: reuse, no dedicated helper). When `max_depth > 1`, keep the current logic verbatim (cached BFS, `use_fallback` → live fallback). Rationale for reuse: `subgraph_via_db` already satisfies every non-freshness contract (filter, dedup, dangling filter, hydration, metadata, max_nodes) and issues the exact edge query the issue names — reuse avoids a second code path and keeps blast radius at dispatch level.

**Ordering (Open Q3 resolved).** Depth-1 live may reorder `nodes`/`edges` vs the cached path. Pin a stable, documented ordering (e.g. by ascending `id` for nodes, canonical `(source_id, target_id, relation_type)` for edges) so the DoD one-shot is deterministic and testable.

**Tests.** DoD one-shot (filtered + hydrated + live + deterministic order + `truncated == false` at realistic fan-in), depth-1 freshness (write-then-read visible), depth>1 staleness (within-tick write not visible), and the doc surfaces exercised where practical.

## Acceptance Criteria

- AC-01: `subgraph` with `max_depth == 1` resolves the neighborhood from the live DB and reflects every edge/node write committed before the call — no tick lag.
- AC-02: `subgraph` with `max_depth > 1` is behaviorally unchanged: cached BFS over `TypedRelationGraph`, with the existing `use_fallback` → live cold-start path intact.
- AC-03: the depth-1 live path issues the same single `query_direct_neighbors` edge query `neighbors` depth=1 uses — no per-edge round-trips.
- AC-04: depth-1 nodes are hydrated via the existing post-BFS batch pattern (id, title, content, status, kind, tags) — identical `EntryRecord` shape to the cached path.
- AC-05: `edge_types` is honored at depth-1 — only the given relation types are traversed/returned; absent/`[]` = all types except Supersedes.
- AC-06: `direction` (incoming/outgoing/both) is honored at depth-1; returned `EdgeRecord`s keep canonical `source_id → target_id` with `direction: "outgoing"` (filter affects inclusion, not label).
- AC-07: DoD one-shot — `subgraph, seed_ids:[goal], max_depth:1, edge_types:["Advances"], direction:"incoming"` — returns exactly the incoming `Advances` capabilities, hydrated, reflecting an edge/status write committed immediately before the call, with no client-side filter.
- AC-08: `resolve_supersessions` still defaults true and behaves identically on the depth-1 live path (raw as-stored on explicit `false`).
- AC-09: subgraph tool-description **staleness** text updated: depth-1 = live DB (all committed writes visible); depth>1 = tick-cache (30–60s).
- AC-10: no change to `RelationEdge` or any hot-path struct (Principle #7); no `GraphParams` wire-shape change.
- AC-11: a freshness test writes an edge then immediately queries subgraph depth-1 asserting the edge **appears**; a depth>1 test still asserts a within-tick edge does **not** appear (staleness contract preserved).
- AC-12: `chain` / `current` / `neighbors` filtering and dispatch are unchanged — no new acceptance or rejection wiring on those modes.
- AC-13: (DOC — Goal 1) the `edge_types` schemars doc (`graph_read.rs:85`) and `direction` schemars doc (`graph_read.rs:82`) state that both apply to subgraph; the subgraph section of `CONTEXT_GRAPH_DESCRIPTION` **and its mirror-const duplicate** state that edge_types/direction filtering is honored on subgraph. Both copies stay in sync.
- AC-14: depth-1 subgraph results are returned in a stable, documented order (nodes and edges), so the DoD one-shot is deterministic across runs.
- AC-15: (no-silent-truncation) the DoD one-shot returns `truncated == false` at realistic goal fan-in, and `truncated` is surfaced/asserted so a capped board read is never silently partial.

## Constraints

- `GraphParams` wire layout is locked (ADR-003 vnc-018): additive `Option<T>` only; the needed fields already exist — no wire change permitted.
- `RelationEdge` and the tick hot path must not be touched (Principle #7 / issue).
- `fetch_edge_metadata` OR-chain must stay within `MAX_EDGES_UPPER` (1000) — the depth-1 live path already respects this via `subgraph_via_db`.
- `TypedGraphState` is `std::sync::RwLock` — read with `.read().unwrap_or_else(...)`; clone graph + `use_fallback` under one guard before any async work (GH #623). Depth-1 live routing should not need the lock at all.
- Staleness may only be disclosed in tool-description text; `SubgraphResponse` shape is fixed (ADR-004 vnc-019).
- Behavioral split (depth-1 live vs depth>1 cache) must be explicitly tested per the ADR-005 vnc-018 precedent (SR-02 staleness test model).

## Open Questions

Q1–Q3 resolved by coordinator; retained for provenance.

1. **RESOLVED — issue premise correction.** Gap 1 (edge_types/direction filtering) already ships (vnc-019 #597). Human approved narrowing: vnc-043 = live depth-1 code change + the doc fix that actually closes the discoverability origin problem. Filter logic itself is unchanged (verified by the DoD one-shot + regression coverage).
2. **RESOLVED — reuse.** Route `max_depth == 1` to the existing `subgraph_via_db` unconditionally; no dedicated depth-1 helper.
3. **RESOLVED — pin ordering.** Depth-1 live output must be returned in a stable, documented order (nodes + edges) so the one-shot is deterministic (AC-14).
4. **OPEN — snapshot pinning.** Confirm no external doc/schema/description snapshot test pins the exact `CONTEXT_GRAPH_DESCRIPTION` string (or the `GraphParams` JSON schema) before editing the doc surfaces (Goal 1 / AC-13). If one exists, update it in-scope.
5. **OPEN — max_nodes at depth-1.** `subgraph_via_db` honors `max_nodes` and sets `truncated`. Confirm depth-1 truncation semantics (seed + one hop capped at `max_nodes`) match the board query's expectation at scale; AC-15 asserts `truncated == false` at realistic fan-in, but a very high-fan-in goal could still cap — decide whether the board caller should raise `max_nodes` or accept truncation signalling.

## Tracking

GH Issue #903 — https://github.com/dug-21/unimatrix/issues/903 (design Session 1 complete).
