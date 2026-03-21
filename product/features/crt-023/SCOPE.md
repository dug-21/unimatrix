# crt-023: NLI + Cross-Encoder Re-ranking (W1-4)

## Problem Statement

Unimatrix's current search pipeline returns the most topically similar entries (bi-encoder cosine
similarity via HNSW), not necessarily the entries that best answer the actual query. Cross-encoders
measure whether an entry *answers* the query rather than merely sharing vocabulary with it. The
combination of bi-encoder retrieval (fast, approximate) and cross-encoder re-ranking (slower, precise)
is the standard pattern for high-quality RAG systems.

Contradiction detection currently runs a multi-signal lexical heuristic (`conflict_heuristic` in
`infra/contradiction.rs`): negation opposition (0.6 weight), incompatible directives (0.3), and opposing
sentiment (0.1). These signals produce false positives on paraphrase and false negatives on synonymous
opposites. Every `Contradicts` edge bootstrapped from `shadow_evaluations` carries `bootstrap_only=1`
and is excluded from scoring exactly because the heuristic is known to be unreliable. NLI provides
semantic grounding — {entailment, neutral, contradiction} softmax probabilities from a model trained
on SNLI/MultiNLI — replacing the heuristic for new edge creation and confirming or refuting the
existing bootstrap edges.

Together, these two use cases (post-store NLI detection and search re-ranking) are the highest-leverage
search quality improvements available without a full local LLM. W3-1 (GNN training) directly depends on
NLI confidence scores as edge quality features; without them, GNN training labels are degraded.

**Who is affected**: Every MCP session that runs `context_search` (re-ranking improves recall for
natural-language-dense knowledge bases). Contradiction quality affects every search that involves
graph-penalized entries. W3-1 is blocked on NLI edge quality.

**Why now**: W1-1 shipped `GRAPH_EDGES` with `bootstrap_only=1` Contradicts edges awaiting NLI
confirmation. W1-2 established the rayon pool that NLI runs on. W1-3 delivered the eval harness
that gates this feature. All three prerequisites are satisfied.

---

## Goals

1. Add `NliProvider` to `unimatrix-embed` — an ONNX cross-encoder that takes `(query, passage)` pairs
   and returns softmax `{entailment, neutral, contradiction}` probabilities. Uses the same `ort =
   "=2.0.0-rc.9"` pinned version and `Mutex<Session>` pattern as `OnnxProvider`.
2. Add an `NliServiceHandle` to `unimatrix-server` — lazy-loading state machine
   (Loading → Ready | Failed → Retrying) mirroring `EmbedServiceHandle`. NLI model absent or
   hash-invalid transitions to `Failed`; server continues on cosine fallback.
3. Implement search re-ranking: after HNSW retrieves top-N candidates (`nli_top_k`, default 20),
   NLI re-scores each (query, candidate) pair; results are sorted by NLI entailment score before
   truncation to top-K. Re-ranking runs on the rayon pool via `rayon_pool.spawn_with_timeout`.
4. Implement post-store NLI detection: after a successful `context_store`, fire-and-forget a rayon task
   that embeds the new entry, fetches top-K HNSW neighbors, runs NLI on each (new, neighbor) pair,
   and writes `Contradicts` / `Supports` edges to `GRAPH_EDGES` via direct `write_pool` with
   `source='nli'`, `bootstrap_only=0`. NLI confidence stored in `metadata` column for W3-1.
5. Implement bootstrap edge promotion: on the first background tick after startup, promote any
   `bootstrap_only=1` Contradicts edges by re-scoring each (source, target) pair through NLI.
   Confirmed (NLI contradiction > threshold) → DELETE + INSERT with `source='nli'`, `bootstrap_only=0`.
   Refuted → DELETE only. W1-1 shipped zero such rows; this path is implemented as a future-proof
   background task that runs once.
