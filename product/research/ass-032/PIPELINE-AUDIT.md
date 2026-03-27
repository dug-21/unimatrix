# ASS-032: Current Pipeline Audit — End-to-End

**Generated**: 2026-03-25
**Scope**: Full audit of the Unimatrix search/scoring/feedback pipeline as of crt-026 (WA-2 phase histogram affinity)

---

## Executive Summary

The Unimatrix search and scoring pipeline implements a multi-stage ranking system combining dense semantic retrieval (HNSW), NLI cross-encoder re-ranking, and an 8-component fused scoring formula. All components are documented below with exact weights, formulas, and file locations.

---

## 1. Candidate Retrieval

**File**: `crates/unimatrix-vector/src/index.rs`

**Index Type**: `hnsw_rs` wrapper with `DistDot` distance metric (cosine similarity via dot product on L2-normalized vectors)

**Key Parameters**:
- `EF_SEARCH: usize = 32` (hardcoded in `search.rs:38`)
- `config.max_nb_connection`: max neighbors per node
- `config.max_elements`: max vectors in index
- `config.ef_construction`: construction parameter

**Search Methods**:
1. `search(&self, query: &[f32], top_k, ef_search)` — unfiltered
2. `search_filtered(&self, query: &[f32], top_k, ef_search, allowed_entry_ids: &[u64])` — ID allowlist

**Active-Only Filtering**: NOT applied at retrieval time. Status filtering applied post-retrieval via soft penalties or hard filter depending on `RetrievalMode`.

---

## 2. Fused Scoring Formula

**File**: `crates/unimatrix-server/src/services/search.rs`

### 2.1 FusedScoreInputs (lines 59–92)

Eight normalized [0.0, 1.0] input signals:

| Field | Description |
|-------|-------------|
| `similarity` | HNSW cosine [bi-encoder] |
| `nli_entailment` | NLI softmax entailment [cross-encoder] |
| `confidence` | Wilson score composite (EntryRecord.confidence) |
| `coac_norm` | Co-access boost / MAX_CO_ACCESS_BOOST |
| `util_norm` | Utility delta normalized [−PENALTY, +BOOST] → [0, 1] |
| `prov_norm` | Provenance boost / PROVENANCE_BOOST |
| `phase_histogram_norm` | p(entry.category) from session histogram (crt-026) |
| `phase_explicit_norm` | **Always 0.0** — W3-1 placeholder (ADR-003) |

### 2.2 FusionWeights defaults (ADR-003, ADR-004)

```
w_sim              = 0.25   # bi-encoder similarity
w_nli              = 0.35   # NLI entailment (dominant)
w_conf             = 0.15   # confidence tiebreaker
w_coac             = 0.10   # co-access affinity
w_util             = 0.05   # effectiveness classification
w_prov             = 0.05   # category provenance hint
w_phase_histogram  = 0.02   # session histogram affinity (crt-026)
w_phase_explicit   = 0.00   # W3-1 placeholder (crt-026)

Core sum = 0.95, Total = 0.97
```

### 2.3 compute_fused_score (lines 212–222)

```rust
pub fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64 {
    weights.w_sim * inputs.similarity
        + weights.w_nli * inputs.nli_entailment
        + weights.w_conf * inputs.confidence
        + weights.w_coac * inputs.coac_norm
        + weights.w_util * inputs.util_norm
        + weights.w_prov * inputs.prov_norm
        + weights.w_phase_histogram * inputs.phase_histogram_norm
        + weights.w_phase_explicit * inputs.phase_explicit_norm
}
```

**Status penalty applied multiplicatively at call site** (line 876–877):
```rust
let fused = compute_fused_score(&inputs, &effective_weights);
let final_score = fused * penalty;
```

### 2.4 NLI Absent Re-normalization (lines 146–193)

When NLI unavailable: set `w_nli = 0.0`, divide each of the five non-NLI core weights by their sum. Phase terms (`w_phase_histogram`, `w_phase_explicit`) are **not re-normalized** — they pass through unchanged.

---

## 3. Scoring Component Details

### 3.1 Similarity (w=0.25)
- Source: HNSW cosine on L2-normalized 384-d embeddings
- Range: [0, 1] directly from `SearchResult.similarity`

### 3.2 NLI Entailment (w=0.35, dominant)
**File**: `crates/unimatrix-embed/src/cross_encoder.rs`

