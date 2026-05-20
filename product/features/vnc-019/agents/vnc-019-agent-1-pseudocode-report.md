# Agent Report: vnc-019-agent-1-pseudocode

## Task

Produce per-component pseudocode files for vnc-019 (context_graph subgraph mode),
covering four components: graph_read.rs, graph_read_subgraph.rs (new),
graph_read_neighbors.rs, and tools.rs.

## Output Files

- `product/features/vnc-019/pseudocode/OVERVIEW.md` — component interactions, data flow,
  shared types, sequencing constraints
- `product/features/vnc-019/pseudocode/graph_read.md` — GraphParams extension,
  SubgraphResponse, validate_no_unsupported_params changes, handle_graph restructuring
- `product/features/vnc-019/pseudocode/graph_read_subgraph.md` — full BFS algorithm,
  all 10 steps per BFS Contract, error handling, key test scenarios
- `product/features/vnc-019/pseudocode/graph_read_neighbors.md` — visibility change only,
  rationale, delivery order note
- `product/features/vnc-019/pseudocode/tools.md` — exact tool description text,
  AC-13 verification checklist, test extension guidance

## Components Covered

1. `graph_read.rs` — GraphParams + SubgraphResponse + dispatch restructuring
2. `graph_read_subgraph.rs` — full BFS implementation (new file)
3. `graph_read_neighbors.rs` — `follow_to_current` visibility change
4. `tools.rs` — tool description update

## Open Questions / Gaps Found

### Gap 1: `handle_graph` id-extraction restructuring (flagged in graph_read.md)

The current `handle_graph` in vnc-018 extracts `id` unconditionally before the mode
match, then requires it for all three modes. Adding `"subgraph"` breaks this — subgraph
uses `seed_ids`, not `id`. The pseudocode documents the required structural fix:
move the `id.ok_or_else(...)` guard inside the chain/current/neighbors arms (or use
an outer match on mode to route subgraph before the id check). The implementor must
choose and apply one approach — the pseudocode shows the recommended two-level match
pattern.

### Gap 2: `store.get_many` API name

The architecture references `Store::get_many` for batch node hydration. The pseudocode
uses this name. The implementor must verify the actual function name in `unimatrix-store`
at delivery time — the vnc-018 source is not available to confirm, and the batch
hydration function may have a different name or signature. If no batch function exists,
the implementor must flag this as a blocker before writing code.

### Gap 3: sqlx dynamic bind-parameter pattern

The OR-chain metadata SQL requires chaining `.bind()` calls in a loop. The pseudocode
documents the structural intent but defers the exact sqlx API call to the implementor.
The implementor should reference the existing `query_direct_neighbors` in `unimatrix-store`
for the established codebase pattern before writing the batch query.

### Gap 4: FR-19 text vs. ADR-004 text divergence

The SPECIFICATION FR-19 and IMPLEMENTATION-BRIEF §Tool Description Requirements specify
two slightly different exact texts for the tool description. The tools.md pseudocode uses
the SPECIFICATION FR-19 text (which has stronger ordering requirements) as the primary
source. The implementor should treat FR-19 as authoritative since it explicitly mandates
the ordering of direction semantics first.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to architecture, ADR files, or codebase source
- [x] Output is per-component (OVERVIEW.md + 4 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO or TBD sections — all gaps flagged explicitly above
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/vnc-019/pseudocode/`
- [x] Knowledge Stewardship report block included below

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 12 entries returned. Key entries:
  #4491 (ADR-002 file split), #4478 (EdgeRecord placement), #4482 (BFS primitives in
  neighbors mode), #4490 (ADR-001 max_depth extension), #4486 (post-BFS metadata
  hydration pattern), #4494 (BFS visited-set keyed on resolved ID), #4493 (ADR-004
  staleness disclosure).
- Queried: `mcp__unimatrix__context_search` (pattern/BFS) — entries #4066, #4494, #4486
  returned; all consistent with design decisions in architecture.
- Queried: `mcp__unimatrix__context_search` (decision/vnc-019) — entries #83, #4477,
  #4493 returned; confirmed ADR-004 stored at #4493.
- Deviations from established patterns: none. All pseudocode follows the lock-acquire-
  clone-release pattern from neighbors_bfs, the `pub(super)` visibility convention, the
  `#[path]`-submodule pattern, and the error-propagation conventions established in vnc-018.
