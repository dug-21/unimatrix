# Component: nli_detection_tick.rs (unimatrix-server)

## Purpose

Extend `run_graph_inference_tick` with Phase 4b (Informs HNSW scan) and Phase 8b (Informs
write loop). Define module-private `NliCandidatePair` tagged union and `InformsCandidate`
struct. Extend Phase 5 (cap accounting) and Phase 7 (merged batch) to operate on a
`Vec<NliCandidatePair>` carrying both `SupportsContradict` and `Informs` variants.

Wave 2. Depends on Wave 1: `RelationType::Informs` (`graph.rs`), three new `InferenceConfig`
fields (`config.rs`), `Store::query_existing_informs_pairs` (`read.rs`).

## Files Modified

`crates/unimatrix-server/src/services/nli_detection_tick.rs`

## New Types (module-private)

### NliCandidatePair — tagged union

```rust
#[derive(Debug, Clone)]
enum NliCandidatePair {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
        nli_scores: NliScores,
    },
    Informs {
        candidate: InformsCandidate,
        nli_scores: NliScores,
    },
}
```

Module-private (no `pub`). Routing discriminator: Phase 8 matches on `SupportsContradict`;
Phase 8b matches on `Informs`. The compiler enforces exhaustive matching and prevents
misrouting (FR-10, SR-08, ADR-001, C-11). `NliScores` is embedded in each variant — no
parallel index-matched vecs.

`nli_scores` field starts as a sentinel value before Phase 7 runs. The implementation must
choose a construction strategy for Phase 4/4b that defers `nli_scores` population until
after score_batch returns. Two valid approaches:

Option A: Construct with a placeholder `NliScores { entailment: 0.0, neutral: 0.0,
contradiction: 0.0 }` during Phase 4/4b, then rebuild the vec after Phase 7 by
destructuring each element and re-constructing with the real scores.

Option B: Use a two-phase construction — collect metadata-only structs during Phase 4/4b,
then zip with scores from Phase 7 to construct the final `NliCandidatePair` vec.

Option B is preferred (clearer ownership, avoids placeholder ambiguity). The pseudocode
below uses Option B. The implementation agent may use Option A if it simplifies borrow
lifetimes.

### InformsCandidate — required-field record

```rust
#[derive(Debug, Clone)]
struct InformsCandidate {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    source_created_at: i64,       // Unix seconds — required; not Option
    target_created_at: i64,       // Unix seconds — required; not Option
    source_feature_cycle: String, // required; not Option (cross-feature guard)
    target_feature_cycle: String, // required; not Option (cross-feature guard)
    source_category: String,      // required; not Option (category pair filter)
    target_category: String,      // required; not Option (category pair filter)
}
```

Module-private (no `pub`). All nine fields required. Construction forces all guard metadata
to be present — the compiler makes None-field vacuous-pass (R-05) impossible. Phase 8b
reads guard fields directly from struct fields — no unwrap, no Option handling.

Note on `EntryRecord.created_at`: in `read.rs` `entry_from_row`, `created_at` is stored
as `u64` in `EntryRecord`. The `InformsCandidate` fields use `i64` to match SQL semantics
and the temporal comparison (`source_created_at < target_created_at`) which uses signed
arithmetic. Cast `entry.created_at as i64` when constructing. Typical Unix timestamps
are well within positive i64 range.

Note on `feature_cycle`: `EntryRecord.feature_cycle` is `Option<String>` in the schema.
Phase 4b excludes pairs where either side has a `None` feature_cycle (cross-feature guard
requires both sides to have a defined, distinct cycle). On `None`, skip that neighbor.

## Modified: run_graph_inference_tick

The function signature is unchanged. All additions are within the function body.

### Phase 2 — add query_existing_informs_pairs call

After the existing `query_existing_supports_pairs` block, add:

```
let existing_informs_pairs: HashSet<(u64, u64)> =
    match store.query_existing_informs_pairs().await:
        Ok(pairs) => pairs
        Err(e) =>
            tracing::warn!(
                error = %e,
                "graph inference tick: failed to fetch existing Informs pairs; INSERT OR IGNORE dedup"
            )
            HashSet::new()
            // Degraded: same pattern as existing_supports_pairs degradation
```

### Phase 3 — unchanged

`select_source_candidates` is unchanged. Its output `source_candidates` is reused by
Phase 4b as the source pool. No new source selection logic for Informs.

