# Agent Report — nan-018-agent-3-engine-penalty (Wave 1)

## Scope
Engine penalty entry point: `GraphPenaltyParams` + `graph_penalty_with` in
`crates/unimatrix-engine/src/graph.rs`. The const source of truth for all downstream
penalty config.

## Files modified
- `crates/unimatrix-engine/src/graph.rs` (modify)
- `crates/unimatrix-engine/src/graph_penalty_params_tests.rs` (new — focused test module,
  kept out of the already-1700-line `graph_tests.rs` per the 500-line rule)

## What was implemented (per Integration Surface, exact names/types)
- `GraphPenaltyParams` — `#[derive(Debug, Clone, Copy, PartialEq)]`; `Default` resolves
  every field to the existing engine consts (ORPHAN=0.75, CLEAN_REPLACEMENT=0.40,
  HOP_DECAY=0.60, PARTIAL_SUPERSESSION=0.60, DEAD_END=0.65, FALLBACK=0.70,
  MAX_TRAVERSAL_DEPTH=10). Consts retained as the single numeric source of truth (#4064).
- `graph_penalty_with(node_id, graph, entries, &GraphPenaltyParams)` — current body moved
  in verbatim, each const swapped for `params.*`.
- `graph_penalty` is now a THIN WRAPPER over `graph_penalty_with(.., &Default::default())`.
- Clamp coupling (ADR-001, load-bearing): hop-decay ceiling = `params.clean_replacement`
  (NOT the const); lower bound `0.10` stays a literal (`HOP_DECAY_CLAMP_FLOOR`).
- `max_traversal_depth` threaded into `dfs_active_reachable` + `bfs_chain_depth` (form (a)):
  a lowered depth truncates traversal to a defined dead-end, never panics. `find_terminal_active`
  (separate public fn, not part of graph_penalty) keeps using the const — unchanged.

## Deviation flagged + resolved (not a silent deviation)
The pseudocode/ADR clamp line `raw.clamp(0.10, params.clean_replacement)` PANICS when a swept
`clean_replacement < 0.10` (std::f64::clamp requires min <= max). The pseudocode did not flag
this. Added a guard: when `ceiling <= floor`, return the ceiling (the base penalty is already
below the floor; the sub-floor ceiling correctly dominates). Preserves bit-for-bit default
behavior (default ceiling 0.40 > floor) and the amplified-knob monotonicity; only changes the
sub-floor sweep extreme from panic → defined value. Stored as pattern #4900.

## Tests (test-plan/engine-penalty.md)
- Default-equivalence enumerated per status-shape branch AND the clamp: `graph_penalty ==
  graph_penalty_with(&Default::default()) == named const` (literal values, #3548) — orphan,
  dead-end, partial, clean depth-1, hop-decay depth-2, clamp-floor depth-5.
- Guard preservation: node-absent + entry-absent → 1.0 on both fns.
- R-02 `Default` triangulation to consts (and literal values).
- Clamp coupling: ceiling tracks swept clean_replacement; equality-boundary clamp;
  depth-2 ≤ depth-1 monotonicity across 5 clean_replacement values; literal 0.10 floor;
  sub-floor no-panic.
- R-13: severity scaling changes output (shape params unchanged); max_traversal_depth below
  deepest chain truncates → dead-end (no panic); max_depth=0 degenerate (no panic/overflow).
- Wrapper integrity across all shapes; Copy/PartialEq semantics.

**Result: `cargo test -p unimatrix-engine --lib` → 482 passed, 0 failed, 1 ignored.**
`cargo build -p unimatrix-engine` clean. `cargo fmt` applied. `cargo clippy` clean on my
files (only pre-existing warning in `graph_ppr_tests.rs`, unrelated).

## Blockers / coordination note
`cargo test --workspace` currently fails to COMPILE the `unimatrix-server` lib test — 3 errors,
all `E0603: module \`types\` is private` in the UNTRACKED `crates/unimatrix-server/src/eval/corpus/`
files (sibling Wave-1 corpus-loader agent's in-progress work). Zero references to graph_penalty /
GraphPenaltyParams; NOT caused by this engine change. The corpus-loader / scenarios-types agent
must make `crate::eval::scenarios::types` (or the needed items) `pub`. My component is fully green
in isolation; full-workspace green is gated on sibling Wave-1 server work landing.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern) + context_get #4897 (ADR-001) -- confirmed the
  clamp-ceiling/clean_replacement coupling, dual-default discipline (#4064), and amplified-knob
  semantics; no prior pattern existed for the clamp min>max inversion.
- Stored: entry #4900 "f64::clamp panics when a swept ceiling falls below a literal floor
  (graph_penalty_with)" via /uni-store-pattern.
