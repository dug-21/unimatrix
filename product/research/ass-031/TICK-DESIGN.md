# ASS-031: Tick Scheduling and Resource Envelope

---

## 1. Current Maintenance Tick (15-minute cycle, 2-minute timeout)

The maintenance tick runs the following in sequence:

1. Load maintenance snapshot (status service)
2. Confidence refresh (batch ~100 entries)
3. Graph compaction (HNSW rebuild + VECTOR_MAP rewrite, if due)
4. Co-access cleanup (staleness)
5. GRAPH_EDGES orphan compaction
6. TypedGraphState rebuild (in-memory graph, swap via write lock)
7. NLI bootstrap promotion (first tick)
8. Contradiction scan (every N ticks)
9. Session GC (stale session sweep)
10. Extraction tick (observation quality-gate, separate supervisor)

---

## 2. W3-1 Additions to the Tick

W3-1 adds three new tick operations. They are designed to fit within the existing 15-minute / 2-minute timeout budget without disrupting existing work.

### 2.1 Sample Ingestion (every tick, cheap)

After step 9 (Session GC), before extraction tick:

```
10. GNN sample ingestion
    - Query signal_queue for records not yet processed by GNN
      (new column: signal_queue.gnn_processed INTEGER DEFAULT 0)
    - For each unprocessed record (max 50 per tick to bound latency):
        a. Reconstruct partial session context from DB
        b. Load entry features from entries table + GnnFeatureCache
        c. Build RelevanceSample
        d. Add to in-memory TrainingReservoir
    - Mark processed records: UPDATE signal_queue SET gnn_processed = 1 WHERE ...
    - Log: ingested N samples, reservoir now M items
```

**Estimated latency**: 50 records × (1 DB read + feature construction + reservoir insert) ≈ 5-20ms. Negligible relative to the 2-minute tick budget.

**Why bounded at 50 per tick**: If the queue has 500 unprocessed records (e.g., first tick after a very active period), ingesting all of them would take 100-400ms and delay downstream tick work. The per-tick bound ensures steady-state operation. Backlog is cleared over multiple ticks.

### 2.2 GNN Training (gated, async, non-blocking)

After step 10 (GNN sample ingestion):

```
11. GNN training gate check
    - Evaluate training gate conditions (see TRAINING-DESIGN.md §5.2)
    - If gate passes AND no training run currently in flight:
        spawn_rayon(training_closure, pool = ml_inference_pool)
        training_in_flight.store(true, Ordering::Release)
    - If gate does not pass: log reason, skip
    - If already in flight: log "training in progress, skipping", skip
```

The training run executes on the rayon ML inference pool (the same pool used for NLI and embedding inference). It is **non-blocking**: `spawn()` (not `spawn_with_timeout()`) — training must complete even if it takes longer than the MCP handler timeout.

