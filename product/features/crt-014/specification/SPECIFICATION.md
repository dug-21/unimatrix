# Specification: crt-014 — Topology-Aware Supersession

## Objective

Replace hardcoded deprecation and supersession penalty constants in `unimatrix-engine` with a topology-derived penalty function backed by a directed acyclic graph (DAG) built from entry supersession edges. Enable full multi-hop successor resolution in the search pipeline, allowing chains A→B→C to follow to the terminal active node C. Add cycle detection as a data integrity check with graceful fallback.

---

## Domain Models

### SupersessionGraph

A directed graph over knowledge entries, where an edge from node `P` to node `S` means "entry `S` supersedes entry `P`" (P is the predecessor, S is the successor). Nodes are `u64` entry IDs. Edges carry no weight (unit type). The graph is always a DAG in valid data; cycles are a data integrity violation.

### Topology Signals

| Signal | Definition |
|--------|-----------|
| `is_orphan` | Entry has status `Deprecated` and zero outgoing edges in the supersession graph |
| `active_reachable` | At least one Active, non-superseded entry is reachable via directed edges from this node |
| `chain_depth` | Shortest-path distance (hop count) to the nearest Active terminal node; `None` if no active terminal reachable |
| `successor_count` | Number of direct outgoing edges from this node |
| `terminal_active` | The first Active, non-superseded node found by DFS from a given starting node |

### Penalty

A `f64` multiplier in `(0.0, 1.0)` applied to a re-ranked search score. `1.0` means no penalty. Lower values push the entry further down in results. Penalties are derived from topology signals, not from entry status alone.

### CycleError

A data integrity violation: the supersession edges contain a directed cycle (A supersedes B supersedes ... supersedes A). The system must detect this and degrade gracefully rather than failing.

---

## Functional Requirements

### FR-01: petgraph Dependency
The `unimatrix-engine` crate must declare `petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }` as a dependency. No other petgraph features are enabled.

### FR-02: graph.rs Module
A new public module `graph` must exist in `unimatrix-engine` (`src/graph.rs`, exported via `lib.rs`). It must expose the functions and constants defined in the Integration Surface (ARCHITECTURE.md).

### FR-03: Graph Construction
`build_supersession_graph(entries: &[EntryRecord]) -> Result<SupersessionGraph, GraphError>` must:
- Create one node per entry keyed by `entry.id`
- For each entry with `supersedes: Some(pred_id)`: add a directed edge `pred_id → entry.id`
- Skip (with `tracing::warn!`) entries whose `supersedes` references an ID not present in `entries`
- Run cycle detection after graph construction
- Return `Err(GraphError::CycleDetected)` if `petgraph::algo::is_cyclic_directed` returns `true`

### FR-04: Graph Penalty
`graph_penalty(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> f64` must:
- Return `1.0` (no penalty) for node IDs not present in the graph
- Return a value in `(0.0, 1.0)` for all deprecated or superseded entries
- Apply the following rules in priority order:
  1. `is_orphan` → `ORPHAN_PENALTY`
  2. `!active_reachable` → `DEAD_END_PENALTY`
  3. `successor_count > 1` → `PARTIAL_SUPERSESSION_PENALTY`
  4. `chain_depth == Some(1)` → `CLEAN_REPLACEMENT_PENALTY`
  5. `chain_depth == Some(d >= 2)` → `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1)`, clamped to `[0.10, CLEAN_REPLACEMENT_PENALTY]`
  6. Defensive fallback → `DEAD_END_PENALTY`

### FR-05: Terminal Active Lookup
`find_terminal_active(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> Option<u64>` must:
- Perform iterative DFS following directed edges from `node_id`
- Return `Some(id)` for the first node found where the corresponding entry is `Status::Active` and `superseded_by.is_none()`
- Return `None` if no such node found within `MAX_TRAVERSAL_DEPTH` hops
- Return `None` if the starting node is not in the graph

### FR-06: Search Pipeline — Penalty Map (Flexible Mode)
In `search.rs` Step 6a, for `RetrievalMode::Flexible` without an explicit status filter:
- Before penalty marking, load all entries and build the supersession graph
- For each candidate entry where `entry.superseded_by.is_some() || entry.status == Status::Deprecated`:
  - If graph built successfully: `penalty_map.insert(entry.id, graph_penalty(entry.id, &graph, &all_entries))`
  - If `CycleDetected`: `penalty_map.insert(entry.id, FALLBACK_PENALTY)`