- **Default model**: `cross-encoder/nli-MiniLM2-L6-H768` (FP32, ~313 MB)
- **Quantized option**: `NliMiniLM2L6H768Q8` (INT8, 50 MB, requires AVX-512)
- **Per-side char limit**: 2000 chars (adversarial OOM guard)
- **Max tokens**: 512 (truncated before ONNX)
- **Output**: `NliScores { contradiction, entailment, neutral }` (softmax, sum ≈ 1.0)
- **Ranking key**: `entailment` score (index 1 in label order)

### 3.3 Confidence Composite (w=0.15)
**File**: `crates/unimatrix-engine/src/confidence.rs`

Stored as f64 in `EntryRecord.confidence`. Six sub-components:

| Sub-component | Weight | Formula |
|---|---|---|
| base_score | 0.16 | Status + trust_source lookup table |
| usage_score | 0.16 | `ln(1 + access_count) / ln(1 + 50)`, clamped to [0,1] |
| freshness_score | 0.18 | `exp(-age_hours / 168.0)` (1-week half-life) |
| helpfulness_score | 0.12 | `(helpful + α₀) / (total + α₀ + β₀)`, α₀=β₀=3.0 |
| correction_score | 0.14 | Lookup: 0→0.5, 1–2→0.8, 3–5→0.6, 6+→0.3 |
| trust_score | 0.16 | human→1.0, system→0.7, agent→0.5, neural→0.40, auto→0.35 |

**Sum of weights = 0.92**, result clamped to [0, 1].

Helpfulness is **Bayesian Beta-Binomial** (crt-019, replaces Wilson score). Cold-start: 0.5 with no votes. Responds immediately to any vote.

### 3.4 Co-Access Affinity (w=0.10)
**File**: `crates/unimatrix-engine/src/coaccess.rs`

1. Top-3 results selected as anchors; deprecated excluded (crt-010)
2. Pairs generated: min(10, results) entries → max 45 pairs
3. Boost formula:
   ```
   raw = ln(1 + count) / ln(1 + 20)   [MAX_MEANINGFUL_CO_ACCESS=20]
   raw_capped = min(raw, 1.0) * 0.03   [MAX_CO_ACCESS_BOOST=0.03]
   coac_norm = raw_capped / 0.03
   ```
4. Staleness cutoff: 30 days (entries older than 30d excluded)

### 3.5 Utility Delta (w=0.05)
- Source: `EffectivenessCategory` enum (Effective/Neutral/Ineffective)
- Precomputed by background tick; raw delta = ±0.05
- Normalized: `(delta + 0.05) / 0.10` → maps [−0.05, 0.05] → [0.0, 1.0]

### 3.6 Provenance Boost (w=0.05)
- Binary signal: 1.0 if `entry.category` in `KnowledgeConfig.boosted_categories`, else 0.0
- Default boosted: `["lesson-learned"]`
- `PROVENANCE_BOOST = 0.02`

### 3.7 Phase Histogram Affinity (w=0.02, crt-026 WA-2)
- Source: `SessionState.category_counts` (in-memory, ephemeral)
- Formula: `p(entry.category) = count[entry.category] / sum(all_counts)`
- Cold-start (empty histogram): 0.0 for all entries (NFR-02 backward compat)

### 3.8 Phase Explicit (w=0.0, W3-1 placeholder)
- **Always 0.0** in crt-026
- Reserved for W3-1 GNN training phase output
- Must not be removed (named learnable dimension, ADR-003)

### 3.9 Status Penalty (multiplicative)
Two retrieval modes:
- **Strict** (Briefing/Injection): Hard filter — Active-only, superseded dropped
- **Flexible** (Search): Soft penalty — deprecated entries penalized via `graph_penalty()` (multi-hop TypedRelationGraph traversal, single-hop fallback for cycles)

---

## 4. Serving Modes

### Mode 1: context_search (MCP tool)
- `RetrievalMode::Flexible` (soft penalty)
- Configurable k (default 20)
- Full 10-step pipeline: embed → HNSW → quarantine filter → status penalty → supersession → co-access → NLI → fused score → sort → truncate

### Mode 2: context_briefing (MCP tool)
- `RetrievalMode::Strict` (Active-only hard filter)
- Hardcoded k=20 (`UNIMATRIX_BRIEFING_K` env var deprecated)
- Post-filter defensive Active-only check after search returns

### Mode 3: Proactive injection (UDS CompactPayload)
- Same pipeline as Mode 2 (Strict)
- Query derived from hook context via `derive_briefing_query()`

**No category diversity enforcement in any mode.** All modes are pure score-sorted top-k.

---

## 5. Feedback Channels

