# SPECIFICATION: crt-023 — NLI + Cross-Encoder Re-ranking (W1-4)

---

## Objective

Add an ONNX cross-encoder NLI model to Unimatrix operating in two complementary modes:
(1) post-store detection of `Contradicts`/`Supports` relationships between new entries
and their HNSW neighbors, replacing the lexical `conflict_heuristic` for new edge
creation; (2) search re-ranking that re-scores the top HNSW candidates against the
actual query before truncation, replacing the existing `rerank_score` sort. The feature
is gated by human-reviewed eval harness results from the W1-3 harness comparing baseline
(cosine-only) against NLI-enabled profiles, and must not impair server startup or
availability when the NLI model is absent or hash-invalid.

---

## Functional Requirements

### Provider Infrastructure (FR-01 – FR-07)

**FR-01** — The system must provide a `CrossEncoderProvider` trait in `unimatrix-embed`
with the following interface:

```rust
pub trait CrossEncoderProvider: Send + Sync {
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>;
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>;
    fn name(&self) -> &str;
}
```

The trait must be `Send + Sync`. Both `score_pair` and `score_batch` are synchronous
(called from rayon threads, not async tasks).

**FR-02** — The system must provide an `NliScores` struct in `unimatrix-embed`:

```rust
pub struct NliScores {
    pub entailment: f32,
    pub neutral: f32,
    pub contradiction: f32,
}
```

For any `NliScores` produced by a valid inference call, `entailment + neutral + contradiction`
must equal `1.0` within a tolerance of `1e-4` (softmax normalization guarantee).

**FR-03** — The system must provide an `NliProvider` struct in `unimatrix-embed` that
implements `CrossEncoderProvider`. `NliProvider` must:

- Hold a `Mutex<Session>` for ONNX inference serialization (ADR-001, entry #67/#19)
- Hold a `Tokenizer` outside the mutex for lock-free tokenization
- Apply softmax to the model's 3-element logit output `[entailment, neutral, contradiction]`
  to produce normalized `NliScores`
- Be `Send + Sync`

**FR-04** — The system must provide an `NliModel` enum in `unimatrix-embed/src/model.rs`
with at least the variant `NliMiniLM2L6H768` (model ID `cross-encoder/nli-MiniLM2-L6-H768`,
~85MB, Apache 2.0). A second variant `NliDebertaV3Small` (model ID
`cross-encoder/nli-deberta-v3-small`, ~180MB) must be included; its availability is
verified at implementation time (SR-01). Each variant must implement:

- `onnx_repo_path() -> &str`
- `onnx_filename() -> &str`
- `cache_subdir() -> &str`

following the same conventions as `EmbeddingModel`.

**FR-05** — The `NliModel` enum must be selectable via configuration using a string
identifier (e.g., `nli_model = "minilm2"` or `nli_model = "deberta"`), not only via
file path. This enables model swap through configuration rather than code changes (D-03).
The config field `nli_model_name: Option<String>` (with `#[serde(default)]`) specifies
the model variant; `nli_model_path: Option<PathBuf>` overrides the auto-resolved cache
path when set.

**FR-06** — Before each NLI inference call, each input string (query side and passage
side independently) must be truncated to a maximum of 512 tokens or approximately 2000
characters, whichever limit fires first. Truncation is silent (no error returned, no
warning logged per-call). This is a security requirement, not a performance optimization
(see NFR-08).

**FR-07** — The model download CLI subcommand must be extended to support NLI model
download: `unimatrix model-download --nli` downloads the configured NLI model to the
configured cache directory and outputs the SHA-256 hash of the downloaded file so the
operator can pin it in `config.toml`. Download follows the existing `ensure_model`
pattern via `hf-hub`.

### Service Handle and Configuration (FR-08 – FR-13)

**FR-08** — The system must provide an `NliServiceHandle` in
`unimatrix-server/src/infra/nli_handle.rs` implementing the following state machine:

```
Loading ──success──> Ready
Loading ──failure──> Failed
Failed  ──retry───> Loading  (after backoff; exhaustion leaves it in Failed)
Ready   ──panic────> Failed  (on Mutex poison, next get_provider() call)
```

`NliServiceHandle` must mirror the structure of `EmbedServiceHandle`. `get_provider()`
returns `Err(ServerError::NliNotReady)` while in `Loading` and
`Err(ServerError::NliFailed)` when retries are exhausted.

**FR-09** — When `nli_model_sha256` is present in config, `NliServiceHandle` must verify
the SHA-256 hash of the model file before constructing the ONNX session. Hash mismatch
must:

1. Transition `NliServiceHandle` to `Failed`
2. Log a `tracing::error!` message containing both the words "security" and "hash mismatch"
3. Leave the server running on cosine-similarity fallback

Hash verification failure must never cause a panic or process exit.

**FR-10** — If the ONNX `Mutex` inside `NliProvider` is poisoned by a rayon thread panic,
the next call to `NliServiceHandle::get_provider()` must detect the poisoned state,
transition to `Failed`, and initiate the retry sequence. This mirrors `EmbedServiceHandle`'s
poison recovery.

**FR-11** — The `[inference]` section of `UnimatrixConfig` must be extended with the
following fields (all `#[serde(default)]`):

| Field | Type | Default | Validated Range |
|-------|------|---------|----------------|
| `nli_enabled` | `bool` | `true` | — |
| `nli_model_name` | `Option<String>` | `None` (resolves to `NliMiniLM2L6H768`) | valid enum variant name |
| `nli_model_path` | `Option<PathBuf>` | `None` (auto-resolved from cache) | — |
| `nli_model_sha256` | `Option<String>` | `None` | 64-char hex when set |
| `nli_top_k` | `usize` | `20` | `[1, 100]` |
| `nli_post_store_k` | `usize` | `10` | `[1, 100]` |
| `nli_entailment_threshold` | `f32` | `0.6` | `(0.0, 1.0)` exclusive |
| `nli_contradiction_threshold` | `f32` | `0.6` | `(0.0, 1.0)` exclusive |
| `max_contradicts_per_tick` | `usize` | `10` | `[1, 100]` |
| `nli_auto_quarantine_threshold` | `f32` | `0.85` | `(0.0, 1.0)` exclusive, must be > `nli_contradiction_threshold` |

`nli_top_k` is used exclusively for search re-ranking candidate expansion (D-04).
`nli_post_store_k` is used exclusively for post-store neighbor detection (D-04).
`nli_auto_quarantine_threshold` governs when NLI-origin `Contradicts` edges may trigger
auto-quarantine; it must be strictly greater than `nli_contradiction_threshold` to ensure
NLI-derived auto-quarantine requires higher confidence than edge creation alone.

**FR-12** — All ten NLI config fields must be validated at startup via the existing
`InferenceConfig::validate` pattern. Out-of-range or invalid values must abort startup
with a structured error message naming the offending field. An additional cross-field
validation must verify `nli_auto_quarantine_threshold > nli_contradiction_threshold`;
if violated, startup aborts with a structured error. Validation runs before
`NliServiceHandle` is constructed.

**FR-13** — `Arc<NliServiceHandle>` must be added to `AppState`/`ServiceLayer` and
wired through server startup. `NliServiceHandle` must be initialized regardless of
`nli_enabled`; when `nli_enabled = false`, `get_provider()` immediately returns
`Err(ServerError::NliNotReady)` without attempting to load the model.

### Search Re-ranking (FR-14 – FR-17)

**FR-14** — When NLI is ready and `nli_enabled = true`, `SearchService::search` must
execute the following pipeline (replacing the `rerank_score` sort step):

```
embed
→ HNSW top-nli_top_k (expanded candidate pool)
→ quarantine filter
→ status filter / penalty
→ supersession injection
→ NLI batch score (query, each_candidate) via rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT)
→ sort by NLI entailment score descending
→ truncate to top-K
→ co-access boost
→ floors
```

The NLI entailment score replaces the `rerank_score(similarity, confidence, cw)` formula
entirely for this sort step (D-02). The `rerank_score` computation is retained in code
for the fallback path but is not called when NLI is active.

**FR-15** — When NLI is not ready (`get_provider()` returns any `Err`) or `nli_enabled = false`,
`SearchService::search` must fall back to the existing `rerank_score`-based sort path
unchanged. The fallback must be transparent to callers — no error is propagated, no
field names in the response change.

**FR-16** — NLI inference for search re-ranking must run via
`rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` on the shared `Arc<RayonPool>`.
If the timeout fires or the rayon task returns `Err(RayonError::Cancelled)`, the search
service must fall back to the `rerank_score` path for that request (not an error to
the caller).

**FR-17** — The response field names and schema of `context_search` must not change.
Result ordering may change (NLI re-ranking changes which entries appear and in what
order). NLI scores are internal signal only and are not included in any MCP tool
response.

### Post-Store NLI Detection (FR-18 – FR-22)

**FR-18** — After a successful `context_store`, `StoreService::insert` must spawn a
fire-and-forget tokio task (not blocking the MCP response) that executes the following
post-store NLI detection pipeline:

1. Retrieve the top-`nli_post_store_k` HNSW neighbors of the new entry using the
   embedding already computed during the insert (no re-embedding)
2. Batch-score `(new_entry_text, neighbor_text)` pairs via the shared rayon pool
3. For pairs where `nli_scores.entailment > nli_entailment_threshold`: write a `Supports`
   edge to `GRAPH_EDGES`
4. For pairs where `nli_scores.contradiction > nli_contradiction_threshold`: write a
   `Contradicts` edge to `GRAPH_EDGES`
5. Cap total edges written per call at `max_contradicts_per_tick`; excess pairs are
   dropped (logged at `tracing::debug!`)

**FR-19** — All edges written by the post-store NLI task must use `write_pool_server()`
directly, not the analytics queue. Edge fields must be:

- `source = 'nli'`
- `bootstrap_only = 0`
- `metadata` = JSON string `{"nli_entailment": <f32>, "nli_contradiction": <f32>}`

The `UNIQUE(source_id, target_id, relation_type)` constraint with `INSERT OR IGNORE`
provides idempotency.

**FR-20** — The embedding computed during `context_store` must be threaded through to
the spawned post-store NLI task without recomputation. The architecture must define the
handoff contract explicitly (see Open Questions — SR-09 embedding handoff).

**FR-21** — The post-store NLI task must not propagate panics to the tokio runtime. A
rayon panic results in `RayonError::Cancelled`, which the task handles by logging at
`tracing::warn!` and exiting without writing any edges for that call.

**FR-22** — The `max_contradicts_per_tick` cap applies per `context_store` call (one
fire-and-forget task invocation), not globally per background tick. The cap name is
retained from SCOPE.md for config compatibility but its semantic unit is per-call. This
resolves SR-06.

**FR-22b** — The background tick's auto-quarantine logic must apply a higher confidence
threshold for NLI-origin `Contradicts` edges than for manually-corrected entries. An entry
whose topology penalty is driven solely by NLI-origin `Contradicts` edges must only trigger
auto-quarantine when the NLI scores for those edges all exceed `nli_auto_quarantine_threshold`
(default 0.85). This is a second, higher bar beyond `nli_contradiction_threshold` (the edge
creation threshold, default 0.6). The existing hold-on-error behavior (crt-018b, entry #1544)
remains in effect and is unaffected by this change.

### Bootstrap Edge Promotion (FR-23 – FR-25)

**FR-23** — On the first background tick after server startup, a one-shot task must
query all `GRAPH_EDGES` rows with `bootstrap_only = 1` and `relation_type = 'Contradicts'`.
For each row:

- Score the `(source_entry_text, target_entry_text)` pair through NLI
- If `nli_scores.contradiction > nli_contradiction_threshold`: DELETE the row and INSERT
  a replacement with `source = 'nli'`, `bootstrap_only = 0`, and NLI metadata
- If score does not exceed threshold: DELETE the row only

**FR-24** — The bootstrap promotion task must be idempotent. A durable completion
marker (e.g., a flag in the `COUNTERS` table, key `bootstrap_nli_promotion_done`) must
be written after the task completes successfully. On subsequent ticks or restarts, the
presence of this marker causes the task to skip without querying `GRAPH_EDGES`. The
zero-row case (current production state) completes successfully and sets the marker.

**FR-25** — If NLI is not ready when the first tick fires, the bootstrap promotion task
must be deferred to the next tick where NLI is ready. The task must not run on cosine
fallback — NLI promotion is NLI-only. A `tracing::info!` must note the deferral.

### Eval Gate (FR-26 – FR-29)

**FR-26** — The `EvalServiceLayer::from_profile()` stub must be filled in: when the
eval profile specifies `nli_enabled = true` and either `nli_model_path` or
`nli_model_name`, the eval service layer must construct an `NliServiceHandle` and wire
it into the `SearchService`. Profiles with `nli_enabled = false` or no NLI fields use
the baseline cosine path.

**FR-27** — The feature eval run must compare at minimum two profiles:

- `baseline.toml`: `nli_enabled = false` (or empty TOML with compiled defaults, NLI absent)
- `candidate.toml`: `nli_enabled = true`, `nli_model_name = "minilm2"` (or equivalent path)

If `cross-encoder/nli-deberta-v3-small` ONNX export is available and verified at
implementation time, a third profile `candidate-deberta.toml` must be added (D-03).
If deberta ONNX is unavailable, the eval run proceeds with two profiles and the
unavailability is documented.

**FR-28** — The eval gate passes when all of the following conditions are met for the
available profiles:

- Aggregate P@K or aggregate MRR for each NLI candidate profile is >= the baseline
  aggregate across all scenarios in the snapshot
- The zero-regression section of the `unimatrix eval report` output is either empty, or
  all per-scenario regressions are individually documented and approved by the human
  reviewer

**FR-29** — The eval gate is waived (D-01) when and only when:

- `unimatrix eval scenarios` returns zero rows for the available snapshot
- No hand-authored eval scenarios exist in the repository

When the gate is waived, the waiver must be documented in the feature delivery report
with the reason ("no query history available"). The waiver is not a quality approval;
it is a record that the gate condition was inapplicable. A minimum evidence bar is
required even under waiver: at least one successful NLI inference call must be
demonstrated via the test suite (AC-01 passing).

---

## Non-Functional Requirements

**NFR-01 — Latency (search re-ranking, p95)**: NLI re-ranking of `nli_top_k = 20`
candidates must not push `context_search` p95 latency above the `MCP_HANDLER_TIMEOUT`
threshold. The eval harness quantifies actual latency overhead for the specific knowledge
base. If p95 exceeds acceptable bounds under load testing, the architect may reduce the
default `nli_top_k` or document the tradeoff in the ADR.

**NFR-02 — Latency (post-store detection)**: Post-store NLI detection runs fire-and-forget
and must not add latency to the `context_store` MCP response. The spawned task's
execution time is not bounded by `MCP_HANDLER_TIMEOUT`; it uses `rayon_pool.spawn()`
(no timeout) for the CPU-bound segment.

**NFR-03 — Availability**: Server startup must succeed and all MCP tools must be
functional regardless of NLI model presence, hash validity, or model loading outcome.
NLI absence must not cause any MCP tool to return an error it would not return without
NLI enabled. A single `tracing::warn!` at startup is the only required signal.

**NFR-04 — Rayon pool contention**: NLI inference shares the `Arc<RayonPool>` established
in W1-2 (crt-022). The spec does not mandate a pool size floor; the architect must
document pool sizing as an ADR (SR-02). The `max_contradicts_per_tick` cap limits
post-store batch concurrency as a coarse bound.

**NFR-05 — Memory**: The NLI model session must be held in a single `Mutex<Session>`
(one session, not a pool). This is consistent with ADR-001 (entry #67/#19) and bounds
peak VRAM/RAM consumption to one model instance (~85MB for MiniLM2, ~180MB for deberta).

**NFR-06 — Reliability (panic containment)**: NLI inference panics (OOM, malformed
tensor) must not propagate beyond the rayon oneshot channel boundary. `RayonError::Cancelled`
is the signal; the calling async task handles it via graceful degradation. The MCP
handler thread must not observe the panic.

**NFR-07 — Config-driven behavior**: All thresholds, limits, and the model selection
must be config-driven. No threshold or model choice is hardcoded in the implementation.
Changes to inference behavior require only `config.toml` edits, not recompilation.

**NFR-08 — Security (input truncation)**: Per-side input truncation to 512 tokens or
~2000 characters before ONNX inference is a security requirement. Adversarial long inputs
that could cause OOM or extreme latency in the ONNX session must be silently truncated.
This must be enforced in `NliProvider` itself, not in call sites, so all callers are
automatically protected.

**NFR-09 — Security (model integrity)**: SHA-256 hash verification of the NLI model file
at load time is a critical security requirement (product vision W1-4). A tampered model
file is an undetectable model-poisoning attack. When `nli_model_sha256` is set, the hash
must be verified before the ONNX session is initialized. Production deployments are
expected to set this field.

**NFR-10 — Compatibility (ort version)**: `ort = "=2.0.0-rc.9"` is pinned and must not
change. `NliProvider` must use the same pinned ort version as `OnnxProvider`. No ort
version conflict is acceptable; both providers live in `unimatrix-embed`.

**NFR-11 — No schema migration**: `GRAPH_EDGES` from crt-021 (schema version 13) is
used as-is. The `metadata` column (TEXT DEFAULT NULL) already exists. crt-023 must not
introduce any schema migration.

**NFR-12 — Eval environment portability**: The eval CLI must not hard-fail when the NLI
model file is absent from the eval environment. A missing model on a profile marked
`nli_enabled = true` should either (a) attempt download if network is available, or
(b) skip that profile with a documented warning in the report, rather than aborting
the entire `eval run` invocation (SR-08).

---

## Acceptance Criteria

All 18 criteria from SCOPE.md are retained below with expanded precision. Additional
criteria AC-19 through AC-24 cover resolved decisions D-01 through D-04 and risk SR-06.

**AC-01** — `NliProvider` in `unimatrix-embed` implements `CrossEncoderProvider` (trait
with `score_pair`, `score_batch`, `name`). Given any valid `(query, passage)` pair,
`score_pair` returns `NliScores` where `entailment + neutral + contradiction` is within
`1e-4` of `1.0`. `NliProvider` is `Send + Sync`.
Verification: unit test calling `score_pair` on a fixture pair and asserting sum constraint.

**AC-02** — `NliProvider` holds a `Mutex<Session>` for ONNX inference and a `Tokenizer`
outside the mutex. A concurrent test with two threads calling `score_pair` simultaneously
must complete without deadlock or data race. `NliProvider` satisfies the `Send + Sync`
auto-trait bounds (compile-time verification).
Verification: compile-time trait check + concurrent unit test.

**AC-03** — Input truncation is enforced inside `NliProvider` before tokenization.
Inputs of 10 000 characters on either side must not panic, return an error, or produce
OOM; they return valid `NliScores`. Truncated inputs are silently accepted.
Verification: unit test with oversized inputs asserting no panic and valid score sum.

**AC-04** — `NliModel` enum in `unimatrix-embed/src/model.rs` includes at minimum
`NliMiniLM2L6H768` with `model_id = "cross-encoder/nli-MiniLM2-L6-H768"`. Methods
`onnx_repo_path()`, `onnx_filename()`, and `cache_subdir()` return non-empty strings
following the same naming conventions as `EmbeddingModel` variants.
Verification: unit test asserting method return values.

**AC-05** — `NliServiceHandle` in `unimatrix-server/src/infra/nli_handle.rs` implements
the `Loading → Ready | Failed → Retrying` state machine. `get_provider()` returns
`Err(ServerError::NliNotReady)` when the handle is in `Loading` state and
`Err(ServerError::NliFailed)` when retries are exhausted. A server instance with a
missing NLI model file starts successfully and serves all MCP requests.
Verification: integration test with missing model file confirming server starts and
`context_search` returns results (via cosine fallback).

**AC-06** — SHA-256 hash verification: when `nli_model_sha256` is present in config and
does not match the model file on disk, `NliServiceHandle` transitions to `Failed`, emits
a `tracing::error!` log line containing the substring "security" and the substring
"hash mismatch", and the server continues operating. All MCP tools remain functional.
Verification: test with a corrupted model file and a hash mismatch asserting log content
and server uptime.

**AC-07** — `[inference]` config section contains all nine NLI fields listed in FR-11
with the specified defaults and types. A config file omitting all NLI fields deserializes
successfully with defaults applied.
Verification: unit test deserializing an empty `[inference]` section and asserting each
field's default value.

**AC-08** — Search re-ranking: when NLI is `Ready` and `nli_enabled = true`,
`SearchService::search` expands the HNSW candidate pool to `nli_top_k`, scores each
`(query, candidate_text)` pair via the rayon pool using `spawn_with_timeout(MCP_HANDLER_TIMEOUT)`,
sorts by NLI entailment score descending, and returns the top-K results. When NLI is
not ready or `nli_enabled = false`, the existing `rerank_score` path is used unchanged.
Verification: integration test with NLI mocked/stub returning known scores, asserting
result ordering matches expected entailment sort.

**AC-09** — Eval gate: `unimatrix eval run` results comparing the baseline profile
(NLI disabled) and the MiniLM2 candidate profile (NLI enabled) on a snapshot with real
query history must show: (a) aggregate P@K or aggregate MRR for the candidate is >= the
baseline aggregate across all evaluated scenarios, AND (b) the zero-regression section
in the `unimatrix eval report` output is empty or all regressions are individually
documented and approved by the human reviewer. This gate must be satisfied before the
feature is marked deliverable.
Verification: human review of `unimatrix eval report` output; eval artifacts stored in
feature delivery report.

**AC-10** — Post-store detection: after a successful `context_store`, a fire-and-forget
tokio task executes the NLI detection pipeline on the top-`nli_post_store_k` HNSW
neighbors. Pairs above `nli_entailment_threshold` produce a `Supports` edge; pairs above
`nli_contradiction_threshold` produce a `Contradicts` edge. Both edge types use
`write_pool_server()` directly with `source='nli'`, `bootstrap_only=0`. Total new edges
per call is capped at `max_contradicts_per_tick`.
Verification: integration test storing an entry with a known contradictory neighbor and
asserting a `Contradicts` edge is written with correct fields.

**AC-11** — NLI metadata in edges: every edge written by the NLI pipeline (post-store
and bootstrap promotion) must include a `metadata` JSON string with at minimum the keys
`nli_entailment` (f32) and `nli_contradiction` (f32).
Verification: integration test asserting `metadata` column content for a written edge.

**AC-12** — Bootstrap edge promotion: on the first background tick after startup (when
NLI is ready), all `GRAPH_EDGES` rows with `bootstrap_only = 1` are processed. Each row
is scored through NLI; confirmed rows are replaced with `source='nli'`, `bootstrap_only=0`
entries; refuted rows are deleted. On a database with zero `bootstrap_only=1` rows, the
task completes without error and sets the completion marker. Running the task again
(e.g., on restart) is a no-op due to the durable completion marker in `COUNTERS`.
Verification: (a) unit test with zero-row case asserting marker is set; (b) integration
test with synthetic `bootstrap_only=1` rows asserting promotion and deletion.

**AC-13** — Circuit breaker: total edges written per `context_store` call is capped at
`max_contradicts_per_tick`. Excess pairs that would exceed the cap are dropped with a
`tracing::debug!` log and not written to `GRAPH_EDGES`.
Verification: integration test with `max_contradicts_per_tick = 2` and 5 neighbor pairs
all above threshold, asserting exactly 2 edges written.

**AC-14** — Graceful degradation: with `nli_enabled = false`, or with no NLI model file
present, or with hash mismatch, the server starts and handles all MCP requests using
the cosine-similarity fallback pipeline. No error is returned to callers. A single
`tracing::warn!` at startup notes NLI unavailability.
Verification: server startup test with each degradation condition (disabled, missing,
hash mismatch) asserting: (a) startup succeeds, (b) `context_search` returns results,
(c) exactly one warn-level log mentioning NLI.

**AC-15** — Panic containment: if an NLI inference call panics on the rayon thread,
`rayon_pool.spawn_with_timeout` returns `Err(RayonError::Cancelled)`. The search service
maps this to cosine fallback for re-ranking, or to a no-op edge write for post-store.
The MCP handler thread is not affected.
Verification: test injecting a panic via a mock `CrossEncoderProvider` and asserting
the MCP handler returns a successful response (fallback results).

**AC-16** — `unimatrix model-download --nli` downloads the configured NLI model to the
cache directory and prints the SHA-256 hash of the downloaded file. The output format
must be sufficient for the operator to copy-paste the hash into `nli_model_sha256` in
`config.toml`.
Verification: CLI test (or manual smoke test in delivery report) asserting download
succeeds and hash is printed.

**AC-17** — Config validation: all ten NLI config fields are validated at startup. The
following conditions abort startup with a structured error naming the offending field:
`nli_top_k` outside `[1, 100]`; `nli_post_store_k` outside `[1, 100]`;
`nli_entailment_threshold` outside `(0.0, 1.0)`; `nli_contradiction_threshold` outside
`(0.0, 1.0)`; `max_contradicts_per_tick` outside `[1, 100]`; `nli_model_name` set to an
unrecognized variant string; `nli_model_sha256` set to a string whose length is not 64
hex characters; `nli_auto_quarantine_threshold` outside `(0.0, 1.0)`. Additionally, the
cross-field invariant `nli_auto_quarantine_threshold > nli_contradiction_threshold` is
validated — violation aborts startup with a structured error naming both fields.
Verification: unit tests for each out-of-range case and the cross-field violation case,
asserting startup error with the offending field name(s) in the message.

**AC-18** — `EvalServiceLayer::from_profile()` fills the W1-4 stub: profiles with
`nli_enabled = true` and a resolvable model construct `NliServiceHandle` and wire it
into `SearchService`. Profiles with `nli_enabled = false` or absent NLI fields use the
baseline cosine path. An `eval run` with both profiles produces two result sets enabling
A/B comparison.
Verification: integration test running `eval run` with a baseline and candidate TOML
profile against a fixture snapshot, asserting two result files are produced.

**AC-19** — `nli_post_store_k` (default 10) is distinct from `nli_top_k` (default 20)
in config and in code (D-04). `StoreService` uses `nli_post_store_k` for neighbor
retrieval; `SearchService` uses `nli_top_k` for candidate expansion. Setting one does
not affect the other.
Verification: unit test asserting each service reads the correct config field.

**AC-20** — NLI entailment score replaces `rerank_score` entirely in the search
re-ranking sort step when NLI is active (D-02). `rerank_score(similarity, confidence, cw)`
is not called as part of the NLI-active sort. The fallback path (NLI not ready) continues
to use `rerank_score` unchanged.
Verification: integration test with NLI active confirming result ordering matches
entailment sort, not the composite `rerank_score` formula.

**AC-21** — Model selection via config string is supported (D-03). Setting
`nli_model_name = "minilm2"` in config selects `NliMiniLM2L6H768`; setting
`nli_model_name = "deberta"` selects `NliDebertaV3Small` (if available). An unrecognized
model name string fails startup validation with a structured error (see AC-17).
Verification: unit test asserting `NliModel` resolves from each valid string identifier.

**AC-22** — The eval gate waiver condition (D-01) is applied correctly: when
`unimatrix eval scenarios` returns zero rows for the available snapshot, the gate is
documented as waived in the delivery report. The waiver does not exempt AC-01 (at least
one successful NLI inference call demonstrated by the test suite passing).
Verification: delivery report contains waiver documentation when applicable; test suite
AC-01 must pass regardless.

**AC-23** — `max_contradicts_per_tick` semantics are per-`context_store` call (FR-22,
resolves SR-06). The config field name is `max_contradicts_per_tick` for compatibility
with SCOPE.md and product vision references. Implementation comments must note the
per-call semantic.
Verification: AC-13 integration test (per-call cap enforcement).

**AC-24** — Bootstrap promotion idempotency is achieved via a durable marker in the
`COUNTERS` table (key `bootstrap_nli_promotion_done`), not by row absence as a guard
(FR-24, addresses SR-07). On restart after a completed promotion run, the task is a
provable no-op regardless of current `GRAPH_EDGES` state.
Verification: integration test restarting the server after a completed promotion run
and asserting the `bootstrap_nli_promotion_done` counter is present and no duplicate
promotions occur.

**AC-25** — NLI-origin auto-quarantine uses a higher confidence threshold (FR-22b). An
entry whose topology penalty is driven solely by NLI-origin `Contradicts` edges does not
trigger auto-quarantine unless the NLI scores stored in those edges' `metadata` column all
exceed `nli_auto_quarantine_threshold` (default 0.85). Entries penalised by a mix of
NLI-origin and manually-corrected edges continue to follow the existing auto-quarantine
logic.
Verification: integration test that writes NLI `Contradicts` edges with `nli_contradiction`
score 0.7 (above `nli_contradiction_threshold=0.6`, below `nli_auto_quarantine_threshold=0.85`)
and asserts the target entry is NOT auto-quarantined on the next background tick.

---

## Domain Models

### `NliScores`

```rust
pub struct NliScores {
    pub entailment: f32,   // P(premise entails hypothesis)
    pub neutral: f32,      // P(premise and hypothesis are unrelated)
    pub contradiction: f32,// P(premise contradicts hypothesis)
    // Invariant: entailment + neutral + contradiction ≈ 1.0 (within 1e-4)
}
```

Produced by softmax over the 3-element logit output of the cross-encoder ONNX model.
The model is fine-tuned on SNLI/MultiNLI; NLI terminology applies: the query is the
"premise" and the candidate passage is the "hypothesis."

### `CrossEncoderProvider` (trait)

The abstraction over any NLI cross-encoder model. Takes `(query, passage)` string pairs
and returns `NliScores`. `score_pair` is the single-pair interface; `score_batch` is
the batch interface for efficiency. Both are synchronous (called from rayon threads).
Implementations hold the ONNX session and tokenizer internally.

### `NliProvider` (struct)

Concrete implementation of `CrossEncoderProvider` backed by an ONNX session.
Fields: `session: Mutex<Session>`, `tokenizer: Tokenizer`, `model_name: String`.
Constructed by `NliServiceHandle` during the `Loading` state. Truncates inputs to
512 tokens / 2000 chars before tokenization. Applies softmax to raw logits.

### `NliModel` (enum)

Catalog of known NLI ONNX model variants. Variants:

- `NliMiniLM2L6H768`: `cross-encoder/nli-MiniLM2-L6-H768`, ~85MB, Apache 2.0, primary
- `NliDebertaV3Small`: `cross-encoder/nli-deberta-v3-small`, ~180MB, availability TBD

Resolves from string identifiers `"minilm2"` and `"deberta"` respectively.
Provides `onnx_repo_path()`, `onnx_filename()`, `cache_subdir()` methods consistent
with `EmbeddingModel`.

### `NliServiceHandle` (struct)

State machine managing the lifecycle of an `NliProvider` instance. States:

| State | Meaning | `get_provider()` return |
|-------|---------|------------------------|
| `Loading` | Model load in progress | `Err(NliNotReady)` |
| `Ready` | Provider available | `Ok(Arc<NliProvider>)` |
| `Failed` | Load failed / mutex poisoned | `Err(NliFailed)` |
| `Retrying` | Backoff before re-attempt | `Err(NliNotReady)` |

Mirrors `EmbedServiceHandle`. Initialized at server startup and held as `Arc<NliServiceHandle>`
on `AppState`.

### `NliEdge` (write contract, not a struct)

When the NLI pipeline writes to `GRAPH_EDGES`, the following fields must be populated:

| Column | Value |
|--------|-------|
| `source_id` | ID of the premise entry |
| `target_id` | ID of the hypothesis entry |
| `relation_type` | `'Contradicts'` or `'Supports'` |
| `weight` | NLI score for the relation type (f32) |
| `created_by` | `'nli'` |
| `source` | `'nli'` |
| `bootstrap_only` | `0` |
| `metadata` | `{"nli_entailment": <f32>, "nli_contradiction": <f32>}` |

Writes use `INSERT OR IGNORE` for idempotency on the `UNIQUE(source_id, target_id, relation_type)` constraint.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **NLI** | Natural Language Inference — classification of a (premise, hypothesis) pair into {entailment, neutral, contradiction} |
| **cross-encoder** | A model that takes a concatenated (query, passage) input and outputs a relevance or NLI score, as opposed to a bi-encoder that embeds each text independently |
| **bi-encoder** | The embedding model used in Unimatrix today; fast but measures topical similarity, not answer relevance |
| **re-ranking** | The process of re-scoring a set of candidates retrieved by the bi-encoder using the more expensive cross-encoder |
| **entailment threshold** | The `nli_entailment_threshold` config value; a pair scoring above this produces a `Supports` edge |
| **contradiction threshold** | The `nli_contradiction_threshold` config value; a pair scoring above this produces a `Contradicts` edge |
| **bootstrap edge** | A `GRAPH_EDGES` row with `bootstrap_only=1` written by the W1-1 bootstrap process, excluded from confidence scoring until NLI promotion |
| **promotion** | Replacing a `bootstrap_only=1` edge with a `source='nli'`, `bootstrap_only=0` edge after NLI confirms it |
| **circuit breaker** | The `max_contradicts_per_tick` cap limiting edges written per `context_store` call |
| **graceful degradation** | The behavior of the server when NLI is unavailable: cosine-similarity search continues unchanged |
| **fire-and-forget** | A tokio task spawned without awaiting its result; used for post-store NLI detection to avoid blocking the MCP response |

---

## User Workflows

### Workflow 1: Operator Setup

1. Operator runs `unimatrix model-download --nli` to download the NLI model
2. CLI prints the SHA-256 hash of the downloaded file
3. Operator adds `nli_model_sha256 = "<hash>"` to `[inference]` in `config.toml`
4. Operator optionally sets `nli_model_name = "minilm2"` (or `"deberta"`)
5. Server starts; `NliServiceHandle` verifies hash and transitions to `Ready`
6. `tracing::info!` confirms NLI is active

### Workflow 2: Context Search (NLI Active)

1. MCP caller invokes `context_search` with a natural-language query
2. `SearchService` embeds the query, retrieves top-`nli_top_k` HNSW candidates
3. NLI scores each `(query, candidate)` pair on the rayon pool
4. Results are sorted by entailment score descending, truncated to top-K
5. Co-access boost applied; floors applied
6. Results returned to caller — same response schema, better ordering

### Workflow 3: Context Store with NLI Detection

1. MCP caller invokes `context_store` with a new entry
2. `StoreService` inserts the entry; embedding is computed and stored
3. MCP response is returned immediately (store is complete)
4. Fire-and-forget tokio task: retrieves top-`nli_post_store_k` HNSW neighbors
5. NLI scores each `(new_entry, neighbor)` pair
6. `Contradicts`/`Supports` edges written to `GRAPH_EDGES` (capped at `max_contradicts_per_tick`)
7. On the next background tick, the `TypedRelationGraph` is rebuilt with the new edges

### Workflow 4: Eval Gate Execution

1. Human runs `unimatrix snapshot --out snapshot.db`
2. Human runs `unimatrix eval scenarios --db snapshot.db` — mines query_log
3. If zero scenarios: gate is waived per D-01; document waiver; AC-01 must still pass
4. If scenarios exist: human runs `unimatrix eval run --db snapshot.db --scenarios scenarios.jsonl --configs baseline.toml,candidate.toml[,candidate-deberta.toml]`
5. Human runs `unimatrix eval report --results <dir>` and reviews the output
6. Gate passes per AC-09 / FR-28; feature is marked deliverable

### Workflow 5: Graceful Degradation

1. Server starts with `nli_enabled = false` (or missing model, or hash mismatch)
2. `NliServiceHandle` remains in `Failed` or is bypassed; one `tracing::warn!` emitted
3. All MCP tools operate normally on cosine fallback
4. `context_search` returns results ordered by `rerank_score` (existing behavior)
5. Post-store NLI detection is silently skipped; no edges are written via NLI path

---

## Constraints

1. **`ort = "=2.0.0-rc.9"` is pinned.** Both `OnnxProvider` (embedding) and `NliProvider`
   (NLI) must use this exact pinned version. No version conflict is acceptable.

2. **NLI edge writes use `write_pool_server()` directly.** `AnalyticsWrite::GraphEdge`
   carries a shedding policy for bootstrap-origin writes only. NLI-confirmed edges are
   integrity writes and must bypass the analytics queue (SR-02, already documented in
   `analytics.rs`).

3. **Shared rayon pool (W1-2).** `NliProvider` inference uses the `Arc<RayonPool>` from
   crt-022. No new pool is created. W2-4 (GGUF) gets a separate pool; that is not in scope
   here.

4. **NLI absence must not prevent server startup.** This is non-negotiable.

5. **`max_contradicts_per_tick` circuit breaker is mandatory.** Applies per `context_store`
   call (FR-22, resolves SR-06). The background tick auto-quarantine hold-on-error behavior
   (ADR-002, crt-018b, entry #1544) remains in effect as a downstream gate.

6. **Length truncation is a security requirement.** 512 tokens / ~2000 chars per side,
   enforced in `NliProvider`, not in call sites.

7. **`NliModel` and `CrossEncoderProvider` live in `unimatrix-embed`.** `NliServiceHandle`
   lives in `unimatrix-server/src/infra/`. This is consistent with the split of
   `EmbeddingModel`/`EmbeddingProvider` vs `EmbedServiceHandle`.

8. **No schema migration.** `GRAPH_EDGES` from crt-021 (schema v13) is used as-is.
   The `metadata` column (TEXT DEFAULT NULL) already exists.

9. **Eval gate is blocking.** AC-09 is not optional when query history is available.

10. **Mutex poison recovery is required.** `NliServiceHandle` must transition to `Failed`
    on poison detection and initiate retry, consistent with `EmbedServiceHandle`.

11. **Bootstrap promotion is NLI-only.** The task must not run on cosine fallback.
    If NLI is not ready at first tick, the task defers until NLI is ready (FR-25).

---

## Dependencies

### Crate Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `ort` | `=2.0.0-rc.9` (pinned) | ONNX Runtime for NLI session |
| `tokenizers` | (existing, same as OnnxProvider) | Tokenization for NLI inputs |
| `hf-hub` | (existing) | Model download via `ensure_model` pattern |
| `sha2` | (check if already present; add if not) | SHA-256 hash verification for model pinning |
| `serde_json` | (existing) | `metadata` JSON serialization |
| `rayon` | (via `Arc<RayonPool>`) | CPU-bound NLI inference off tokio thread |

### Internal Component Dependencies

| Component | Dependency Type | Reason |
|-----------|----------------|--------|
| `crt-021` GRAPH_EDGES | Schema dependency | NLI writes to `GRAPH_EDGES` as-is |
| `crt-022` RayonPool | Runtime dependency | Shared pool for NLI inference |
| `nan-007` eval harness | Gate dependency | `unimatrix eval run` gates the feature |
| `EmbedServiceHandle` | Design pattern | `NliServiceHandle` mirrors this pattern |
| `OnnxProvider` | Design pattern | `NliProvider` mirrors `Mutex<Session>` + `Tokenizer` pattern |
| `InferenceConfig` | Config extension | NLI fields added to existing section |
| `EvalServiceLayer` | Stub fill-in | W1-4 stub in `from_profile()` filled in |
| `StoreService::insert` | Integration point | Fire-and-forget task spawned here |
| `SearchService::search` | Integration point | Re-ranking step inserted here |
| `COUNTERS` table | Idempotency marker | `bootstrap_nli_promotion_done` key |

---

## NOT in Scope

The following are explicitly excluded from crt-023. Any work touching these areas is
a scope variance.

- **GGUF integration (W2-4)**: separate rayon pool, separate model, separate feature
- **GNN training (W3-1)**: crt-023 produces NLI confidence scores in `metadata`; it does
  not train or invoke any GNN
- **Automated CI gate for eval results**: eval is human-reviewed (nan-007 Non-Goal);
  no automated pipeline check for eval output
- **Multi-model NLI ensemble**: one model, one session, one `Mutex<Session>`. No ensemble
- **Full `scan_contradictions` upgrade**: the background scan over all active entries
  continues to use the cosine heuristic. Only the incremental post-store path and bootstrap
  promotion use NLI
- **Exposing NLI scores via MCP tool response**: NLI is internal signal only; no new
  MCP tool fields
- **`context_search` response schema changes**: field names and types are unchanged;
  ordering changes but callers see the same response shape
- **New `unimatrix-onnx` crate**: deferred to before W3-1 per crt-022a architect
  consultation
- **Removing `conflict_heuristic`**: the lexical cosine heuristic remains as graceful
  degradation fallback; it is not deleted
- **`GRAPH_EDGES` schema changes**: no migration; schema v13 from crt-021 is used as-is
- **Length-prefix injection or content scanning beyond truncation**: 512-token truncation
  is the only input sanitization applied; a fuller content-scanner pass is not in scope
- **Blended rerank formula**: D-02 resolves to pure replacement; blending is a
  follow-on feature if eval warrants it
- **`nli-deberta-v3-small` as forced primary**: deberta is an eval-validated alternative
  only; MiniLM2 is the primary model unless eval results and ONNX availability support
  deberta selection
- **Symmetric bi-encoder-then-NLI contradiction scan**: the full `scan_contradictions`
  all-entries scan is not upgraded here

---

## Open Questions for Architect

These are unresolved at specification time and must be addressed in the architecture
before or during Phase 1 implementation.

**OQ-01 (SR-02 / SR-03 — Pool sizing and session concurrency)**: Must pool sizing be
raised when NLI is enabled? Is the single `Mutex<Session>` (ADR-001) acceptable given
that 20-pair re-ranking holds the mutex for up to ~4s at worst case, or is a session
pool needed? This is the highest-risk design decision per the risk assessment and must
be captured as an ADR.

**OQ-02 (SR-09 — Embedding handoff)**: How does the embedding computed during
`context_store` reach the fire-and-forget NLI task without recomputation? What is the
ownership/lifetime contract in async Rust for moving a `Vec<f32>` into a spawned tokio
task that then moves it into a rayon closure? The architecture must make this explicit.

**OQ-03 (SR-07 — Bootstrap promotion idempotency mechanism)**: FR-24 specifies a
`COUNTERS` table key `bootstrap_nli_promotion_done`. The architect must confirm the
`COUNTERS` table supports string keys or specify the exact storage mechanism. Do not
leave this as "runs once on first tick" without the durable marker.

**OQ-04 (SR-08 — Eval CLI missing-model behavior)**: Should `unimatrix eval run`
skip a profile whose `nli_model_path` is absent and continue (producing partial results),
or should it fail fast? NFR-12 specifies the skip-with-warning behavior, but the CLI
design must be confirmed in the architecture.

**OQ-05 (SR-01 — Deberta ONNX availability)**: Must be verified as the first
implementation step. If deberta ONNX is unavailable, the `NliDebertaV3Small` enum
variant is still implemented (for future use) but the 3-profile eval degrades to
2-profile. Document the finding in the ADR before finalizing model selection.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "NLI cross-encoder search reranking ONNX provider pattern" -- Findings: ADR-001 (entry #67/#19) `Mutex<Session>` for ONNX inference is the established pattern; ADR-002 (entry #82/#34) lazy embedding init mirrors NliServiceHandle design; lesson #685 confirms degraded-mode requirement for MCP embedding init failure.
- Queried: `/uni-query-patterns` for "circuit breaker contradiction auto-quarantine rate limit edges" -- Findings: entry #1544 (ADR-002 crt-018b: hold-not-increment on background tick error) and entry #1542 (background tick writers error semantics for consecutive counters) are directly applicable to the circuit breaker and bootstrap promotion designs.
- Queried: `/uni-query-patterns` for "EmbedServiceHandle state machine loading ready failed retry" -- Findings: confirmed the degraded-mode and lazy-init patterns; no existing NliServiceHandle pattern to reference (novel component).
- No novel generalizable patterns identified at specification stage. ADR decisions from architecture phase (session pooling, pool sizing) will be stored after architect session.
