# Component: Step 10b insertion in `search.rs`

## Purpose

Insert the Contradicts collision suppression step into `SearchService::search` between
Step 10 (similarity/confidence floors) and Step 11 (ScoredEntry construction). This is
the sole call site for `suppress_contradicts`. It applies the returned bitmask to both
`results_with_scores` and the aligned prefix of `final_scores` in a single indexed pass,
emits a `debug!` log for each suppressed entry, and shadows `final_scores` so Step 11
operates on the post-suppression Vec.

---

## File Location

`crates/unimatrix-server/src/services/search.rs`

---

## Insertion Point

After line 904 (end of Step 10 confidence floor `retain`) and before line 906 (Step 11
comment). The block is inserted as a labeled comment section "Step 10b".

---

## Import Addition

Add `suppress_contradicts` to the existing `unimatrix_engine::graph` import on line 20:

```
// Before:
use unimatrix_engine::graph::{FALLBACK_PENALTY, find_terminal_active, graph_penalty};

// After:
use unimatrix_engine::graph::{FALLBACK_PENALTY, find_terminal_active, graph_penalty, suppress_contradicts};
```

No other import changes. `tracing::debug!` is already available via the existing `tracing`
import in `search.rs`.

---

## Existing Variable Context (at insertion point)

| Variable | Type | Binding | Source |
|----------|------|---------|--------|
| `use_fallback` | `bool` | `let` (from destructure at line 611) | cloned from `TypedGraphState` under read lock at Step 6 |
| `typed_graph` | `TypedRelationGraph` | `let` (from destructure at line 611) | cloned from `TypedGraphState` under read lock at Step 6 |
| `results_with_scores` | `Vec<(EntryRecord, f64)>` | `let mut` (line 594, reassigned at line 892) | floor-filtered, sorted DESC by final_score |
| `final_scores` | `Vec<f64>` | `let` (line 893) — NOT `let mut` | parallel to `results_with_scores` before floors; may be longer |

CRITICAL (R-03): `final_scores` at line 893 is a `let` (immutable) binding. The Step 10b
block must NOT change line 893 to `let mut`. It must shadow `final_scores` with
`let final_scores = new_fs;` after the mask-application loop.

CRITICAL (R-07): `aligned_len` must be computed as `results_with_scores.len()`, not
`final_scores.len()`. After Step 10 floors, `results_with_scores` may be shorter than
`final_scores`. The slice `final_scores[..aligned_len]` is the only correct prefix.

---

## Step 10b Block Pseudocode

```
// Step 10b: Contradicts collision suppression (col-030).
// Guard: only when TypedRelationGraph is built (use_fallback = false).
// When use_fallback = true (cold-start), skip entirely — all results pass through unchanged (AC-05).
// Both Vecs are rebuilt in a single indexed pass to preserve the parallel Vec invariant (ADR-004, SR-02).
if !use_fallback {

    // Extract result IDs in rank order (index 0 = highest ranked = lowest final_scores index)
    let result_ids: Vec<u64> = results_with_scores
        .iter()
        .map(|(entry, _)| entry.id)
        .collect()

    // Compute keep/drop mask and contradicting IDs.
    // suppress_contradicts returns (Vec<bool>, Vec<Option<u64>>) of length result_ids.len().
    let (keep_mask, contradicting_ids) = suppress_contradicts(&result_ids, &typed_graph)

    // aligned_len MUST be results_with_scores.len(), NOT final_scores.len() (R-07).
    // After Step 10 floors, results_with_scores may be shorter than final_scores.
    let aligned_len = results_with_scores.len()

    let mut new_rws: Vec<(EntryRecord, f64)> = Vec::with_capacity(aligned_len)
    let mut new_fs:  Vec<f64>                = Vec::with_capacity(aligned_len)

    // Single indexed pass over zip of the aligned prefix (ADR-004).
    // Never two separate retain calls on each Vec — that violates SR-02 (silently misaligns).
    for (i, (rw, &fs)) in results_with_scores
        .iter()
        .zip(final_scores[..aligned_len].iter())
        .enumerate()
    {
        if keep_mask[i] {
            new_rws.push(rw.clone())
            new_fs.push(fs)
        } else {
            // FR-09, NFR-05: emit DEBUG log with both IDs.
            // contradicting_ids[i] is Some(id) when keep_mask[i] == false (guaranteed by suppress_contradicts).
            // rw is &(EntryRecord, f64); rw.0 is the EntryRecord.
            debug!(
                suppressed_entry_id    = rw.0.id,
                contradicting_entry_id = contradicting_ids[i],   // Option<u64> — tracing formats as Some(id)
                "contradicts collision suppression: entry suppressed"
            )
        }
    }

    // Reassign results_with_scores (it is let mut from line 594)
    results_with_scores = new_rws

    // SHADOW final_scores — do NOT add let mut at line 893 (R-03 critical trap).
    // This shadow takes precedence for the rest of the function (Step 11 zip uses this binding).
    let final_scores = new_fs
}
// end Step 10b
```