### Phase 4 — refactor existing candidate collection

The existing Phase 4 collects `candidate_pairs: Vec<(u64, u64, f32)>`. Rename or keep this
as `supports_metadata: Vec<(u64, u64, f32)>` — plain metadata tuples, not yet
`NliCandidatePair`. The `NliCandidatePair::SupportsContradict` variants are constructed
after Phase 7 (Option B) when scores are available.

No logic changes to Phase 4. The existing filtering (similarity > threshold, dedup via
seen_pairs and existing_supports_pairs) is unchanged.

### Phase 4b — NEW: Informs HNSW scan

Insert after Phase 4 completes (after the `if candidate_pairs.is_empty()` early return).

```
// Phase 4b — Informs HNSW scan at nli_informs_cosine_floor (crt-037)
//
// Uses the same source_candidates pool as Phase 4.
// Applies cross-category, temporal, cross-feature, and dedup guards before NLI.
// Domain vocabulary must not appear here — config fields are the sole source (C-12).

// Build O(1) lookup structures
let informs_lhs_set: HashSet<&str> =
    config.informs_category_pairs
        .iter()
        .map(|pair| pair[0].as_str())
        .collect()

let entry_meta: HashMap<u64, &EntryRecord> =
    all_active.iter().map(|e| (e.id, e)).collect()

let mut informs_metadata: Vec<InformsCandidate> = Vec::new()
let mut seen_informs_pairs: HashSet<(u64, u64)> = HashSet::new()

// Skip Phase 4b entirely if no configured category pairs (empty list disables detection)
if !config.informs_category_pairs.is_empty():

    for source_id in &source_candidates:
        // Look up source metadata
        let source_meta = match entry_meta.get(source_id):
            Some(m) => m
            None => continue   // entry disappeared from snapshot

        // Source-category pre-filter (C-12: source_meta.category is a runtime value)
        if !informs_lhs_set.contains(source_meta.category.as_str()):
            continue            // not an Informs-eligible source category

        // source_feature_cycle must be Some
        let source_feature_cycle = match &source_meta.feature_cycle:
            Some(fc) => fc.clone()
            None => continue    // no feature cycle — cross-feature guard requires both sides

        // Get embedding (same pattern as Phase 4)
        let embedding = match vector_index.get_embedding(*source_id):
            Some(emb) => emb
            None =>
                tracing::debug!(entry_id = source_id, "graph inference tick Phase 4b: no embedding, skipping")
                continue

        // HNSW search — same k and EF_SEARCH as Phase 4
        let search_results = match vector_index.search(&embedding, config.graph_inference_k, EF_SEARCH):
            Ok(results) => results
            Err(e) =>
                tracing::debug!(entry_id = source_id, error = %e, "graph inference tick Phase 4b: HNSW search failed")
                continue

        for result in search_results:
            let neighbor_id = result.entry_id
            let similarity = result.similarity as f32

            // Self-skip
            if neighbor_id == *source_id: continue

            // Cosine floor: inclusive >= (not strict >, unlike Phase 4)
            // AC-17: pair at exactly 0.45 is a valid candidate (floor is inclusive)
            if similarity < config.nli_informs_cosine_floor: continue

            // Look up target metadata
            let target_meta = match entry_meta.get(&neighbor_id):
                Some(m) => m
                None => continue

            // Cross-category filter: [source_category, target_category] must be in informs_category_pairs
            // Uses runtime config values — no domain string literals here (C-12)
            let source_cat = source_meta.category.as_str()
            let target_cat = target_meta.category.as_str()
            let is_valid_pair = config.informs_category_pairs
                .iter()
                .any(|pair| pair[0] == source_cat && pair[1] == target_cat)
            if !is_valid_pair: continue

            // target_feature_cycle must be Some
            let target_feature_cycle = match &target_meta.feature_cycle:
                Some(fc) => fc.clone()
                None => continue

            // Temporal ordering guard: source must predate target (strictly)
            let source_ts = source_meta.created_at as i64
            let target_ts = target_meta.created_at as i64
            if source_ts >= target_ts: continue   // equal or reversed — excluded

            // Cross-feature guard: different feature cycles required
            if source_feature_cycle == target_feature_cycle: continue

            // DB-level dedup: pair not already written
            if existing_informs_pairs.contains(&(*source_id, neighbor_id)): continue

            // In-tick dedup: pair not already seen this tick
            if seen_informs_pairs.contains(&(*source_id, neighbor_id)): continue
            seen_informs_pairs.insert((*source_id, neighbor_id))

            // Construct InformsCandidate — all nine fields required, no Option
            informs_metadata.push(InformsCandidate {
                source_id: *source_id,
                target_id: neighbor_id,
                cosine: similarity,
                source_created_at: source_ts,
                target_created_at: target_ts,
                source_feature_cycle,
                target_feature_cycle,
                source_category: source_meta.category.clone(),
                target_category: target_meta.category.clone(),
            })
        // end for result
    // end for source_id
// end if !informs_category_pairs.is_empty()
```

