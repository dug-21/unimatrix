# search_step_6d.md — Step 6d PPR Expansion Block in search.rs

## Purpose

Insert Step 6d into `crates/unimatrix-server/src/services/search.rs` between the existing
Step 6b (supersession candidate injection, ends at line ~755) and Step 6c (co-access boost
prefetch, starts at line ~757). After insertion, Step 6c runs over the full expanded pool.

Step 6d propagates relevance mass from HNSW seeds through positive-edge chains, blends PPR
scores into existing candidates, and injects new PPR-only entries (quarantine-checked).

---

## Pre-Conditions (available at Step 6d insertion point)

All of the following are already in scope when Step 6d executes — no new lock acquisitions:

| Variable | Type | Source |
|----------|------|--------|
| `typed_graph` | `TypedRelationGraph` | Cloned at line 638, before Step 6a |
| `use_fallback` | `bool` | Cloned at line 638 from `TypedGraphState` |
| `results_with_scores` | `Vec<(EntryRecord, f64)>` | Mutated throughout Steps 6-6b |
| `phase_snapshot` | `Option<HashMap<String, Vec<(u64, f32)>>>` | Extracted by col-031 pre-loop block before Step 7 |
| `params` | search params | Contains `current_phase: Option<String>` (col-031) |
| `cfg` | `InferenceConfig` reference | Source of all five PPR fields |
| `self.entry_store` | `Arc<dyn EntryStore>` | For sequential async fetches |

The `phase_snapshot` was extracted by col-031's pre-loop block using:
```
snapshot.get(&entry.category)
    .and_then(|bucket| bucket.iter().find(|(id, _)| *id == entry.id))
    .map(|(_, score)| *score as f64)
    .unwrap_or(1.0)
```
Step 6d reads the snapshot directly. No `PhaseFreqTableHandle` lock is re-acquired (ADR-006,
NFR-04).

---

## Insertion Point

The block inserts between two existing comments. The result after insertion:

```
// Step 6b: Supersession candidate injection (crt-010)
[... existing Step 6b code ...]

// Step 6d: PPR expansion (crt-030)
[... new code below ...]

// Step 6c: Co-access boost map prefetch (crt-024, SR-07).
[... existing Step 6c code ...]
```

---

## Step 6d Block Pseudocode

