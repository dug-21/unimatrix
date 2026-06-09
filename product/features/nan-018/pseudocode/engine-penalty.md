# Component: Engine penalty entry point — `engine-penalty.md`

**Wave**: 1
**Location**: `crates/unimatrix-engine/src/graph.rs` (modify; consts retained)
**ADR**: ADR-001 (#4897). **Risks**: R-01 (critical), R-02, R-13 (clamp).

## Purpose

Parameterize the crt-014 penalty body so a swept profile can move the levers, while
`graph_penalty` (existing signature) stays bit-for-bit identical for every current
caller and test. The engine consts remain the single source of truth for defaults.

## Existing surface (preserved exactly)

```
pub const ORPHAN_PENALTY: f64 = 0.75;
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40;
pub const HOP_DECAY_FACTOR: f64 = 0.60;
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60;
pub const DEAD_END_PENALTY: f64 = 0.65;
pub const FALLBACK_PENALTY: f64 = 0.70;
pub const MAX_TRAVERSAL_DEPTH: usize = 10;
// clamp lower bound 0.10 — literal floor, NOT a const, NOT a lever

pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64
```

## New: `GraphPenaltyParams`

```
pub struct GraphPenaltyParams {        // derive Copy, Clone, Debug, PartialEq
    pub orphan: f64,
    pub clean_replacement: f64,
    pub hop_decay: f64,
    pub partial_supersession: f64,
    pub dead_end: f64,
    pub fallback: f64,
    pub max_traversal_depth: usize,
}

impl Default for GraphPenaltyParams {
    fn default() -> Self {
        // SINGLE SOURCE OF TRUTH: every field references the existing const.
        // (Dual-default discipline #4064 — the server config's Default must triangulate to these.)
        GraphPenaltyParams {
            orphan:               ORPHAN_PENALTY,
            clean_replacement:    CLEAN_REPLACEMENT_PENALTY,
            hop_decay:            HOP_DECAY_FACTOR,
            partial_supersession: PARTIAL_SUPERSESSION_PENALTY,
            dead_end:             DEAD_END_PENALTY,
            fallback:             FALLBACK_PENALTY,
            max_traversal_depth:  MAX_TRAVERSAL_DEPTH,
        }
    }
}
```

Note: `fallback` lives on `GraphPenaltyParams` even though `graph_penalty_with` itself
never reads it (the fallback branch is applied in `search.rs:727`, not inside the engine fn).
It rides on the struct so the search layer resolves one params object. This is intentional —
the struct is the unit threaded through `with_rate_config`.

## New: `graph_penalty_with` — parameterized body

`graph_penalty` becomes a thin wrapper; the current body moves verbatim into
`graph_penalty_with`, with each const reference replaced by the matching `params.*`
and the clamp ceiling coupled to `params.clean_replacement`.

```
pub fn graph_penalty_with(
    node_id: u64,
    graph: &TypedRelationGraph,
    entries: &[EntryRecord],
    params: &GraphPenaltyParams,
) -> f64 {
    node_idx = graph.node_index.get(node_id)         else return 1.0   // unchanged guard
    entry    = entry_by_id(node_id, entries)         else return 1.0   // unchanged guard

    outgoing_count  = count Supersedes outgoing edges of node_idx       // unchanged
    successor_count = outgoing_count
    is_orphan       = entry.status == Deprecated && outgoing_count == 0

    if is_orphan:                       return params.orphan            // was ORPHAN_PENALTY

    active_reachable = dfs_active_reachable(node_idx, graph, entries)   // see depth note below
    if not active_reachable:            return params.dead_end         // was DEAD_END_PENALTY

    if successor_count > 1:             return params.partial_supersession   // was PARTIAL_SUPERSESSION_PENALTY

    chain_depth = bfs_chain_depth(node_idx, graph, entries)            // see depth note below
    if chain_depth == Some(1):          return params.clean_replacement     // was CLEAN_REPLACEMENT_PENALTY

    if chain_depth == Some(d) where d >= 2:
        raw = params.clean_replacement * params.hop_decay.powi((d - 1) as i32)
        return raw.clamp(0.10, params.clean_replacement)    // *** CEILING = params.clean_replacement, NOT the const ***

    return params.dead_end             // was DEAD_END_PENALTY (defensive fallback)
}

pub fn graph_penalty(node_id, graph, entries) -> f64 {
    graph_penalty_with(node_id, graph, entries, &GraphPenaltyParams::default())
}
```

### Clamp coupling (LOAD-BEARING — ADR-001, R-13)

The line at `graph.rs:531` was `raw.clamp(0.10, CLEAN_REPLACEMENT_PENALTY)`. It becomes
`raw.clamp(0.10, params.clean_replacement)`. Reasoning:

- Because `hop_decay < 1`, `raw <= clean_replacement` for `d >= 2`, so the ceiling is the
  monotonicity cap (depth-2 never harsher than depth-1).
- If the ceiling stayed the const while `clean_replacement` is swept higher, a depth-2 entry
  could be clamped MORE harshly than depth-1 — inverting the formula. The ceiling MUST track
  the swept base.
- The lower bound `0.10` stays a literal floor.
- Consequence: `clean_replacement` is an **amplified** sweep knob (base + ceiling move together,
  same direction). This is intended; documented in Band-2 (`docs.md`).
- The ceiling is deliberately NOT a separate `GraphPenaltyParams` field (an independent ceiling
  could fall below the base and break monotonicity — rejected in ADR-001).

### `max_traversal_depth` note

The current `dfs_active_reachable`/`bfs_chain_depth` cap at `MAX_TRAVERSAL_DEPTH`. When
parameterized, these helpers must read `params.max_traversal_depth`. Two acceptable forms
(delivery picks one, both preserve default behavior bit-for-bit):

- (a) Pass `params.max_traversal_depth` into the helper signatures, OR
- (b) Keep the helpers reading the const for the wrapper path and add `_with` variants.

Prefer (a): thread `max_traversal_depth` so a sweep that lowers depth truncates traversal.
Edge case (R-TEST): `max_traversal_depth` below the deepest fixture chain ⇒ defined truncation
(traversal stops, entry treated as unreachable / dead-end), NEVER a panic. `hop_decay` and
`max_traversal_depth` are SHAPE params — the multiplier overlay (in `penalty-config.md`) must
NOT scale them.

## Data flow

- **Inputs**: `node_id`, `&TypedRelationGraph`, `&[EntryRecord]`, `&GraphPenaltyParams`.
- **Output**: `f64` penalty multiplier (same range/semantics as today).
- **Transformations**: none new — same branch structure; const → `params.*` substitution.

## Error handling

Pure function, no `Result`. Guard returns (`1.0`) for node-not-in-graph and entry-not-found
are preserved exactly. No new panics; the depth-truncation edge returns the dead-end penalty,
never panics.

## Key test scenarios

- **Default-equivalence (R-01.1, NFR-01)**: for every shape branch (orphan, dead-end, partial,
  clean depth-1, hop-decay depth>=2 incl. clamp), assert
  `graph_penalty_with(.., &Default::default()) == graph_penalty(..) == the named const`.
- **Clamp coupling (R-13)**: with `clean_replacement` swept up, assert depth-2 penalty <= depth-1
  penalty still holds (ceiling moved with the base); with `clean_replacement` swept down, depth-2
  clamps to the new ceiling, not the stale const.
- **Depth truncation**: `max_traversal_depth` below the deepest chain ⇒ defined result, no panic.
- **Guard preservation**: node-not-in-graph and entry-not-found still return `1.0`.
- **`Default` triangulation (R-02)**: `GraphPenaltyParams::default().<field> == <CONST>` for all 7.
- Existing `graph_tests.rs` / `pipeline_retrieval.rs` ordering-invariant tests pass UNCHANGED
  (consts retained; wrapper preserves behavior).