### Phase 5 — combined cap with priority ordering (ADR-002)

Replace the existing `candidate_pairs.sort_by(...); candidate_pairs.truncate(max_cap)`
pattern. Phase 5 now governs two candidate lists separately before merging:

```
// Phase 5 — Combined cap: Supports first (priority), Informs second (remainder)
//
// Step 1: Sort supports_metadata by existing priority criteria
//   (cross-category first, isolated endpoint second, similarity desc)
//   This is identical to the existing sort in Phase 5 — do not change sort criteria.

candidate_pairs.sort_by(|(a_src, a_tgt, a_sim), (b_src, b_tgt, b_sim)| {
    let a_cross = category_map.get(a_src) != category_map.get(a_tgt)
    let b_cross = category_map.get(b_src) != category_map.get(b_tgt)
    // ... existing sort body unchanged ...
})
candidate_pairs.truncate(config.max_graph_inference_per_tick)
// supports_metadata is now bounded to max_cap

// Step 2: Compute remaining capacity
let remaining_capacity = config.max_graph_inference_per_tick.saturating_sub(candidate_pairs.len())

// Step 3: Sort informs_metadata by similarity descending
//   Cross-category already guaranteed by Phase 4b filter — no cross-category sort needed.
//   Isolated-endpoint boost not applied to Informs pass (not a priority criterion).
let informs_total_before_cap = informs_metadata.len()
informs_metadata.sort_unstable_by(|a, b| b.cosine.partial_cmp(&a.cosine).unwrap_or(Ordering::Equal))

// Step 4: Truncate Informs to remaining capacity
informs_metadata.truncate(remaining_capacity)

// Step 5: Log cap accounting (SR-03, FR-14)
tracing::debug!(
    supports_candidates = candidate_pairs.len(),
    informs_candidates_total = informs_total_before_cap,
    informs_candidates_accepted = informs_metadata.len(),
    informs_candidates_dropped = informs_total_before_cap.saturating_sub(informs_metadata.len()),
    "graph inference tick: merged cap accounting"
)
```

### Phase 6 — text fetch (modified for merged pairs)

Phase 6 must fetch text for both `supports_metadata` and `informs_metadata`. The existing
code iterates `candidate_pairs` to produce `scored_input: Vec<(u64, u64, String, String)>`.

Extend to fetch for informs pairs as well. Collect into a single `scored_input` vec with
a corresponding `Vec<InformsCandidate>` tracking which rows are Informs (to reconstruct
the tagged union after Phase 7).

```
// Phase 6 — Text fetch for all pairs (Supports + Informs)
let mut scored_input: Vec<(u64, u64, String, String)> = Vec::new()
// Parallel vec tracking source metadata for each scored_input entry.
// Used to reconstruct NliCandidatePair variants after Phase 7.
// Index-aligned to scored_input.
let mut pair_origins: Vec<PairOrigin> = Vec::new()

// Internal enum — determines which variant to construct after scoring
// NOTE: This is NOT a discriminator field on a flat struct.
// It is construction scaffolding that is consumed when building NliCandidatePair.
enum PairOrigin {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32 },
    Informs(InformsCandidate),
}

// Fetch Supports pairs
for (source_id, target_id, _cosine) in &candidate_pairs:
    // ... existing fetch logic unchanged ...
    scored_input.push((source_id, target_id, source_text, target_text))
    pair_origins.push(PairOrigin::SupportsContradict { source_id: *source_id, target_id: *target_id, cosine: *_cosine })

// Fetch Informs pairs
for candidate in &informs_metadata:
    let source_text = match store.get_content_via_write_pool(candidate.source_id).await:
        Ok(text) => text
        Err(e) =>
            tracing::debug!(entry_id = candidate.source_id, error = %e, "graph inference tick Phase 4b: source content fetch failed")
            continue   // skip this pair
    let target_text = match store.get_content_via_write_pool(candidate.target_id).await:
        Ok(text) => text
        Err(e) =>
            tracing::debug!(entry_id = candidate.target_id, error = %e, "graph inference tick Phase 4b: target content fetch failed")
            continue
    scored_input.push((candidate.source_id, candidate.target_id, source_text, target_text))
    pair_origins.push(PairOrigin::Informs(candidate.clone()))

// Note: scored_input.len() == pair_origins.len() invariant maintained by construction.
// Pairs where content fetch fails are dropped — their PairOrigin is not pushed.
// This is consistent with existing Phase 6 behavior for Supports pairs.
```

