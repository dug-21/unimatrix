## ADR-004: Single Indexed Pass for Mask Application (SR-02)

### Context

At Step 10b in `SearchService::search`, suppression must be applied to two parallel data
structures:
- `results_with_scores: Vec<(EntryRecord, f64)>` — the floor-filtered, sorted result set
- `final_scores: Vec<f64>` — the fused scores, built at line 893 from the `scored` Vec
  before floors; NOT reduced by the Step 10 floor `retain` calls

After Step 10 floors, `results_with_scores.len()` may be less than `final_scores.len()`.
The comment at Step 11 (line 909-910) documents this explicitly: "zip stops at the shorter
iterator, which is correct." The aligned prefix is `final_scores[..results_with_scores.len()]`.

SR-02 risk: filtering the two Vecs with separate iterator chains or separate `retain` calls
would silently misalign them if not synchronized precisely. A misalignment produces wrong
`final_score` values in the `ScoredEntry` output — not a panic, just silently incorrect data.

Three approaches considered:
1. **Two separate `retain` calls** — one on each Vec using index tracking. Fragile: index
   tracking across a `retain` on one Vec while the other is not being filtered is error-prone.
2. **Separate Vecs reconstructed from a shared mask** — build `Vec<bool>` mask, then
   iterate both Vecs by index using the mask. Correct, explicit.
3. **Zip-and-unzip in one pass** — zip the aligned prefix of both Vecs, apply mask, unzip
   into new Vecs. Single loop, no index tracking, no separate passes.

### Decision

Use approach 3: a single `enumerate()` pass over a zipped iterator of the aligned prefix:

```rust
let aligned_len = results_with_scores.len();
let keep_mask = suppress_contradicts(&result_ids, &typed_graph);

let mut new_rws: Vec<(EntryRecord, f64)> = Vec::with_capacity(aligned_len);
let mut new_fs: Vec<f64> = Vec::with_capacity(aligned_len);
for (i, (rw, &fs)) in results_with_scores
    .iter()
    .zip(final_scores[..aligned_len].iter())
    .enumerate()
{
    if keep_mask[i] {
        new_rws.push(rw.clone());
        new_fs.push(fs);
    }
}
results_with_scores = new_rws;
let final_scores = new_fs;  // shadow the immutable binding
```

This is a single pass, filters both Vecs simultaneously by the same mask index, and never
calls `retain` on either Vec separately. The `aligned_len` slice bound makes the
`results_with_scores` / `final_scores` alignment explicit in the code.

Note: `final_scores` is a `let` (not `let mut`) binding at line 893. The implementation
agent must shadow it with `let final_scores = new_fs;` at Step 10b output. The
implementation brief must call this out explicitly.

### Consequences

- SR-02 risk is eliminated: both Vecs are always filtered by the same mask in the same pass.
- The alignment between `results_with_scores` and `final_scores` at Step 11 is preserved.
- The implementation is explicit about the aligned prefix — `final_scores[..aligned_len]`
  makes the floor-desynced state visible in the code.
- Performance is O(n) with constant factor ~2 (clone of `(EntryRecord, f64)` per kept entry).
  For k ≤ 20 this is negligible.
- Future implementors cannot accidentally introduce a separate `retain` that would break
  alignment — the pattern is self-documenting.
