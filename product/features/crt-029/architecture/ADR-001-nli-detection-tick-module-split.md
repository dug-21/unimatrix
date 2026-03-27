## ADR-001: New Module `nli_detection_tick.rs` for Background Inference Tick

### Context

`nli_detection.rs` currently stands at 1,373 lines (measured at architecture time). The
500-line guidance in `rust-workspace.md` is already violated by a factor of 2.7. Adding
`run_graph_inference_tick` and its private helpers (source selection, edge writing, pair
deduplication) inline would push the file to approximately 1,600–1,700 lines, making it
harder to review and test.

The SCOPE.md already anticipated this: "Split to `nli_detection_tick.rs` if the combined
file exceeds 500 lines. Do not merge if the combined file exceeds 800 lines."

The file split also makes the architectural separation explicit: `nli_detection.rs` owns the
reactive path (post-store NLI + bootstrap promotion); `nli_detection_tick.rs` owns the
proactive path (systematic background inference).

### Decision

Create a new file `crates/unimatrix-server/src/services/nli_detection_tick.rs` containing:
- `run_graph_inference_tick` (public async)
- `select_source_candidates` (private)
- `write_inferred_edges_with_cap` (private, but testable via `#[cfg(test)]` access)
- Any other private helpers needed by the tick function

Declare the module in `crates/unimatrix-server/src/services/mod.rs` with
`pub mod nli_detection_tick;`.

Three helpers currently private in `nli_detection.rs` must be promoted to `pub(crate)` so
the new module can use them without duplication:
- `write_nli_edge` (the low-level INSERT helper)
- `format_nli_metadata` (JSON serialisation of NliScores)
- `current_timestamp_secs` (Unix epoch helper)

`nli_detection.rs` itself is not split or reorganised. No function moves out of it; only
the three helpers change visibility.

### Consequences

Easier to review: `nli_detection_tick.rs` has a single concern and will be ~250–350 lines.

Tests for the tick path live in `nli_detection_tick.rs` under `#[cfg(test)]` (per entry
#3631: inline tests in the new sibling module when the parent file is oversized).

The three `pub(crate)` promotions are a minor visibility increase; these helpers have no
external callers and carry no security surface.

Cargo module tree: `services::nli_detection_tick` is a sibling to `services::nli_detection`.
The `background.rs` call site imports `run_graph_inference_tick` from
`crate::services::nli_detection_tick`.
