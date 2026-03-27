## ADR-002: `write_inferred_edges_with_cap` as a Named Variant, Not Reuse of `write_edges_with_cap`

### Context

`write_edges_with_cap` in `nli_detection.rs` writes edges for one source entry against a
list of `(neighbor_id, text)` tuples. Its signature:

```rust
async fn write_edges_with_cap(
    store: &Store,
    source_id: u64,
    neighbor_texts: &[(u64, String)],
    nli_scores: &[NliScores],
    nli_entailment_threshold: f32,
    nli_contradiction_threshold: f32,
    max_edges: usize,
) -> usize
```

The tick path works differently: it has a flat `Vec<(u64, u64)>` of mixed-source pairs
(source_id, target_id) already scored, with no `neighbor_texts` needed at write time
(texts were fetched in Phase 6 but are not needed during the INSERT step). The threshold
parameter has a different semantic: the tick uses `supports_edge_threshold` (default 0.7)
rather than `nli_entailment_threshold` (default 0.6) to guard against the higher false-
positive rate of the broader pair space.

SR-08 (scope risk assessment) explicitly flags that cap logic must be a testable unit
function from the start (entry #2800). Wrapping `write_edges_with_cap` with adapter code
to force the wrong shape would obscure the cap boundary and make tests awkward.

Alternatives considered:
1. **Reuse `write_edges_with_cap` directly** — not possible: signature mismatch on
   `source_id` (scalar) vs. mixed pairs (Vec), and text is not needed at write time.
2. **Refactor `write_edges_with_cap` to accept a generic pair slice** — would change an
   already-working function used by `run_post_store_nli`, adding regression risk with no
   benefit. Rejected.
3. **Named variant** — clean, independently testable, zero coupling to the post-store path.

### Decision

Introduce `write_inferred_edges_with_cap` as a standalone private async function in
`nli_detection_tick.rs`. The function is **Supports-only** — it writes no `Contradicts` edges.
The `contradiction_threshold` parameter is intentionally absent (see rationale below).

```rust
async fn write_inferred_edges_with_cap(
    store: &Store,
    pairs: &[(u64, u64)],           // (source_id, target_id), already ordered and capped
    nli_scores: &[NliScores],
    supports_threshold: f32,        // config.supports_edge_threshold
    max_edges: usize,               // config.max_graph_inference_per_tick
) -> usize
```

It uses `write_nli_edge` (promoted to `pub(crate)` from `nli_detection.rs`) and
`format_nli_metadata` for the INSERT. The cap stops processing once `edges_written >= max_edges`.
Only `scores.entailment` is evaluated; `scores.contradiction` is not read.

**Rationale for Supports-only design (human decision)**: the tick's job is Supports edge
inference. Contradiction detection has its own dedicated path (`run_post_store_nli` and the
contradiction scan in `infra/contradiction.rs`). Mixing both in the tick creates two independent
code paths that can drift out of calibration. The prior design passed `contradiction_threshold`
as an explicit parameter to enforce a threshold floor — the new design removes the concern
entirely by not writing `Contradicts` edges in the tick at all. This is a stronger guarantee.

`write_edges_with_cap` in `nli_detection.rs` (which does write Contradicts edges) is unchanged
and continues to handle the post-store NLI path. The two functions are parallel, not
hierarchical — each owns its call path.

### Consequences

The cap boundary is independently testable without a live ONNX model: construct mock
`NliScores` vectors with known scores, pass them to `write_inferred_edges_with_cap`, verify
edge counts. This is identical to the crt-023 test pattern (entry #2728).

Removing the `contradiction_threshold` parameter eliminates the entire class of false-positive
`Contradicts` edges from the tick — no threshold tuning required. The function is strictly
simpler and its test surface is smaller.

A future caller cannot accidentally pass a softer contradiction threshold because the function
has no such parameter. Code review of the function signature is sufficient to verify the
Supports-only invariant.
