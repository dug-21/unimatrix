# Agent Report: crt-029-agent-1-architect

## Status: Complete

## Output Files

- `/workspaces/unimatrix/product/features/crt-029/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-029/architecture/ADR-001-nli-detection-tick-module-split.md`
- `/workspaces/unimatrix/product/features/crt-029/architecture/ADR-002-write-inferred-edges-variant.md`
- `/workspaces/unimatrix/product/features/crt-029/architecture/ADR-003-source-candidate-bound-derived.md`
- `/workspaces/unimatrix/product/features/crt-029/architecture/ADR-004-query-existing-supports-pairs.md`

## ADR Unimatrix Entry IDs

| ADR | Title | Unimatrix ID |
|-----|-------|-------------|
| ADR-001 | New Module `nli_detection_tick.rs` for Background Inference Tick | #3656 |
| ADR-002 | `write_inferred_edges_with_cap` as Named Variant | #3657 |
| ADR-003 | Source-Candidate Bound Derived from `max_graph_inference_per_tick` | #3658 |
| ADR-004 | Separate `query_existing_supports_pairs()` Store Helper | #3659 |

## Key Decisions

1. **Mandatory file split** — `nli_detection.rs` is 1,373 lines. The new tick function and
   all helpers go in `services/nli_detection_tick.rs`. Three private helpers in
   `nli_detection.rs` promoted to `pub(crate)`: `write_nli_edge`, `format_nli_metadata`,
   `current_timestamp_secs`.

2. **Named variant `write_inferred_edges_with_cap`** — cannot reuse `write_edges_with_cap`
   due to signature mismatch (flat pair list vs. single-source neighbour list) and threshold
   semantic difference. New variant is independently testable without ONNX.

3. **Source-candidate bound = `max_graph_inference_per_tick`** — `get_embedding` is O(N);
   source selection is capped to this value before any embedding lookup. No new config field.

4. **`query_existing_supports_pairs()`** — targeted `SELECT source_id, target_id WHERE
   relation_type='Supports' AND bootstrap_only=0` via `read_pool()`. Returns
   `HashSet<(u64,u64)>` directly. Lighter than `query_graph_edges()` for the pre-filter use.

5. **Contradiction threshold floor (SR-01)** — `write_inferred_edges_with_cap` takes
   `contradiction_threshold` as an explicit parameter; always passed as
   `config.nli_contradiction_threshold`. Never lower than the post-store path.

6. **W1-2 contract** — single `rayon_pool.spawn()` per tick, all pairs batched into one
   `score_batch` call. No per-pair dispatch, no `spawn_blocking`.

7. **SR-07 (`InferenceConfig` literal trap)** — four new fields must be added to the
   struct-literal `Default` impl. Pre-merge grep for `InferenceConfig {` (without
   `..default()`) required.

## Open Questions

None. All design decisions are closed.

## Notes to Delivery Agent

- `write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs` need `pub(crate)` in
  `nli_detection.rs`.
- `query_existing_supports_pairs()` may alternatively be implemented as a Rust-side filter
  over `query_graph_edges()` if implementation agent prefers; interface stays
  `HashSet<(u64,u64)>`.
- `ActiveEntryMeta` struct (`{id: u64, category: String}`) is optional; passing
  `&[EntryRecord]` to `select_source_candidates` is equally acceptable.
- `graph_inference_k` is a fourth new `InferenceConfig` field per SCOPE.md AC-04b — confirm
  it is added alongside the other three.
