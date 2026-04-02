# Agent Report: crt-040-agent-1-pseudocode

## Task

Produce per-component pseudocode for crt-040 (Cosine Supports Edge Detection) across
4 components and 3 implementation waves.

## Files Produced

| File | Purpose |
|------|---------|
| `product/features/crt-040/pseudocode/OVERVIEW.md` | Component map, data flow, wave dependencies, shared types |
| `product/features/crt-040/pseudocode/store-constant.md` | Wave 1a: EDGE_SOURCE_COSINE_SUPPORTS constant in read.rs + lib.rs re-export |
| `product/features/crt-040/pseudocode/inference-config.md` | Wave 1b: supports_cosine_threshold (5 sites) + nli_post_store_k removal (6 sites) + merge function |
| `product/features/crt-040/pseudocode/write-graph-edge.md` | Wave 2: write_graph_edge sibling function, write_nli_edge immutability |
| `product/features/crt-040/pseudocode/path-c-loop.md` | Wave 3: full Path C loop with all 4 guards, category HashMap pre-build, observability log |

## Components Covered

1. `unimatrix-store/src/read.rs` + `lib.rs` — EDGE_SOURCE_COSINE_SUPPORTS constant (Wave 1a)
2. `unimatrix-server/src/infra/config.rs` — InferenceConfig field + removal (Wave 1b)
3. `unimatrix-server/src/services/nli_detection.rs` — write_graph_edge sibling (Wave 2)
4. `unimatrix-server/src/services/nli_detection_tick.rs` — Path C loop + constant (Wave 3)

## Key Grounding from Codebase

- Verified `write_nli_edge` exact signature and SQL literal `'nli', 'nli'` in nli_detection.rs
- Verified `EDGE_SOURCE_NLI` / `EDGE_SOURCE_CO_ACCESS` constant pattern and lib.rs re-export format
- Verified all 6 `nli_post_store_k` sites in config.rs (lines ~296-310, ~596, ~646-648, ~843-850, ~2222-2228, test assertions)
- Verified `nli_informs_cosine_floor` merge pattern (f32 epsilon comparison, lines ~2414-2422) as template for `supports_cosine_threshold` merge
- Verified `MAX_INFORMS_PER_TICK = 25` location (line 51) for adjacent constant placement
- Verified `candidate_pairs` is `Vec<(u64, u64, f32)>` in canonical `(lo, hi)` form from Phase 4
- Verified `category_map` already being built in Phase 5 for the sort (line ~419) — the Path C pre-build uses the same `all_active` source but must be a separate build timed before Phase 5 sort (both use the same data)
- Verified Path A observability log structure (fields: `informs_candidates_found`, `informs_candidates_after_dedup`, `informs_candidates_after_cap`, `informs_edges_written`) — confirmed no collision with Path C field names
- Verified `timestamp` is declared at Path A entry (`let timestamp = current_timestamp_secs()`) and must be reused in Path C

## Critical Implementation Decisions Embedded

| Decision | Pseudocode Location | Source |
|----------|-------------------|--------|
| write_nli_edge NOT modified | write-graph-edge.md | WARN-04, FR-12, ADR-001 |
| `write_graph_edge` returns `rows_affected()>0`: true=insert, false=UNIQUE conflict or SQL error | write-graph-edge.md, path-c-loop.md | R-07, ADR-001, gate-3a rework |
| UNIQUE conflict (false, rows_affected=0) is NOT an error — no warn! at call site | write-graph-edge.md, path-c-loop.md | R-07, ADR-001 |
| Budget counter only on true return (rows_affected=1) | path-c-loop.md | RISK-TEST-STRATEGY failure modes |
| Joint early-return removed so Path C observability log fires unconditionally | path-c-loop.md | AC-19, gate-3a rework |
| Observability log unconditional after loop | path-c-loop.md | WARN-02, ADR-003, R-06 |
| HashMap pre-build mandatory (no per-pair DB lookup) | path-c-loop.md, OVERVIEW.md | WARN-01, NFR-09, R-01, R-10 |
| Guard order: finite → threshold → budget → category → pre-filter | path-c-loop.md | IMPLEMENTATION-BRIEF.md §Path C Loop Pseudocode |
| Dual-site default (serde fn + impl Default calling backing fn) | inference-config.md | ADR-002, R-03 |
| All 6 nli_post_store_k removal sites | inference-config.md | FR-09, AC-17 |
| merge function f32 epsilon pattern | inference-config.md | R-13, SPECIFICATION.md FR-08 |

