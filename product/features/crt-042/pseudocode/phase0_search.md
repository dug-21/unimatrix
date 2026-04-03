# crt-042: Phase 0 in search.rs — Pseudocode

## Purpose

Async orchestration layer: calls `graph_expand`, fetches and scores expanded entries, and
merges them into `results_with_scores` before Phase 1 (personalization vector construction).
Executes only when `ppr_expander_enabled = true` AND `use_fallback = false`.

---

## Insertion Point

Phase 0 is the FIRST block inside the existing `if !use_fallback` branch in Step 6d
(approximately line 857 of search.rs). The block runs before any Phase 1 code.

Step 6d structure after crt-042:

```
// Step 6d: PPR expansion (crt-030, extended by crt-042).
if !use_fallback {

    // -----------------------------------------------------------------------
    // Phase 0 [crt-042]: graph_expand — widen seed pool if ppr_expander_enabled
    // -----------------------------------------------------------------------
    [Phase 0 block — inserted here, before Phase 1]

    // Phase 1: Build the personalization vector (FR-06 / ADR-006).
    let mut seed_scores: HashMap<u64, f64> = ...  [existing code, unchanged]
    ...

    // Phase 2: Run PPR.  [existing, unchanged]
    // Phase 3: Blend.    [existing, unchanged]
    // Phase 4: Identify PPR-only candidates.  [existing, unchanged]
    // Phase 5: Fetch and inject PPR-only entries.  [existing, unchanged]
}
```

Why before Phase 1: expanded entries must appear in `results_with_scores` when Phase 1
builds `seed_scores`, so they receive non-zero personalization mass (ADR-002).

Why inside `if !use_fallback`: Phase 0 inherits the same PPR-disable guard as Phases 1–5.
When `use_fallback = true` (cold-start or cycle), Phase 0 never executes (AC-01).

The two-flag system is orthogonal:
- `use_fallback = true` → Phase 0 does not execute (PPR entirely disabled).
- `use_fallback = false, ppr_expander_enabled = false` → Phase 0 guard fires; no-op.
- `use_fallback = false, ppr_expander_enabled = true` → Phase 0 executes fully.

---

## SearchService Struct Additions

Three new fields are added to the `SearchService` struct (after `ppr_max_expand`, ~line 381):

```rust
/// crt-042: enable graph_expand candidate pool widening before PPR.
/// Default false — gated behind A/B eval before default enablement.
ppr_expander_enabled: bool,
/// crt-042: BFS hop depth from seeds. Default 2.
expansion_depth: usize,
/// crt-042: maximum entries added by Phase 0 per query. Default 200.
max_expansion_candidates: usize,
```

## SearchService::new() Parameter Additions

Three new parameters appended to `SearchService::new()`, following the crt-030 PPR
parameter pattern (after `ppr_max_expand: usize`):

```rust
ppr_expander_enabled: bool,       // crt-042
expansion_depth: usize,           // crt-042
max_expansion_candidates: usize,  // crt-042
```

And in the constructor body, after `ppr_max_expand,`:

```rust
ppr_expander_enabled,
expansion_depth,
max_expansion_candidates,
```

## Call Site: services/mod.rs

In `mod.rs`, the `SearchService::new(...)` call is extended with three new arguments
after `inference_config.ppr_max_expand,`:

```rust
inference_config.ppr_expander_enabled,      // crt-042
inference_config.expansion_depth,           // crt-042
inference_config.max_expansion_candidates,  // crt-042
```

---

## Phase 0 Block Pseudocode

### Imports Required

`graph_expand` must be imported in search.rs. Add to the existing engine graph import block
(~line 20–23):

```rust
use unimatrix_engine::graph::{
    FALLBACK_PENALTY, find_terminal_active, graph_expand, graph_penalty,  // graph_expand added
    personalized_pagerank, suppress_contradicts,
};
```

### Phase 0 Block

