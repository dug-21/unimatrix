# Implementation Brief: crt-014 — Topology-Aware Supersession

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-014/SCOPE.md |
| Scope Risk Assessment | product/features/crt-014/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-014/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-014/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-014/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-014/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| graph.rs (NEW) | pseudocode/graph.md | test-plan/graph.md |
| search.rs (MODIFIED) | pseudocode/search.md | test-plan/search.md |
| confidence.rs (MODIFIED) | pseudocode/confidence.md | test-plan/confidence.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Stage 3a complete. All pseudocode and test-plan files produced.

---

## Goal

Replace hardcoded scalar deprecation/supersession penalty constants in `unimatrix-engine` with a topology-derived penalty function backed by a directed acyclic graph (DAG) built per-query from entry supersession edges. Enable full multi-hop successor resolution in the search pipeline so chains A→B→C follow to the terminal active node C. Add cycle detection via `petgraph::algo::is_cyclic_directed` as a data integrity check with graceful fallback to flat penalties on cycle detection.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| petgraph feature set | `stable_graph` only; no `graphmap`, `matrix_graph`, `rayon`, `serde-1`, or `generate` | SCOPE.md Constraints | architecture/ADR-001-petgraph-stable-graph-only.md |
| Graph construction strategy | Option A — per-query full-store rebuild via `Store::query(QueryFilter::default())` | OQ-2 answer, ASS-017 | architecture/ADR-002-per-query-graph-rebuild.md |
| Multi-hop traversal | Remove single-hop limit; use `find_terminal_active` (DFS, depth-capped at 10) | ADR-003 superseded | architecture/ADR-003-supersede-system-adr-003-multi-hop.md |
| Penalty constants removal | Remove `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` from `confidence.rs`; replace with named constants + `graph_penalty` in `graph.rs` | OQ-1, OQ-3 answers | architecture/ADR-004-supersede-system-adr-005-penalties.md |
| Cycle detection fallback | On `CycleDetected`: `tracing::error!`, `use_fallback = true`, apply `FALLBACK_PENALTY = 0.70`, use single-hop injection; search availability preserved | OQ-4 answer | architecture/ADR-005-cycle-fallback-strategy.md |
| Penalty constants configurability | Named `pub const` values in `graph.rs`, fixed for v1; no runtime configuration | OQ-1 answer | architecture/ADR-006-graph-penalty-constants.md |

---

## Files to Create / Modify

| File | Change | Notes |
|------|--------|-------|
| `crates/unimatrix-engine/src/graph.rs` | CREATE | New graph module — see Data Structures and Function Signatures below |
| `crates/unimatrix-engine/src/lib.rs` | MODIFY | Add `pub mod graph;` |
| `crates/unimatrix-engine/Cargo.toml` | MODIFY | Add `petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }`; verify `thiserror` is present |
| `crates/unimatrix-engine/src/confidence.rs` | MODIFY | Remove `DEPRECATED_PENALTY`, `SUPERSEDED_PENALTY` constants and 4 associated tests |
| `crates/unimatrix-server/src/services/search.rs` | MODIFY | Remove constant imports; add graph construction before Step 6a; replace penalty_map insertion; replace single-hop injection with `find_terminal_active` |

---

## Data Structures

```rust
// graph.rs — new types

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("supersession cycle detected")]
    CycleDetected,
}

pub struct SupersessionGraph {
    pub(crate) inner: StableGraph<u64, ()>,       // directed; node weight = entry id
    pub(crate) node_index: HashMap<u64, NodeIndex>, // O(1) id → NodeIndex lookup
}

// Penalty constants (all pub const in graph.rs)
pub const ORPHAN_PENALTY: f64 = 0.75;             // deprecated, no successors
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40;  // active terminal at depth 1
pub const HOP_DECAY_FACTOR: f64 = 0.60;           // multiplier per additional hop
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60; // >1 active successor
pub const DEAD_END_PENALTY: f64 = 0.65;           // no active terminal reachable
pub const FALLBACK_PENALTY: f64 = 0.70;           // cycle detection fallback
pub const MAX_TRAVERSAL_DEPTH: usize = 10;        // DFS depth cap
```

**Topology Signals** (computed internally by `graph_penalty`):

| Signal | Definition |
|--------|-----------|
| `is_orphan` | `status == Deprecated` AND zero outgoing edges |
| `active_reachable` | At least one Active + `superseded_by.is_none()` entry reachable via directed edges |
| `chain_depth` | Shortest-path hop count to nearest active terminal; `None` if unreachable |
| `successor_count` | Number of direct outgoing edges |
| `terminal_active` | First Active + non-superseded node found by DFS |

**Supersession edge direction**: When `entry.supersedes = Some(pred_id)`, the graph edge is `pred_id → entry.id` (predecessor points to successor). Outgoing edges from a node point toward more-recent knowledge.

---

## Function Signatures

```rust
// graph.rs — public API

/// Build directed supersession DAG from all entries.
/// Edge: pred_id → entry.id when entry.supersedes == Some(pred_id).
/// Dangling refs: skip with tracing::warn!.
/// Returns Err(CycleDetected) if is_cyclic_directed() is true.
pub fn build_supersession_graph(
    entries: &[EntryRecord],
) -> Result<SupersessionGraph, GraphError>;

/// Topology-derived penalty for a node.
/// Returns 1.0 (no penalty) for node IDs absent from the graph.
/// Priority order:
///   1. is_orphan → ORPHAN_PENALTY
///   2. !active_reachable → DEAD_END_PENALTY
///   3. successor_count > 1 → PARTIAL_SUPERSESSION_PENALTY
///   4. chain_depth == Some(1) → CLEAN_REPLACEMENT_PENALTY
///   5. chain_depth == Some(d >= 2) → CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1), clamped to [0.10, CLEAN_REPLACEMENT_PENALTY]
///   6. fallback → DEAD_END_PENALTY
pub fn graph_penalty(
    node_id: u64,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> f64;

/// DFS from node_id; returns first node where status==Active && superseded_by.is_none().
/// Depth-capped at MAX_TRAVERSAL_DEPTH. Returns None if not found or not in graph.
pub fn find_terminal_active(
    node_id: u64,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> Option<u64>;
```