6. Add circuit breaker on NLI → auto-quarantine feedback loop: cap `Contradicts` edges created per
   tick at `max_contradicts_per_tick` (config-driven, default 10). NLI-derived auto-quarantine
   requires a higher confidence threshold than the existing manual-correction path.
7. Add SHA-256 hash pinning for the NLI model: `nli_model_sha256` in `[inference]` config. Hash
   mismatch transitions `NliServiceHandle` to `Failed` with a logged security warning; server
   continues on cosine fallback.
8. Extend `[inference]` config section with NLI-specific parameters: `nli_model_path` (optional,
   overrides auto-download), `nli_model_sha256` (required for production), `nli_top_k` (default 20),
   `nli_entailment_threshold` (default 0.6), `nli_contradiction_threshold` (default 0.6),
   `max_contradicts_per_tick` (default 10), `nli_enabled` (default true, graceful degradation toggle).
9. Run the W1-3 eval harness as an explicit gate condition: produce `unimatrix eval run` results
   comparing baseline (cosine only) vs candidate (NLI re-ranking) profiles on a representative
   scenario set. Gate condition: measurable improvement in P@K or MRR, OR documented equivalence
   with zero regression, on the specific knowledge base under test.
10. Add `NliModel` enum variant to `unimatrix-embed/src/model.rs` for the selected model
    (`cross-encoder/nli-MiniLM2-L6-H768`), consistent with the existing `EmbeddingModel` catalog
    pattern.

---

## Non-Goals

- **GGUF integration (W2-4)**: separate rayon pool, separate model, separate feature.
- **GNN training (W3-1)**: crt-023 produces the NLI confidence scores in `metadata` that W3-1 needs;
  it does not train the GNN.
- **Automated CI gate for eval results**: the eval report is a human-reviewed artifact (nan-007
  Non-Goals). crt-023 requires the human to run the harness and review the output, not an automated
  pipeline check.
- **Multi-model NLI ensemble**: one model, one session, one `Mutex<Session>` per NliProvider. No
  ensemble scoring.
- **Symmetric bi-encoder-then-NLI for contradiction scan**: the full `scan_contradictions` background
  task (which iterates all active entries) is not upgraded in this feature. Post-store detection covers
  the incremental case. Full scan upgrade is a follow-on concern.
- **Exposing NLI scores via MCP tool**: NLI output is internal signal only. No new tool, no changes to
  `context_search` response schema visible to callers (ordering changes but field names do not).
- **`nli-deberta-v3-small` (~180MB) as primary model**: deberta is the higher-quality alternative noted
  in the product vision. Selection is deferred to eval harness results. If deberta's ONNX export is
  available and eval shows it outperforms MiniLM2 within the latency budget, the architect may select
  it. The scope assumes MiniLM2 as primary with deberta as an eval-validated alternative.
- **Length-prefix injection or prompt injection defense beyond truncation**: crt-023 applies 512-token
  / ~2000-char truncation per side before inference. A fuller content-scanner pass is out of scope.
- **New `unimatrix-onnx` crate**: deferred to before W3-1 per crt-022a architect consultation.
- **Removing `conflict_heuristic`**: the cosine heuristic remains as the graceful degradation fallback
  path when NLI is absent or failed.
- **Changing the `GRAPH_EDGES` schema**: crt-023 writes to the existing schema established in crt-021.
  No schema migration is needed.

---

## Background Research

### W1-1 (crt-021) — GRAPH_EDGES Foundation

`GRAPH_EDGES` table is live with schema version 13. Key fields:

```
id, source_id, target_id, relation_type TEXT, weight REAL, created_at, created_by,
source TEXT (e.g. 'bootstrap', 'nli'), bootstrap_only INTEGER, metadata TEXT DEFAULT NULL
```

`UNIQUE(source_id, target_id, relation_type)` constraint with `INSERT OR IGNORE` for idempotency.
`bootstrap_only=1` Contradicts edges exist from `shadow_evaluations` bootstrap (zero rows on current
production databases that went through the migration, since the shadow_evaluations → entry_id mapping
was unresolved in W1-1 — AC-08 was flagged as an open question). The promotion path must handle both
zero-row and non-zero-row cases gracefully.

