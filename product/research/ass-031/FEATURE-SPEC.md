# ASS-031: Feature Vector Specification

---

## 1. Design Principles

1. **All dimensions normalized to [0.0, 1.0]** — required for stable gradient flow through the scorer
2. **Config-driven vocabulary sizes** — category count `k` comes from `config.toml`, not hardcoded
3. **Zero-safe** — every dimension has a well-defined value when source data is absent (cold session, new entry, no graph edges)
4. **DB-reconstructible (with one exception)** — all dimensions are derivable from persisted records for historical training samples; see the category_counts gap below

---

## 2. Entry Feature Vector

**Total dims**: `10 + k` where `k` = number of configured categories. With default categories (convention, pattern, decision, lesson-learned, gap, procedure, outcome = 7): **17 dims**.

### 2.1 Confidence and Quality (5 scalar dims)

| Dim | Name | Source | Normalization |
|-----|------|--------|---------------|
| 0 | `confidence` | `entries.confidence` (f64) | Already [0,1] — use directly |
| 1 | `helpful_ratio` | `entries.helpful_count`, `entries.unhelpful_count` | `helpful / (helpful + unhelpful)` if votes > 0, else 0.5 (prior) |
| 2 | `access_count_norm` | `entries.access_count` | `log1p(access_count) / log1p(500)`, clipped [0,1] |
| 3 | `correction_count_norm` | `entries.correction_count` | `log1p(correction_count) / log1p(10)`, clipped [0,1] |
| 4 | `days_since_access_norm` | `entries.last_accessed_at`, current time | `days_elapsed / 30.0`, clipped [0,1]. If never accessed: 1.0 (maximum staleness) |

### 2.2 Graph Features (4 scalar dims)

Source: precomputed from `GRAPH_EDGES` via the `TypedGraphState` rebuilt each tick. Stored in `GnnFeatureCache` (in-memory, rebuilt alongside graph on each tick).

| Dim | Name | Derivation | Normalization |
|-----|------|-----------|---------------|
| 5 | `support_degree_norm` | Count of `Supports` edges where entry is source or target | `count / 10.0`, clipped [0,1] |
| 6 | `contradict_degree_norm` | Count of `Contradicts` edges involving this entry | `count / 5.0`, clipped [0,1] (lower ceiling — contradictions are rare) |
| 7 | `coaccess_degree_norm` | Count of `CoAccess` edges involving this entry | `count / 50.0`, clipped [0,1] |
| 8 | `nli_edge_confidence` | Max `weight` across all NLI-sourced edges involving this entry | Direct float, 0.0 if no NLI edges |

### 2.3 Contextual Features (2 scalar dims, computed at query time)

| Dim | Name | Source | Normalization |
|-----|------|--------|---------------|
| 9 | `topic_match` | `entry.topic == session.feature_cycle` | 1.0 if match, 0.0 otherwise |
| 10 | `phase_match` | `FEATURE_ENTRIES.phase == session.current_phase` for this entry | 1.0 if match, 0.0 if mismatch, 0.0 if either is None |

Note: `topic_match` and `phase_match` are computed at query time, not cached. They depend on the current session state.

### 2.4 Category One-Hot (k dims)

| Dims | Name | Source | Encoding |
|------|------|--------|---------|
| 11..10+k | `category_onehot` | `entry.category` | One-hot over configured category allowlist, sorted alphabetically. Unknown category → all-zero vector. |

**Category vocabulary is loaded from config at startup and fixed for the model's lifetime.** If categories change, the model must be retrained from scratch (generation bump in ModelRegistry).

---

## 3. Session Context Vector

**Total dims**: `k + 1 + 5` where `k` = number of configured categories.
With k=7 (default): **13 dims**.

### 3.1 Phase Signal (1 or k+1 dims — see note)

| Dim | Name | Source | Encoding |
|-----|------|--------|---------|
| 0..k | `phase_onehot` | `session.current_phase`, phase vocabulary | One-hot over known phases + "other" bucket |

**Phase vocabulary problem**: WA-1 intentionally made phase strings opaque — no enforced vocabulary. The model cannot have a fixed one-hot over an open string set.

**Resolution**: The phase vocabulary is **learned from FEATURE_ENTRIES.phase during training** and **saved as part of the model checkpoint** (alongside weights). At training time, all distinct phase strings seen in FEATURE_ENTRIES are collected, sorted, and indexed. At inference time, `current_phase` is mapped to this learned index. Unknown phases (not seen during training) map to the "other" bucket.

The learned vocabulary is bounded at 16 phases (config: `[gnn] max_phase_vocab = 16`). If more than 16 distinct phases are observed, the least-frequent phases are collapsed into "other". In practice, deployment workflows use 5-8 phase names.

Phase one-hot dims: 17 (16 known + 1 other). If `current_phase` is None: all-zero vector.

Updated total: `k + 17 + 5` = **29 dims** with k=7.

### 3.2 Category Histogram (k dims)

| Dims | Name | Source | Normalization |
|------|------|--------|--------------|
| 17..16+k | `category_histogram` | `session.category_counts` | `count[cat] / total_stores`, each dim in [0,1], sum=1 |

