# crt-029 Researcher Report

## Agent
crt-029-researcher

## SCOPE.md
`product/features/crt-029/SCOPE.md`

## Key Findings

### What Already Exists (Zero New Infrastructure Required)

1. **`Supports` write path already works** — `write_edges_with_cap` in `nli_detection.rs`
   already writes both `Supports` and `Contradicts` edges from a single `NliScores` result.
   The post-store path (`run_post_store_nli`) calls it today. The tick pass reuses this helper.

2. **HNSW pre-filter pattern is established** — `run_post_store_nli` uses
   `vector_index.search(embedding, k, EF_SEARCH=32)` to pre-filter before NLI. Same pattern
   applies in the tick pass. No new vector index API needed.

3. **W1-2 contract is the hard constraint** — all `score_batch` calls must go via
   `rayon_pool.spawn()`. The bootstrap promotion function in the same file shows the exact
   pattern: collect pairs async → single rayon spawn → write results on tokio thread.

4. **`RelationType::Prerequisite` is defined** — the enum variant and string "Prerequisite"
   exist in `unimatrix-engine/src/graph.rs` with note "reserved for W3-1; no write path exists
   in crt-021". crt-029 adds the write path. `bootstrap_only = true` is already in schema.

5. **`query_by_status(Active)` fetches all active entries** — used by the contradiction scan
   in `background.rs` today. The tick pass uses the same call.

6. **Tick placement is clear** — after `maybe_run_bootstrap_promotion` in
   `background_tick_loop`, gated on `nli_enabled`. The existing `CONTRADICTION_SCAN_INTERVAL_TICKS = 4`
   pattern is the model for optional tick-modulo gating.

7. **col-029 metrics are the observability layer** — `isolated_entry_count`,
   `cross_category_edge_count`, and `inferred_edge_count` in `context_status` will show
   the tick pass output directly. No new observability needed.

### What Is New

1. **Three `InferenceConfig` fields** — `supports_candidate_threshold` (0.5),
   `supports_edge_threshold` (0.7), `max_graph_inference_per_tick` (100). None exist today.

2. **`run_graph_inference_tick` function** — the new recurring tick function in
   `nli_detection.rs` (or `nli_detection_tick.rs` if file size requires split).

3. **Priority ordering logic** — cross-category pairs → isolated entries → high-similarity.
   Requires knowing which entries are isolated, which requires a new store helper.

4. **`Store::query_entries_without_edges()`** — a single SQL query returning IDs of active
   entries with no non-bootstrap edges. Follows the col-029 store query pattern.

5. **`Prerequisite` write path** — the first-ever write path for `Prerequisite` edges.
   Written with `bootstrap_only = true`, asymmetric entailment condition.

### Constraints Confirmed

- `nli_detection.rs` is currently ~650 lines. The 500-line file guidance will require a split
  if the new function is substantial. The implementation team should plan for this.
- `max_contradicts_per_tick` in `InferenceConfig` is named for backward compat with SCOPE.md
  (its semantic is per-call, not per-tick). The new `max_graph_inference_per_tick` is the
  per-tick cap for the new pass. They are independent.
- The `supports_edge_threshold` default (0.7) is intentionally higher than
  `nli_entailment_threshold` (0.6) — the tick processes a larger pair space so false positives
  are more costly.

## Open Questions Surfaced

Five open questions are included in SCOPE.md. The highest-priority ones:

- **OQ-1 (tick interval gate)**: every tick vs. every N-th tick. Low-stakes; recommend every
  tick with named constant `GRAPH_INFERENCE_INTERVAL_TICKS = 1`.
- **OQ-2 (Prerequisite scope)**: whether Prerequisite inference belongs in crt-029 or is
  deferred to W3-1. The GH #412 spec includes it but the "category prerequisite-of
  relationship" is undefined. Recommend: implement pure asymmetric-entailment version
  (no category table needed), mark edges `bootstrap_only = true`.
- **OQ-5 (embedding access in tick)**: `EntryRecord` does not include the embedding vector.
  The tick needs to call HNSW search per entry, which requires the embedding. Clarify what
  `VectorIndex` exposes for bulk embedding lookup before pseudocode is written.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — 15 entries returned; entries #3628, #3591,
  #3627 (col-030/col-029 ADRs) and #2716 (crt-023 NLI ADR) were directly relevant.
- Queried: context_search (×2) — confirmed no prior pattern for single-dispatch-per-tick
  NLI atomicity; confirmed no Prerequisite write path exists.
- Stored: entry #3653 "Batch all NLI pairs into a single rayon_pool.spawn() per tick pass"
  via /uni-store-pattern.
