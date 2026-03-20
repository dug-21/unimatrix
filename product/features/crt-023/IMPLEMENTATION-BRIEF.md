# crt-023 Implementation Brief: NLI + Cross-Encoder Re-ranking (W1-4)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-023/SCOPE.md |
| Scope Risk Assessment | product/features/crt-023/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-023/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-023/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/crt-023/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-023/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| NliProvider (`unimatrix-embed`) | pseudocode/nli-provider.md | test-plan/nli-provider.md |
| NliServiceHandle (`unimatrix-server/infra`) | pseudocode/nli-service-handle.md | test-plan/nli-service-handle.md |
| Config Extension (`infra/config.rs`) | pseudocode/config-extension.md | test-plan/config-extension.md |
| Search Re-ranking (`services/search.rs`) | pseudocode/search-reranking.md | test-plan/search-reranking.md |
| Post-Store NLI Detection (`services/nli_detection.rs`) | pseudocode/post-store-detection.md | test-plan/post-store-detection.md |
| Bootstrap Edge Promotion (`services/nli_detection.rs`) | pseudocode/bootstrap-promotion.md | test-plan/bootstrap-promotion.md |
| Auto-Quarantine Threshold Guard (`background.rs`) | pseudocode/auto-quarantine-threshold.md | test-plan/auto-quarantine-threshold.md |
| Eval Integration (`EvalServiceLayer`) | pseudocode/eval-integration.md | test-plan/eval-integration.md |
| Model Download CLI | pseudocode/model-download-cli.md | test-plan/model-download-cli.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add an ONNX NLI cross-encoder to Unimatrix that operates in two complementary modes: (1) search
re-ranking that re-scores HNSW candidates against the actual query using entailment probability,
replacing the `rerank_score` composite formula when NLI is active; and (2) post-store detection that
writes semantic `Contradicts`/`Supports` edges to `GRAPH_EDGES` with `source='nli'` after each
`context_store`, replacing the unreliable lexical `conflict_heuristic` for new edge creation. A
bootstrap promotion task upgrades any `bootstrap_only=1` edges from W1-1 on the first ready tick.
The feature is gated by human-reviewed eval harness results from the W1-3 harness (nan-007), and
NLI absence must never prevent server startup or impair any existing MCP tool.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| NLI session concurrency | Single `Mutex<Session>` (consistent with OnnxProvider pattern); rayon pool floor raised to 6 when `nli_enabled=true` | ADR-001 (#2700) | architecture/ADR-001-nli-session-concurrency.md |
| NLI sort key for search | NLI entailment score *replaces* `rerank_score` entirely when NLI is active; rollback via `nli_enabled=false` | ADR-002 (#2701) | architecture/ADR-002-nli-score-replacement.md |
| Model selection mechanism | Config string (`nli_model_name = "minilm2"` or `"deberta"`); single `nli_model_sha256` field per config file; `NliModel::from_config_name` validates at startup | ADR-003 (#2702) | architecture/ADR-003-model-config-and-hashing.md |
| Post-store embedding handoff | Move `Vec<f32>` after HNSW insert; zero-copy; early return if duplicate or embedding failed; no task spawned | ADR-004 (#2703) | architecture/ADR-004-post-store-embedding-handoff.md |
| Bootstrap promotion idempotency | `COUNTERS` table key `bootstrap_nli_promotion_done = 1`; marker set in same transaction as last edge batch; zero-row case completes and sets marker | ADR-005 (#2704) | architecture/ADR-005-bootstrap-promotion-idempotency.md |
| Eval CLI missing model | Skip profile with `SKIPPED` annotation; `EvalServiceLayer` waits up to 60s for NLI readiness; baseline always runs | ADR-006 (#2705) | architecture/ADR-006-eval-cli-missing-model.md |
| NLI auto-quarantine threshold | `nli_auto_quarantine_threshold` (default 0.85) required to be > `nli_contradiction_threshold`; NLI-only-penalized entries quarantine only when all NLI edge scores exceed this higher bar | ADR-007 (#2716) | architecture/ADR-007-nli-auto-quarantine-threshold.md |
| `max_contradicts_per_tick` semantic | Per-`context_store` call cap (not per background tick); config field name retained for compatibility; implementation comments must note per-call meaning | FR-22, AC-23 | specification/SPECIFICATION.md |
| Eval gate waiver condition | Waived only when `eval scenarios` returns zero rows AND no hand-authored scenarios exist; AC-01 (NLI inference unit test) must pass regardless of waiver | FR-29, AC-22 | specification/SPECIFICATION.md |
| `nli_post_store_k` vs `nli_top_k` | Separate config fields: `nli_top_k` (default 20) for search re-ranking; `nli_post_store_k` (default 10) for post-store neighbor detection (D-04) | FR-11, AC-19 | specification/SPECIFICATION.md |

---

## Files to Create

| File | Purpose |
|------|---------|
| `crates/unimatrix-embed/src/cross_encoder.rs` | `CrossEncoderProvider` trait, `NliScores` struct, `NliProvider` impl with `Mutex<Session>` + `Tokenizer`; per-side truncation enforced here |
| `crates/unimatrix-server/src/infra/nli_handle.rs` | `NliServiceHandle` state machine (Loading → Ready | Failed → Retrying); SHA-256 hash verification; mutex poison detection |
| `crates/unimatrix-server/src/services/nli_detection.rs` | `run_post_store_nli` (fire-and-forget) and `run_bootstrap_promotion` / `maybe_run_bootstrap_promotion` standalone async functions |

## Files to Modify

| File | Change |
|------|--------|
| `crates/unimatrix-embed/src/model.rs` | Add `NliModel` enum (`NliMiniLM2L6H768`, `NliDebertaV3Small`) with `from_config_name`, `model_id`, `onnx_repo_path`, `onnx_filename`, `cache_subdir` |
| `crates/unimatrix-embed/src/download.rs` | Add `ensure_nli_model` following existing `ensure_model` pattern via `hf-hub` |
| `crates/unimatrix-embed/src/lib.rs` | Export `cross_encoder` module and public types |
| `crates/unimatrix-server/src/infra/config.rs` | Add 10 NLI fields to `InferenceConfig`; extend `InferenceConfig::validate()` with range checks and cross-field invariant; pool floor override logic |
| `crates/unimatrix-server/src/infra/mod.rs` | Expose `nli_handle` module |
| `crates/unimatrix-server/src/error.rs` | Add `ServerError::NliNotReady` and `ServerError::NliFailed(String)` variants |
| `crates/unimatrix-server/src/services/search.rs` | Add `nli_handle: Arc<NliServiceHandle>` field; insert NLI re-ranking step into pipeline; retain `rerank_score` fallback path |
| `crates/unimatrix-server/src/services/store_ops.rs` | Add `nli_handle: Arc<NliServiceHandle>` field; fire-and-forget spawn after HNSW insert; embedding move hand-off; `nli_is_configured` guard |
| `crates/unimatrix-server/src/services/background.rs` | Call `maybe_run_bootstrap_promotion` on each tick (no-op after first successful run) |
| `crates/unimatrix-server/src/main.rs` (or startup module) | Construct `Arc<NliServiceHandle>`; wire into `AppState`/`ServiceLayer`; apply pool floor; call `start_loading()` |
| `crates/unimatrix-server/src/services/eval.rs` (or eval module) | Fill W1-4 stub in `EvalServiceLayer::from_profile()`; add `wait_for_nli_ready(60s)` logic; handle SKIPPED profile annotation |
| CLI model-download subcommand | Extend to support `--nli [--nli-model minilm2|deberta]`; compute and print SHA-256 hash of downloaded file |
| `crates/unimatrix-server/src/services/background_tick.rs` (auto-quarantine path) | Apply `nli_auto_quarantine_threshold` check for entries penalized exclusively by NLI-origin `Contradicts` edges; read `nli_contradiction` from edge `metadata` JSON |
| `crates/unimatrix-server/Cargo.toml` | Add `sha2` crate if not already present |

---

## Data Structures

```rust
// unimatrix-embed/src/cross_encoder.rs

pub struct NliScores {
    pub entailment: f32,    // P(premise entails hypothesis)
    pub neutral: f32,       // P(premise and hypothesis are unrelated)
    pub contradiction: f32, // P(premise contradicts hypothesis)
    // Invariant: entailment + neutral + contradiction ≈ 1.0 (within 1e-4)
}

pub struct NliProvider {
    session: Mutex<Session>,   // serializes ONNX inference
    tokenizer: Tokenizer,      // lock-free tokenization
    model_name: String,
}

// unimatrix-embed/src/model.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NliModel {
    NliMiniLM2L6H768,   // cross-encoder/nli-MiniLM2-L6-H768, ~85MB, Apache 2.0
    NliDebertaV3Small,  // cross-encoder/nli-deberta-v3-small, ~180MB (ONNX TBD)
}

// NliEdge write contract (fields populated in GRAPH_EDGES):
// source_id, target_id, relation_type ('Contradicts' | 'Supports'),
// weight (NLI score for the relation type),
// created_by = 'nli', source = 'nli', bootstrap_only = 0,
// metadata = '{"nli_entailment": <f32>, "nli_contradiction": <f32>}'

// unimatrix-server/src/infra/config.rs (additions to InferenceConfig):
// nli_enabled: bool                          default true
// nli_model_name: Option<String>             default None (resolves to NliMiniLM2L6H768)
// nli_model_path: Option<PathBuf>            default None (auto-resolved from cache)
// nli_model_sha256: Option<String>           default None
// nli_top_k: usize                           default 20, range [1, 100]
// nli_post_store_k: usize                    default 10, range [1, 100]
// nli_entailment_threshold: f32              default 0.6, range (0.0, 1.0)
// nli_contradiction_threshold: f32           default 0.6, range (0.0, 1.0)
// max_contradicts_per_tick: usize            default 10, range [1, 100] (per-call cap)
// nli_auto_quarantine_threshold: f32         default 0.85, range (0.0, 1.0),
//                                            must be > nli_contradiction_threshold
```

---

## Function Signatures

```rust
// unimatrix-embed/src/cross_encoder.rs

pub trait CrossEncoderProvider: Send + Sync {
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>;
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>;
    fn name(&self) -> &str;
}

impl NliProvider {
    pub fn new(model: NliModel, model_path: &Path) -> Result<Self>;
    // Enforces per-side truncation (512 tokens / ~2000 chars) inside score_batch
    // before Mutex<Session> is acquired. Full batch runs under one lock acquisition.
}

// unimatrix-embed/src/model.rs

impl NliModel {
    pub fn from_config_name(name: &str) -> Option<Self>;
    pub fn model_id(&self) -> &'static str;
    pub fn onnx_repo_path(&self) -> &'static str;
    pub fn onnx_filename(&self) -> &'static str;  // returns "model.onnx"
    pub fn cache_subdir(&self) -> &'static str;
}

// unimatrix-server/src/infra/nli_handle.rs

impl NliServiceHandle {
    pub fn new() -> Arc<Self>;
    pub fn start_loading(self: &Arc<Self>, config: NliConfig);
    pub async fn get_provider(&self) -> Result<Arc<NliProvider>, ServerError>;
    pub fn is_ready_or_loading(&self) -> bool;
    // Poison detection: try_lock() in get_provider(); transitions to Failed on PoisonError
}

// unimatrix-server/src/services/nli_detection.rs

pub async fn run_post_store_nli(
    embedding: Vec<f32>,
    new_entry_id: u64,
    new_entry_text: String,
    nli_handle: Arc<NliServiceHandle>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    rayon_pool: Arc<RayonPool>,
    nli_post_store_k: usize,
    nli_entailment_threshold: f32,
    nli_contradiction_threshold: f32,
    max_edges_per_call: usize,
);

pub async fn maybe_run_bootstrap_promotion(
    store: &Store,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
);

// EvalServiceLayer addition:
pub async fn wait_for_nli_ready(&self, timeout: Duration) -> Result<(), NliNotReadyError>;
```

---

## Search Pipeline (NLI Active vs Fallback)

NLI-active path:
```
embed
→ HNSW top-nli_top_k (expanded candidate pool)
→ quarantine filter
→ status filter / penalty (score = base_score * status_penalty, applied before NLI)
→ supersession injection
→ NLI batch score via rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT)
→ sort by nli_scores.entailment DESCENDING
→ truncate to top-K
→ co-access boost
→ floors
```

Fallback path (NLI not ready, disabled, or timeout):
```
embed → HNSW top-K → quarantine filter → status filter/penalty
→ supersession injection → sort by rerank_score (unchanged)
→ co-access boost → truncate → floors
```

Post-store NLI detection pipeline (fire-and-forget, no MCP timeout):
```
1. nli_handle.get_provider() → Err? return immediately
2. vector_index.search(embedding, nli_post_store_k)
3. store.get_batch(neighbor_ids)
4. rayon_pool.spawn(|| provider.score_batch(pairs))   [no timeout]
5. for each (neighbor, scores):
   - entailment > threshold → Supports edge via write_pool_server()
   - contradiction > threshold → Contradicts edge via write_pool_server()
   - total edges >= max_edges_per_call → break (log at debug)
   metadata JSON: {"nli_entailment": f32, "nli_contradiction": f32}
```

GRAPH_EDGES write SQL (NLI path):
```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by,
     source, bootstrap_only, metadata)
VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)
```

---

## Constraints

1. `ort = "=2.0.0-rc.9"` is pinned and must not change. Both `OnnxProvider` and `NliProvider` use this exact version.
2. NLI-confirmed edge writes use `write_pool_server()` directly. `AnalyticsWrite::GraphEdge` is prohibited for NLI writes (SR-02; already documented in `analytics.rs`).
3. **All CPU-bound NLI inference runs on the rayon pool (W1-2 contract).** Every `NliProvider::score_pair` / `score_batch` call in every path — search re-ranking, post-store fire-and-forget, AND bootstrap promotion — must be dispatched via `rayon_pool.spawn()` or `rayon_pool.spawn_with_timeout()`. No NLI inference on tokio threads or via `spawn_blocking`. Single shared rayon pool from crt-022. No new pool. Pool floor raised to 6 when `nli_enabled=true` at startup.
4. NLI absence must not prevent server startup. All MCP tools must function on cosine fallback.
5. `max_contradicts_per_tick` circuit breaker applies per `context_store` call (FR-22). Config name retained for compatibility.
6. Per-side input truncation (512 tokens / ~2000 chars) is a security requirement. Enforced inside `NliProvider`, not at call sites.
7. `NliModel` and `CrossEncoderProvider` live in `unimatrix-embed`. `NliServiceHandle` lives in `unimatrix-server/src/infra/`.
8. No schema migration. `GRAPH_EDGES` schema v13 from crt-021 used as-is. `metadata` column already exists.
9. Eval gate (AC-09) is blocking when query history is available.
10. Mutex poison recovery: `NliServiceHandle` must detect poisoned `Mutex<Session>` via `try_lock()` in `get_provider()` and transition to Failed + retry.
11. Bootstrap promotion is NLI-only; must not run on cosine fallback; deferred to next ready tick if NLI is loading.
12. Status penalty applied before NLI scoring (as multiplier on base_score, not on NliScores values). NliScores stored in metadata are raw scores.
13. `nli_auto_quarantine_threshold` must be validated > `nli_contradiction_threshold` at startup; violation aborts with structured error naming both fields.

---

## Dependencies

### Crate Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `ort` | `=2.0.0-rc.9` (pinned) | ONNX Runtime for NLI session (same as OnnxProvider) |
| `tokenizers` | existing | Tokenization for NLI cross-encoder input pairs |
| `hf-hub` | existing | Model download via `ensure_nli_model` |
| `sha2` | add if absent | SHA-256 hash verification for model file integrity |
| `serde_json` | existing | `metadata` JSON serialization for GRAPH_EDGES |
| `rayon` (via `Arc<RayonPool>`) | existing (crt-022) | CPU-bound NLI inference off tokio thread |

### Internal Component Dependencies

| Component | Dependency Type | Reason |
|-----------|----------------|--------|
| crt-021 GRAPH_EDGES (schema v13) | Schema dependency | NLI writes to `GRAPH_EDGES` as-is |
| crt-022 RayonPool | Runtime dependency | Shared `Arc<RayonPool>` for NLI inference |
| nan-007 eval harness | Gate dependency | `unimatrix eval run` gates the feature (AC-09) |
| `EmbedServiceHandle` | Design pattern | `NliServiceHandle` mirrors state machine exactly |
| `OnnxProvider` | Design pattern | `NliProvider` mirrors `Mutex<Session>` + `Tokenizer` |
| `InferenceConfig` | Config extension | 10 NLI fields added to existing section |
| `EvalServiceLayer` | Stub fill-in | W1-4 stub in `from_profile()` filled |
| `StoreService::insert` | Integration point | Fire-and-forget spawn after HNSW insert |
| `SearchService::search` | Integration point | NLI re-ranking step inserted |
| `COUNTERS` table | Idempotency | `bootstrap_nli_promotion_done` key |

---

## NOT in Scope

- GGUF integration (W2-4): separate pool, separate model, separate feature
- GNN training (W3-1): crt-023 produces NLI metadata; it does not train the GNN
- Automated CI gate for eval results: eval is human-reviewed (nan-007 Non-Goal)
- Multi-model NLI ensemble: one model, one session, one `Mutex<Session>`
- Full `scan_contradictions` upgrade: incremental post-store and bootstrap promotion only
- Exposing NLI scores via MCP tool response: internal signal only; no schema changes
- New `unimatrix-onnx` crate: deferred to before W3-1 per crt-022a consultation
- Removing `conflict_heuristic`: retained as graceful degradation fallback; not deleted
- GRAPH_EDGES schema changes: no migration; schema v13 used as-is
- Length-prefix injection defense beyond 512-token truncation
- Blended rerank formula: pure replacement per D-02; blending is a follow-on
- Symmetric bi-encoder-then-NLI contradiction scan over all active entries

---

## Alignment Status

Final alignment status: **PASS** (post-review, 2026-03-20).

**VARIANCE 1 — NLI-derived auto-quarantine threshold**: RESOLVED.

The vision guardian flagged that SCOPE.md Constraints-5 required a higher auto-quarantine
confidence threshold for NLI-derived edges than for manually-corrected entries. This was not
initially carried into any FR, AC, or ADR. Resolution: ADR-007 added (Unimatrix entry #2716);
`nli_auto_quarantine_threshold` config field added (default 0.85, validated >
`nli_contradiction_threshold`); FR-22b added to SPECIFICATION.md; AC-25 added.

**Scope additions (WARN, accepted)**: Three additions beyond SCOPE.md Goals-8:
- `nli_post_store_k` config field (D-04, decouples post-store neighbor count from search; justified)
- `nli_model_name: Option<String>` config field (D-03, enables config-string model swap; justified)
- `EvalServiceLayer::wait_for_nli_ready()` method (ADR-006 consequence; internal, not a new MCP tool)

All other checks: vision alignment PASS, milestone fit PASS, architecture consistency PASS,
risk completeness PASS.

**Non-negotiable tests** (feature must not ship without all six):
1. R-01: Concurrent load test at 3 simultaneous NLI searches — pool saturation
2. R-03: Stable sort under identical NLI scores — tie-breaking determinism
3. R-05: Hash mismatch → Failed + "security"+"hash mismatch" log + cosine fallback confirmed
4. R-09: Cap enforcement across both Supports and Contradicts edge types
5. R-10: Miscalibration cascade end-to-end: store → edges → tick → no auto-quarantine
6. R-13: Mutex poison detected at `get_provider()` boundary → Failed → retry