**Cold start**: empty histogram (no stores yet) → all-zero vector. This is safe: the model sees a zero-histogram session the same as a brand-new session.

**Training data gap**: `category_counts` is currently **never persisted** (in-memory only, reset on reconnect). Historical sessions cannot have their histogram reconstructed from the DB. **W3-1 delivery must add a `session_category_snapshots` table** as a prerequisite — see `OPEN-QUESTIONS.md` OQ-01.

### 3.3 Scalar Session Signals (5 dims)

| Dim | Name | Source | Normalization |
|-----|------|--------|--------------|
| 17+k | `injection_count_norm` | `session.injection_history.len()` | `log1p(count) / log1p(20)`, clipped [0,1] |
| 18+k | `query_count_norm` | counted from `query_log` for session or new `query_count` field | `log1p(count) / log1p(30)`, clipped [0,1] |
| 19+k | `rework_event_count_norm` | `session.rework_events.len()` | `log1p(count) / log1p(5)`, clipped [0,1] |
| 20+k | `cycle_position` | `(now - session.started_at) / expected_cycle_duration_ms` | clipped [0,1]; if no expected duration: 0.0 |
| 21+k | `phase_count_norm` | number of distinct phases this session has passed through | `count / 8.0`, clipped [0,1] |

**`query_count` gap**: `SessionState` does not currently track query count as a field. It must be added (or counted from `query_log` at session context build time). `OPEN-QUESTIONS.md` OQ-02.

**`expected_cycle_duration_ms`**: Configurable. Default: 4h (14400000ms). Used to normalize `cycle_position` so the model learns time-in-phase patterns across sessions of different total lengths.

---

## 4. Query Signal (3 dims — appended at inference, not trained separately)

| Dim | Name | Source | Notes |
|-----|------|--------|-------|
| 0 | `query_similarity` | HNSW cosine similarity | From search pipeline; 0.0 for Mode 1/2 |
| 1 | `nli_score` | NLI entailment score for this (query, entry) pair | From NLI re-ranker; 0.0 for Mode 1/2 |
| 2 | `query_present` | Boolean: 1.0 if Mode 3, 0.0 if Mode 1/2 | Mode discriminator signal |

---

## 5. Complete Input Specification

```
RelevanceDigest {
    features: [f32; RELEVANCE_DIM]   // RELEVANCE_DIM = entry_dim + session_dim + 3
}

entry_dim    = 10 + k            (k = num categories)
session_dim  = k + 17 + 5       (k histogram + 17 phase_onehot + 5 scalars)
query_signal = 3

RELEVANCE_DIM = (10 + k) + (k + 17 + 5) + 3 = 2k + 35
```

With k=7 (default): `RELEVANCE_DIM = 49`
With k=8: `RELEVANCE_DIM = 51`

The model weights are invalidated if `k` or the phase vocabulary changes. The `ModelRegistry.schema_version` field (already in `ModelVersion`) tracks this: schema_version is incremented when the feature vector spec changes, forcing retraining from scratch.

---

## 6. GnnFeatureCache (Precomputed Graph Features)

Graph-derived features (dims 5-8 of the entry vector) are expensive to compute per-query from the raw `GRAPH_EDGES` table. They are precomputed after each TypedGraphState rebuild (already on the maintenance tick) and stored in an in-memory `HashMap<u64, GraphFeatures>`.

```rust
pub struct GraphFeatures {
    pub support_degree: u32,
    pub contradict_degree: u32,
    pub coaccess_degree: u32,
    pub nli_edge_confidence: f32,  // max NLI edge weight, 0.0 if none
}

pub type GnnFeatureCache = Arc<RwLock<HashMap<u64, GraphFeatures>>>;
```

Rebuilt on each tick alongside `TypedGraphState`. Missing entry (new since last tick) → `GraphFeatures::default()` (all zeros). This is safe: zero graph features means the model treats the entry as unconnected, which is approximately correct for brand-new entries.

---

## 7. Feature Construction — DB Reconstruction for Training

For building training samples from historical sessions, the entry feature vector is fully reconstructable from DB records at the time the training label was generated. The session context vector is **partially** reconstructable.

| Dim group | Reconstructible from DB? | Source |
|---|---|---|
| Entry confidence/quality (dims 0-4) | Yes | `entries` table, snapshot at label time |
| Graph features (dims 5-8) | Approximately (current graph, not historical) | `GRAPH_EDGES` — only current graph state is available |
| Contextual features (dims 9-10) | Yes | `feature_entries`, `sessions` |
| Category one-hot (dims 11+k) | Yes | `entries.category` |
| Phase one-hot (session) | Partially | `sessions.current_phase` at close, not at each query |
| Category histogram (session) | **No** — requires `session_category_snapshots` (OQ-01) | Currently in-memory only |
| Scalar session signals (session) | Partially | `injection_log`, `query_log` for counts; rework from `observations` |

**Graph features are approximated using current state, not historical state.** An entry's graph degree in a training sample from 6 months ago may differ from its current degree. This is acceptable: graph structure evolves slowly, and the model is retrained regularly. The approximation error is bounded.