### FR-07: Search Pipeline — Successor Injection (Multi-Hop)
In `search.rs` Step 6b, for entries with `superseded_by.is_some()`:
- If graph built successfully: use `find_terminal_active(entry.id, &graph, &all_entries)` to find the terminal active node
- If `CycleDetected` (fallback mode): use `entry.superseded_by` (single-hop, old behavior)
- Inject the found successor ID only if it is not already in the result set
- Fetch the successor entry from the store and add it with its computed similarity score

### FR-08: Cycle Fallback
On `GraphError::CycleDetected`:
- Log `tracing::error!` with message indicating cycle detected and fallback activation
- Apply `FALLBACK_PENALTY` to all penalized entries for this query
- Use single-hop injection for successor resolution for this query
- Return search results to caller without error — availability must be preserved

### FR-09: Constant Removal
`DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` must be removed from `unimatrix-engine/src/confidence.rs`. No references to these constants in non-test production code remain after this feature.

### FR-10: Penalty Constants in graph.rs
All penalty constants (`ORPHAN_PENALTY`, `CLEAN_REPLACEMENT_PENALTY`, `HOP_DECAY_FACTOR`, `PARTIAL_SUPERSESSION_PENALTY`, `DEAD_END_PENALTY`, `FALLBACK_PENALTY`, `MAX_TRAVERSAL_DEPTH`) must be declared as named `pub const` in `graph.rs`.

---

## Non-Functional Requirements

### NFR-01: Graph Construction Latency
Full-store graph construction must complete in ≤5ms at up to 1,000 entries. (ASS-017 measured ~1-2ms at ~500 entries.) Verified by benchmark in integration tests.

### NFR-02: graph_penalty Purity
`graph_penalty` must be a pure function: deterministic, no I/O, no side effects. Callers can cache or repeat calls with identical results.

### NFR-03: find_terminal_active Depth Cap
`find_terminal_active` must never traverse more than `MAX_TRAVERSAL_DEPTH` (10) hops regardless of chain length.

### NFR-04: No Unsafe Code
`graph.rs` must compile with `#![forbid(unsafe_code)]` (already enforced workspace-wide in `unimatrix-engine/src/lib.rs`).

### NFR-05: No New Async in graph.rs
All functions in `graph.rs` are synchronous. Async wrapping is the caller's responsibility (following existing engine patterns using `spawn_blocking`).

### NFR-06: No Schema Changes
`EntryRecord.supersedes` and `superseded_by` remain `Option<u64>`. No new database tables or columns. No migration required.

### NFR-07: Test Infrastructure
All new tests must extend existing test fixtures. No isolated scaffolding. The `graph.rs` unit tests use `EntryRecord` values directly (no store required for unit tests). Integration tests use the existing tempfile-based store setup from other engine tests.

---