`AnalyticsWrite::GraphEdge` variant exists in `analytics.rs` but carries a **shedding policy comment**:
bootstrap-origin writes only via analytics queue. W1-4 NLI-confirmed edge writes MUST use direct
`write_pool` path per SR-02. This constraint is already documented in the `AnalyticsWrite::GraphEdge`
doc comment.

The `TypedRelationGraph` is rebuilt from `GRAPH_EDGES` each background tick, under `Arc<RwLock<_>>`.
NLI-written edges are visible to the next tick's graph rebuild — no hot-path read concern.

### W1-2 (crt-022) — Rayon Pool

`RayonPool` is live in `unimatrix-server/src/infra/rayon_pool.rs`:
- `spawn(f)` — no timeout, for background tasks
- `spawn_with_timeout(timeout, f)` — for MCP handler paths (preserves `MCP_HANDLER_TIMEOUT` coverage)
- `pool_size()` accessor
- Panic containment via oneshot channel drop → `RayonError::Cancelled`

Pool is `Arc<RayonPool>` on `AppState`/`ServiceLayer` and on all service structs that do inference
(`SearchService`, `StoreService`, etc.). NLI inference in both the search re-ranking path (MCP) and
post-store detection (fire-and-forget) uses this shared pool.

**Pool sizing interaction**: the existing pool already serves embedding inference for all MCP paths
and the contradiction scan background task. Adding NLI inference increases the load on the same pool.
The circuit breaker (`max_contradicts_per_tick`) limits post-store batch size. The architect should
consider whether the default pool size (4–8 threads) remains adequate or whether a floor increase is
needed.

### W1-3 (nan-007) — Eval Harness

The eval harness is fully shipped. Available commands:

- `unimatrix snapshot --out <path>` — creates read-only SQLite copy
- `unimatrix eval scenarios --db <snapshot>` — mines query_log → JSONL scenarios
- `unimatrix eval run --db <snapshot> --scenarios <file> --configs baseline.toml,candidate.toml` — per-profile replay with P@K, MRR, Kendall tau, latency
- `unimatrix eval report --results <dir>` — Markdown report with zero-regression check

Profile TOML format is `UnimatrixConfig` subset with required `[profile] name`. The `[inference]`
section stub is already validated in `EvalServiceLayer::from_profile()` with a comment:
"InferenceConfig is stub-only for nan-007; when W1-4 adds nli_model, validation goes here."

The **candidate profile** for crt-023 eval would set `[inference] nli_enabled = true` plus the NLI
model path. The **baseline profile** is an empty TOML (compiled defaults, NLI disabled or absent).

Gate condition semantics: the eval report's zero-regression check lists scenarios where candidate MRR
or P@K regresses below baseline. Gate PASSES when: (a) aggregate MRR or P@K for NLI candidate is ≥
baseline across all scenarios, OR (b) per-scenario regressions are documented and judged acceptable by
the human reviewer. The harness produces the evidence; the human makes the shipping decision.

### Existing ONNX Provider Pattern

