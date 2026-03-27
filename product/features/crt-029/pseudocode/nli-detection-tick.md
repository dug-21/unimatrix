# Component: nli-detection-tick

## Purpose

New module `crates/unimatrix-server/src/services/nli_detection_tick.rs` — the primary
deliverable for crt-029. Implements the recurring background tick that fills `Supports` graph
edges across the full active entry population using HNSW expansion + rayon NLI scoring.

The tick is **Supports-only** (C-13 / AC-10a). It writes NO `Contradicts` edges. The
dedicated contradiction detection path remains the sole `Contradicts` writer.

File size constraint: must not exceed 800 lines (NFR-05, C-08).

---

## File

`crates/unimatrix-server/src/services/nli_detection_tick.rs` — create new.

---

## Module Header

```rust
//! Background graph inference tick — Supports edges only (crt-029).
//!
//! `run_graph_inference_tick` is the counterpart to `maybe_run_bootstrap_promotion`:
//! that function is one-shot and idempotency-gated; this one is recurring and cap-throttled.
//! Both share the W1-2 contract and the rayon pool.
//!
//! # W1-2 Contract
//!
//! ALL `CrossEncoderProvider::score_batch` calls are dispatched via `rayon_pool.spawn()`.
//! `spawn_blocking` is prohibited. Inline async NLI is prohibited.
//!
//! # Supports-Only Design (C-13 / AC-10a)
//!
//! This module writes ONLY `Supports` edges. It has no `contradiction_threshold` parameter.
//! The `contradiction` score returned by the NLI model is discarded. The dedicated
//! contradiction detection path (run_post_store_nli, infra/contradiction.rs) is the sole
//! writer of `Contradicts` edges.
//!
//! # R-09 Rayon/Tokio Boundary (C-14)
//!
//! The rayon closure in Phase 7 MUST be synchronous CPU-bound only. PROHIBITED inside any
//! `rayon_pool.spawn()` closure: `tokio::runtime::Handle::current()`, `.await` expressions,
//! any function that internally awaits. Rayon worker threads have no Tokio runtime; violations
//! panic at runtime. This is compile-invisible.
//!
//! Detection: `grep -n 'Handle::current' nli_detection_tick.rs` must return empty.
//! Independent review of the rayon closure body is mandatory before merge.
```

---

## Imports

```rust
use std::collections::HashSet;
use std::sync::Arc;

use unimatrix_core::{Store, VectorIndex};
use unimatrix_core::model::EntryRecord;      // check exact path in codebase
use unimatrix_embed::CrossEncoderProvider;   // check exact path — used inside rayon closure
use unimatrix_store::EDGE_SOURCE_NLI;        // pub const from unimatrix_store::read

use crate::infra::config::InferenceConfig;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;

// pub(crate) symbols promoted from nli_detection.rs (R-11, ADR-001):
use crate::services::nli_detection::{
    current_timestamp_secs,
    format_nli_metadata,
    write_nli_edge,
};
```

IMPLEMENTATION NOTE: The exact import paths for `EntryRecord`, `CrossEncoderProvider`, and
`NliScores` must be verified against the existing `nli_detection.rs` imports. Use the same
paths to avoid divergence.

---

## Function 1: `run_graph_inference_tick` (public async)

### Signature (from ARCHITECTURE.md integration surface)

```rust
pub async fn run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
)
```

### Pseudocode

