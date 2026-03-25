# ASS-031: Open Questions for W3-1 Delivery

These questions were not resolved by this research spike and must be decided during W3-1 delivery. Each is tagged with a recommended default and the cost of getting it wrong.

---

## OQ-01 (BLOCKER): session_category_snapshots table — schema and retention

**Question**: The WA-2 category_counts histogram is in-memory only and never persisted. W3-1 training requires it for historical session reconstruction. What is the exact schema, write path, and retention policy for the new `session_category_snapshots` table?

**Why it's a blocker**: Without it, training samples for all historical sessions will have zero category histograms. The model will never learn category-phase correlation signals, which is one of the primary advantages of W3-1 over the manual formula.

**Recommended schema**:
```sql
CREATE TABLE session_category_snapshots (
    session_id  TEXT    NOT NULL,
    category    TEXT    NOT NULL,
    count       INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (session_id, category)
);
CREATE INDEX idx_scs_session ON session_category_snapshots (session_id);
```

**Write path**: Extend `SessionRegistry::record_category_store()` to also write/upsert into `session_category_snapshots` via the analytics write queue (non-blocking, fire-and-forget, same pattern as `injection_log`).

**Retention**: Hard-delete rows for sessions older than `[gnn] snapshot_retention_days` (default 30) in the co-access cleanup phase of the tick.

**Decision gate**: This must be merged before or alongside W3-1 delivery. It is a non-breaking schema addition (analytics.db, version bump).

---

## OQ-02: query_count in SessionState

**Question**: `SessionState` does not have a `query_count` field. The `query_log` table records all queries but counting it per-session at context-build time is a DB read on the search hot path.

**Options**:
- A: Add `query_count: u32` to `SessionState`, incremented in `context_search` handler
- B: Count from `query_log` at session context build time (DB read, ~0.5ms)
- C: Omit `query_count` from the session context vector entirely

**Recommendation**: Option A. `query_count` is a lightweight addition to `SessionState` (u32, no lock contention). The search handler already mutates session state for injection_history. Increments are cheap. DB reads on the hot path should be avoided.

**Cost of wrong choice**: Low. `query_count` is one dimension of 29 in the session context. Getting it slightly stale (Option B with caching) or omitting it (Option C) would reduce signal quality marginally. The model will still train and converge.

---

## OQ-03: Phase vocabulary extraction procedure

**Question**: Phase strings are opaque in WA-1 — no enforced vocabulary. How does W3-1 extract the phase vocabulary from training data, and what is the exact procedure for rebuilding the vocabulary when new phase names appear?

**The constraint**: If a new phase name appears after the model is trained, it maps to the "other" bucket. The model must be retrained to learn the new phase's signal. When is retraining triggered for a vocabulary change?

**Recommended procedure**:
1. During each training run, collect all distinct `current_phase` values from training samples
2. Sort by frequency descending, take top `max_phase_vocab - 1` (default 16)
3. Remaining phases → "other" bucket
4. Save vocabulary alongside model weights in `ModelVersion.schema_version` metadata
5. If the new vocabulary has > 2 new phases compared to current production model → increment `schema_version` → force retrain from scratch on next gate pass
6. If 0-2 new phases → "other" bucket absorbs them → no forced retrain

**Cost of wrong choice**: Medium. A stale vocabulary means new workflow phases get zero phase signal. The model degrades gracefully (zero phase vector = "other"). But over time, if an entire new workflow type is introduced (e.g., migrating from SDLC to SRE operational workflow), the model would never learn the new phase vocabulary without a manual retrain trigger.

---

## OQ-04: Mode 3 query signal injection — concatenation vs. separate head

**Question**: For Mode 3 (reactive search re-ranking), the query signals (query_sim, nli_score) are appended as the last 3 dims of the input vector. An alternative is a two-headed architecture: shared backbone, separate Mode 1/2 output head and Mode 3 output head.

**Current recommendation**: Single model, query signal suffix ([query_sim, nli_score, query_present]) — simpler, fewer parameters, easier to train and debug.

**When to revisit**: If eval harness (W1-3) shows that Mode 3 relevance quality is materially worse than a two-headed model on the same training set, upgrade to two heads. This should be measurable after the first 2-3 training cycles.

**Cost of wrong choice**: Low. The current architecture can be retrained with a new head structure without discarding training data (the label format is the same, only the model topology changes).

---

## OQ-05: Blend alpha activation — hard threshold vs. smooth ramp