## Acceptance Criteria

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-01 | petgraph with `stable_graph` feature added to `unimatrix-engine/Cargo.toml` | `cargo build --workspace` succeeds without warnings |
| AC-02 | `pub mod graph` exported from `unimatrix-engine` | `cargo doc --package unimatrix-engine` lists graph module |
| AC-03 | `build_supersession_graph` returns `Err(GraphError::CycleDetected)` for entries with A→B→A supersession cycle | Unit test: construct two entries with cyclic `supersedes` refs, assert `Err` |
| AC-04 | `build_supersession_graph` returns `Ok` for valid DAGs: depth 1, depth 2, depth 3+ | Unit tests: chain of 1, 2, 3 entries, assert `Ok` |
| AC-05 | `graph_penalty` returns value in `(0.0, 1.0)` for all penalized inputs | Unit test: sample each topology scenario, assert range |
| AC-06 | Orphan deprecated entry receives softer penalty than superseded entry with active terminal | Unit test: `ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY` ordering assertion |
| AC-07 | 2-hop outdated entry receives harsher penalty than 1-hop outdated entry | Unit test: depth-1 vs depth-2 chain, assert `graph_penalty(A_2hop) < graph_penalty(A_1hop)` |
| AC-08 | Partially-superseded entry (>1 successor) receives softer penalty than single-successor entry | Unit test: `PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY` ordering assertion |
| AC-09 | `find_terminal_active` returns `Some(C)` for A→B→C where B is also superseded and C is Active | Unit test: three-entry chain, assert result == C.id |
| AC-10 | `find_terminal_active` returns `None` when no active terminal reachable | Unit test: chain terminates at deprecated/quarantined entry |
| AC-11 | `find_terminal_active` returns `None` when chain depth exceeds `MAX_TRAVERSAL_DEPTH` | Unit test: chain of 11 entries, assert `None` |
| AC-12 | In `search.rs` Flexible mode, `penalty_map` is populated via `graph_penalty`, not removed constants | Code review + integration test: deprecate entry B, supersede A→B→C, search returns A/B with topology-derived penalties |
| AC-13 | Multi-hop injection: search for superseded A (chain A→B→C where C is active) injects C, not B | Integration test: assert injected successor ID == C.id |
| AC-14 | `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` absent from production code | `cargo build` with no unused-import warnings; `grep -r DEPRECATED_PENALTY crates/` returns no hits in non-test files |
| AC-15 | Behavioral ordering tests replace removed constant-value tests in `confidence.rs` | Test file: no `assert_eq!(DEPRECATED_PENALTY, 0.7)` style assertions; ordering tests present in `graph.rs` |
| AC-16 | Cycle fallback: `build_supersession_graph` returning `CycleDetected` causes search to log error and use `FALLBACK_PENALTY` | Integration test with injected cycle data, assert search succeeds + log contains cycle message |
| AC-17 | Dangling `supersedes` reference is skipped with `tracing::warn!` (no panic, no error) | Unit test: entry references non-existent pred_id, assert `Ok(graph)` |
| AC-18 | Workspace builds clean with no new warnings after all changes | `cargo build --workspace 2>&1 \| grep "^error" \| wc -l` == 0 |

---

## User Workflows

### Workflow 1: Agent queries knowledge with deprecated/superseded entries in results

1. Agent calls `context_search(query: "...", k: 10)`
2. HNSW returns 10 candidates including some deprecated and superseded entries
3. Before re-ranking: search service loads all entries from store, builds supersession DAG
4. Each deprecated/superseded candidate receives a topology-derived penalty from `graph_penalty`
5. Superseded candidates trigger `find_terminal_active` — their active terminal successor is injected into results
6. Re-ranking applies penalties: orphan deprecated entries rank higher than 2-hop-outdated entries (less penalty)
7. Agent receives results ordered by correct topological severity

### Workflow 2: Data integrity cycle detected during search

1. Production data contains a supersession cycle (data bug)
2. Agent calls `context_search`
3. `build_supersession_graph` detects cycle, returns `CycleDetected`
4. Search logs `tracing::error!` and activates fallback mode
5. All penalized entries receive flat `FALLBACK_PENALTY = 0.70` (old behavior)
6. Search result returned to agent — no error, no degradation in availability
7. Operator sees `ERROR` log entry, investigates and fixes the cycle via `context_correct` or `context_deprecate`

---

## Constraints

- `petgraph` feature set: `stable_graph` only (ADR-001)
- Graph construction: per-query rebuild, all entries from store (ADR-002)
- No new async in `graph.rs` — sync only
- No schema changes — `supersedes`/`superseded_by` remain `Option<u64>`
- No graph persistence (no `serde-1` feature)
- Traversal depth capped at `MAX_TRAVERSAL_DEPTH = 10`
- Workspace Rust edition 2024, MSRV 1.89 — petgraph 0.8.x compatible
- `#![forbid(unsafe_code)]` — inherited from workspace

---

## Dependencies

| Dependency | Version | Notes |
|-----------|---------|-------|
| petgraph | 0.8.x | New — `stable_graph` feature only |
| unimatrix-store | workspace | `Store::query`, `EntryRecord` |
| unimatrix-core | workspace | `Status`, `EntryRecord` |
| thiserror | workspace | `#[derive(Error)]` on `GraphError` |

---

## NOT In Scope

- Co-access graph (Phase 2 — crt-015 or equivalent)
- Correction chain graph traversal (Phase 3)
- Graphviz/DOT export (`petgraph::dot`) — Phase 3
- Graph caching with `RwLock` (Option B) — deferred to profiling
- Runtime-configurable penalty parameters
- New `context_status` fields for cycle reporting (log-only in v1)
- Any changes to the briefing service (`briefing.rs`)
- Any changes to the coherence gate (`crt-005` lambda computation)
- Schema migration