```
// -----------------------------------------------------------------------
// Phase 0 [crt-042]: graph_expand — widen seed pool if ppr_expander_enabled
//
// Combined ceiling (SR-04 / NFR-08):
//   HNSW k=20 + Phase 0 max 200 + Phase 5 max 50 = 270 maximum candidates
//   before PPR scoring and final truncation to k.
//
// Runs ONLY when both:
//   (a) use_fallback = false (PPR is active — outer guard)
//   (b) ppr_expander_enabled = true (expander feature flag)
//
// When ppr_expander_enabled = false (default): zero overhead — no BFS, no fetch,
// no Instant::now(), no debug! emission. Bit-identical to pre-crt-042 (AC-01, NFR-02).
//
// Lock order: typed_graph is the pre-cloned value (lock already released before Step 6d).
// graph_expand holds no locks (C-04, NFR-06).
// -----------------------------------------------------------------------
if self.ppr_expander_enabled {

    // SR-01 investigation: if VectorIndex.id_map.entry_to_data provides an O(1)
    // embedding lookup path (entry_id → data_id → Vec<f32>), implement it here
    // instead of calling vector_store.get_embedding() (O(N) HNSW scan per entry).
    // See IMPLEMENTATION-BRIEF.md §SR-01 for investigation instructions.
    // Default path (O(N)): use vector_store.get_embedding(expanded_id).await below.

    let phase0_start = std::time::Instant::now();

    // Collect seed IDs from current results_with_scores (post Steps 6a + 6b).
    let seed_ids: Vec<u64> = results_with_scores
        .iter()
        .map(|(e, _)| e.id)
        .collect();

    // BFS traversal: collect entry IDs reachable from seeds via positive edges.
    // Synchronous, pure, no I/O (C-05, NFR-05).
    let expanded_ids: HashSet<u64> = graph_expand(
        &typed_graph,
        &seed_ids,
        self.expansion_depth,
        self.max_expansion_candidates,
    );

    // Deduplication guard: skip any expanded ID already in the current pool.
    // graph_expand excludes seeds by design (AC-08), but this in_pool check ensures
    // correctness if results_with_scores was modified between seed collection and here.
    let in_pool: HashSet<u64> = seed_ids.iter().copied().collect();
    let mut results_added: usize = 0;

    // Process expanded entries in sorted order for determinism (NFR-04).
    let mut sorted_expanded: Vec<u64> = expanded_ids.iter().copied().collect();
    sorted_expanded.sort_unstable();

    for expanded_id in sorted_expanded {
        if in_pool.contains(&expanded_id) {
            continue;  // Already present — skip without counting.
        }

        // Async fetch (same pattern as Phase 5).
        // On error: silently skip the entry. Do not fail the search request.
        let entry = match self.entry_store.get(expanded_id).await {
            Ok(e) => e,
            Err(_) => continue,  // silent skip
        };

        // Quarantine check: MANDATORY (R-03, AC-13, NFR-03, FR-05 step 3b).
        // This is the ONLY quarantine enforcement point for Phase 0 expanded entries.
        // If omitted, quarantined entries reach results_with_scores — security violation.
        if SecurityGateway::is_quarantined(&entry.status) {
            continue;  // silent skip — no warn/error log (NFR-03)
        }

        // Embedding lookup (O(N) HNSW scan per entry — primary latency driver, C-02).
        // SR-01 investigation: replace with O(1) path if feasible; see above.
        // On None: silently skip entries with no stored embedding (AC-15).
        let emb = match self.vector_store.get_embedding(expanded_id).await {
            Some(e) => e,
            None => continue,  // silent skip — no embedding stored for this entry
        };

        // True cosine similarity (ADR-003): real semantic signal, not a floor constant.
        // query_embedding is the Vec<f32> computed earlier in the search pipeline.
        let cosine_sim = cosine_similarity(&query_embedding, &emb);

        results_with_scores.push((entry, cosine_sim));
        results_added += 1;
    }

    // Timing instrumentation (ADR-005, NFR-01, AC-24).
    // Emitted at debug! level — never info! (R-10). RUST_LOG=..search=debug to capture.
    // All six fields are mandatory for the latency gate measurement (seeds required by NFR-01/AC-24).
    tracing::debug!(
        seeds = seed_ids.len(),
        expanded_count = expanded_ids.len(),
        fetched_count = results_added,
        elapsed_ms = phase0_start.elapsed().as_millis(),
        expansion_depth = self.expansion_depth,
        max_expansion_candidates = self.max_expansion_candidates,
        "Phase 0 (graph_expand) complete"
    );
}
// Phase 1: Build the personalization vector (FR-06 / ADR-006).
// [existing code continues unchanged here]
```

---

## Variable Binding Notes

- `query_embedding: Vec<f32>` — bound earlier in the search pipeline (Step 3 embed call).
  The existing search.rs already has this binding available in scope when Step 6d executes.
  No new binding needed.

- `typed_graph: TypedRelationGraph` — the pre-cloned value extracted under the short read lock
  at ~line 673 of search.rs. Already in scope. No new lock acquisition in Phase 0.