**search.rs changes** (pseudocode pattern):

```rust
// Before Step 6a — load all entries for graph construction
let all_entries = store.query(QueryFilter::default())?;
let graph_result = build_supersession_graph(&all_entries);
let (graph_opt, use_fallback) = match graph_result {
    Ok(g) => (Some(g), false),
    Err(GraphError::CycleDetected) => {
        tracing::error!("supersession cycle detected in knowledge graph — search using fallback penalties");
        (None, true)
    }
};

// Step 6a — penalty marking (Flexible mode, no explicit status filter)
for entry in &candidates {
    if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
        let penalty = if use_fallback {
            FALLBACK_PENALTY
        } else {
            graph_penalty(entry.id, graph_opt.as_ref().unwrap(), &all_entries)
        };
        penalty_map.insert(entry.id, penalty);
    }
}

// Step 6b — successor injection (multi-hop)
for entry in superseded_candidates {
    let terminal_id = if use_fallback {
        entry.superseded_by
    } else {
        find_terminal_active(entry.id, graph_opt.as_ref().unwrap(), &all_entries)
    };
    if let Some(id) = terminal_id {
        if !result_ids.contains(&id) {
            // fetch and inject
        }
    }
}
```

**confidence.rs removals**:

```rust
// REMOVE these lines:
pub const DEPRECATED_PENALTY: f64 = 0.7;
pub const SUPERSEDED_PENALTY: f64 = 0.5;

// REMOVE these tests:
// deprecated_penalty_value
// superseded_penalty_value
// superseded_penalty_harsher_than_deprecated
// penalties_independent_of_confidence_formula
```

---

## Constraints

- `petgraph` feature set: `stable_graph` only. Enforced by Cargo.toml comment (ADR-001).
- Graph construction: per-query rebuild from `Store::query(QueryFilter::default())` — no caching (ADR-002).
- No async in `graph.rs` — all functions are sync. Caller wraps in `spawn_blocking` (ADR-002).
- No schema changes — `supersedes`/`superseded_by` remain `Option<u64>`.
- No graph persistence — no `serde-1` feature, `StableGraph` is not serialized.
- Traversal depth capped at `MAX_TRAVERSAL_DEPTH = 10` (NFR-03).
- `graph.rs` compiles under `#![forbid(unsafe_code)]` (inherited workspace-wide, NFR-04).
- Test infrastructure is cumulative — extend existing fixtures; no isolated scaffolding (NFR-07).
- Workspace Rust edition 2024, MSRV 1.89 — petgraph 0.8.x is compatible.
- Graph construction must complete in ≤5ms at 1,000 entries (NFR-01).
- `graph_penalty` must be a pure function: deterministic, no I/O, no side effects (NFR-02).

---

## Dependencies

| Dependency | Version | Notes |
|-----------|---------|-------|
| petgraph | 0.8.x | NEW — `stable_graph` feature only; adds `fixedbitset`, `indexmap` transitively |
| unimatrix-store | workspace | `Store::query`, `EntryRecord` |
| unimatrix-core | workspace | `Status`, `EntryRecord` |
| thiserror | workspace | `#[derive(Error)]` on `GraphError` — verify present in engine Cargo.toml before adding |

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

---

## Test Migration Notes (CRITICAL)

The following tests must be removed from `confidence.rs` in the SAME commit that adds behavioral ordering tests to `graph.rs`:

**Tests to remove** (`crates/unimatrix-engine/src/confidence.rs`, lines 720–752):
- `deprecated_penalty_value`
- `superseded_penalty_value`
- `superseded_penalty_harsher_than_deprecated`
- `penalties_independent_of_confidence_formula`

**Replacement ordering tests** (add to `graph.rs` test module):
- `orphan_softer_than_clean_replacement`: assert `ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY`
- `two_hop_harsher_than_one_hop`: assert `graph_penalty(depth-2 entry) < graph_penalty(depth-1 entry)`
- `partial_supersession_softer_than_clean`: assert `PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY`
- Full AC-05 through AC-08 behavioral ordering coverage

**Search.rs test migration**: Tests in `search.rs` that assert `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` exact values must be updated to use topology-derived ordering assertions. See AC-12, AC-13 in SPECIFICATION.md.

---

## Alignment Status

**Overall**: PASS — No variances requiring human approval.

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly executes Graph Enablement milestone Phase 1 |
| Milestone Fit | PASS — scoped to supersession graph only |
| Scope Gaps | PASS — all 6 SCOPE.md goals addressed |
| Scope Additions | WARN — AC-17 and AC-18 added in SPECIFICATION.md; both are defensive and non-expanding |
| Architecture Consistency | PASS |
| Risk Completeness | PASS — 13 risks, all SR-XX traced |

One documentation note: ARCHITECTURE.md Technology Decisions section references ADR-003 filename as `ADR-003-supersede-prior-adr-003.md` but actual file is `ADR-003-supersede-system-adr-003-multi-hop.md`. No behavioral impact.