```
FUNCTION run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) -> ()    // infallible; errors logged at warn/debug

// -------------------------------------------------------------------------
// Phase 1 — Guard (O(1))
// -------------------------------------------------------------------------

    provider = match nli_handle.get_provider().await {
        Ok(p)  => p
        Err(_) => {
            // NLI not ready; silent no-op (matches maybe_run_bootstrap_promotion pattern)
            // Do NOT log info here — this fires every tick when NLI is disabled
            return
        }
    }
    // provider: Arc<dyn CrossEncoderProvider>

// -------------------------------------------------------------------------
// Phase 2 — Data fetch (async DB, tokio thread)
// Three sequential reads — order does not affect correctness, sequential chosen
// to avoid concurrent write-pool contention.
// -------------------------------------------------------------------------

    all_active = match store.query_by_status(Status::Active).await {
        Ok(entries) => entries    // Vec<EntryRecord>
        Err(e) => {
            tracing::warn!(error = %e, "graph inference tick: failed to fetch active entries");
            return
        }
    }

    IF all_active is empty {
        tracing::debug!("graph inference tick: no active entries, skipping");
        return
    }

    isolated_ids_vec = match store.query_entries_without_edges().await {
        Ok(ids) => ids    // Vec<u64>
        Err(e) => {
            // Degraded mode: proceed with empty isolated set (priority tier 2 lost)
            tracing::warn!(error = %e, "graph inference tick: failed to fetch isolated entry IDs; proceeding without isolation priority");
            vec![]
        }
    }
    isolated_ids: HashSet<u64> = isolated_ids_vec.into_iter().collect()

    existing_supports_pairs = match store.query_existing_supports_pairs().await {
        Ok(pairs) => pairs    // HashSet<(u64, u64)>
        Err(e) => {
            // Degraded mode: proceed with empty pre-filter (INSERT OR IGNORE backstop)
            tracing::warn!(error = %e, "graph inference tick: failed to fetch existing Supports pairs; NLI will score all pairs (INSERT OR IGNORE dedup)");
            HashSet::new()
        }
    }

// -------------------------------------------------------------------------
// Phase 3 — Source candidate selection (cap BEFORE embedding)
// AC-06c / R-02: select_source_candidates runs on metadata only (IDs + category strings).
// No get_embedding call occurs in this phase.
// -------------------------------------------------------------------------

    source_candidates: Vec<u64> = select_source_candidates(
        &all_active,
        &existing_supports_pairs,
        &isolated_ids,
        config.max_graph_inference_per_tick,    // ADR-003: derived bound, no separate field
    )
    // Invariant: source_candidates.len() <= config.max_graph_inference_per_tick

// -------------------------------------------------------------------------
// Phase 4 — HNSW expansion (embeddings for capped list only)
// get_embedding called AT MOST max_graph_inference_per_tick times.
// -------------------------------------------------------------------------

    candidate_pairs: Vec<(u64, u64, f32)> = vec![]  // (source_id, target_id, similarity)
    seen_pairs: HashSet<(u64, u64)> = HashSet::new() // in-phase dedup set

    FOR source_id IN source_candidates {
        embedding = match vector_index.get_embedding(source_id).await {
            Some(emb) => emb    // Vec<f32>
            None => {
                tracing::debug!(entry_id = source_id, "graph inference tick: no embedding for source, skipping");
                continue
            }
        }

        const EF_SEARCH: usize = 32;    // match SearchService constant
        search_results = match vector_index.search(&embedding, config.graph_inference_k, EF_SEARCH).await {
            Ok(results) => results
            Err(e) => {
                tracing::debug!(entry_id = source_id, error = %e, "graph inference tick: HNSW search failed for source");
                continue
            }
        }

        FOR result IN search_results {
            neighbour_id = result.id
            similarity = result.score    // f32

            IF neighbour_id == source_id { continue }
            IF similarity <= config.supports_candidate_threshold { continue }  // strict >

            // Normalise pair to (min, max) for symmetric dedup
            pair_key = (source_id.min(neighbour_id), source_id.max(neighbour_id))

            // Skip if pair already has a Supports edge (pre-filter optimisation; INSERT OR IGNORE is backstop)
            IF existing_supports_pairs.contains(&pair_key) { continue }

            // Skip if we've already collected this pair from a different source direction
            IF seen_pairs.contains(&pair_key) { continue }
            seen_pairs.insert(pair_key)

            candidate_pairs.push((source_id, neighbour_id, similarity))
        }
    }

// -------------------------------------------------------------------------
// Phase 5 — Priority sort and truncation
// -------------------------------------------------------------------------

    // Build a category lookup for cross-category determination
    category_map: HashMap<u64, &str> = all_active.iter()
        .map(|e| (e.id, e.category.as_str()))
        .collect()

    candidate_pairs.sort_by(|(a_src, a_tgt, a_sim), (b_src, b_tgt, b_sim)| {
        // Priority 1: cross-category pairs first (false = 0 < true = 1, so invert for desc)
        let a_cross = category_map.get(a_src) != category_map.get(a_tgt);
        let b_cross = category_map.get(b_src) != category_map.get(b_tgt);
        match b_cross.cmp(&a_cross) {   // true > false in descending order
            Ordering::Equal => {
                // Priority 2: either endpoint isolated
                let a_iso = isolated_ids.contains(a_src) || isolated_ids.contains(a_tgt);
                let b_iso = isolated_ids.contains(b_src) || isolated_ids.contains(b_tgt);
                match b_iso.cmp(&a_iso) {
                    Ordering::Equal => {
                        // Priority 3: similarity descending (higher similarity first)
                        b_sim.partial_cmp(a_sim).unwrap_or(Ordering::Equal)
                    }
                    other => other
                }
            }
            other => other
        }
    })

    candidate_pairs.truncate(config.max_graph_inference_per_tick)
    // candidate_pairs is now at most max_graph_inference_per_tick entries

// -------------------------------------------------------------------------
// Phase 6 — Text fetch (async DB, tokio thread)
// Uses write_pool via get_content_via_write_pool() to see recently committed rows
// (matches bootstrap promotion pattern).
// -------------------------------------------------------------------------

    // Collect pairs that have both texts
    scored_input: Vec<(u64, u64, String, String)> = vec![]  // (src, tgt, src_text, tgt_text)

    FOR (source_id, target_id, _similarity) IN candidate_pairs {
        source_text = match store.get_content_via_write_pool(source_id).await {
            Ok(text) => text
            Err(e) => {
                tracing::debug!(entry_id = source_id, error = %e, "graph inference tick: failed to fetch source content, skipping pair");
                continue
            }
        }
        target_text = match store.get_content_via_write_pool(target_id).await {
            Ok(text) => text
            Err(e) => {
                tracing::debug!(entry_id = target_id, error = %e, "graph inference tick: failed to fetch target content, skipping pair");
                continue
            }
        }
        scored_input.push((source_id, target_id, source_text, target_text))
    }

    IF scored_input is empty {
        tracing::debug!("graph inference tick: no pairs with fetchable content, skipping NLI");
        return
    }

// -------------------------------------------------------------------------
// Phase 7 — W1-2 dispatch (single rayon spawn)
// -----------------------------------------------------------------------
// C-14 / R-09 CRITICAL CONSTRAINT:
// The closure body MUST be synchronous CPU-bound only.
// PROHIBITED inside this closure:
//   - tokio::runtime::Handle::current()
//   - .await expressions
//   - Any function that internally awaits or accesses Tokio runtime
// Rayon worker threads have no Tokio runtime; violations panic at runtime.
// Pre-merge gate: grep -n 'Handle::current' nli_detection_tick.rs must return empty.
// Independent validator required (not the author of this closure).
// -------------------------------------------------------------------------

    // Collect owned (String, String) pairs BEFORE spawn (entry #2742 pattern)
    // Cloning ensures the closure owns all data — no lifetime issues across rayon boundary
    nli_pairs: Vec<(String, String)> = scored_input.iter()
        .map(|(_, _, src_text, tgt_text)| (src_text.clone(), tgt_text.clone()))
        .collect()

    provider_clone: Arc<dyn CrossEncoderProvider> = Arc::clone(&provider)

    // Single rayon dispatch for entire batch (W1-2 contract — AC-08, C-01)
    // One spawn per tick maximum (entry #3653)
    nli_result = rayon_pool.spawn(move || {
        // SYNC-ONLY CLOSURE BODY — no .await, no Handle::current()
        let pairs_ref: Vec<(&str, &str)> = nli_pairs.iter()
            .map(|(q, p)| (q.as_str(), p.as_str()))
            .collect();
        provider_clone.score_batch(&pairs_ref)
        // Returns Result<Vec<NliScores>, _>
    }).await;
    // .await here is OUTSIDE the closure — on the tokio thread, waiting for rayon result

    nli_scores: Vec<NliScores> = match nli_result {
        Ok(Ok(scores)) => scores
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "graph inference tick: score_batch failed");
            return
        }
        Err(rayon_err) => {
            // RayonError: rayon pool panic (session poisoned or other rayon panic)
            tracing::warn!(error = %rayon_err, "graph inference tick: rayon task cancelled (panic in rayon worker?)");
            return
        }
    }

    // Defensive: if lengths mismatch, log and bail rather than index-panic
    IF nli_scores.len() != scored_input.len() {
        tracing::warn!(
            scores_len = nli_scores.len(),
            pairs_len = scored_input.len(),
            "graph inference tick: score_batch result length mismatch; skipping write"
        );
        return
    }

    // Reconstruct flat pairs for write step
    write_pairs: Vec<(u64, u64)> = scored_input.iter()
        .map(|(src, tgt, _, _)| (*src, *tgt))
        .collect()

// -------------------------------------------------------------------------
// Phase 8 — Write (Supports only)
// No contradiction_threshold; tick writes no Contradicts edges (C-13 / AC-10a).
// -------------------------------------------------------------------------

    edges_written = write_inferred_edges_with_cap(
        store,
        &write_pairs,
        &nli_scores,
        config.supports_edge_threshold,          // NOT nli_entailment_threshold (C-06)
        config.max_graph_inference_per_tick,
    ).await

    tracing::debug!(
        edges_written = edges_written,
        pairs_scored = nli_scores.len(),
        source_candidates = source_candidates.len(),
        "graph inference tick complete"
    )

END FUNCTION
```