- `self.entry_store` — already available on SearchService. Same reference used in Phase 5.

- `self.vector_store` — already available on SearchService. Used here for get_embedding().

---

## O(1) Investigation Note (SR-01)

`VectorIndex.id_map.entry_to_data` is a `HashMap<u64, usize>` mapping `entry_id → data_id`.
The HNSW `PointIndexation` layer-0 stores point vectors accessible by `data_id`. If the
delivery agent can retrieve `Vec<f32>` via `data_id` without the full `IntoIterator` layer
scan (bypassing `get_embedding`'s layer traversal), Phase 0 latency drops from
O(N) per entry to O(1) per entry.

Investigation instruction for delivery agent:
1. Check `VectorIndex` struct for a method or field that returns `&[f32]` by data_id.
2. If `PointIndexation.get_vector(data_id)` or equivalent exists: implement O(1) path here.
3. If not: proceed with O(N) `get_embedding` call; latency gate is the measurement gate.
4. Document result in PR description (IMPLEMENTATION-BRIEF.md SR-01 requirement).

---

## Error Handling

| Failure | Handling |
|---------|----------|
| `entry_store.get()` returns Err | Silent skip (same as Phase 5) |
| `SecurityGateway::is_quarantined()` returns true | Silent skip, no log (NFR-03) |
| `vector_store.get_embedding()` returns None | Silent skip (AC-15) |
| `graph_expand` returns empty set | Phase 0 adds zero entries; loop body executes zero times |
| All expanded entries quarantined or embedding-missing | results_with_scores unchanged vs. HNSW seeds; no error |
| `ppr_expander_enabled = false` | Entire block skipped; zero overhead |

---

## Key Test Scenarios

**AC-01 — flag-off regression.**
With `ppr_expander_enabled = false` (default): run existing search integration suite. Assert
zero diffs in result sets, scores, and ordering vs. pre-crt-042 baseline. Assert
`results_with_scores` length equals HNSW k=20 count (no expansion).

**AC-02 — Phase 0 called before Phase 1.**
With `ppr_expander_enabled = true` and a graph with reachable entries from seeds: assert
Phase 1's `seed_scores` map includes expanded entry IDs (not just HNSW seeds).

**AC-13 — quarantine bypass prevention.**
Construct graph: seed B → entry Q (quarantined). Phase 0 enabled. Assert Q absent from
`results_with_scores`. Assert no warn/error log.

**AC-14 — explicit quarantine fixture.**
Same as AC-13, with transitive path: seed → A → Q (quarantined). Assert Q absent, A present.

**AC-15 — no-embedding skip.**
Construct scenario: expanded_id has no stored embedding (get_embedding returns None).
Assert entry silently skipped; not in results_with_scores.

**AC-24 — debug trace emitted.**
With `ppr_expander_enabled = true`: assert `tracing::debug!` event emitted with fields
`seeds`, `expanded_count`, `fetched_count`, `elapsed_ms`, `expansion_depth`, `max_expansion_candidates`.
Use `tracing-test` subscriber. Note: do not defer this test (entry #3935).

**AC-25 — cross-category behavioral regression test (core feature proof).**
Construct scenario: entry E has embedding dissimilar to query (outside HNSW k=20). E is
connected to an HNSW seed S by a positive graph edge S→E.
- With `ppr_expander_enabled = true`: assert E appears in results.
- With `ppr_expander_enabled = false`: assert E is absent.
This test is MANDATORY regardless of eval gate outcome.

**R-05 — combined ceiling.**
Construct scenario: Phase 0 returns 200 entries (max_expansion_candidates hit). Phase 5
attempts to inject 50 more. Assert total pool size <= 270. Assert Phase 5 correctly treats
Phase 0 entries as already-in-pool.

**R-16 — Phase 0 before Phase 1 (insertion point correctness).**
Assert that `seed_scores` (built in Phase 1) contains entry IDs sourced from Phase 0
expansion. Verify by constructing a known graph expansion and checking Phase 1 input.

**R-10 — no Instant::now() on flag-false path.**
Assert that when `ppr_expander_enabled = false`, no timing instrumentation is emitted and
no `Instant::now()` is observable from Phase 0.

**NFR-02 — bit-identical when disabled.**
Run the same query with `ppr_expander_enabled = false` vs. pre-crt-042. Compare result
sets entry-by-entry. Scores and ordering must match exactly.
