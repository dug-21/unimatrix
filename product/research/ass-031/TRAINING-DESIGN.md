# ASS-031: Training Design

---

## 1. Training Sample Definition

```rust
pub struct RelevanceSample {
    /// Input feature vector for the scorer.
    pub digest: RelevanceDigest,

    /// Label: 1.0 = relevant/helpful, 0.0 = irrelevant/unhelpful.
    pub label: f32,

    /// Sample weight: 1.0 for strong labels, 0.3-0.7 for weak/implicit.
    pub weight: f32,

    /// Source for provenance and debugging.
    pub source: RelevanceLabelSource,

    /// The entry this sample describes.
    pub entry_id: u64,

    /// Session in which the label was generated.
    pub session_id: String,

    /// Timestamp of label generation (millis).
    pub timestamp: u64,
}

pub enum RelevanceLabelSource {
    ExplicitHelpful,
    ExplicitUnhelpful,
    ImplicitSuccessSession,
    ImplicitReworkSession,
    ImplicitReSearch,       // W1-5: agent re-searched same topic after retrieval
    ImplicitPhaseComplete,  // W1-5: successful phase completion after retrieval
    MissedRetrieval,        // WA-3 (deferred): entry existed, not served, agent filled gap
}
```

---

## 2. Label Generation — Three Signal Sources

### 2.1 Explicit Helpfulness Votes (strong labels, sparse)

**Source**: `signal_queue` where `signal_type = Helpful` or `Flagged`.

```
For each record in signal_queue (Helpful):
    entry_ids → label = 1.0, weight = 1.0, source = ExplicitHelpful

For each record in signal_queue (Flagged):
    entry_ids → label = 0.0, weight = 1.0, source = ExplicitUnhelpful
```

Session context at label time: reconstructed from `injection_log`, `query_log`, `sessions`, and `session_category_snapshots` (OQ-01) at the timestamp of the vote.

**Why strong weight**: The agent explicitly evaluated this entry. Direct supervision signal.

### 2.2 Implicit Behavioral Outcomes (moderate labels, automatic, session-level)

**Source**: `signal_queue` where `signal_source = ImplicitOutcome`.

```
session outcome = SUCCESS → all entries in entry_ids:
    label = 1.0, weight = 0.4, source = ImplicitSuccessSession

session outcome = REWORK → all entries in entry_ids served before first rework event:
    label = 0.0, weight = 0.4, source = ImplicitReworkSession
```

Session context: reconstructed from session close time (when `ImplicitOutcome` is recorded).

**Why 0.4 weight** (not 0.5 or 1.0): Session-level labels are noisy. A rework session may have contained useful entries; a successful session may have been trivially easy. Weight 0.4 allows the model to learn from the signal without being dominated by it. The product vision's `LabelGenerator` uses 0.3 for similar weak labels.

### 2.3 W1-5 Behavioral Signals (targeted implicit labels, per-entry)

**Source**: `signal_queue` where `signal_source = ImplicitRework`.

```
For each record (ImplicitRework):
    entry_ids → these entries triggered the rework signal
    label = 0.0, weight = 0.6, source = ImplicitReworkSession
```

W1-5 also provides positive labels via phase completion events. When fully implemented (col-023), these will feed:
```
successful phase completion → entries served in that phase:
    label = 1.0, weight = 0.5, source = ImplicitPhaseComplete

re-search within same session on same topic after entry served:
    entry = the served entry that preceded the re-search
    label = 0.0, weight = 0.7, source = ImplicitReSearch
```

W3-1 delivery does not need W1-5 complete to launch. The explicit + ImplicitOutcome signals are sufficient for first training. W1-5 signals improve label quality and quantity.

### 2.4 MissedRetrieval (deferred — WA-3 not implemented)

The `MISSED_RETRIEVALS` table does not exist. MissedRetrieval samples cannot be generated. This is acceptable: positive labels from explicit votes and ImplicitSuccessSession cover entry relevance; the absence of MissedRetrieval means some negative labels are missing. The model may be slightly over-optimistic for rare entries. Revisit after W3-1 ships and W1-3 eval coverage is assessed.

---

## 3. Class Balance

Explicit votes are typically imbalanced: agents mark things helpful more than unhelpful (confirmation bias). ImplicitOutcome introduces more negative labels via rework sessions.

**Balancing strategy**: Weighted reservoir sampling. The `TrainingReservoir<RelevanceSample>` already supports arbitrary `weight` per sample. At batch construction time, over-sample from the minority class:

```rust
// In batch construction:
let pos_samples = reservoir.items.iter().filter(|s| s.label > 0.5);
let neg_samples = reservoir.items.iter().filter(|s| s.label <= 0.5);

// Target: 60% positive, 40% negative
// If actual ratio is different, use weighted sampling at batch time
```

If fewer than 20% of reservoir samples are negative, training is gated until at least 20% negative label coverage is achieved. This prevents the model from trivially learning "everything is relevant".

---

## 4. Session Context Reconstruction for Historical Training Samples

For each historical training label, we need to reconstruct the session context at the time the label was generated (approximately at session close for implicit labels, at vote time for explicit labels).

**Reconstructible dimensions** (from DB):