---

## Function 2: `select_source_candidates` (private)

### Signature (from ARCHITECTURE.md integration surface)

```rust
fn select_source_candidates(
    all_active: &[EntryRecord],
    existing_edge_set: &HashSet<(u64, u64)>,
    isolated_ids: &HashSet<u64>,
    max_sources: usize,
) -> Vec<u64>
```

### Purpose

Select up to `max_sources` source IDs in priority order. Operates on metadata only (IDs,
category strings). NO `get_embedding` calls. The returned list is directly bounded — this is
the Phase 3 cap that gates Phase 4.

`existing_edge_set` is accepted as a parameter to allow cross-category detection (entries
that have at least one edge to a differently-categorised entry are "cross-category sources").
If implementation judges the cross-category computation too expensive over the full edge set,
the parameter may be ignored and cross-category may be approximated by checking whether any
other active entry has a different category (simpler: detect entries whose category differs
from at least one other active entry's category).

### Pseudocode

```
FUNCTION select_source_candidates(
    all_active: &[EntryRecord],
    existing_edge_set: &HashSet<(u64, u64)>,    // normalised (min,max) pairs
    isolated_ids: &HashSet<u64>,
    max_sources: usize,
) -> Vec<u64>

    IF all_active is empty OR max_sources == 0 {
        return vec![]
    }

    // Compute per-entry classification in two tiers:
    //   Tier 1: cross-category sources — entry has (or could have) edges to different-category entries
    //   Tier 2: isolated sources — in isolated_ids set
    //   Tier 3: all remaining, ordered by created_at descending

    // For cross-category classification: an entry is "cross-category relevant" if the active
    // population contains at least one entry with a different category. This is a proxy for
    // "this entry might form a cross-category pair" and requires only the active list.
    // Exact cross-category determination (does this entry have edges to different-category entries)
    // is computed during Phase 5 over the expanded pair set; here we classify sources cheaply.
    //
    // Simplest correct implementation:
    //   compute the set of all categories in all_active
    //   IF there is only one category: no cross-category sources (all same category)
    //   ELSE: mark entries from any category that is different from the majority/modal category
    //         OR: mark all entries as potentially cross-category (conservative — Phase 5 re-ranks)
    //
    // Recommended approach (correct and cheap):
    //   All entries are potential cross-category sources if the population has > 1 distinct category.
    //   Priority is applied at Phase 5 on the actual expanded pairs, not here.
    //   select_source_candidates prioritises: isolated first, then created_at desc.
    //
    // Architecture note: Phase 5 applies the full 3-tier sort to the expanded pair set.
    // Phase 3 source selection only needs to ensure isolated entries have priority over
    // non-isolated for the source slot. Cross-category pair priority is enforced in Phase 5.

    tier1: Vec<&EntryRecord> = []    // isolated entries (highest source priority)
    tier2: Vec<&EntryRecord> = []    // non-isolated entries

    FOR entry IN all_active {
        IF isolated_ids.contains(&entry.id) {
            tier1.push(entry)
        } ELSE {
            tier2.push(entry)
        }
    }

    // Sort tier2 by created_at descending (newest first as tiebreaker)
    tier2.sort_by(|a, b| b.created_at.cmp(&a.created_at))

    // Assemble ordered candidate list
    ordered: Vec<u64> = tier1.iter()
        .chain(tier2.iter())
        .map(|e| e.id)
        .take(max_sources)
        .collect()

    return ordered

END FUNCTION
```

### Boundary Conditions

- `all_active` empty: return `vec![]`
- `max_sources` 0: return `vec![]` (defensive; `validate()` ensures max_sources >= 1)
- All entries isolated: all go into tier1; truncated to `max_sources`
- No entries isolated: tier1 empty; tier2 sorted by `created_at` desc, truncated

---

## Function 3: `write_inferred_edges_with_cap` (private, testable)

### Signature (from ARCHITECTURE.md integration surface)

```rust
async fn write_inferred_edges_with_cap(
    store: &Store,
    pairs: &[(u64, u64)],
    nli_scores: &[NliScores],
    supports_threshold: f32,        // config.supports_edge_threshold
    max_edges: usize,               // config.max_graph_inference_per_tick
) -> usize
```

**Supports-ONLY**: No `contradiction_threshold` parameter. No `Contradicts` writes.
The `contradiction` score in `NliScores` is not read. This is an intentional design
constraint (C-13 / AC-10a / ADR-002), not an omission.

### Pseudocode

```
FUNCTION write_inferred_edges_with_cap(
    store: &Store,
    pairs: &[(u64, u64)],
    nli_scores: &[NliScores],
    supports_threshold: f32,
    max_edges: usize,
) -> usize

    PRECONDITION: pairs.len() == nli_scores.len()
    // Caller already verified this before calling (Phase 7 mismatch check)

    edges_written: usize = 0
    timestamp: u64 = current_timestamp_secs()    // pub(crate) from nli_detection.rs

    FOR i IN 0..pairs.len() {
        IF edges_written >= max_edges {
            break    // cap reached; stop processing (FR-09, AC-11)
        }

        (source_id, target_id) = pairs[i]
        scores = &nli_scores[i]

        // Evaluate ONLY entailment — contradiction score is DISCARDED (C-13)
        IF scores.entailment > supports_threshold {    // strict >; at-threshold = no edge (AC-09)
            metadata_json: String = format_nli_metadata(scores)    // pub(crate) from nli_detection.rs
            // format_nli_metadata serialises both entailment and contradiction as metadata fields
            // even though only entailment drove the edge decision. The metadata is informational.

            written: bool = write_nli_edge(
                store,
                source_id,
                target_id,
                "Supports",              // relation_type — ONLY "Supports" ever written here
                scores.entailment,       // weight
                timestamp,
                &metadata_json,
            ).await
            // write_nli_edge uses INSERT OR IGNORE — returns true if row was inserted or already existed

            IF written {
                edges_written += 1
                // Note: INSERT OR IGNORE on a pre-existing row returns true in the current
                // write_nli_edge implementation. If this changes, the cap counts writes
                // regardless of INSERT IGNORE outcome — verify against write_nli_edge behaviour.
            }
        }
    }

    return edges_written

END FUNCTION
```

IMPLEMENTATION NOTE: Verify `write_nli_edge` return semantics. Current implementation at
`nli_detection.rs` line ~532: the function returns `true` if the INSERT executed without
DB error (it returns `true` even for `INSERT OR IGNORE` conflicts — the INSERT IGNORE is not
an error). If this means duplicate pairs count against the cap, that is acceptable: the
pre-filter minimises duplicates and the cap is a budget constraint, not a uniqueness guarantee.

---

## State Machine / Lifecycle

The tick is stateless. Each invocation:
1. Reads current DB state (Phases 1-3).
2. Expands via HNSW (Phase 4).
3. Scores via NLI (Phase 7).
4. Writes edges (Phase 8).
5. Returns.

No persistent state between invocations. The DB is the source of truth for all resumption
on the next tick. If the tick is aborted mid-Phase 8 (e.g., `TICK_TIMEOUT` fires), partial
writes are committed — this is acceptable because `INSERT OR IGNORE` is idempotent.

---

## Error Handling Summary

| Error site | Behaviour |
|-----------|-----------|
| `nli_handle.get_provider()` returns Err | Silent return (no log — fires every tick when NLI disabled) |
| `query_by_status` fails | Log warn, return |
| `query_entries_without_edges` fails | Log warn, continue with empty isolated set |
| `query_existing_supports_pairs` fails | Log warn, continue with empty pre-filter |
| `get_embedding` returns None | Log debug, skip source candidate |
| `vector_index.search` fails | Log debug, skip source candidate |
| `get_content_via_write_pool` fails | Log debug, skip pair |
| `rayon_pool.spawn` result is Err | Log warn, return (0 edges written this tick) |
| `nli_scores.len() != pairs.len()` | Log warn, return |
| `write_nli_edge` returns false | Not incremented; edge already existed or INSERT failed |
| Tick exceeds TICK_TIMEOUT | Aborted externally; partial writes committed (idempotent) |

All errors are logged and swallowed — the function is infallible (returns `()`). This matches
the `maybe_run_bootstrap_promotion` precedent (errors logged at warn, no propagation).

---

## Key Test Scenarios

All inline tests live in `nli_detection_tick.rs` under `#[cfg(test)]` (ADR-001: per entry
#3631, inline tests in the new sibling module when parent is oversized).

### AC-05: No-op when NLI not ready
```
nli_handle = NliServiceHandle::new()  // Loading state — get_provider() returns Err
run_graph_inference_tick(store, &nli_handle, vector_index, rayon_pool, &config).await
// assert: no DB queries fired, 0 edges written
```

### AC-06c: get_embedding called at most max_sources times (R-02)
```
seed 50 active entries
config.max_graph_inference_per_tick = 5
run tick with mock vector_index tracking get_embedding call count
assert: get_embedding called <= 5 times
```

### AC-07: Cross-category pairs prioritised at cap boundary (R-12)
```
seed: 3 entries category="decision", 3 entries category="lesson"
config.max_graph_inference_per_tick = 3
all 6 HNSW neighbours are within same category except the 3 cross-category pairs
run tick
assert: 3 cross-category pairs written (not 3 same-category pairs)
```

### AC-08: Single rayon dispatch per tick
```
// Structural test: verify tick body calls rayon_pool.spawn() exactly once
// Use mock pool that records spawn count; assert count == 1 after tick
```

### AC-09: At-threshold pair produces no edge
```
write_inferred_edges_with_cap(
    store,
    &[(1, 2)],
    &[NliScores { entailment: 0.7, contradiction: 0.1 }],
    0.7,    // supports_threshold — at-threshold, not above
    10,
)
assert: edges_written == 0   // strict >; 0.7 is not > 0.7
```

### AC-10a: No Contradicts edges written (C-13)
```
write_inferred_edges_with_cap(
    store,
    &[(1, 2)],
    &[NliScores { entailment: 0.3, contradiction: 0.95 }],  // high contradiction
    0.7,
    10,
)
// contradiction score ignored; no Contradicts edge written
let edges = store.query_graph_edges().await.unwrap();
assert: no edge with relation_type = "Contradicts" for (1, 2)
```

### AC-11: Cap enforcement
```
// 10 pairs all scoring above threshold, cap = 3
write_inferred_edges_with_cap(
    store,
    &10_pairs,
    &10_scores_above_threshold,
    0.5,
    3,           // max_edges = 3
)
assert: edges_written == 3
assert: store has exactly 3 new Supports edges
```

### AC-15 (pre-filter skip): Existing Supports pair skipped before NLI
```
// Seed a Supports edge (1, 2) in GRAPH_EDGES
// existing_supports_pairs includes (1, 2) (or normalised pair)
// Phase 4: pair (1, 2) should not appear in candidate_pairs
// NLI mock should not be called for (1, 2)
```

### AC-16: Idempotency (run twice, same data)
```
run_graph_inference_tick(store, ...).await   // first tick writes N edges
run_graph_inference_tick(store, ...).await   // second tick: same data, INSERT OR IGNORE
assert: total edges in store == N (not 2N)
```

### R-09 pre-merge grep checks (gate conditions)
```bash
# No Handle::current() in the file
grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

# No spawn_blocking in the file
grep -n 'spawn_blocking' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

# No Contradicts in the file
grep -n 'Contradicts' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

# Manual inspection: verify rayon_pool.spawn() closure body contains no .await
# Reviewer must not be the author (C-14, R-09)
```