---

## Post-condition for Step 11

After Step 10b exits, the following invariant holds regardless of the `if !use_fallback` branch:

- `results_with_scores` contains only kept entries (suppressed entries removed, or unmodified
  when `use_fallback = true`).
- `final_scores` is either the new shadow Vec (suppressed entries removed) or the original
  `let` binding from line 893 (unchanged when `use_fallback = true` — the shadow is only
  introduced inside the `if` block and does not outlive it when the branch is not taken).

Wait — there is a scoping issue with the shadow. If `let final_scores = new_fs;` is declared
inside the `if !use_fallback { }` block, it does not shadow the outer binding in Step 11.
Step 11 would use the original `final_scores` (line 893) when the `if` branch executes.

RESOLUTION: The shadow must be placed such that it is visible at Step 11. Two correct approaches:

Option A (recommended by ADR-004): Declare `final_scores` as `let mut` at a scope outer to
the `if` block — but the architecture explicitly prohibits making line 893 `let mut` (R-03).

Option B (correct implementation): Introduce a new binding that wraps both paths:

```
// After Step 10 floors, before Step 10b:
let final_scores = if !use_fallback {
    // ... [mask computation and loop] ...
    // returns new_fs (the post-suppression Vec)
    new_fs
} else {
    // cold-start: pass through
    final_scores   // moves the original binding
};
```

This form:
- Does not add `let mut` to line 893 (R-03 satisfied)
- Produces a single `let final_scores` binding at the outer scope visible to Step 11
- Is a legitimate Rust expression block (the `if` expression evaluates to a `Vec<f64>`)
- `results_with_scores` is still reassigned only inside the `!use_fallback` branch

REVISED Step 10b pseudocode (option B — correct scoping):

```
// Step 10b: Contradicts collision suppression (col-030).
// Uses if-expression to re-bind final_scores at the correct scope for Step 11 (ADR-004, R-03).
let final_scores = if !use_fallback {

    let result_ids: Vec<u64> = results_with_scores
        .iter()
        .map(|(entry, _)| entry.id)
        .collect();

    let (keep_mask, contradicting_ids) = suppress_contradicts(&result_ids, &typed_graph);

    let aligned_len = results_with_scores.len();   // NOT final_scores.len() (R-07)

    let mut new_rws: Vec<(EntryRecord, f64)> = Vec::with_capacity(aligned_len);
    let mut new_fs:  Vec<f64>               = Vec::with_capacity(aligned_len);

    for (i, (rw, &fs)) in results_with_scores
        .iter()
        .zip(final_scores[..aligned_len].iter())
        .enumerate()
    {
        if keep_mask[i] {
            new_rws.push(rw.clone());
            new_fs.push(fs);
        } else {
            debug!(
                suppressed_entry_id    = rw.0.id,
                contradicting_entry_id = contradicting_ids[i],
                "contradicts collision suppression: entry suppressed"
            );
        }
    }

    results_with_scores = new_rws;
    new_fs   // expression: this Vec<f64> is the new value of the outer final_scores binding

} else {
    final_scores   // cold-start: original Vec<f64> passes through unchanged
};
// final_scores is now the post-suppression Vec (or original on cold-start)
// Step 11 uses this binding — alignment with results_with_scores is preserved
```

Note: In Rust, `final_scores` (the original `let` binding from line 893) is MOVED into the
`else` branch. This is valid because `final_scores` is not used again after this block (Step 11
uses the new `let final_scores` binding). The compiler enforces this; if a use-after-move were
introduced, it would be a compile error — not a silent bug.

