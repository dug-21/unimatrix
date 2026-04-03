# Agent Report: crt-042-agent-1-pseudocode

## Scope

Produced per-component pseudocode for all four crt-042 components plus OVERVIEW.md.

## Files Produced

- `product/features/crt-042/pseudocode/OVERVIEW.md`
- `product/features/crt-042/pseudocode/graph_expand.md`
- `product/features/crt-042/pseudocode/phase0_search.md`
- `product/features/crt-042/pseudocode/inference_config.md`
- `product/features/crt-042/pseudocode/eval_profile.md`

## Source Material Read

- `product/features/crt-042/IMPLEMENTATION-BRIEF.md` — primary reference; all function
  signatures, constraints, and resolved decisions sourced from here.
- `product/features/crt-042/architecture/ARCHITECTURE.md` — integration surface, component
  interactions, combined ceiling, lock ordering.
- `product/features/crt-042/specification/SPECIFICATION.md` — FR/NFR/AC requirements; all
  test scenario derivations trace to ACs.
- `product/features/crt-042/RISK-TEST-STRATEGY.md` — risk register; R-01 through R-17 used
  to populate test scenarios in each component file.
- All six ADR files in `architecture/ADR-*.md`.
- `crates/unimatrix-engine/src/graph.rs` — confirmed `#[path]` submodule pattern, exact
  `edges_of_type` signature, `TypedRelationGraph` fields, `RelationType` variants.
- `crates/unimatrix-engine/src/graph_ppr.rs` — confirmed structural mirror pattern (file
  header, imports, test split declaration, no lib.rs entry).
- `crates/unimatrix-server/src/services/search.rs` — confirmed exact insertion point (line
  857 `if !use_fallback`), Phase 1–5 structure, `results_with_scores` type, Phase 5 quarantine
  pattern, `SearchService` struct fields, `new()` parameter list.
- `crates/unimatrix-server/src/infra/config.rs` — confirmed four coordinated sites, serde
  default function naming pattern, validate() PPR block pattern, merge function pattern.
- `crates/unimatrix-server/src/services/mod.rs` — confirmed SearchService::new() call site
  with inference_config.ppr_* argument pattern.
- Existing eval profiles `conf-boost-c.toml` and `synthetic-ppr-enabled.toml` — confirmed
  TOML format.

## Unimatrix Queries

- `context_search(graph expand BFS traversal patterns, category: pattern)` — returned entries
  #3650, #2429, #2403, #3740, #3950. Most relevant: #3740 (submodule pattern), #3950
  (RelationType extension checklist, silent-drop risk).
- `context_search(crt-042 architectural decisions, category: decision, topic: crt-042)` —
  returned ADR-004 (#4052), ADR-002 (#4050), ADR-006 (#4054). Confirmed all six ADRs
  by reading ADR files directly.
- `context_search(InferenceConfig serde default config validation, category: pattern)` —
  returned entries #3817, #2730, #4044, #3928, #4013. Critical: #3817 (dual-site atomic
  change), #4044 (hidden test sites), #2730 (..Default::default() requirement).
- `context_search(search pipeline Phase 0 PPR personalization vector, category: pattern)` —
  returned entries #3746, #3753, #3637, #3156, #3744. Most relevant: #3753 (pre-cloned lock
  snapshot pattern), #3637 (tracing:: qualified — bare debug! unresolved in search.rs).

## Decisions Made / Patterns Followed

All algorithmic decisions trace to architecture ADRs:
- BFS with sorted neighbor deduplication per node — NFR-04, ADR-004 crt-030.
- `neighbors.sort_unstable(); neighbors.dedup()` before queue insertion — determinism with
  multi-edge graphs where two edge types point to the same target.
- `can_expand_further` flag at depth==depth prevents depth+1 neighbor enqueuing without
  preventing the depth==depth node from being added to result.
- `in_pool` HashSet built from `seed_ids` (not `results_with_scores` entry IDs) in Phase 0
  pseudocode. This is safe because `graph_expand` already excludes seeds; `in_pool` is a
  belt-and-suspenders deduplication guard. An alternative is to build `in_pool` from
  `results_with_scores.iter().map(|(e,_)| e.id)` directly — both are correct, but using
  `seed_ids` (already collected) avoids a second pass.

## Open Questions / Gaps

**OQ-A (SR-01 investigation, delivery blocking):** The pseudocode documents the O(1) path
investigation in `phase0_search.md`. The delivery agent must investigate
`VectorIndex.id_map.entry_to_data` before choosing between the O(1) and O(N) embedding
lookup paths. The pseudocode currently specifies the O(N) path (`vector_store.get_embedding`)
as the implementation fallback with an explicit investigation note.

**OQ-B (SR-03 gate, delivery blocking):** S1/S2 directionality must be confirmed before Phase 0
code is written. The pseudocode documents the behavioral test for the single-direction failure
mode. No pseudocode change is needed if the back-fill is applied — the Phase 0 code is the same
either way; the back-fill changes what edges exist in the graph, not how Phase 0 traverses them.

**OQ-C (merge function ppr_expander_enabled type):** `ppr_expander_enabled` is a `bool`. The
merge pseudocode uses `!=` comparison (project wins if non-default). This is correct for bool
but implementation must verify there is no float-epsilon issue — there isn't, since bool is
equality-comparable. No ambiguity.

**OQ-D (sorted_expanded in Phase 0):** The IMPLEMENTATION-BRIEF pseudocode sketch uses
`.sorted()` on `expanded_ids.iter()` (implying an iterator adapter, likely from itertools).
The pseudocode file uses `let mut sorted_expanded: Vec<u64> = ...; sorted_expanded.sort_unstable()`.
Both are correct. The implementation agent should confirm whether `itertools` is already
imported in search.rs and use whichever pattern is consistent with existing code.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` four times — findings summary above.
- Deviations from established patterns: none. All patterns followed exactly:
  - `#[path]` submodule pattern (entry #3740).
  - `edges_of_type()` sole traversal boundary (entry #3627).
  - `InferenceConfig` four-site atomic addition (entries #3817, #2730, #4044).
  - Pre-cloned lock snapshot for new pipeline steps (entry #3753).
  - `tracing::debug!` qualified macro in search.rs (entry #3637).
  - Unconditional validation (ADR-004 crt-042, entry #4052).