```
// Step 6d: PPR expansion (crt-030)
//
// Expands the candidate pool with multi-hop PPR neighbors from the HNSW seed set.
// Guard: skip entirely when use_fallback = true (cold-start / Supersedes cycle).
// Bit-for-bit identical to pre-crt-030 when use_fallback = true (AC-12 / R-02).
if !use_fallback {

    // -----------------------------------------------------------------------
    // Phase 1: Build and normalize the personalization vector (FR-06 / ADR-006)
    // -----------------------------------------------------------------------
    //
    // Read from phase_snapshot (already extracted by col-031 pre-loop block).
    // Do NOT call phase_affinity_score() directly — no lock re-acquisition (ADR-006).
    // Cold-start (no phase, no snapshot): affinity = 1.0 for all seeds (SR-06).

    let mut seed_scores: HashMap<u64, f64> = HashMap::with_capacity(results_with_scores.len())

    FOR (entry, sim) IN &results_with_scores DO
        // Affinity lookup: read from snapshot, default 1.0 if absent (cold-start neutral)
        let affinity: f64 = if let (Some(phase), Some(snapshot)) =
                (&params.current_phase, &phase_snapshot)
        {
            snapshot
                .get(&entry.category)
                .and_then(|bucket| bucket.iter().find(|(id, _)| *id == entry.id))
                .map(|(_, score)| *score as f64)
                .unwrap_or(1.0)   // absent entry → neutral (ADR-003 col-031 contract)
        } else {
            1.0   // no phase or no snapshot → cold-start neutral
        }

        seed_scores.insert(entry.id, sim * affinity)
    END FOR

    // Normalize to sum 1.0
    let total: f64 = seed_scores.values().sum()

    // Zero-sum guard (FR-08 step 3 / FM-05):
    // All HNSW scores are 0.0 — degenerate, should not occur in practice.
    // Skip PPR entirely; proceed to Step 6c with unchanged pool.
    if total == 0.0 {
        // jump to Step 6c (end of if !use_fallback block)
    } ELSE {
        FOR value IN seed_scores.values_mut() DO
            *value /= total
        END FOR

        // -----------------------------------------------------------------------
        // Phase 2: Run PPR
        // -----------------------------------------------------------------------

        let ppr_scores: HashMap<u64, f64> = personalized_pagerank(
            &typed_graph,
            &seed_scores,
            cfg.ppr_alpha,
            cfg.ppr_iterations,
        )
        // Returns empty map if seed_scores is empty (handled above) or graph is empty.

        // -----------------------------------------------------------------------
        // Phase 3: Blend scores for existing HNSW candidates (FR-08 step 5)
        // -----------------------------------------------------------------------
        //
        // For each entry already in results_with_scores, if it appears in ppr_scores:
        //   new_sim = (1 - ppr_blend_weight) * current_sim + ppr_blend_weight * ppr_score
        //
        // This is an in-place update to the f64 score field.

        FOR (entry, sim) IN &mut results_with_scores DO
            if let Some(&ppr_score) = ppr_scores.get(&entry.id) {
                *sim = (1.0 - cfg.ppr_blend_weight) * (*sim)
                     + cfg.ppr_blend_weight * ppr_score
            }
        END FOR

        // -----------------------------------------------------------------------
        // Phase 4: Identify PPR-only candidates for expansion (FR-08 step 6)
        // -----------------------------------------------------------------------
        //
        // Entries in ppr_scores that are NOT already in results_with_scores
        // and whose PPR score STRICTLY exceeds ppr_inclusion_threshold (AC-13, R-06).
        // Threshold comparison: > (not >=).

        let existing_ids: HashSet<u64> =
            results_with_scores.iter().map(|(e, _)| e.id).collect()

        // Collect PPR-only candidates above threshold
        let mut ppr_only_candidates: Vec<(u64, f64)> =
            ppr_scores
                .iter()
                .filter(|(&id, &score)| {
                    !existing_ids.contains(&id) &&
                    score > cfg.ppr_inclusion_threshold   // strictly greater (AC-13 / R-06)
                })
                .map(|(&id, &score)| (id, score))
                .collect()

        // Sort descending by PPR score
        ppr_only_candidates.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal))

        // Cap at ppr_max_expand (E-04)
        ppr_only_candidates.truncate(cfg.ppr_max_expand)

        // -----------------------------------------------------------------------
        // Phase 5: Fetch and inject PPR-only entries (FR-08 step 6 / R-08 Critical)
        // -----------------------------------------------------------------------
        //
        // Sequential async fetches (ADR-008 / C-10).
        // Error from any single fetch: silently skip (AC-13 / FM-02 / R-05).
        // Quarantined entry: silently skip (R-08 Critical — must have dedicated test).

        FOR (entry_id, ppr_score) IN ppr_only_candidates DO
            // Fetch from store
            let entry: EntryRecord = match self.entry_store.get(entry_id).await {
                Ok(e)  => e,
                Err(_) => CONTINUE   // silent skip on error (AC-13 / R-05)
            }

            // R-08 Critical: quarantine check — MANDATORY for every PPR-fetched entry.
            // PPR-only entries bypass the Step 6 HNSW quarantine filter.
            // This check is the ONLY thing preventing quarantined entries from
            // appearing in search results via the PPR path.
            if SecurityGateway::is_quarantined(&entry.status) {
                CONTINUE   // silent skip (AC-13 / R-08)
            }

            // Assign initial similarity (FR-08 step 6 / ADR-007):
            //   initial_sim = ppr_blend_weight * ppr_score
            // PPR-only entries have no HNSW component; ppr_blend_weight is the
            // "PPR trust" coefficient for new entries (dual role, ADR-007).
            // At default 0.15, initial_sim is in [0.0, 0.15] — naturally ranks
            // below HNSW candidates (SR-07 resolution).
            let initial_sim = cfg.ppr_blend_weight * ppr_score

            results_with_scores.push((entry, initial_sim))
        END FOR

    }  // end else (total != 0.0)

}  // end if !use_fallback

// Step 6c: Co-access boost map prefetch (crt-024, SR-07).
// Now runs over the full expanded pool (HNSW + PPR-surfaced entries).
// [... existing Step 6c code unchanged ...]
```