| Dimension | Reconstruction Query |
|---|---|
| `phase_onehot` | `sessions.current_phase` at session close. For mid-session labels: use `FEATURE_ENTRIES.phase` for the feature cycle at label timestamp |
| `category_histogram` | **BLOCKED** — requires `session_category_snapshots` (OQ-01). Fallback: reconstruct approximately from `feature_entries` count per category for this session's feature_cycle |
| `injection_count_norm` | `SELECT COUNT(*) FROM injection_log WHERE session_id = ? AND timestamp <= label_ts` |
| `query_count_norm` | `SELECT COUNT(*) FROM query_log WHERE session_id = ? AND ts <= label_ts` |
| `rework_event_count_norm` | `SELECT COUNT(*) FROM observations WHERE session_id = ? AND hook = 'PostToolUse' AND ts_millis <= label_ts AND (input LIKE '%rework%' OR ...)` — approximate |
| `cycle_position` | `(label_ts - sessions.started_at) / expected_cycle_duration_ms` |
| `phase_count_norm` | Count distinct phases in `feature_entries` for this session |

**Category histogram fallback (while `session_category_snapshots` doesn't exist)**:

```sql
SELECT category, COUNT(*) as cnt
FROM feature_entries fe
JOIN entries e ON fe.entry_id = e.id
JOIN sessions s ON s.feature_cycle = fe.feature_id
WHERE s.session_id = ?
GROUP BY category
```

This counts all entries ever stored for the session's feature cycle, not the incremental histogram at label time. It is a coarser proxy but captures the general category distribution. Once `session_category_snapshots` is added, this fallback is replaced.

---

## 5. Training Procedure

### 5.1 Data Pipeline

```
1. On each maintenance tick:
   SELECT from signal_queue WHERE processed_by_gnn = 0 (new column to add)
   For each record:
       a. Reconstruct session context (DB queries above)
       b. Load entry features from entries + GnnFeatureCache
       c. Build RelevanceSample
       d. Add to TrainingReservoir<RelevanceSample>
       e. Mark record processed_by_gnn = 1

2. No separate RELEVANCE_TRAIN_QUEUE table needed — the reservoir IS the training buffer.
   (TrainingReservoir already handles capacity-bounded sampling.)
```

### 5.2 Training Gate

Training runs when ALL of the following are true:
- `reservoir.len() >= MIN_TRAIN_SIZE` (default: 50)
- `new_samples_since_last_training >= NEW_SAMPLE_THRESHOLD` (default: 25)
- OR `hours_since_last_training >= TRAINING_INTERVAL_HOURS` (default: 24)
- `positive_fraction >= 0.20` (class balance guard)

Training is skipped (logged as info) when gate conditions are not met.

### 5.3 Training Run

On the rayon pool (non-blocking, fire-and-forget):

```
1. Sample batch from reservoir: batch_size = min(reservoir.len(), 256)
2. Shuffle batch
3. For each sample in batch:
   a. forward pass → predicted_score
   b. compute_gradients(features, [label])
   c. accumulate weighted gradients: grad *= sample.weight
4. Apply accumulated gradients (batch gradient step, lr = 0.01)
5. EWC++ update: record Fisher information for current batch
6. Repeat for N_EPOCHS = 5 epochs over the batch
7. Save model to shadow slot via ModelRegistry
8. Log training metrics (loss, batch_size, positive_fraction)
```

EWC++ regularization (already in `unimatrix-learn`) prevents catastrophic forgetting as the model receives new sessions. This is critical for a long-running daemon — a model trained on recent sessions must not forget earlier patterns.

### 5.4 Shadow Promotion

After each training run:
1. New model is saved to `ModelSlot::Shadow`
2. Maintenance tick detects new shadow model
3. Run mini-eval: score held-out samples from the reservoir (20% held out)
4. If shadow loss < production loss: promote shadow → production
5. Previous production → `ModelSlot::Previous` (for rollback)
6. Rebuild `GnnFeatureCache` with new model scores

**Rollback gate**: If the shadow model's held-out loss is worse than production by >5%, shadow is discarded without promotion. The previous model remains in production.

---

## 6. Minimum Viable Training Set

| Threshold | Value | Rationale |
|---|---|---|
| `MIN_TRAIN_SIZE` | 50 samples | Matches product vision gate ("50+ helpfulness votes"). Below this, gradients are too noisy. |
| `FULL_TRUST_SIZE` | 150 samples | Blend alpha reaches 1.0. With 150 samples, the model has seen enough diversity (multiple sessions, multiple phases, multiple categories) to be trusted over the manual formula. |
| `CLASS_BALANCE_MIN` | 0.20 negative fraction | Prevents degenerate "always relevant" model. Revisit if real-world vote patterns show different ratios. |

These are defaults in config (`[gnn] min_train_size`, `[gnn] full_trust_size`, `[gnn] min_negative_fraction`). They must be configurable — different deployments will reach these thresholds at different rates.

---

## 7. Signal Priority During Initial Deployment

**Typical timeline** for a new deployment:
- Days 1-7: Zero explicit votes, limited implicit signals. GNN dormant. Manual formula active.
- Week 2-3: First explicit votes accumulate. ImplicitOutcome signals from session closes.
- Reaching MIN_TRAIN_SIZE (50 samples): First training run. Shadow model evaluated.
- Ongoing: New sessions continuously feed the reservoir. Training runs every 24h or 25 new samples.

This matches the product vision gate: "50+ helpfulness votes OR 2-4 weeks of active daemon use."

With W1-5 (col-023) complete, behavioral signals (re-search, phase completion) accelerate this timeline significantly — a single productive feature delivery session may generate 20-50 implicit labels.