### Phase 7 — single rayon spawn (unchanged closure body)

The closure body is identical to the current implementation — `score_batch` takes
`Vec<(&str, &str)>` regardless of pair origin. No change to the closure itself.

```
// Phase 7 — W1-2 dispatch: single rayon spawn (unchanged)
// C-14 / R-09 CRITICAL: closure body is SYNC-ONLY CPU-bound.
// PROHIBITED: tokio::runtime::Handle::current(), .await, any async call.
let nli_pairs: Vec<(String, String)> = scored_input
    .iter()
    .map(|(_, _, src, tgt)| (src.clone(), tgt.clone()))
    .collect()

let provider_clone = Arc::clone(&provider)

let nli_result = rayon_pool
    .spawn(move || {
        // SYNC-ONLY CLOSURE — no .await, no Handle::current()
        let pairs_ref: Vec<(&str, &str)> = nli_pairs.iter().map(|(q, p)| (q.as_str(), p.as_str())).collect()
        provider_clone.score_batch(&pairs_ref)
    })
    .await
// .await is OUTSIDE the closure — on the tokio thread

let nli_scores: Vec<NliScores> = match nli_result:
    Ok(Ok(scores)) => scores
    Ok(Err(e)) =>
        tracing::warn!(error = %e, "graph inference tick: score_batch failed")
        return
    Err(e) =>
        tracing::warn!(error = %e, "graph inference tick: rayon task cancelled")
        return

// Length mismatch guard — unchanged
if nli_scores.len() != scored_input.len():
    tracing::warn!(
        scores_len = nli_scores.len(),
        pairs_len = scored_input.len(),
        "graph inference tick: score_batch length mismatch; skipping write"
    )
    return

// Construct NliCandidatePair vec by zipping origins with scores
// This is where pair_origins is consumed.
let merged_pairs: Vec<NliCandidatePair> =
    pair_origins
        .into_iter()
        .zip(nli_scores.iter().cloned())
        .map(|(origin, scores)| match origin:
            PairOrigin::SupportsContradict { source_id, target_id, cosine } =>
                NliCandidatePair::SupportsContradict { source_id, target_id, cosine, nli_scores: scores }
            PairOrigin::Informs(candidate) =>
                NliCandidatePair::Informs { candidate, nli_scores: scores }
        )
        .collect()
```

### Phase 8 — SupportsContradict write loop (refactored, same logic)

The existing `write_inferred_edges_with_cap` call is replaced by inline pattern matching
on `merged_pairs`. Same logic — only `SupportsContradict` variants are written here:

```
// Phase 8 — Write Supports edges (SupportsContradict variants only)
// Unchanged threshold: config.supports_edge_threshold (not nli_informs_ppr_weight)
let mut edges_written: usize = 0
let timestamp = current_timestamp_secs()

for pair in &merged_pairs:
    if edges_written >= config.max_graph_inference_per_tick: break

    if let NliCandidatePair::SupportsContradict { source_id, target_id, cosine: _, nli_scores } = pair:
        // Evaluate entailment only; contradiction discarded (C-13 / AC-10a)
        if nli_scores.entailment > config.supports_edge_threshold:
            let metadata_json = format_nli_metadata(nli_scores)
            let written = write_nli_edge(
                store, *source_id, *target_id, "Supports",
                nli_scores.entailment,  // weight = entailment score for Supports
                timestamp,
                &metadata_json,
            ).await
            if written: edges_written += 1

// Note: the existing write_inferred_edges_with_cap helper can be preserved if the
// implementation agent prefers. However, it takes &[(u64, u64)] — the refactored
// approach using merged_pairs directly is cleaner and avoids extracting pairs back out.
// Either approach is valid; Phase 8b MUST use pattern matching directly on merged_pairs.
```