---

## Data Flow Summary

```
BEFORE Step 10b:
  results_with_scores: Vec<(EntryRecord, f64)>    -- floor-filtered, len = n_after_floors
  final_scores:        Vec<f64>                   -- NOT floor-filtered, len >= n_after_floors
  use_fallback:        bool

STEP 10b (if !use_fallback):
  result_ids      = [entry.id for (entry, _) in results_with_scores]     -- len = n_after_floors
  (keep_mask, contradicting_ids) = suppress_contradicts(result_ids, graph)
  aligned_len     = n_after_floors                                         -- NOT final_scores.len()
  new_rws = kept entries from results_with_scores
  new_fs  = kept scores from final_scores[..aligned_len]
  results_with_scores <- new_rws                                           -- len = n_kept
  final_scores    <- new_fs (via outer let rebind)                        -- len = n_kept

AFTER Step 10b:
  results_with_scores: Vec<(EntryRecord, f64)>    -- n_kept entries (n_kept <= n_after_floors)
  final_scores:        Vec<f64>                   -- n_kept scores (strictly parallel)
  INVARIANT: results_with_scores.len() == final_scores.len()             -- Step 11 zip is exact
```

---

## Error Handling

`suppress_contradicts` is a pure function that cannot fail. No error propagation needed.

`results_with_scores.iter().zip(final_scores[..aligned_len].iter())` slices `final_scores`
by `aligned_len = results_with_scores.len()`. Since `final_scores.len() >= results_with_scores.len()`
after Step 10 floors (floor only shortens `results_with_scores`), this slice is always valid.
No panic risk from the slice — this is the same invariant relied on by the existing Step 11 zip.

`keep_mask[i]` is indexed with `i` from the `enumerate()` on `results_with_scores`. Since
`keep_mask.len() == result_ids.len() == results_with_scores.len()` (guaranteed by
`suppress_contradicts`), all accesses are in bounds. If `suppress_contradicts` returns a
shorter mask, Step 10b panics — AC-01 unit test in `graph_suppression.rs` is the pre-delivery
guard for this invariant.

---

## Key Test Scenarios

### Integration test (mandatory — FR-14, AC-07, SR-05)

Location: `search.rs` test module.

Setup:
- Create three `EntryRecord` entries: A (will rank highest), B (will rank lower), C (mid-rank,
  no contradiction).
- Ensure A's embedding is close to the query, B's is further, C's is between them by score.
- Construct a `TypedRelationGraph` with one Contradicts edge between A and B using
  `build_typed_relation_graph` with a `GraphEdgeRow` slice:
  `GraphEdgeRow { source_id: A.id, target_id: B.id, relation_type: "Contradicts".to_string(),
  weight: 1.0, bootstrap_only: false, ... }`
- Do NOT use `create_graph_edges_table` (pre-v13 schema; SR-07, R-12).
- Seed the graph into the `TypedGraphState` with `use_fallback = false`.

Assertions:
- `SearchService::search` result contains A.
- `SearchService::search` result does NOT contain B.
- `SearchService::search` result contains C.
- Result length is 2 (k=3 minus 1 suppressed).
- `ScoredEntry.final_score` for A and C match pre-suppression computed scores (R-03 validation).

### Floor + suppression combo test (R-07)

Setup:
- Four entries: A (high score, above floor), B (score below similarity_floor), C (contradicts A,
  above floor), D (above floor, no contradiction).
- `similarity_floor` set to remove B.
- Contradicts edge between A and C.

Assertions:
- B absent (removed by floor at Step 10).
- C absent (removed by suppression at Step 10b — contradicts A which ranks higher).
- A and D present.
- `ScoredEntry.final_score` for A and D match their pre-suppression scores (not B's or C's scores).
- This test validates `aligned_len = results_with_scores.len()` (R-07): if the implementation
  mistakenly uses `final_scores.len()` as `aligned_len`, the zip slices more scores than
  results and the scores pair incorrectly.

### Cold-start guard test (AC-05)

- Existing cold-start tests in `search.rs` continue passing without modification.
- These tests set `use_fallback = true` and assert results are returned unchanged.
- The `if !use_fallback` guard ensures Step 10b is a no-op in this path.
- No new test required; existing tests serve as the regression check.
