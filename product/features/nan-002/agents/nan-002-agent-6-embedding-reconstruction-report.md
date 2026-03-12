# nan-002-agent-6-embedding-reconstruction Report

## Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/embed_reconstruct.rs` (created)
- `/workspaces/unimatrix/crates/unimatrix-server/src/lib.rs` (modified -- added `pub mod embed_reconstruct;`)

## Implementation Summary

Created `embed_reconstruct.rs` as a standalone public module (not inlined in `import.rs` to avoid file conflicts with the import-pipeline agent). The module exposes a single public function:

```rust
pub fn reconstruct_embeddings(
    store: &Arc<Store>,
    vector_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>>
```

The function follows the validated pseudocode exactly:
1. Initializes `OnnxProvider` with `EmbedConfig::default()`
2. Reads all entries (id, title, content) from the committed DB, then drops the connection lock
3. Creates a new `VectorIndex`
4. Batch-embeds entries (64 per batch) with progress reporting to stderr
5. Inserts each embedding into the vector index
6. Creates the vector directory and persists the index via `dump()`

Internal helper `read_entries()` is separated to keep the main function focused and release the DB lock before CPU-bound embedding work.

### Signature Note for import-pipeline Agent

The function accepts `&Arc<Store>` and `&Path` (vector_dir) rather than `&ProjectPaths` to minimize coupling. The import-pipeline agent should call it as:

```rust
embed_reconstruct::reconstruct_embeddings(&store, &paths.vector_dir)?;
```

## Tests: 6 passed, 0 failed

| Test | Status |
|------|--------|
| test_embed_batch_size_constant | PASS |
| test_batch_count_calculation | PASS |
| test_read_entries_empty_db | PASS |
| test_read_entries_returns_id_title_content | PASS |
| test_read_entries_ordered_by_id | PASS |
| test_read_entries_multiple_entries | PASS |

Integration tests (per test plan) require the ONNX model and a full import pipeline. These belong in the integration test suite and will be exercised when the import-pipeline agent completes.

## Self-Check

- [x] `cargo build --package unimatrix-server --lib` passes (zero errors)
- [x] `cargo test --package unimatrix-server --lib embed_reconstruct` passes (6/6)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK`
- [x] All modified files within scope
- [x] Error handling uses `.map_err()` with descriptive context
- [x] No `.unwrap()` in non-test code
- [x] `EntryTriple` type alias has `Debug` via tuple components
- [x] Code follows validated pseudocode -- no deviations
- [x] File is 344 lines (under 500-line limit)
- [x] `cargo clippy` -- zero warnings from embed_reconstruct.rs
- [x] `cargo fmt` applied

## Issues

None. No blockers encountered.

## Knowledge Stewardship

- Queried: No `/query-patterns` available (knowledge server not running in this context) -- proceeded with source code inspection of existing API patterns in unimatrix-embed, unimatrix-vector, and export.rs
- Stored: Nothing novel to store -- the implementation is a straightforward orchestration of existing APIs (OnnxProvider, embed_entries, VectorIndex). No gotchas discovered beyond what is already documented in the pseudocode (lock release before CPU work, div_ceil for batch counting).