---

## Data Flow Summary

```
INPUTS:
  results_with_scores   Vec<(EntryRecord, f64)>  HNSW pool with similarity scores
  typed_graph           TypedRelationGraph        pre-built, cloned at line 638
  phase_snapshot        Option<...>               col-031 snapshot, nil → 1.0 affinity
  use_fallback          bool                      from TypedGraphState clone
  cfg                   InferenceConfig           five PPR fields

COMPUTATION:
  seed_scores           HashMap<u64, f64>         hnsw_sim × phase_affinity, normalized
  ppr_scores            HashMap<u64, f64>         PPR output, all reachable nodes
  ppr_only_candidates   Vec<(u64, f64)>           new entries above threshold, sorted desc, capped

MUTATIONS:
  results_with_scores   - existing entries: similarity blended with PPR score
                        - new entries: appended with initial_sim = blend_weight × ppr_score

OUTPUT to Step 6c:
  results_with_scores   full expanded pool; co-access prefetch uses this complete pool
```

---

## Initialization / Cold-Start Paths

| Condition | Behavior |
|-----------|----------|
| `use_fallback = true` | Skip Step 6d entirely. Zero allocation. results_with_scores unchanged. |
| `phase_snapshot = None` | Affinity = 1.0 for all seeds. PPR seeds = normalized HNSW scores only. |
| `params.current_phase = None` | Same as phase_snapshot = None: affinity = 1.0. |
| `seed_scores` sum = 0.0 | Zero-sum guard fires. PPR skipped. results_with_scores unchanged. |
| `ppr_scores` is empty | No blend, no expansion. results_with_scores unchanged. |
| `ppr_only_candidates` is empty | No fetches. results_with_scores unchanged (blend may have run). |
| All fetches error | FM-02: pool unchanged except blend. No error surfaced to caller. |

---

## State Machine: Step 6d Execution Paths

```
START
  │
  ├─ use_fallback = true ──────────────────────────────────→ SKIP to Step 6c
  │
  └─ use_fallback = false
       │
       ├─ Build seed_scores (phase_snapshot or cold-start 1.0)
       │
       ├─ sum(seed_scores) = 0.0 ──────────────────────────→ SKIP to Step 6c
       │
       └─ Normalize seed_scores
            │
            └─ Call personalized_pagerank(typed_graph, seed_scores, α, iters)
                 │
                 ├─ Blend existing HNSW candidates' similarity scores (in-place)
                 │
                 ├─ Filter ppr_scores: new entries > threshold, sort desc, cap
                 │
                 └─ FOR each candidate:
                        fetch entry_store.get(id)
                           ├─ Error → skip
                           ├─ Quarantined → skip (R-08)
                           └─ Active → push (entry, blend_weight * ppr_score)
                 │
                 └─ DONE → Step 6c (full expanded pool)
```

---

## Error Handling

| Error Condition | Handling | Risk |
|-----------------|----------|------|
| `entry_store.get()` returns Err | Silent skip, no log, continue loop | R-05 / AC-13 |
| Fetched entry is quarantined | Silent skip via `is_quarantined` check | R-08 Critical |
| `ppr_scores` is empty (no positive edges) | No blend, no expansion, proceed | E-02 / E-07 |
| zero-sum seed_scores | Early exit from Step 6d | FM-05 |
| NaN in ppr_scores from personalized_pagerank | Protected by PPR function (all-finite guarantee) | R-07 |

No `?` operator / no error propagation out of Step 6d. All failures are silent skip paths.
This is consistent with AC-13 and the existing Step 6b pattern.

---

## Key Test Scenarios

### T-6D-01: use_fallback = true → pool unchanged, zero allocation (R-02 / AC-12)
```
Set use_fallback = true
pool_before = results_with_scores.clone()
Execute Step 6d block
pool_after  = results_with_scores.clone()
ASSERT pool_before == pool_after   // bit-for-bit identical: same IDs, same scores
```

### T-6D-02: Quarantine check — quarantined PPR-only entry is NOT injected (R-08 Critical)
```
Setup: HNSW seed A in pool; PPR neighbor B in graph
       B is returned by entry_store.get() with status = Quarantined
Execute Step 6d
ASSERT results_with_scores does NOT contain B
```

