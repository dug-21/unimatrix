## ADR-006: Personalization Vector Construction — hnsw_score × phase_affinity_score

### Context

The PPR personalization vector seeds relevance mass from the HNSW result set. Each HNSW
candidate has a cosine similarity score. The question is how to weight candidates in the
personalization vector — pure HNSW score, or HNSW score modulated by phase affinity.

**Option A: Pure HNSW scores**
`personalization[id] = hnsw_score`. Simple; no dependency on `PhaseFreqTable`. Misses
the opportunity to amplify seeds that match the current agent's phase — a decision entry
relevant to the current phase should propagate more PPR mass than an off-phase entry with
the same HNSW score.

**Option B: hnsw_score × phase_affinity_score**
`personalization[id] = hnsw_score × phase_affinity_score(id)`. Phase-aware; exploits
the frequency table built by col-031. On cold-start (no phase history, `use_fallback =
true` in `PhaseFreqTable`), `phase_affinity_score` returns `1.0` — the multiplication
is a no-op and the vector degrades gracefully to Option A.

The `phase_affinity_score` method has a two-caller contract documented in ADR-003 col-031
(Unimatrix entry #3687):
- **Fused scoring (guarded caller)**: checks `PhaseFreqTable.use_fallback` BEFORE calling;
  when true, sets `phase_explicit_norm = 0.0` directly and skips the method.
- **PPR (direct caller)**: calls the method directly without checking `use_fallback`.
  Receives `1.0` when `use_fallback = true` — neutral, not suppressive.

This asymmetry is intentional: fused scoring uses `phase_explicit_norm = 0.0` to preserve
pre-col-031 score identity (NFR-04). PPR uses `× 1.0` to preserve HNSW score fidelity
(no cold-start suppression of PPR seeds).

**Dependency on #414**: `phase_affinity_score` for entry-level affinity depends on the
per-entry rank-score data in `PhaseFreqTable`, populated by col-031 and #414. If #414 is
not merged, the `PhaseFreqTable` may have bucket data but not per-entry data — in this
case `phase_affinity_score` returns `1.0` for entries absent from the bucket (graceful
fallback per `phase_freq_table.rs:192-210`).

### Decision

The personalization vector construction in Step 6d is:

1. Acquire a snapshot of `phase_affinity_score` data. Since the col-031 pre-loop block
   already extracts a `phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>>` before
   Step 7, Step 6d uses this same snapshot. No additional lock acquisition is needed.

2. For each HNSW candidate `(entry, sim)` in `results_with_scores`:
   ```
   let affinity = if let (Some(phase), Some(snapshot)) = (&params.current_phase, &phase_snapshot) {
       snapshot
           .get(&entry.category)
           .and_then(|bucket| bucket.iter().find(|(id, _)| *id == entry.id))
           .map(|(_, score)| *score as f64)
           .unwrap_or(1.0)  // absent = neutral (ADR-003 col-031 contract)
   } else {
       1.0  // no phase or no snapshot = cold-start neutral
   };
   seed_scores.insert(entry.id, sim * affinity);
   ```

   This does NOT call `phase_affinity_score()` directly — it reads from the already-cloned
   snapshot. This avoids re-acquiring the `PhaseFreqTableHandle` lock at Step 6d (the lock
   is released before the scoring loop per col-031 ADR-004).

3. Normalize `seed_scores` to sum 1.0.

4. Zero-sum guard: if all values are `0.0` (degenerate — all HNSW scores are zero, which
   should not occur in practice), skip PPR and return.

The two-caller contract from ADR-003 col-031 is satisfied: PPR reads `1.0` for absent
entries from the snapshot (same as `phase_affinity_score` returns when `use_fallback = true`).

### Consequences

- Phase-aware PPR personalization amplifies seeds matching the agent's current phase,
  improving recall for the active domain.
- Cold-start (no phase, no snapshot) degrades gracefully to HNSW-score-only seeds —
  identical behavior to Option A.
- No additional lock acquisition at Step 6d: the snapshot is already extracted by the
  col-031 pre-loop block.
- The absence of a direct `phase_affinity_score()` call means the SR-06 risk (wrong guard
  around the method) is mitigated by construction: the snapshot read cannot accidentally
  acquire the wrong fallback behavior.
- When #414 ships, `PhaseFreqTable` buckets gain per-entry scores and the affinity weights
  become meaningful. No code change in Step 6d is required — the snapshot read pattern
  handles it transparently.