## Gate-3a Rework: write_graph_edge return-value contract (resolved)

Gate-3a identified an inconsistency: the original pseudocode body returned `true` for
BOTH successful inserts AND silent UNIQUE conflicts, but path-c-loop.md's error handling
table and TC-08 assumed `false` for UNIQUE conflicts.

Resolution (gate-3a rework): `write_graph_edge` returns `rows_affected() > 0` from the
`Ok` arm. Contract:
- `true` = row inserted (`rows_affected = 1`)
- `false` = UNIQUE conflict (`rows_affected = 0`, no log emitted inside fn)
- `false` = SQL error (warn! emitted inside fn, NOT at call site)

This aligns write-graph-edge.md, path-c-loop.md error handling table, TC-04, TC-08, and
OVERVIEW.md invariants. The delivery agent must use `query_result.rows_affected() > 0`
as the return expression in the `Ok` arm.

## Phase 5 category_map note

Phase 5 in the current tick already builds a temporary `category_map` for the sort
comparator (around line 419: `let category_map: HashMap<u64, &str> = all_active.iter()...`).
The Path C `category_map` uses `String` values rather than `&str` to avoid borrow
lifetime complexity. The delivery agent should evaluate whether to unify the two maps
(lifting the Phase 5 map declaration earlier to serve both Phase 5 sort and Path C).
If unified, use the `HashMap<u64, &str>` form and ensure lifetimes are satisfied for
the full tick function body. This is a delivery-time decision; both approaches are correct.

## Gate-3a Rework: AC-19 early-return resolution (resolved)

Gate-3a identified that the joint early-return in Phase 5 of `run_graph_inference_tick`
suppresses Path C's observability log when both `candidate_pairs` and `informs_metadata`
are empty. AC-19 requires the log to fire unconditionally.

Confirmed codebase early-return (nli_detection_tick.rs line ~452):
```
if candidate_pairs.is_empty() && informs_metadata.is_empty() { return; }
```

Resolution: **Remove this joint early-return entirely.** See path-c-loop.md §"AC-19
Resolution" for full rationale. The delivery agent must delete this block. Path A and
Path C loops are self-guarding (zero iterations when inputs are empty). The Path B entry
gate (`if candidate_pairs.is_empty()`) at line ~514 is RETAINED — it guards NLI batch
only and is positioned after Path C.

## Open Questions

None blocking. The following are delivery-time evaluation points (not design gaps):

1. **Category map unification with Phase 5 sort map** — Whether to lift the Phase 5
   `category_map` declaration earlier to serve both the sort and Path C. Either approach
   is correct; noted in path-c-loop.md.

2. **500-line extraction** — Whether the tick function body exceeds ~150 lines after
   Path C is added. Evaluate at implementation time per NFR-07. Extraction guidance
   provided in path-c-loop.md.

3. **crt-041 rebase** — If crt-041 merges before crt-040, the `impl Default` struct
   literal will have changed. Verify the removal of `nli_post_store_k` and addition of
   `supports_cosine_threshold` apply cleanly to the rebased struct (IR-03).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "write_nli_edge graph_edges edge writer sibling pattern" → found #4025 (write_nli_edge hardcodes source='nli'; sibling pattern), #3884 (INSERT-OR-IGNORE pattern), #3950 (RelationType extension checklist). Entry #4025 directly validates ADR-001.
- Queried: `mcp__unimatrix__context_search` for "crt-040 architectural decisions" → found #4030 (ADR-004 budget), #4028 (ADR-002 dual-site), #4027 (ADR-001 edge writer). All three ADRs confirmed in Unimatrix.
- Queried: `mcp__unimatrix__context_search` for "write_nli_edge INSERT OR IGNORE graph_edges source nli pattern" → found #4025 (0.71 similarity), #4027 (ADR-001), #3591 (EDGE_SOURCE_NLI naming pattern from col-029). All three reinforce the constant naming and edge writer decisions.
- Deviations from established patterns: none. All pseudocode follows the `EDGE_SOURCE_*` constant pattern (#3591), the `write_graph_edge` sibling pattern (#4025), the INSERT-OR-IGNORE idempotency pattern (#3884), and the dual-site InferenceConfig default pattern (#3817 / lesson #4014).