### Phase 8b — NEW: Informs write loop

```
// Phase 8b — Write Informs edges (Informs variants only)
// No secondary cap: budget was reserved in Phase 5; write all that pass the composite guard.
let mut informs_edges_written: usize = 0
let informs_count: usize = merged_pairs.iter().filter(|p| matches!(p, NliCandidatePair::Informs { .. })).count()

for pair in &merged_pairs:
    if let NliCandidatePair::Informs { candidate, nli_scores } = pair:

        // Composite guard (FR-11, SR-01):
        // Guard 1: NLI neutral threshold (fixed constant — C-09, not configurable)
        // Guard 2: temporal ordering re-verified from InformsCandidate fields (not re-queried)
        // Guard 3: cross-feature re-verified from InformsCandidate fields (not re-queried)
        // Guard 4: category pair membership verified in Phase 4b — implicit in Informs variant
        // Guard 5 (FR-11): mutual exclusion — pair must not also trigger entailment or contradiction
        //   (prevents same pair from producing both Supports and Informs edges)
        if nli_scores.neutral > 0.5
            && candidate.source_created_at < candidate.target_created_at
            && candidate.source_feature_cycle != candidate.target_feature_cycle
            && nli_scores.entailment <= config.supports_edge_threshold
            && nli_scores.contradiction <= config.nli_contradiction_threshold:

            let weight = candidate.cosine * config.nli_informs_ppr_weight
            // Weight finitude guard (C-13, NF-08)
            // cosine is from HNSW (finite f32); ppr_weight validated in config (finite f32)
            // Product of two finite f32 values is finite unless one is ±Inf — not possible here.
            // assert!(weight.is_finite()) is acceptable as a debug_assert
            debug_assert!(weight.is_finite(), "Informs edge weight must be finite")

            let metadata_json = format_nli_metadata_informs(nli_scores)
            let written = write_nli_edge(
                store,
                candidate.source_id,
                candidate.target_id,
                "Informs",              // RelationType::Informs.as_str() — must match exactly
                weight,
                timestamp,
                &metadata_json,
            ).await
            if written: informs_edges_written += 1

tracing::debug!(
    informs_edges_written,
    informs_pairs_evaluated = informs_count,
    "graph inference tick: Informs write complete"
)
```

### New helper: format_nli_metadata_informs

Add as a module-private function, adjacent to the existing `format_nli_metadata` import:

```
fn format_nli_metadata_informs(scores: &NliScores) -> String:
    // Includes nli_neutral because that is the Informs decision criterion (Phase 8b guard 1)
    // Includes entailment and contradiction for consistency and cross-type debugging
    serde_json::json!({
        "nli_entailment":    scores.entailment,
        "nli_contradiction": scores.contradiction,
        "nli_neutral":       scores.neutral,     // decision criterion for Informs edges
    })
    .to_string()
```

This is a new function in `nli_detection_tick.rs` — it does NOT modify the existing
`format_nli_metadata` in `nli_detection.rs`. The existing function remains unchanged.

## Final Tick Phase Structure

```
Phase 1  — NLI guard (unchanged)
Phase 2  — query_by_status, query_entries_without_edges,
           query_existing_supports_pairs (unchanged),
           query_existing_informs_pairs (NEW)
Phase 3  — select_source_candidates (unchanged)
Phase 4  — HNSW scan @ supports_candidate_threshold (strict >)
           collects supports_metadata: Vec<(u64, u64, f32)>
Phase 4b — HNSW scan @ nli_informs_cosine_floor (inclusive >=) (NEW)
           collects informs_metadata: Vec<InformsCandidate>
Phase 5  — Sort supports by priority, truncate to max_cap
           remaining = max_cap - supports.len()
           Sort informs by cosine desc, truncate to remaining
           Log cap accounting (debug)
Phase 6  — Text fetch for supports + informs (merged into scored_input + pair_origins)
Phase 7  — Single rayon spawn: score_batch on all pairs (unchanged closure)
           Zip pair_origins + nli_scores → merged_pairs: Vec<NliCandidatePair>
Phase 8  — Pattern match SupportsContradict: write Supports (unchanged logic)
Phase 8b — Pattern match Informs: composite guard + write Informs (NEW)
```

## State Machines