### T-6D-03: Active PPR-only entry IS injected (R-08 / complement)
```
Setup: HNSW seed A in pool; PPR neighbor B in graph
       B is returned by entry_store.get() with status = Active, ppr_score > threshold
Execute Step 6d
ASSERT results_with_scores contains B with initial_sim == cfg.ppr_blend_weight * ppr_score
```

### T-6D-04: fetch error → silent skip, pool unchanged for that entry (R-05 / AC-13)
```
Mock entry_store.get() to return Err for PPR-only candidate B
Execute Step 6d
ASSERT B is not in results_with_scores
ASSERT other PPR candidates (if any) are still processed
```

### T-6D-05: Inclusion threshold strictly greater (R-06 / AC-13)
```
ppr_scores contains entry C with score == cfg.ppr_inclusion_threshold (exactly equal)
ASSERT C is NOT in ppr_only_candidates   // > not >=

ppr_scores contains entry D with score == cfg.ppr_inclusion_threshold + f64::EPSILON
ASSERT D IS in ppr_only_candidates
```

### T-6D-06: ppr_max_expand cap — only top N by score injected (E-04)
```
cfg.ppr_max_expand = 2
ppr_only_candidates has 5 entries all above threshold with distinct scores
Execute Step 6d
ASSERT only 2 entries injected: the two with highest PPR scores
```

### T-6D-07: Blend formula for existing HNSW candidates (FR-08 step 5)
```
Entry A in pool with sim = 0.8, ppr_score = 0.4, ppr_blend_weight = 0.15
Expected new_sim = (1.0 - 0.15) * 0.8 + 0.15 * 0.4 = 0.68 + 0.06 = 0.74
Execute Step 6d
ASSERT results_with_scores[A].1 ≈ 0.74 (within f64 tolerance)
```

### T-6D-08: ppr_blend_weight = 0.0 — existing candidates unaffected, new entries at 0.0 (R-03)
```
cfg.ppr_blend_weight = 0.0
Entry A in pool with sim = 0.8 and ppr_score = 0.5
PPR-only entry B with ppr_score > threshold
Execute Step 6d
ASSERT results_with_scores[A].1 == 0.8   // unchanged (blend formula: 1.0 * 0.8 + 0.0 = 0.8)
ASSERT results_with_scores[B].1 == 0.0   // initial_sim = 0.0 * ppr_score = 0.0
```

### T-6D-09: phase_snapshot = None → affinity = 1.0 cold-start (R-10 / ADR-006)
```
phase_snapshot = None
Entry A in pool with sim = 0.6
seed_scores[A] before normalization = 0.6 * 1.0 = 0.6
ASSERT seed_scores normalization proceeds with this value
```

### T-6D-10: Phase-aware personalization — different from uniform (AC-16 / R-10)
```
phase_snapshot has entry A in category "decision" with score = 2.0
Entry A in HNSW pool with sim = 0.5
seed_scores[A] before normalization = 0.5 * 2.0 = 1.0
Entry B in HNSW pool with sim = 0.5, not in snapshot → affinity = 1.0 → value = 0.5
After normalization: seed_scores[A] > seed_scores[B]   // A amplified by phase affinity
ASSERT personalization vector is NOT uniform
```

### T-6D-11: Integration — PPR-surfaced entry participates in Step 6c co-access prefetch (I-02)
```
PPR-only entry B is injected in Step 6d with high PPR score blending to top position
Confirm Step 6c co-access prefetch includes B's ID in its result_ids parameter
```

### T-6D-12: Integration — PPR-only entry passes through NLI scoring without error (I-04)
```
PPR-only entry B injected with initial_sim = 0.075
Step 7 NLI scores B against the query string
ASSERT no panic or error; B has a valid NLI score in [0.0, 1.0]
```

### T-6D-13: Zero-sum guard fires — pool unchanged (FM-05)
```
Force all HNSW entries to have sim = 0.0 (degenerate)
Execute Step 6d
ASSERT results_with_scores is unchanged after Step 6d
ASSERT personalized_pagerank was NOT called
```