**Training run interacts with NLI inference**: Training runs on the rayon pool, which has 4-8 threads. NLI inference takes 50-200ms per call. To prevent training from starving NLI:
- Training is split into **mini-batches of 32 samples**
- Between mini-batches, the rayon pool is voluntarily yielded (rayon's cooperative scheduling handles this)
- Total training time for 256 samples × 5 epochs = 1280 forward+backward passes at ~5-10μs each ≈ 6-13ms

At these timescales, training and NLI inference can coexist on the same pool without noticeable contention.

**Training completion callback**:

```rust
// In training closure on rayon pool:
let result = run_training_epochs(&reservoir_snapshot, &current_model, &ewc_state);
match result {
    Ok(new_model) => {
        registry.save_shadow(new_model);
        shadow_ready_flag.store(true, Ordering::Release);
    }
    Err(e) => {
        tracing::warn!("GNN training run failed: {e}");
    }
}
training_in_flight.store(false, Ordering::Release);
```

### 2.3 Model Promotion and Cache Rebuild (every tick, after training)

After step 11:

```
12. GNN model promotion check
    - If shadow_ready_flag is set:
        a. Load shadow model from disk
        b. Evaluate on held-out samples (20% of reservoir not used in last training)
        c. If shadow_loss < production_loss * 0.95: promote shadow → production
           Swap model handle (Arc<RwLock<Option<RelevanceScorer>>>) — write lock ~1ms
        d. Rebuild GnnFeatureCache:
           For each Active entry: run forward pass over entry features only (no session ctx)
           Wait — this is not how a concat MLP works. See note below.
        e. Clear shadow_ready_flag
        f. Update blend_alpha based on reservoir.len()
```

**Note on score caching for concat MLP**: The RelevanceScorer's output depends on BOTH entry features AND session context. A score cache is per-(entry, session) pair, not per-entry. You cannot precompute scores for all entries independently.

What CAN be precomputed: **entry feature vectors** (the `entry_features` portion of the input). These do not depend on session state. Store `HashMap<u64, RelevanceEntryFeatures>` computed from the entries table + GnnFeatureCache.

At query time:
- `entry_features` — from precomputed cache (fast lookup)
- `session_context` — built on-demand from session state (cheap, ~0.1ms for 29 scalars)
- Forward pass — 46-input MLP, ~2μs per entry

For Mode 1/2 (phase-transition cache, ~100 candidates): 100 × 2μs = 0.2ms. Negligible.
For Mode 3 (HNSW top-20): 20 × 2μs = 0.04ms. Negligible.

The entry feature cache (`EntryFeatureCache: Arc<RwLock<HashMap<u64, [f32; ENTRY_DIM]>>>`) is rebuilt:
- After each model promotion (model schema changed)
- After each TypedGraphState rebuild (graph features changed, same tick step 6)
- Incrementally on new entry store (add single entry to cache, cheap)

---

## 3. Revised Full Tick Sequence

```
Maintenance tick (15-min cycle, 2-min timeout):
  1.  Load maintenance snapshot
  2.  Confidence refresh
  3.  Graph compaction (if due)
  4.  Co-access cleanup
  5.  GRAPH_EDGES orphan compaction
  6.  TypedGraphState rebuild (in-memory swap)
      → Trigger: EntryFeatureCache partial rebuild (graph dims 5-8 changed)
  7.  NLI bootstrap promotion
  8.  Contradiction scan (every N ticks)
  9.  Session GC
  10. GNN sample ingestion (new, max 50/tick, ~10ms)
  11. GNN training gate check + conditional async spawn (new, ~1ms gate + async)
  12. GNN model promotion check + shadow evaluation + cache rebuild (new, ~5ms if no promotion)
  13. Extraction tick (existing, separate supervisor)
```

**Estimated new tick overhead**: ~15-30ms in the common case (no training, no promotion). Training is fully async (no tick blocking). Promotion + mini-eval: ~50ms if triggered.

---

## 4. Resource Envelope

### 4.1 Rayon Pool Thread Budget

The ML inference pool has `(num_cpus / 2).max(4).min(8)` threads.

| Operation | Threads used | Frequency | Duration |
|---|---|---|---|
| ONNX embedding (NLI, embedding) | 1-2 (ONNX internal) | Per MCP call | 50-200ms |
| ONNX NLI inference | 1-2 | Post-store, async | 100-300ms |
| GNN RelevanceScorer inference (Mode 1/2) | 1 | Per phase transition | ~0.2ms |
| GNN RelevanceScorer inference (Mode 3) | 1 | Per search | ~0.04ms |
| GNN training run | 1-2 (mini-batch loop) | Every 24h / 25 samples | ~10-20ms total |
| ContradictionScan | 1 | Every N ticks | Varies |

The GNN's inference and training overhead is trivial relative to ONNX inference. No pool sizing changes needed.

### 4.2 Memory Budget

| Component | Size | Lifetime |
|---|---|---|
| `TrainingReservoir<RelevanceSample>` (capacity=1000) | 1000 × ~500 bytes = ~500KB | Process lifetime |
| `EntryFeatureCache` (10K entries × 17 dims × 4 bytes) | ~680KB | Rebuilt per tick |
| `RelevanceScorer` model weights | ~20KB | Loaded at startup |
| Model registry on disk (3 slots × ~20KB) | ~60KB | On-disk only |
| `GnnState` (reservoir + model handle + metadata) | ~510KB total | Process lifetime |

Total new memory: ~1.2MB. Well within available server memory.

### 4.3 Disk Budget

| File | Size | Notes |
|---|---|---|
| `gnn_shadow.bin` | ~20KB | Shadow model, bincode |
| `gnn_production.bin` | ~20KB | Production model |
| `gnn_previous.bin` | ~20KB | Rollback model |
| `gnn_registry.json` | ~1KB | ModelSlots metadata |
| `session_category_snapshots` table | ~50 bytes/session | Retained N days |

Storage is negligible. The `session_category_snapshots` retention window (default: 30 days, config: `[gnn] snapshot_retention_days`) bounds this table's growth.

---

## 5. Training Frequency Rationale

**Why 24h or 25 new samples, not every tick?**

Training on every tick would:
- Produce models based on tiny batches (often < 5 new samples per tick)
- Oscillate weights due to high gradient variance on small batches
- Waste rayon pool threads every 15 minutes

Training every 24h or when 25 new samples arrive:
- Batches are large enough for stable gradient estimates (min 50, typically 100+)
- Models are promoted at most once per day in steady state
- In initial deployment (high activity, many new sessions), 25-sample threshold triggers training sooner than 24h

**No online (per-sample) learning**: Despite EWC++ being available, per-sample online learning produces unstable models in early deployment. Batch training with EWC++ for regularization is the right pattern. EWC++ is used to prevent forgetting across training runs, not to enable online updates.

---

## 6. Interaction with Existing Tick Work

The new GNN tick steps do not interact with or depend on any existing tick step except:

- **Step 6 (TypedGraphState rebuild)**: Triggers partial EntryFeatureCache rebuild (graph dims only). This is an internal signal via shared state, not a direct dependency.
- **Step 3 (Graph compaction)**: HNSW rebuild may swap out entry embeddings. Graph compaction happens before TypedGraphState rebuild (step 6), so EntryFeatureCache is rebuilt after compaction completes. No ordering issue.

All other tick steps are independent of GNN state.

The extraction tick (step 13) runs in a separate supervisor and does not interact with GNN state.