None. `run_graph_inference_tick` is a stateless async function. All state is local.

## Error Handling

All errors within the tick are handled by returning early with `tracing::warn!` (fatal
per-tick errors) or `tracing::debug!` + `continue` (per-pair skips). No `?` propagation
from the tick function — it returns `()` (infallible contract).

Phase 2 DB failures degrade to empty sets (INSERT OR IGNORE backstop).
Phase 4b embedding or HNSW failures skip the source entry (debug log).
Phase 6 content fetch failures skip the pair (debug log, pair_origin not pushed).
Phase 7 rayon failure: early return with warn.
Phase 8b write_nli_edge failure: warn inside write_nli_edge; returns false; no edge counted.

## Key Test Scenarios

AC-13: Integration test. Two entries satisfying all guards (correct categories, temporal
order, different feature cycles, cosine >= 0.45, NLI neutral > 0.5). Run tick. Assert
`GRAPH_EDGES` has one row with `relation_type = "Informs"`, `source = "nli"`, correct
`source_id`, `target_id`, finite `weight`, and metadata containing `"nli_neutral"`.

AC-14: Same pair but `source.created_at = target.created_at`. Assert no Informs row.
AC-14b: Same pair but `source.created_at > target.created_at` (reversed). Assert no row.

AC-15: Same pair but `source.feature_cycle = target.feature_cycle`. Assert no Informs row.

AC-16: Pair with categories `("decision", "decision")` not in `informs_category_pairs`.
Assert no Informs row.

AC-17: Pair with cosine 0.44 against default floor 0.45. Assert no Informs row.

AC-18: Asserts Phase 4b scan uses `nli_informs_cosine_floor` (not `supports_candidate_threshold`).
Code inspection or unit test verifying the config field referenced.

AC-19: After tick, assert `source` column in the written `GRAPH_EDGES` row equals `"nli"`
(EDGE_SOURCE_NLI constant, via `write_nli_edge`'s SQL literal `'nli'`).

AC-20: Known cosine 0.50, ppr_weight 0.6. Written edge weight = 0.30 (within f32 epsilon).

AC-21: CI gate: `grep -n 'Handle::current' nli_detection_tick.rs` returns empty.

AC-22: CI gate: `grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' nli_detection_tick.rs`
returns empty. Domain strings are in `config.rs` only.

AC-23: Run tick twice on same pair. Assert exactly one `Informs` row in GRAPH_EDGES
(dedup via `existing_informs_pairs` pre-filter + INSERT OR IGNORE backstop).

R-04 cross-route test: Vec containing one SupportsContradict (high entailment, low neutral)
and one Informs (high neutral, low entailment). Run Phase 8 only — assert only Supports
written. Run Phase 8b only — assert only Informs written.

R-11 cap accounting: `max_graph_inference_per_tick = 5`, 5 Supports candidates, 3 Informs
candidates. After Phase 5: `remaining = 0`, informs truncated to 0. Assert debug log shows
`informs_candidates_dropped = 3`.

Neutral boundary: pair where `nli_scores.neutral = 0.5` exactly. Assert no Informs edge
written (strictly > 0.5 required).

FR-11 mutual exclusion: pair where both `entailment > supports_edge_threshold` AND
`neutral > 0.5`. Assert no Informs edge written (guard 5 prevents dual-edge contamination).

## Constraints

- C-04 (W1-2): No inline async NLI. No `spawn_blocking`. Single rayon spawn per tick.
- C-05 / C-14: Rayon closure body must remain SYNC-ONLY. No `tokio::runtime::Handle::current()`
  or `.await` inside the closure. CI gate enforced.
- C-08: `max_graph_inference_per_tick` is the sole cap. No new cap field.
- C-09: `nli.neutral > 0.5` is a hard-coded literal. Not configurable.
- C-11: `NliCandidatePair` is a tagged union. Parallel index-matched vecs prohibited.
- C-12: Domain vocabulary strings must not appear as string literals in this file.
  They are passed in via `config.informs_category_pairs` (read at runtime).
- C-13 / NF-08: Informs edge weight must be finite before write_nli_edge call.
  Use `debug_assert!(weight.is_finite())`.
- The string passed to `write_nli_edge` as `relation_type` must be `"Informs"` exactly —
  matching `RelationType::Informs.as_str()`. A mismatch causes the R-10 warn in
  `build_typed_relation_graph` and silently drops the edge from PPR.
