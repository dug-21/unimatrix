# SPECIFICATION — vnc-043

context_graph subgraph: Class-1 doc fix + live depth-1 read (GH #903)

## Objective

Correct the discoverable `context_graph` contract so agents can find that `edge_types`/`direction` filtering is available on `subgraph` mode (it already ships; the contract mis-documents it as unavailable — the root cause of #903), and route `subgraph` at `max_depth == 1` to the existing live `subgraph_via_db` path so a write committed immediately before the call is visible (no tick lag). Two co-equal Class-1 deliverables — a documentation fix and a dispatch-only code change — with no interface, wire, or hot-path change.

## Domain Model / Ubiquitous Language

- **subgraph mode** — a `context_graph` query mode: multi-hop BFS from `seed_ids` over `max_depth` (1..=10), returning hydrated nodes + edges (`SubgraphResponse`).
- **depth-1-live / depth>1-cache asymmetry** — the established convention (ADR-005 vnc-018, extended here to subgraph): at `max_depth == 1` the neighborhood is read from the **live DB** (reflects all writes committed before the call); at `max_depth > 1` it is read from the **in-memory tick-cache** (`TypedRelationGraph`, rebuilt every 30–60s, so a within-tick write may not yet appear). "Depth-1 must be fresh; depth>1 tolerates a tick window."
- **`subgraph_via_db`** — the existing live-SQL BFS function (`graph_read_subgraph.rs`). Issues `query_direct_neighbors`, applies `edge_types`/`direction`, dedups by canonical triple (R-02), runs the post-BFS dangling-edge filter (R-05), hydrates via `fetch_nodes_batch` + `fetch_edge_metadata`, honors `max_nodes`/`truncated`. Today invoked only when `use_fallback == true` (cold-start/cycle). This feature also routes every `max_depth == 1` call to it, unconditionally, before the `use_fallback` branch.
- **DoD one-shot** — the target capability-board query: `subgraph, seed_ids:[goal], max_depth:1, edge_types:["Advances"], direction:"incoming"`. Must return exactly the incoming `Advances` capabilities, hydrated, live, in a stable order, with `truncated == false` at realistic goal fan-in — no client-side filter, no tick lag.
- **Hydration contract** — every returned node carries `id, title, content, status, kind, tags` (tags via `load_tags_for_entries`, ADR-006). The depth-1 live path must produce an `EntryRecord` set identical in shape to the cached path.
- **Three discoverable surfaces / four edit points** — the filter-availability contract is documented in: (1) `edge_types` schemars doc (`graph_read.rs:85`); (2) `direction` schemars doc (`graph_read.rs:82`); (3) the subgraph section of `CONTEXT_GRAPH_DESCRIPTION` (`tools.rs`) **and** (4) its mirror-const duplicate. Surfaces 3+4 are two copies that must stay byte-identical in the subgraph-relevant text.
- **Mirror-const** — the duplicate of `CONTEXT_GRAPH_DESCRIPTION` flagged by the zero-reviewer; drift between the two copies is the mechanism that produced #903.

## Functional Requirements

Each requirement is testable; the verifying test/method is named under Acceptance Criteria.

- **FR-1 (DOC).** The `edge_types` schemars doc (`graph_read.rs:85`) must state that `edge_types` applies to subgraph mode (not "neighbors only").
- **FR-2 (DOC).** The `direction` schemars doc (`graph_read.rs:82`) must list subgraph among the modes it applies to (currently chain + neighbors only).
- **FR-3 (DOC).** The subgraph section of `CONTEXT_GRAPH_DESCRIPTION` (`tools.rs`) **and its mirror-const duplicate** must both state that `edge_types`/`direction` filtering is honored on subgraph. The two copies must stay in sync — enforced by a single source-of-truth const or a test asserting the two bodies match (SR-01).
- **FR-4 (DOC).** The subgraph tool-description **staleness** text (distinct from FR-1..FR-3 filter-availability text) must state the split: depth-1 = live DB (all committed writes visible); depth>1 = tick-cache (30–60s). This text edit also lands in both `CONTEXT_GRAPH_DESCRIPTION` copies.
- **FR-5 (DOC / gating).** Before editing any description string or schema (FR-1..FR-4), locate any external snapshot/schema test that pins the exact `CONTEXT_GRAPH_DESCRIPTION` string or the `GraphParams` JSON schema. If one exists, update it in-scope so it tracks the new source text; if none exists, record that confirmation. (Resolves Open Q4 / SR-04.)
- **FR-6 (CODE).** In `handle_subgraph`, after parameter validation, when `max_depth == 1`, delegate to the existing `subgraph_via_db` **unconditionally** — dispatched by exact `max_depth == 1` match, placed **before** the `use_fallback` branch. No dedicated depth-1 helper.
- **FR-7 (CODE).** At depth-1 the live path must preserve `edge_types` filtering (only given relation types traversed/returned; absent/`[]` = all types except Supersedes), `direction` filtering (incoming/outgoing/both; filter affects inclusion, not the canonical `source_id → target_id` label / `direction: "outgoing"`), batch node hydration (id, title, content, status, kind, tags), edge metadata, `max_nodes`/`truncated`, and `resolve_supersessions` semantics (default true; raw as-stored on explicit false).
- **FR-8 (CODE).** At `max_depth > 1`, behavior is unchanged at the SET level — the returned node/edge set and all filtering/supersession semantics are identical: cached BFS over `TypedRelationGraph`, with the existing `use_fallback` → live-DB cold-start path intact and still firing on an empty/cold graph. This SET-level guarantee does NOT preserve the prior (arbitrary, undocumented) result byte-order; result ordering is governed by FR-9.
- **FR-9 (CODE).** Result ordering is a stable, documented, presentation-only, set-preserving contract applied uniformly to BOTH depths: nodes by ascending `id`, edges by canonical `(source_id, target_id, relation_type)`. It reorders how the existing set is presented; it never adds/removes members. The prior depth>1 order was arbitrary, so applying this uniform order to depth>1 is not a behavioral change to the set (reconciles with AC-02/FR-8). Callers see one ordering contract across depths, not two (SR-03).
- **FR-10.** No behavioral or dispatch change to `chain`, `current`, or `neighbors` modes.

## Non-Functional Requirements

- **NFR-1 (no interface/wire change).** `GraphParams` wire layout is unchanged — no field add/remove/retype (ADR-003 vnc-018 lock). All needed fields (`edge_types`, `direction`, `seed_ids`, `max_nodes`, `max_depth`) already exist. `SubgraphResponse` shape is unchanged — no `graph_rebuilt_at`/freshness field (ADR-004 vnc-019).
- **NFR-2 (no hot-path touch).** No change to `RelationEdge` or any tick hot-path struct (Principle #7). Depth-1 live routing must not acquire the `TypedGraphState` lock (A3).
- **NFR-3 (dual-path parity).** For the same seed + filter, the depth-1 live path and the prior cache path must return the same node SET and edge SET (ordering aside), with identical `edge_types`/`direction`/`resolve_supersessions`/Supersedes-exclusion semantics. No silent behavior divergence (SR-06).
- **NFR-4 (deterministic ordering).** Depth-1 results are deterministic across runs under the FR-9 ordering, so the DoD one-shot does not flake (AC-14).
- **NFR-5 (no silent truncation).** `truncated` is surfaced and asserted; a capped board read is never silently partial. The DoD one-shot returns `truncated == false` at the defined realistic fan-in (AC-15).
- **NFR-6 (load-bearing live path).** Depth-1 live is now the default path for a formerly-rare branch. Its dedup (R-02), dangling-edge filter (R-05), hydration, and metadata cap (`MAX_EDGES_UPPER` = 1000) must be regression-covered on normal board reads, not only the happy DoD one-shot (SR-02).
- **NFR-7 (fan-in threshold).** "Realistic goal fan-in" for AC-15 must be a concrete node count (architect to set; default assumption: seed + one hop below `max_nodes` default). The contract decision — board caller raises `max_nodes` vs. accepts+surfaces `truncated` — must be recorded (resolves Open Q5 / SR-05).

## Acceptance Criteria

AC-01..AC-15 carried from SCOPE.md, each with a named verification method.

| AC | Criterion | Verification |
|----|-----------|--------------|
| AC-01 | subgraph `max_depth==1` resolves from live DB; reflects every edge/node write committed before the call — no tick lag. | Freshness test: write edge, immediately query subgraph d1, assert edge present. |
| AC-02 | subgraph `max_depth>1` behaviorally unchanged in returned node/edge SET and filtering/supersession semantics: cached BFS, `use_fallback`→live cold-start intact. "Unchanged" is set-level, NOT byte-order — the prior depth>1 result order was documented as arbitrary; FR-9's uniform ordering is presentation-only and set-preserving (see FR-9 / NFR-4). | Depth>1 regression asserts the SET (not order) + cold-start test (empty `TypedRelationGraph` asserts fallback fires). |
| AC-03 | depth-1 issues the same single `query_direct_neighbors` edge query as neighbors d1 — no per-edge round-trips. | Code assertion / path inspection; covered via `subgraph_via_db` reuse. |
| AC-04 | depth-1 nodes hydrated via batch pattern (id, title, content, status, kind, tags); identical `EntryRecord` shape to cache path. | Hydration-field assertion on depth-1 result. |
| AC-05 | `edge_types` honored at depth-1; only given relation types; absent/`[]` = all except Supersedes. | Filter test at d1 (given-type-only + absent-defaults-all). |
| AC-06 | `direction` (incoming/outgoing/both) honored at depth-1; `EdgeRecord`s keep canonical `source→target` with `direction:"outgoing"` label. | Direction test at d1 asserting inclusion + label invariant. |
| AC-07 | DoD one-shot returns exactly incoming `Advances` capabilities, hydrated, reflecting a write committed immediately before, no client-side filter. | DoD one-shot integration test (write-then-read). |
| AC-08 | `resolve_supersessions` defaults true and behaves identically on depth-1 live (raw as-stored on explicit false). | Supersession test at d1 (default + explicit-false). |
| AC-09 | subgraph description staleness text updated: depth-1 = live; depth>1 = tick-cache (30–60s). | Doc-surface assertion (text present in both copies). |
| AC-10 | no `RelationEdge`/hot-path struct change; no `GraphParams` wire-shape change. | Wire/struct diff review; schema snapshot (FR-5). |
| AC-11 | freshness both ways: d1 write-then-read edge **appears**; d>1 within-tick write does **not** appear. | Two-direction freshness test (SR-08 mandatory). |
| AC-12 | chain/current/neighbors filtering + dispatch unchanged. | Regression on those modes. |
| AC-13 | (DOC) `edge_types` (`:85`) + `direction` (`:82`) schemars docs state subgraph applies; `CONTEXT_GRAPH_DESCRIPTION` + mirror-const state filtering honored; both copies in sync. | Doc-surface sync test (four edit points; two-copy same-body invariant, SR-01). |
| AC-14 | depth-1 results returned in stable documented order (nodes + edges); DoD one-shot deterministic across runs. | Ordering test asserting FR-9 keys; DoD run-twice determinism. |
| AC-15 | (no silent truncation) DoD one-shot returns `truncated==false` at realistic fan-in; `truncated` surfaced/asserted. | Truncation assertion at NFR-7 fan-in count. |

## User / Agent Workflows

- **Capability-board / frontier read (uni-zero §6).** A curator writes a capability status or edge, then immediately issues the DoD one-shot to see the current board. Depth-1 live guarantees the just-written change is visible; edge_types/direction filtering + hydration return exactly the board rows with no client-side id-join. This is the workflow whose hand-assembly caused the context overflow in #903.
- **Filter discovery.** An agent reading the `context_graph` tool description or `GraphParams` schema learns that `edge_types`/`direction` are available on subgraph (FR-1..FR-3), so it does not re-implement a client-side filter — closing the discoverability origin of #903.
- **Multi-hop exploration (unchanged).** A depth>1 subgraph query reads the tick-cache as today; the description now discloses the tick-window staleness explicitly (FR-4).

## Constraints (from SCOPE + SR-01..SR-08)

- `GraphParams` wire layout locked — additive `Option<T>` only; fields already present, no wire change (ADR-003 vnc-018).
- `RelationEdge` and the tick hot path must not be touched (Principle #7).
- `fetch_edge_metadata` OR-chain stays within `MAX_EDGES_UPPER` (1000); depth-1 live inherits this via `subgraph_via_db`.
- `TypedGraphState` is `std::sync::RwLock`; depth>1 must clone graph + `use_fallback` under one guard before async work (GH #623). Depth-1 live routing must not touch the lock (A3).
- Staleness disclosed in tool-description text only; `SubgraphResponse` shape fixed (ADR-004 vnc-019).
- Behavioral split (depth-1 live vs depth>1 cache) must be explicitly tested both ways (ADR-005 vnc-018 / SR-02 / SR-08).
- Mirror-const two-copy drift must be structurally prevented, not left to manual sync (SR-01).
- Dispatch must be exact `max_depth==1` match placed before the `use_fallback` branch so depth>1 and its cold-start fallback are not captured (SR-07).
- Snapshot/schema pin discovery (FR-5) resolved before description edits land (SR-04).
- Realistic-fan-in threshold is a concrete number, not aspirational (NFR-7 / SR-05).

## Dependencies

- Existing code (no new crates): `handle_subgraph`, `subgraph_via_db`, `validate_subgraph_params` (`crates/unimatrix-server/src/mcp/graph_read_subgraph.rs`, `graph_read_validation.rs`); `GraphParams` (`graph_read.rs`); `CONTEXT_GRAPH_DESCRIPTION` + mirror-const (`tools.rs`); `fetch_nodes_batch`, `fetch_edge_metadata`, `load_tags_for_entries`, `query_direct_neighbors`.
- Precedent: `handle_neighbors` `depth==1 → neighbors_sql` dispatch (the analog this feature mirrors).
- ADRs honored: ADR-003 vnc-018 (wire lock), ADR-004/005 vnc-019 & vnc-018 (staleness disclosure + depth asymmetry), ADR-003/006 vnc-019 (batch hydration + tags).

## NOT in Scope

- Accelerating typed-graph currency below the tick window for depth>1 (heal-acceleration / hot path) — reopens ADR-004 vnc-019; separate research spike.
- Depth>1 live read — depth>1 stays cached.
- Adding `edge_types`/`direction` to chain/current/neighbors (neighbors already has both; chain/current are supersession-only).
- Any `RelationEdge` / hot-path struct change or `GraphParams` wire-shape change.
- Adding a `graph_rebuilt_at`/freshness field to `SubgraphResponse` (ADR-004 vnc-019 rejected).
- Re-deriving or re-validating the existing edge_types/direction filter logic beyond confirming the one-shot works and asserting dual-path parity.
- A dedicated depth-1 helper — reuse `subgraph_via_db`.

## Open Questions (for architect)

- **OQ-A (NFR-7 / SR-05).** Set the concrete "realistic goal fan-in" node count for AC-15 and decide the truncation contract: board caller raises `max_nodes` vs. accepts+surfaces `truncated`.
- **OQ-B (FR-3 / SR-01).** Choose the drift-prevention mechanism: collapse the mirror-const into a single source-of-truth const, or add a same-body assertion test. Collapsing is preferred if no consumer requires two separate consts.
- **OQ-C (FR-5 / SR-04).** Confirm presence/absence of a `CONTEXT_GRAPH_DESCRIPTION` string snapshot or `GraphParams` JSON-schema snapshot test before description edits; update in-scope if present. (Delivery is blocked on this being answered in design.)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- confirmed ADR-003 vnc-018 GraphParams wire lock (4490/4503), ADR-005 vnc-018 neighbors depth-1 live vs depth>1 BFS asymmetry (4479), ADR-004 vnc-019 staleness-disclosed-in-text / no response freshness field (4493), resolve_supersessions default-true across neighbors+subgraph (4507, 5409). No conflicting prior convention; this feature extends the established asymmetry into subgraph.