**Question**: The blend alpha (manual formula → GNN) ramps from 0.0 to 1.0 between MIN_TRAIN_SIZE (50) and FULL_TRUST_SIZE (150). The ramp function is linear by default. Should it be:
- A: Linear ramp (current recommendation)
- B: Sigmoid ramp (smoother, slower initial activation, faster middle)
- C: Hard threshold (0.0 below MIN, 1.0 above MIN)

**Recommendation**: Linear ramp (A). Simple to reason about, easy to tune via config. Sigmoid provides no practical benefit at these sample counts. Hard threshold (C) risks a sudden ranking change that surprises operators; the linear ramp makes the transition gradual and observable in eval metrics.

**How to monitor**: Log `blend_alpha` in the tick metadata. Operators see the transition progressing. W1-3 eval harness should track P@5 and MRR over time as alpha increases.

---

## OQ-06: EWC++ Fisher information — full or diagonal?

**Question**: `unimatrix-learn`'s `EwcState` likely computes diagonal Fisher information (standard EWC approximation). For the `RelevanceScorer`'s 5121 parameters, full Fisher is a 5121×5121 matrix (100MB) — impractical. Diagonal Fisher (5121 floats, ~20KB) is the correct choice. Confirm this is the `EwcState` implementation.

**Recommendation**: Verify `EwcState` uses diagonal Fisher before W3-1 delivery. If it uses full Fisher, cap at the 32-dim models it was designed for — the `RelevanceScorer` at 5121 params would require a diagonal-only implementation.

**Cost of wrong choice**: High if not verified. Full Fisher for 5121 params would make training 100× slower and 1000× more memory-intensive.

---

## OQ-07: EntryFeatureCache rebuild — full or incremental?

**Question**: The `EntryFeatureCache` stores precomputed entry feature vectors (dims 0-10 of the entry vector, excluding dynamic dims 9-10 which are session-dependent). Two rebuild strategies:

- **Full rebuild**: Every maintenance tick, scan all Active entries and recompute features. Predictable, always current.
- **Incremental rebuild**: On `context_store` success, add the new entry. On `context_deprecate`, remove. On TypedGraphState rebuild, update only the graph-derived dims (5-8) for all entries.

**Recommendation**: Start with full rebuild (simpler). Switch to incremental if profiling shows full rebuild latency > 50ms. At 10K active entries × ~50μs per feature construction = 500ms — this may be too slow for a tick with a 2-minute budget. Profile before committing.

**Cost of wrong choice**: Medium. Full rebuild at 10K entries is ~500ms (15% of tick budget). If the tick budget is tight, this forces incremental. If the knowledge base stays under 5K entries (typical Unimatrix deployment), full rebuild is fine.

---

## OQ-08: signal_queue.gnn_processed column vs. separate tracking table

**Question**: The training design adds a `gnn_processed` column to `signal_queue` to mark which records have been ingested into the training reservoir. An alternative is a separate `gnn_training_cursor` table (a single row recording the last processed `signal_id`).

**Recommendation**: Cursor table approach. It avoids wide column in `signal_queue` (which may have analytics retention policies that conflict with GNN processing state), and the cursor is simpler to reset if the reservoir is rebuilt. Schema:

```sql
CREATE TABLE IF NOT EXISTS gnn_training_cursor (
    model_name  TEXT PRIMARY KEY,  -- e.g. "relevance_scorer"
    last_signal_id INTEGER NOT NULL DEFAULT 0
);
```

**Cost of wrong choice**: Low. Both approaches work. The cursor table is marginally cleaner.

---

## Summary Table

| OQ | Severity | Decision needed by | Recommended default |
|---|---|---|---|
| OQ-01 (session_category_snapshots) | **BLOCKER** | W3-1 design gate | Add table, upsert from record_category_store |
| OQ-02 (query_count in SessionState) | Medium | W3-1 implementation | Add field, increment in context_search handler |
| OQ-03 (phase vocabulary procedure) | Medium | W3-1 design gate | Extract from training data, top-16, "other" bucket |
| OQ-04 (Mode 3 architecture) | Low | After first eval cycle | Single model with query suffix |
| OQ-05 (blend alpha function) | Low | W3-1 implementation | Linear ramp |
| OQ-06 (EWC++ Fisher type) | High | W3-1 design gate (verify) | Diagonal only — verify EwcState implementation |
| OQ-07 (cache rebuild strategy) | Medium | Profile during W3-1 | Full rebuild; switch to incremental if >50ms |
| OQ-08 (training cursor) | Low | W3-1 implementation | Separate cursor table |