| Channel | What Updates | Trigger |
|---|---|---|
| Helpful vote | `helpful_count`, confidence recompute | `context_correct`, explicit feedback |
| Unhelpful vote | `unhelpful_count`, confidence recompute | `context_correct`, explicit feedback |
| Access count | `access_count`, usage_score | `context_get` observation |
| Co-access pairs | `CO_ACCESS` table (count, last_accessed_at) | After search/briefing returns |
| Category histogram | `SessionState.category_counts` (in-memory) | After non-duplicate `context_store` |
| Injection history | `SessionState.injection_history` (in-memory) | After search returns |
| MicroLoRA training | LoRA weight matrices | Training step from reservoir |

**InjectionRecord** structure (session.rs:98–104):
```rust
pub struct InjectionRecord {
    pub entry_id: u64,
    pub confidence: f64,
    pub timestamp: u64,
}
```
Ephemeral — not persisted to DB. Used for session-scoped signal generation on `SessionClose`.

---

## 6. Embedding Pipeline

**File**: `crates/unimatrix-embed/src/model.rs`

- **Default model**: `all-MiniLM-L6-v2` (sentence-transformers)
- **Output dimension**: 384 for all supported models
- **Format**: ONNX (loaded via `ort` crate)
- **Tokenizer**: `tokenizer.json` from HuggingFace repo

**Computation path** (search.rs:542–558):
1. Embed query via `rayon_pool.spawn_with_timeout()`
2. MicroLoRA adaptation: `adapt_service.adapt_embedding(&raw, None, None)`
3. L2-normalization: `unimatrix_embed::l2_normalized(&adapted)`

**At store time**: Document embeddings computed and stored in VECTOR_MAP.

### MicroLoRA (unimatrix-adapt)
- Purpose: Domain-specific low-rank projection of embeddings
- Rank: configurable (typically 16–32)
- Prototype soft pull: optional, if category/topic provided
- Training: via `execute_training_step()` from training reservoir
- Thread-safe: `RwLock<MicroLoRA>` + `RwLock<TrainingReservoir>`

---

## 7. Configuration Defaults

**File**: `crates/unimatrix-server/src/infra/config.rs`

### InferenceConfig (lines 233–366)
```
rayon_pool_size          = (ncpus/2).max(4).min(8)
nli_enabled              = false
nli_top_k                = 20
nli_entailment_threshold = 0.6
nli_contradiction_threshold = 0.6
max_contradicts_per_tick = 10
nli_auto_quarantine_threshold = 0.85
```

### KnowledgeConfig (lines 124–147)
```
categories = INITIAL_CATEGORIES  [software-dev defaults]
boosted_categories = ["lesson-learned"]
freshness_half_life_hours = None → 168.0 (1 week)
```

### ConfidenceParams
```
w_base=0.16, w_usage=0.16, w_fresh=0.18
w_help=0.12, w_corr=0.14, w_trust=0.16
freshness_half_life_hours=168.0
alpha0=3.0, beta0=3.0
```

---

## 8. Key Invariants

- **Confidence weights sum**: W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = **0.92**
- **Fusion core sum**: w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0 (default 0.95)
- **Phase terms additive**: outside the 1.0 core constraint
- **All FusedScoreInputs fields**: [0.0, 1.0] (enforced by normalization)
- **Final score**: [0.0, 1.0] × status_penalty ∈ [0.0, 1.0]

---

## 9. Anomalies / Placeholders

| Issue | Location | Notes |
|---|---|---|
| `w_phase_explicit` always 0.0 | search.rs:870–871 | W3-1 placeholder, ADR-003 guard. Do not remove. |
| Phase terms not re-normalized when NLI absent | search.rs:160–192 | Intentional — additive, outside core sum |
| No category diversity enforcement | index_briefing.rs | All modes are pure score-sorted top-k |
| `UNIMATRIX_BRIEFING_K` env var ignored | index_briefing.rs:126 | Deprecated, always 20 |
| Injection history not persisted to DB | session.rs:98–104 | Ephemeral only |

---

## 10. Component Summary Table

| Component | Weight | Source | Formula Summary |
|---|---|---|---|
| Similarity | 0.25 | HNSW cosine | direct |
| NLI Entailment | 0.35 | cross-encoder | softmax entailment |
| Confidence | 0.15 | stored composite | 6-component, weights sum 0.92 |
| Co-Access | 0.10 | CO_ACCESS table | log-transform, max 0.03 boost |
| Utility | 0.05 | effectiveness enum | ±0.05 → [0,1] |
| Provenance | 0.05 | boosted_categories config | binary: 1.0 or 0.0 |
| Phase Histogram | 0.02 | session category counts | p(category) in session |
| Phase Explicit | 0.00 | **placeholder** | always 0.0 |
| Status Penalty | ×1.0 | entry.status + graph | multiplicative |