`OnnxProvider` in `unimatrix-embed/src/onnx.rs` is the canonical pattern:
- `Mutex<Session>` for ONNX inference serialization (ADR-001, entry #67)
- `Tokenizer` outside the mutex (lock-free `&self` methods)
- `EmbeddingProvider` trait: `embed(&self, text) -> Result<Vec<f32>>`

NLI cross-encoders differ: they take *pairs* (query, passage), not single texts. They output a 3-element
logit/softmax vector `[entailment, neutral, contradiction]`, not an embedding. A new trait
`CrossEncoderProvider` is needed:

```rust
pub trait CrossEncoderProvider: Send + Sync {
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>;
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>;
    fn name(&self) -> &str;
}

pub struct NliScores {
    pub entailment: f32,
    pub neutral: f32,
    pub contradiction: f32,
}
```

The `NliProvider` struct mirrors `OnnxProvider` with `Mutex<Session>` + `Tokenizer`. The ONNX model
output shape for cross-encoders is `[batch_size, 3]` rather than `[batch_size, hidden_dim]` — pooling
and normalization are replaced by softmax application.

### Current Contradiction Detection

`infra/contradiction.rs` implements `scan_contradictions` and `check_entry_contradiction` as synchronous
functions (called from rayon closures in `background.rs`). The conflict heuristic is pure lexical
analysis — no embedding, no NLI. NLI replaces this heuristic *for new edge creation* at post-store
time and for bootstrap edge promotion. The background scan (`scan_contradictions`) continues to use the
cosine heuristic and produces `ContradictionPair` structs for the `shadow_evaluations` table; it is not
removed or replaced.

### Post-Store Integration Point

`StoreService::insert` in `services/store_ops.rs` is the entry point for `context_store`. After a
successful insert, a fire-and-forget NLI task must be spawned. The pattern is: `tokio::spawn(async
move { ... })` wrapping a `rayon_pool.spawn(...)` call for the CPU-bound NLI work. The spawned task:

1. Gets top-K HNSW neighbors for the new entry's embedding (already computed during insert)
2. Runs NLI on each (new_entry_text, neighbor_text) pair via `rayon_pool.spawn`
3. For pairs above threshold, writes `Contradicts` or `Supports` edge to `GRAPH_EDGES` via
   `store.write_pool_server()` directly (not analytics queue — SR-02)
4. Stores NLI confidence in `metadata` JSON

The new entry's embedding is already available from the insert path — avoid recomputing it.

### SearchService Re-ranking Integration Point

`SearchService::search` in `services/search.rs` currently runs the pipeline:

```
embed → HNSW top-N → quarantine filter → status filter/penalty → supersession injection
→ re-rank (rerank_score) → co-access boost → truncate → floors
```

With NLI re-ranking, the pipeline becomes:

```
embed → HNSW top-nli_top_k → quarantine filter → status filter/penalty → supersession injection
→ NLI re-score (query, each_candidate) → sort by NLI score → truncate to top-K → co-access boost → floors
```

The NLI re-ranking step replaces the final `rerank_score` sorting for the MCP path when NLI is
available. `NliServiceHandle::get_provider()` returns `Err(NliNotReady)` when the model is loading or
absent — the search service falls back to the existing `rerank_score` path cleanly.

`SearchService` already holds `rayon_pool: Arc<RayonPool>`. NLI inference runs via
`rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`. The latency overhead of re-ranking 20
pairs at 50–200ms each on a rayon thread adds bounded, predictable overhead; the eval harness gate
condition quantifies this for the specific knowledge base.

### EvalServiceLayer stub

`EvalServiceLayer::from_profile()` already has the comment:
```rust
// InferenceConfig is stub-only for nan-007 (no nli_model field yet).
// When W1-4 adds nli_model, validation goes here.
```
crt-023 fills in this stub: parse `nli_model_path` from `EvalProfile`'s config overrides, construct
`NliServiceHandle`, wire it into `EvalServiceLayer::search_service`. This enables eval comparison
between NLI-enabled and NLI-disabled profiles.

### Model Selection

Primary: `cross-encoder/nli-MiniLM2-L6-H768` (~85MB, Apache 2.0, ONNX export confirmed available,
50–200ms per pair on CPU). Alternative: `cross-encoder/nli-deberta-v3-small` (~180MB) — ONNX export
availability must be verified at implementation time. Model selection is validated through W1-3 eval
harness. The `NliModel` enum should include both variants; the architect selects based on eval results.

### Security Constraints from Product Vision

- **[Critical]** SHA-256 hash pinning for NLI model file — a replaced model file is an undetectable
  model-poisoning attack vector. `nli_model_sha256` in config; mismatch → `NliServiceHandle::Failed`.
- **[High]** Per-side length truncation: max 512 tokens / ~2000 chars per query and passage before NLI
  inference, to prevent adversarial inputs that cause OOM or extreme latency in the ONNX session.
- **[High]** NLI inference panics must not propagate to the MCP handler thread. The rayon oneshot
  channel drop handles this (RayonError::Cancelled).
- **[Medium]** All thresholds and limits (`nli_top_k`, `max_contradicts_per_tick`,
  `nli_entailment_threshold`, `nli_contradiction_threshold`) must be config-driven, not hardcoded.

---

## Proposed Approach

### Phase 1: NLI Provider Infrastructure (`unimatrix-embed`)

Add `NliModel` enum variant(s) to `model.rs`. Implement `CrossEncoderProvider` trait and `NliScores`
struct in `provider.rs` (or a new `cross_encoder.rs`). Implement `NliProvider` mirroring `OnnxProvider`
with `Mutex<Session>` + `Tokenizer`, producing softmax scores rather than embeddings. Add to model
download path (`download.rs`) via `hf-hub` following the existing `ensure_model` pattern.

### Phase 2: NliServiceHandle and Config Extension (`unimatrix-server`)

Add NLI config fields to `InferenceConfig` in `infra/config.rs`:
`nli_enabled`, `nli_model_path`, `nli_model_sha256`, `nli_top_k`, `nli_entailment_threshold`,
`nli_contradiction_threshold`, `max_contradicts_per_tick`. All with `#[serde(default)]`.

Add `NliServiceHandle` in `infra/nli_handle.rs` — state machine (Loading → Ready | Failed → Retrying)
mirroring `EmbedServiceHandle`. SHA-256 hash verification at load time. `get_provider()` returns
`Err(NliNotReady)` / `Err(NliFailed)` for graceful degradation.

Add `Arc<NliServiceHandle>` to `AppState`/`ServiceLayer` and wire through server startup.

### Phase 3: Search Re-ranking

Extend `SearchService` to hold `Arc<NliServiceHandle>`. In the `search()` pipeline, after the existing
status/penalty step, if NLI is ready: batch-score `(query, candidate_text)` pairs via
`rayon_pool.spawn_with_timeout`, sort by entailment score, truncate. If NLI is not ready: fall back
to existing `rerank_score` path unchanged.

### Phase 4: Post-Store NLI Detection

Extend `StoreService::insert` to fire-and-forget a tokio task that: (a) fetches HNSW neighbors using
the already-computed embedding, (b) batch-scores NLI pairs on the rayon pool, (c) writes
`Contradicts`/`Supports` edges via `write_pool_server()` directly, respecting `max_contradicts_per_tick`
as a per-call cap.

### Phase 5: Bootstrap Edge Promotion

Add a one-shot background task (runs once on first tick after startup) that queries all
`bootstrap_only=1` Contradicts edges, scores each `(source_entry, target_entry)` pair through NLI, and
promotes or deletes them. Idempotent: completed state tracked via a counter or flag; no re-run after
first tick.

### Phase 6: Eval Gate (W1-3 Harness)

The feature cannot be marked deliverable until:
1. `unimatrix snapshot` produces a snapshot from a knowledge base with real query history.
2. `unimatrix eval scenarios` mines scenarios from the snapshot.
3. `unimatrix eval run` is executed with two profiles: `baseline.toml` (NLI disabled) and
   `candidate.toml` (NLI enabled, pointing at the downloaded model).
4. `unimatrix eval report` produces a report showing aggregate P@K and MRR comparison.
5. Human reviews and approves the gate condition (AC-09).

---

## Acceptance Criteria

- AC-01: `NliProvider` in `unimatrix-embed` implements `CrossEncoderProvider` (trait with
  `score_pair`, `score_batch`, `name`). Given a (query, passage) pair, it returns `NliScores`
  with `entailment + neutral + contradiction ≈ 1.0` (sum within 1e-4).
- AC-02: `NliProvider` uses `Mutex<Session>` for ONNX inference and `Tokenizer` outside the mutex,
  consistent with ADR-001 (nxs-003 entry #67). `NliProvider` is `Send + Sync`.
- AC-03: NLI input truncation: each of query and passage is truncated to max 512 tokens (or ~2000
  characters, whichever fires first) before tokenization. Inputs exceeding this limit do not cause
  panics or OOM; truncated text is silently accepted.
- AC-04: `NliModel` enum in `unimatrix-embed/src/model.rs` includes at least
  `NliMiniLM2L6H768` (model_id `cross-encoder/nli-MiniLM2-L6-H768`). The `NliModel::onnx_repo_path()`,
  `onnx_filename()`, and `cache_subdir()` methods follow the same conventions as `EmbeddingModel`.
- AC-05: `NliServiceHandle` in `unimatrix-server/src/infra/nli_handle.rs` implements the
  Loading → Ready | Failed → Retrying state machine. `get_provider()` returns
  `Err(ServerError::NliNotReady)` when loading and `Err(ServerError::NliFailed)` when retries are
  exhausted. NLI model loading failure does not prevent server startup.
- AC-06: SHA-256 hash verification: when `nli_model_sha256` is present in config, the loaded model
  file is verified against the hash before the ONNX session is constructed. Hash mismatch transitions
  `NliServiceHandle` to `Failed`, logs a `tracing::error!` with the word "security" or "hash mismatch",
  and falls back to cosine similarity. Server continues operating.
- AC-07: `[inference]` config section in `UnimatrixConfig` is extended with: `nli_enabled: bool`
  (default true), `nli_model_path: Option<PathBuf>`, `nli_model_sha256: Option<String>`,
  `nli_top_k: usize` (default 20, valid range 1–100), `nli_entailment_threshold: f32` (default 0.6,
  range (0.0, 1.0)), `nli_contradiction_threshold: f32` (default 0.6, range (0.0, 1.0)),
  `max_contradicts_per_tick: usize` (default 10, valid range 1–100). All fields use `#[serde(default)]`.
- AC-08: Search re-ranking: when NLI is ready and `nli_enabled = true`, `SearchService::search`
  expands the HNSW candidate pool to `nli_top_k` (default 20), scores each `(query, candidate)` pair
  via NLI on the rayon pool using `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`, sorts by entailment
  score descending, and returns the top-K results. When NLI is not ready or `nli_enabled = false`,
  the existing `rerank_score` path is used unchanged.
- AC-09: Eval gate: `unimatrix eval run` results comparing baseline (NLI disabled) vs candidate
  (NLI enabled) profiles on a representative scenario set must show (a) aggregate P@K or MRR for
  the candidate profile is >= baseline aggregate, AND (b) the zero-regression check section in the
  `unimatrix eval report` output is empty or all regressions are documented and approved by the human
  reviewer. This gate must be passed before the feature is marked complete.
- AC-10: Post-store detection: after a successful `context_store`, a fire-and-forget tokio task runs
  NLI on (new_entry, neighbor) pairs for the top-K HNSW neighbors. Pairs with entailment score >
  `nli_entailment_threshold` produce a `Supports` edge in `GRAPH_EDGES` with `source='nli'`,
  `bootstrap_only=0`. Pairs with contradiction score > `nli_contradiction_threshold` produce a
  `Contradicts` edge. Both use `store.write_pool_server()` directly (not analytics queue). Total
  new edges per call is capped at `max_contradicts_per_tick`.
- AC-11: NLI confidence stored in `metadata` column: edge writes include `metadata` as a JSON string
  with at minimum `{"nli_entailment": f32, "nli_contradiction": f32}`, enabling W3-1 GNN edge feature
  extraction.
- AC-12: Bootstrap edge promotion: on first background tick after startup (or on a dedicated one-shot
  task), all `bootstrap_only=1` Contradicts edges are re-scored through NLI. Confirmed → DELETE +
  INSERT with `source='nli'`, `bootstrap_only=0`. Refuted → DELETE. The task is idempotent — running
  it again on zero bootstrap-only rows produces no side effects.
- AC-13: Circuit breaker: the total number of `Contradicts` edges written per post-store call is capped
  at `max_contradicts_per_tick`. Excess pairs are dropped (logged at `tracing::debug!`). This prevents
  a single noisy store operation from flooding `GRAPH_EDGES` with contradiction edges that could
  trigger widespread auto-quarantine.
- AC-14: Graceful degradation: with `nli_enabled = false` or with no NLI model file present, the
  server starts and handles all MCP requests using the existing cosine-similarity search pipeline.
  No error is returned to callers; a single `tracing::warn!` at startup notes NLI is unavailable.
- AC-15: NLI inference panics are contained at the rayon oneshot channel boundary:
  `rayon_pool.spawn_with_timeout(...)` returns `Err(RayonError::Cancelled)`, which is mapped to
  graceful degradation (fall back to cosine for re-ranking; skip edge write for post-store). The
  MCP handler thread is not affected.
- AC-16: `model-download` CLI subcommand is extended (or a separate command added) to support
  downloading the NLI model: `unimatrix model-download --nli` downloads
  `cross-encoder/nli-MiniLM2-L6-H768` to the configured cache directory. Output includes the
  SHA-256 hash of the downloaded file so the operator can pin it in `config.toml`.
- AC-17: All new config parameters are validated at startup: `nli_top_k` in [1, 100],
  `nli_entailment_threshold` in (0.0, 1.0), `nli_contradiction_threshold` in (0.0, 1.0),
  `max_contradicts_per_tick` in [1, 100]. Out-of-range values abort startup with structured errors
  naming the offending field, consistent with existing `InferenceConfig::validate` pattern.
- AC-18: `EvalServiceLayer::from_profile()` fills in the W1-4 stub: when `[inference]` in the eval
  profile specifies `nli_enabled = true` and `nli_model_path`, the eval service layer loads
  `NliServiceHandle` and wires it into the `SearchService`. Profiles with `nli_enabled = false`
  (or absent field) use the baseline cosine path. This enables A/B comparison in `unimatrix eval run`.

---

## Constraints

1. **`ort = "=2.0.0-rc.9"` is pinned and must not change.** Both `OnnxProvider` (embedding) and
   `NliProvider` (NLI) must use the same pinned version. No ort version conflict is acceptable.
2. **NLI-confirmed edge writes use `write_pool_server()` directly, not the analytics queue.**
   `AnalyticsWrite::GraphEdge` carries a shed-safety note: bootstrap-origin writes only. W1-4 NLI
   confirmed edges are integrity writes. This is SR-02, already documented in `analytics.rs`.
3. **Single rayon pool, shared with embedding inference.** W1-4 NLI uses the pool from W1-2.
   W2-4 (GGUF) gets a separate pool for long-duration inference. No new pool is created here.
4. **NLI absence must not prevent server startup.** `nli_enabled = false` or missing model file:
   graceful degradation to cosine similarity. This is a non-negotiable reliability constraint.
5. **`max_contradicts_per_tick` circuit breaker is mandatory.** The NLI → `Contradicts` edge →
   topology penalty → auto-quarantine feedback loop must be rate-limited at the edge-creation step.
   The background tick's auto-quarantine counter hold-on-error behavior (ADR-002, crt-018b, entry
   #1544) remains in effect; this is an additional upstream gate.
6. **Length truncation before inference is mandatory (security).** Max 512 tokens / ~2000 chars per
   side. This is a security requirement ([High], product vision W1-4), not a performance optimization.
7. **`NliModel` lives in `unimatrix-embed`**, consistent with `EmbeddingModel`. `NliProvider` and
   `CrossEncoderProvider` trait also live in `unimatrix-embed`. `NliServiceHandle` lives in
   `unimatrix-server/src/infra/`, consistent with `EmbedServiceHandle`.
8. **No schema migration.** `GRAPH_EDGES` schema from crt-021 (version 13) is used as-is. The
   `metadata` column (TEXT DEFAULT NULL) already exists for W3-1 GNN edge features.
9. **Eval gate is blocking.** AC-09 is not optional. The feature cannot be marked deliverable without
   human-reviewed eval results from the W1-3 harness.
10. **`NliServiceHandle` mutex poisoning recovery**: if the NLI session `Mutex` is poisoned by a panic,
    `NliServiceHandle` must transition to `Failed` on the next `get_provider()` call and initiate
    retry, consistent with `EmbedServiceHandle`'s retry pattern.

---

## Open Questions

1. **Pool sizing adequacy**: The current rayon pool default is 4–8 threads (ADR-003, crt-022). NLI
   adds a concurrent inference workload: post-store fire-and-forget tasks compete with MCP embedding
   calls. At `nli_top_k=20` and 50–200ms per pair, a re-ranking call could occupy 1–2 rayon threads
   for 1–4 seconds. Should the pool floor be raised (e.g., to 6) when NLI is enabled, or should the
   architect accept the queue-based work-stealing behavior as sufficient?

5. **Bootstrap edge promotion timing**: The product vision says "first-tick background task." Should
   this run during the very first maintenance tick after startup (potentially before the HNSW is
   fully warmed), or should it be deferred to a subsequent tick? The bootstrap rows are zero in
   practice (crt-021 open question OQ-1 was unresolved), but the implementation must be robust.

## Resolved Decisions (human-approved pre-Phase 1b)

**D-01: Eval gate for zero-query-history deployments** — WAIVED. When a knowledge base has no real
query history and `unimatrix eval scenarios` returns no rows, the eval gate (AC-09) is waived for
that deployment. No hand-authored scenarios are required. The gate applies only when a snapshot with
query history is available.

**D-02: NLI score formula — replace, not blend** — In this first iteration, NLI entailment score
*replaces* the existing `rerank_score(similarity, confidence, cw)` formula entirely. If performance
warrants, a blended formula can be introduced in a follow-on feature. Architect documents the
replacement decision as an ADR.

**D-03: Model selection via config, 3-profile eval** — The model selection architecture should
externalize model choice via config wherever possible (e.g., `nli_model = "minilm2"` vs
`nli_model = "deberta"`), enabling model swap through configuration rather than code changes. Hash
pinning interacts with this: if config-driven model selection makes per-model hash pinning complex,
the architect may accept a more hardcoded hash solution for the initial implementation. The eval run
**must** compare all three profiles: baseline (cosine only), MiniLM2, and deberta-v3-small (if ONNX
export is available). If deberta ONNX is unavailable, document and fall back to 2-profile comparison.

**D-04: Post-store neighbor count uses separate `nli_post_store_k` config** — Post-store NLI
detection uses a dedicated `nli_post_store_k` config value (default 10), separate from `nli_top_k`
(default 20, used for search re-ranking). Post-store detection is less latency-sensitive and scans
a smaller neighbor set; the separation avoids coupling unrelated latency budgets.

---

## Tracking

https://github.com/dug-21/unimatrix/issues/327

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "NLI cross-encoder rayon inference", "bootstrap contradiction
  edges NLI confirmation", "search reranking eval harness gate", "circuit breaker auto-quarantine"
  -- Key findings: OnnxProvider `Mutex<Session>` ADR (#67) is the established pattern for NLI session.
  AnalyticsWrite::GraphEdge shed-policy note confirms NLI-confirmed edges must bypass analytics queue
  (SR-02 already encoded in the analytics.rs docstring). Background-tick counter hold-on-error pattern
  (entry #1542) applies to the NLI circuit breaker design. No existing Unimatrix patterns for
  cross-encoder re-ranking or NLI post-store pipeline.
- Stored: nothing novel to store yet — findings are crt-023-specific scope details or confirmations
  of already-stored patterns. Pattern for NLI post-store → GRAPH_EDGES integrity write path and the
  eval gate mechanics will be stored after the architect session produces ADR decisions.
