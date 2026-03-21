# Risk-Based Test Strategy: crt-023 — NLI + Cross-Encoder Re-ranking (W1-4)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Rayon pool starvation: 3+ concurrent NLI search re-ranking calls serialize through single `Mutex<Session>`; at 20 pairs × 200ms worst case, embedding inference for new queries starves on remaining threads | High | Med | Critical |
| R-02 | Pool floor raise race: `nli_enabled=true` raises floor to 6 at startup, but pool is initialized before `InferenceConfig::validate()` completes — if the floor logic is applied too late, NLI-enabled startup may run on a 4-thread pool until next restart | High | Low | High |
| R-03 | NLI score tie-breaking instability: when multiple candidates return identical entailment scores (e.g., 0.33 on uniform-distribution terse entries), the sort is unstable — result ordering is nondeterministic across calls | High | Med | Critical |
| R-04 | `MCP_HANDLER_TIMEOUT` fires mid-batch: `spawn_with_timeout` cancels a 20-pair NLI batch after 30s; the rayon task continues running (no cooperative cancellation), occupying a pool thread and holding the `Mutex<Session>` — next search call's `get_provider()` may block | High | Low | High |
| R-05 | Hash verification absent in production: `nli_model_sha256` is `Option<String>` with no default — a production deployment omitting this field silently skips model integrity verification; a tampered model runs undetected | High | Med | Critical |
| R-06 | Partially-written model file: if the model download is interrupted (network loss, OOM during download), a truncated ONNX file reaches `Session::builder().commit_from_file()`; the session panics or returns a corrupt tensor error rather than a clean `Failed` transition | High | Low | High |
| R-07 | Embedding move-before-HNSW-insert: if the insert pipeline order is refactored and the embedding `Vec<f32>` is consumed (moved into an intermediate step) before the ADR-004 hand-off point, the fire-and-forget task receives an empty or recomputed embedding — HNSW search returns wrong neighbors | High | Low | High |
| R-08 | Fire-and-forget task owns a stale embedding after HNSW insert failure: SQL insert succeeds, HNSW insert fails, NLI task is spawned with the embedding but the entry has no HNSW node — neighbor search returns 0 results, task silently writes no edges, entry is forever orphaned from the graph | Med | Low | Med |
| R-09 | Circuit breaker applies to total edges, not only Contradicts edges: the cap `max_contradicts_per_tick` is named for Contradicts but FR-22 applies it to all edges (Supports + Contradicts combined); if implementation counts only Contradicts toward the cap, Supports edges are unlimited and the cap does not protect against flooding | High | Med | Critical |
| R-10 | NLI miscalibration cascade: a single `context_store` call scores 10 neighbors all above `nli_contradiction_threshold` (NLI is wrong); all 10 Contradicts edges are written (at cap); on next tick, the graph penalty depresses all 10 affected entries in search; if any cross the auto-quarantine threshold, a cascade begins | High | Med | Critical |
| R-11 | Bootstrap promotion partial failure: the transaction committing the last batch of DELETE/INSERT operations fails (write pool contention — entry #2130); the completion marker is not set; on next tick, the task re-runs and re-processes rows that were already deleted — `INSERT OR IGNORE` is safe but re-processing empty source entries is not | Med | Med | High |
| R-12 | Bootstrap promotion runs before HNSW is warm: if the background tick fires before HNSW loads from disk, the source/target entry lookups succeed but neighbor context is wrong — NLI scores pairs correctly but edge weights reflect cold-index state | Med | Low | Med |
| R-13 | `NliServiceHandle` mutex poisoning not detected between calls: if a rayon thread panics inside `score_batch` holding the `Mutex<Session>`, subsequent `get_provider()` calls succeed (returning the `Arc<NliProvider>`), but the very next `score_batch` call immediately gets `PoisonError` — the handle stays in `Ready` state while the session is permanently unusable | High | Low | High |
| R-14 | Eval profile SKIPPED misread as gate pass: when both NLI candidate profiles are SKIPPED (model absent), only the baseline runs; per ADR-006, the eval gate is "effectively waived" — but AC-29/FR-29 require AC-01 to still pass; if the test harness only checks `eval report` output, AC-01 may not be verified independently | Med | Med | High |
| R-15 | `nli_model_name` unrecognized string reaches `NliModel::from_config_name` at runtime rather than at `validate()`: if validation is called after `NliServiceHandle` construction begins, the handle starts loading with `None` model, silently falls back to MiniLM2 default, and the config error is swallowed | Med | Med | High |
| R-16 | Post-store NLI writes via `write_pool_server()` contend with background tick writers: both the post-store fire-and-forget task and the background tick (graph rebuild, co-access cleanup) use the write pool; at `max_connections=1` (entry #2130), the post-store task blocks until the tick completes — up to several seconds | Med | Med | High |
| R-17 | Status penalty applied before NLI scoring creates invisible depressed scores: a deprecated entry receives a 0.7× multiplier before NLI scoring (ADR-002 spec); if the multiplier is applied to the entailment score rather than as a post-sort modifier, a relevant-but-deprecated entry may rank below irrelevant active entries | Med | Med | High |
| R-18 | Deberta tokenizer incompatibility: `NliDebertaV3Small` uses a different tokenizer (SentencePiece + DeBERTa-specific) than MiniLM2 (WordPiece BPE); if the same `Tokenizer` loading path is used for both variants without tokenizer-type dispatch, deberta inference produces garbage logits silently | High | Low | High |
| R-19 | Input truncation boundary: 512-token truncation is applied per-side before pair concatenation; cross-encoder models concatenate `[CLS] query [SEP] passage [SEP]` — the combined sequence may still exceed the model's max-position-embedding limit (512 for MiniLM2) after per-side truncation if the query itself is 512 tokens and the passage is 1 token | Med | Med | High |
| R-20 | `INSERT OR IGNORE` idempotency masks duplicate detection: if the same (source_id, target_id, Contradicts) edge already exists from the bootstrap (bootstrap_only=1), the NLI write is silently dropped by `IGNORE` — no NLI-confirmed edge is written, no update to `bootstrap_only` or metadata — the bootstrap edge persists with stale quality markers | Med | Med | High |
| R-21 | Eval harness latency measurement conflates NLI batch time with search pipeline overhead: the eval run measures end-to-end `context_search` latency; if the rayon pool is busy with post-store NLI tasks during eval scenario replay, the p95 latency measurement inflates from interference rather than from the search re-ranking itself | Med | Low | Med |
| R-22 | `sha2` crate absent from `unimatrix-embed` dependencies: SHA-256 hash verification is in `NliServiceHandle` (server crate); if `sha2` is not in `unimatrix-server/Cargo.toml`, the build fails silently on the first hash verification call rather than at compile time | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Rayon Pool Starvation Under Concurrent NLI Load
**Severity**: High
**Likelihood**: Med
**Impact**: Embedding inference for new MCP requests queues behind NLI batches; p99 `context_search` latency spikes; `MCP_HANDLER_TIMEOUT` fires on embedding steps, not just NLI steps — searches fail rather than degrade gracefully.

**Historical evidence**: Entry #735 ("spawn_blocking pool saturation from unbatched fire-and-forget DB writes") confirms the pattern: shared pools saturate when new workload categories are added without accounting for existing occupants.

**Test Scenarios**:
1. Spawn 3 concurrent `context_search` tasks with NLI active, each requiring 20-pair batch scoring; assert all three complete (possibly via fallback) without timeout propagating to the embedding step.
2. While 2 NLI search batches are in-flight, call `context_store` (triggering a fire-and-forget NLI detection task) and assert the `context_store` MCP response returns before the NLI task completes.
3. Run the pool at `nli_enabled=true` floor=6; confirm `pool_size()` returns >= 6 before any inference begins.

**Coverage Requirement**: Load test at 3 concurrent search calls with NLI active. Verify embedding path never starves (i.e., a non-NLI `context_search` issued concurrently with 3 NLI searches completes within 2× single-call baseline latency).

---

### R-02: Pool Floor Raise Race at Startup
**Severity**: High
**Likelihood**: Low
**Impact**: Server starts with 4-thread pool when NLI expects 6-thread minimum; the first burst of NLI + embedding requests saturates the pool and all subsequent requests queue.

**Test Scenarios**:
1. Start server with `nli_enabled=true`; call `AppState::rayon_pool().pool_size()` immediately after startup (before any inference); assert result >= 6.
2. Start server with `nli_enabled=false`; assert pool size is the formula default (not raised to 6).

**Coverage Requirement**: Unit test on startup config resolution: pool floor is applied before `NliServiceHandle::start_loading()` is called.

---

### R-03: NLI Score Tie-Breaking Instability
**Severity**: High
**Likelihood**: Med
**Impact**: Nondeterministic search ordering for short/terse entries (ADRs, tag-heavy entries) that NLI scores uniformly at ~0.33; user observes different result ordering on repeated identical queries — undermines trust in search consistency.

**Historical evidence**: Entry #724 ("Behavior-based ranking tests: assert ordering not scores") establishes the test pattern — tests must assert relative ordering between known-better and known-worse pairs, not exact scores.

**Test Scenarios**:
1. Create two entries where entry A demonstrably entails the query (narrative text) and entry B is a terse 3-word tag entry; assert A ranks above B on every call with NLI active.
2. Inject a mock `CrossEncoderProvider` returning identical scores for all candidates; assert the result ordering is deterministic across 10 repeated calls (i.e., sort is stable).
3. Test boundary: NLI scores at exactly `nli_entailment_threshold` (0.6); assert whether a Supports edge is written (boundary is `>`, not `>=` per FR-18 — verify the strict inequality is implemented).

**Coverage Requirement**: Stable sort guarantee: when NLI scores are equal, secondary sort key (original HNSW rank or entry ID) must produce deterministic ordering. Test must fail if `sort_unstable_by` is used without a deterministic tiebreaker.

---

### R-04: MCP_HANDLER_TIMEOUT Fires Mid-Batch
**Severity**: High
**Likelihood**: Low
**Impact**: The cancelled rayon task continues holding the `Mutex<Session>` for the remainder of its batch; the next `context_search` call's `spawn_with_timeout` for NLI blocks at mutex acquisition, not at ONNX inference — the timeout fires on the mutex wait, not on inference, producing confusing logs and potentially double-timeout delays.

**Test Scenarios**:
1. Mock `CrossEncoderProvider` with a 35s delay; call `SearchService::search` with `MCP_HANDLER_TIMEOUT=30s`; assert the search returns cosine-fallback results within 31s (not 65s).
2. After the timeout, call `NliServiceHandle::get_provider()` and assert it returns `Ok` (not `Failed`); the handle must not transition to Failed on a timeout-cancelled task.
3. Call `SearchService::search` again immediately after the timeout; assert the second call succeeds (mutex is eventually released by the rayon thread completing its work).

**Coverage Requirement**: Timeout-then-fallback path is tested with a slow mock provider. Verify `Mutex<Session>` is not permanently locked after a rayon timeout.

---

### R-05: Hash Verification Absent in Production
**Severity**: High
**Likelihood**: Med
**Impact**: A model file replaced by an attacker (or accidentally overwritten) is loaded and used for inference without any integrity check; adversarial model outputs corrupt search ranking and GRAPH_EDGES silently.

**Test Scenarios**:
1. Start server with `nli_model_sha256 = None`; assert a `tracing::warn!` is emitted noting that hash verification is disabled (not just a silent skip).
2. Start server with a valid model file and correct hash; assert `NliServiceHandle` reaches `Ready`.
3. Start server with a valid model file but wrong 64-char hex hash; assert `NliServiceHandle` transitions to `Failed`, error log contains "security" and "hash mismatch", server continues serving cosine-fallback results.
4. Start server with `nli_model_sha256` set to a string that is not 64 hex chars; assert `InferenceConfig::validate()` aborts startup (AC-17).

**Coverage Requirement**: Hash mismatch must produce observable signal even when `nli_model_sha256` is absent. Security log must be present in the failure case. No test may assert `NliServiceHandle` reaches `Ready` without hash verification when a hash is present.

---

### R-06: Partial Model File Loaded Without Clean Failure
**Severity**: High
**Likelihood**: Low
**Impact**: ORT `Session::builder()` receives a truncated file and either panics (propagating through `spawn_blocking` as a `JoinError`), returns an unstructured error, or constructs a corrupted session that panics on first inference — none of these produce a clean `NliServiceHandle → Failed` transition.

**Test Scenarios**:
1. Write a 1KB file to the model path (simulating interrupted download); start server with that path; assert `NliServiceHandle` transitions to `Failed` (not panic, not hang).
2. Write a syntactically valid ZIP/ONNX header followed by garbage bytes; assert same `Failed` transition.
3. Verify that a `Failed` transition from a corrupt file triggers the retry sequence (up to MAX_RETRIES=3) and ultimately stays `Failed` without crashing.

**Coverage Requirement**: All `Session::builder()` failure modes must be caught within `NliServiceHandle`'s loading task, not propagated as panics to the tokio runtime.

---

### R-07: Embedding Consumed Before ADR-004 Hand-off Point
**Severity**: High
**Likelihood**: Low
**Impact**: The fire-and-forget task receives a zero-length `Vec<f32>` or a moved-out binding; HNSW neighbor search with an empty vector returns 0 results; no NLI edges are written; the failure is silent with no log.

**Historical evidence**: ADR-004 explicitly states "A comment in `store_ops.rs` must note the NLI hand-off dependency" — the risk is that a future refactor removes the comment and reorders steps.

**Test Scenarios**:
1. Integration test: call `context_store`; await the fire-and-forget task's completion (via a test-only channel or by polling `GRAPH_EDGES`); assert at least one NLI scoring call was made with a non-empty embedding (mock `CrossEncoderProvider` records call count).
2. Unit test on `run_post_store_nli`: call with an empty `Vec<f32>` as embedding; assert the function logs a warning and returns without calling the NLI provider.
3. Regression guard: assert the `embedding` binding is not referenced after the `let embedding_for_nli = embedding;` hand-off (compile-time ownership enforcement — borrow checker guarantees this if correctly structured).

**Coverage Requirement**: Integration test must verify the NLI task receives and uses the same embedding that was inserted into HNSW (compare by `vector_index.search(embedding, 1)` result against the inserted entry ID).

---

### R-08: HNSW Insert Failure — Orphaned Entry With No NLI Edges
**Severity**: Med
**Likelihood**: Low
**Impact**: The new entry exists in the SQL store but has no HNSW node. The NLI fire-and-forget task runs, finds 0 HNSW neighbors, writes no edges. The entry never participates in the contradiction graph. This is silent; no error is returned.

**Test Scenarios**:
1. Mock `VectorIndex::insert_hnsw_only` to return an error; call `context_store`; assert `StoreService::insert` still returns `Ok` (HNSW failure is non-fatal); assert a `tracing::warn!` is logged noting HNSW insert failure.
2. Assert that when HNSW insert fails, the NLI fire-and-forget task is still spawned but exits cleanly after finding 0 neighbors (logged at debug level, not error level).

**Coverage Requirement**: HNSW failure path must be documented in test comments as an intentional silent-degradation case, not a missed error handler.

---

### R-09: Circuit Breaker Counts Only Contradicts, Not All Edges
**Severity**: High
**Likelihood**: Med
**Impact**: `max_contradicts_per_tick = 10` (named for Contradicts) is interpreted by the implementer as applying only to Contradicts edges; Supports edges are unlimited. A single `context_store` call with 10 high-similarity neighbors writes 10 Supports + 10 Contradicts = 20 edges. The cap does not protect against flooding.

**Test Scenarios**:
1. Set `max_contradicts_per_tick = 2`; mock `CrossEncoderProvider` to return scores above both thresholds for all 5 neighbors (each pair produces both a Supports and a Contradicts candidate); assert exactly 2 total edges written to `GRAPH_EDGES`, regardless of type.
2. Set `max_contradicts_per_tick = 3`; arrange 2 Supports + 2 Contradicts candidates; assert exactly 3 edges written (whichever 3 are processed first by the pipeline iteration order).
3. Assert the `tracing::debug!` log for dropped edges appears with the correct count.

**Coverage Requirement**: AC-13 integration test must parametrize over both edge types in the cap enforcement. Test must fail if cap is applied to only one relation type.

---

### R-10: NLI Miscalibration Cascade Into Auto-Quarantine
**Severity**: High
**Likelihood**: Med
**Impact**: A miscalibrated NLI model (or adversarial content in a stored entry) produces 10 false Contradicts edges in one `context_store` call. On the next background tick, the graph penalty is applied to all 10 affected entries; if any meet the auto-quarantine threshold (from crt-018b, entry #1544), they are quarantined. Search results are severely degraded for legitimate knowledge.

**Test Scenarios**:
1. Store one entry; mock `CrossEncoderProvider` to return `contradiction=0.99` for all 10 neighbors; assert exactly `max_contradicts_per_tick` Contradicts edges are written (cap enforced).
2. With those edges in place, run the background tick; assert no auto-quarantine fires for any entry (because 10 edges alone should not meet the auto-quarantine threshold — verify the threshold is not unreachable).
3. Verify the hold-on-error behavior (entry #1542): if the background tick's auto-quarantine counter increments to its hold threshold, subsequent ticks do not increment further without manual reset.
4. Confirm `max_contradicts_per_tick = 1` effectively sandboxes a single noisy store call to one Contradicts edge.

**Coverage Requirement**: Cascade scenario must be tested end-to-end: store → NLI detection → GRAPH_EDGES write → tick → graph penalty applied → assert auto-quarantine does NOT fire below the documented threshold. This test documents the threshold contract.

---

### R-11: Bootstrap Promotion Partial Transaction Failure
**Severity**: Med
**Likelihood**: Med
**Impact**: Write pool contention (entry #2130: max_connections=1) causes the transaction covering the final batch of DELETE/INSERT operations to fail; the completion marker is not set; on the next tick, all rows are re-processed — rows already deleted are gone (HNSW search would have found them if they were `Supports` edges), and the task may create duplicate NLI edges for rows already successfully promoted.

**Test Scenarios**:
1. Inject a write pool error on the final `set_counter` call within the bootstrap promotion transaction; restart the task; assert no duplicate `GRAPH_EDGES` rows exist (idempotency via `INSERT OR IGNORE`).
2. Assert that if `bootstrap_nli_promotion_done = 1` is already in `COUNTERS`, re-running `maybe_run_bootstrap_promotion` returns immediately without querying `GRAPH_EDGES`.
3. Manually insert synthetic `bootstrap_only=1` Contradicts rows; run promotion; assert the completion marker is set and subsequent runs are no-ops (AC-24).
4. With zero `bootstrap_only=1` rows: run promotion; assert marker is set and the function returns `Ok` (zero-row case, AC-12a).

**Coverage Requirement**: Idempotency test: run promotion task twice (second run finds marker present); assert `GRAPH_EDGES` is identical after both runs.

---

### R-12: Bootstrap Promotion Runs Before HNSW Warmup
**Severity**: Med
**Likelihood**: Low
**Impact**: NLI scores the (source, target) pairs correctly using entry text, but if future code paths require HNSW neighbor lookups during promotion (e.g., weight recalculation), they return stale results from a cold index.

**Test Scenarios**:
1. Run `maybe_run_bootstrap_promotion` in a test where `VectorIndex` is initialized but not yet populated (cold); assert the promotion completes without error and scores pairs using text only (no HNSW dependency in the promotion path).
2. Confirm via code review that `run_bootstrap_promotion` does not call any HNSW search methods (promotion only reads entry text from the SQL store).

**Coverage Requirement**: Bootstrap promotion must not have any HNSW dependency. If a future code change adds one, this test must fail.

---

### R-13: Mutex Poison Not Detected Between Calls
**Severity**: High
**Likelihood**: Low
**Impact**: `NliServiceHandle` remains in `Ready` state after a rayon panic poisons the session mutex; subsequent callers receive `Ok(Arc<NliProvider>)` and immediately get `PoisonError` inside `score_batch` — this propagates as `RayonError::Cancelled`, which falls back to cosine. The handle never retries, so NLI is permanently degraded without transitioning to `Failed` or `Retrying`.

**Historical evidence**: Entry #770 ("non-reentrant mutex deadlock when lock is held while calling re-acquiring methods") highlights that mutex state after panics is a real failure mode in this codebase.

**Test Scenarios**:
1. Poison the `NliProvider`'s `Mutex<Session>` by calling `score_batch` with a mock that panics inside the mutex; assert the next `NliServiceHandle::get_provider()` call returns `Err(NliFailed)` (not `Ok`).
2. After poison detection, assert `NliServiceHandle` initiates the retry sequence (transitions to `Loading`/`Retrying`) and eventually reaches `Ready` again if the retry succeeds.
3. After retry exhaustion, assert `get_provider()` returns `Err(NliFailed)` and the server continues on cosine fallback.

**Coverage Requirement**: Poison detection must occur at the `get_provider()` boundary, not inside the NLI inference call. A `try_lock()` call in `get_provider()` is the required implementation pattern (ADR-001).

---

### R-14: Eval SKIPPED Profiles Misread as Gate Pass
**Severity**: Med
**Likelihood**: Med
**Impact**: A CI run where both NLI candidate profiles are SKIPPED produces a report showing only the baseline; reviewers see "zero regressions" (trivially true) and approve the gate without any NLI quality evidence. FR-29 requires AC-01 to pass independently, but if that test is not run alongside the eval, the gate is vacuously satisfied.

**Test Scenarios**:
1. Run `eval run` with an NLI candidate profile where the model is absent; assert the report contains a `SKIPPED` entry for that profile with the reason string "NLI model not available" (or equivalent).
2. Assert the eval run exit code is non-zero when all candidate profiles are SKIPPED and no baseline-vs-candidate comparison is possible (making the gate waiver explicit).
3. Confirm AC-01 (unit test for `score_pair` sum constraint) is present and passes independently of the eval gate path.

**Coverage Requirement**: Eval report must be machine-parseable for SKIPPED status. Gate waiver documentation in delivery report is required by FR-29; test suite must verify AC-01 independently.

---

### R-15: Invalid `nli_model_name` Reaches Runtime
**Severity**: Med
**Likelihood**: Med
**Impact**: `InferenceConfig::validate()` is called after `NliServiceHandle` construction begins; the unrecognized name silently resolves to the default (MiniLM2) or causes a `None`-unwrap panic inside `start_loading()`.

**Test Scenarios**:
1. Set `nli_model_name = "gpt4"` in config; assert `InferenceConfig::validate()` returns an error naming the field `nli_model_name` before `NliServiceHandle::start_loading()` is called.
2. Assert the startup abort message includes the invalid value (`"gpt4"`) so the operator can diagnose the config error without reading source code.
3. Assert that `NliModel::from_config_name("gpt4")` returns `None` (not a panic).

**Coverage Requirement**: AC-17 parametric test must include `nli_model_name` as one of the validated fields.

---

### R-16: Post-Store NLI Write Contention on SQLite Write Pool
**Severity**: Med
**Likelihood**: Med
**Impact**: Fire-and-forget post-store NLI tasks write to `GRAPH_EDGES` via `write_pool_server()`. At `max_connections=1` (the WAL write pool constraint, entry #2130), a burst of concurrent `context_store` calls produces N concurrent NLI tasks, each competing for the single write connection. Tasks queue; some may wait several seconds. If the write pool has a timeout, tasks fail silently and write no edges.

**Test Scenarios**:
1. Call `context_store` 5 times in rapid succession; await all fire-and-forget tasks; assert all expected `GRAPH_EDGES` rows are present (no rows lost to write pool timeout or SQLITE_BUSY).
2. Assert that write pool contention in the NLI task does not propagate an error back to the original `context_store` MCP response (the fire-and-forget is truly decoupled).
3. Verify the write pool uses `INSERT OR IGNORE` — a duplicate write attempt on the same edge does not produce an error (idempotency under retried tasks).

**Coverage Requirement**: Burst store test with at least 5 concurrent stores; verify `GRAPH_EDGES` count at completion.

---

### R-17: Status Penalty Applied as Entailment Score Multiplier
**Severity**: Med
**Likelihood**: Med
**Impact**: ADR-002 specifies status penalty applies before NLI scoring as a "multiplicative modifier." If implemented as `nli_entailment *= status_penalty`, a deprecated entry with genuine entailment score 0.85 becomes 0.85 × 0.7 = 0.595 — below the `nli_entailment_threshold` for edge writing (0.6) and below less-relevant active entries. The result ordering violates the intended pipeline semantics.

**Test Scenarios**:
1. Insert a deprecated entry and an active entry; arrange their texts so the deprecated entry genuinely entails the query (high NLI score) and the active entry does not (low NLI score); call `context_search`; assert the deprecated entry still appears in results (penalized but present) and that the ordering reflects both the NLI signal and the status penalty per the pipeline spec (FR-14).
2. Assert that the status penalty does not affect the `NliScores` values stored in `GRAPH_EDGES` metadata — metadata must contain the raw NLI scores, not penalty-adjusted scores.
3. Review the pipeline ordering: penalty must be applied as a result-level rank modifier, not as a pre-inference input transformation.

**Coverage Requirement**: Behavior-based test per entry #724 pattern: assert result ordering relative to a known-deprecated but highly-relevant entry.

---

### R-18: Deberta Tokenizer Path Uses MiniLM2 Tokenizer Config
**Severity**: High
**Likelihood**: Low
**Impact**: `NliProvider` loads a tokenizer config from the model repo. If `ensure_nli_model` downloads the ONNX file for deberta but uses the MiniLM2 tokenizer (or vice versa), `score_batch` produces garbage logits with no runtime error — softmax of garbage is still a valid probability distribution.

**Test Scenarios**:
1. Unit test: `NliModel::NliDebertaV3Small.cache_subdir()` must be distinct from `NliModel::NliMiniLM2L6H768.cache_subdir()` (verified via string inequality).
2. Integration test (when deberta is available): load deberta variant; call `score_pair` on a semantically obvious entailment pair; assert `entailment > 0.5` (would fail if tokenizer mismatch produces uniform-distribution garbage).
3. Assert `ensure_nli_model` downloads the tokenizer config from the same HuggingFace repo as the ONNX file (not from a hardcoded MiniLM2 path).

**Coverage Requirement**: Tokenizer-model pairing must be verified for each `NliModel` variant. If only MiniLM2 is available at implementation time, this test applies only to that variant — but the code path for deberta must be covered by a unit test asserting distinct paths.

---

### R-19: Combined Sequence Exceeds Model's Position Embedding Limit
**Severity**: Med
**Likelihood**: Med
**Impact**: Per-side truncation to 512 tokens is applied independently. The cross-encoder concatenates `[CLS] query [SEP] passage [SEP]` — if query is 511 tokens and passage is 1 token, the combined sequence is 514 tokens (511 + 1 + 3 special tokens), exceeding the 512-token position limit. ONNX runtime silently truncates or panics; output logits are for a truncated pair that does not match the intended (query, passage) relationship.

**Test Scenarios**:
1. Call `score_pair` with query = 511-token string and passage = 10-token string; assert no panic and `entailment + neutral + contradiction ≈ 1.0`.
2. Call `score_pair` with query = 512 tokens and passage = 512 tokens; assert no OOM, no panic, and valid `NliScores` returned.
3. Verify in `NliProvider` implementation that the combined tokenization length after per-side truncation is clamped to the model's `max_position_embeddings` limit (typically 512 for MiniLM2) before being passed to the ONNX session.

**Coverage Requirement**: Boundary tests at 511+1, 256+256, and 512+512 token combinations. All must return valid `NliScores`.

---

### R-20: `INSERT OR IGNORE` Silently Preserves Bootstrap Edge
**Severity**: Med
**Likelihood**: Med
**Impact**: A `bootstrap_only=1` Contradicts edge between (A, B) exists from W1-1. The post-store NLI task for a new entry adjacent to A scores (A, B) and attempts to write `source='nli', bootstrap_only=0` — but `INSERT OR IGNORE` on the `UNIQUE(source_id, target_id, relation_type)` constraint ignores the NLI write. The bootstrap edge survives with `bootstrap_only=1` and continues being excluded from confidence scoring.

**Test Scenarios**:
1. Insert a `bootstrap_only=1` Contradicts edge between entries A and B; trigger post-store NLI for an entry adjacent to both; assert that after NLI detection, the (A, B) edge has `bootstrap_only=0` and `source='nli'` (requires the post-store task to detect and UPDATE, not just INSERT OR IGNORE).
2. Alternatively, verify this scenario is handled exclusively by the bootstrap promotion task and the post-store task intentionally does not overwrite bootstrap edges (document which path handles it).
3. Assert the bootstrap promotion task correctly replaces bootstrap edges before post-store detection could conflict (sequencing guarantee).

**Coverage Requirement**: The interaction between post-store NLI and existing bootstrap edges must be explicitly tested and the intended behavior documented. If `INSERT OR IGNORE` is correct (bootstrap promotion handles the upgrade), the test must verify bootstrap promotion runs first on any database with bootstrap rows.

---

### R-21: Eval Latency Measurement Contaminated by Background NLI Tasks
**Severity**: Med
**Likelihood**: Low
**Impact**: During `eval run`, fire-and-forget NLI tasks from scenario store operations (if any) compete for rayon pool threads with the eval's search re-ranking. The p95 latency in the eval report reflects contention that would not exist in a read-only search workload. Eval gate comparison between baseline and NLI candidate is invalid if baseline p95 is measured under different load than candidate p95.

**Test Scenarios**:
1. Ensure the eval harness uses a read-only snapshot (`unimatrix snapshot` produces a read-only SQLite copy) — no `context_store` operations occur during eval scenario replay, eliminating fire-and-forget NLI task interference.
2. Document in the eval setup guide that the eval snapshot must be read-only; any mutation during eval invalidates the latency comparison.

**Coverage Requirement**: Specification test: assert `EvalServiceLayer` does not call `StoreService::insert` during scenario replay.

---

### R-22: `sha2` Crate Absent From Server Dependencies
**Severity**: Low
**Likelihood**: Med
**Impact**: SHA-256 hash verification requires `sha2` crate. If absent from `Cargo.toml`, the build fails only when the hash verification code path is compiled — which may be missed if the feature is guarded by a cfg flag.

**Test Scenarios**:
1. Build the workspace with `cargo build` (or `cargo check`); assert no missing dependency errors. This is caught at CI level, not a runtime test.
2. Verify `sha2` appears in `unimatrix-server/Cargo.toml` before hash verification code is written (implementation gate).

**Coverage Requirement**: Cargo dependency resolution test: `cargo tree -p unimatrix-server | grep sha2` must return a result.

---

## Integration Risks

**NLI inference + embedding inference on the same rayon pool**: The pool serves three concurrent workloads in crt-023: (1) search embedding via `spawn_with_timeout`, (2) NLI re-ranking via `spawn_with_timeout`, (3) post-store NLI detection via `spawn` (no timeout). The floor-6 mitigation (ADR-001) addresses average-case saturation. Worst-case: 3 concurrent re-ranking calls × 20 pairs × 200ms = 12s of pool occupancy; post-store tasks pile up behind them; a search-then-store sequence (common in MCP sessions) may experience 12s+ latency on the store's associated NLI detection even though the store MCP response returned immediately. Test: verify fire-and-forget NLI task queue does not grow unboundedly under sustained concurrent load.

**`write_pool_server()` in fire-and-forget NLI tasks vs background tick**: Both paths write to `GRAPH_EDGES` via the single-connection write pool (entry #2130). The background tick (graph rebuild, auto-quarantine) holds the write pool for extended periods. Post-store NLI edge writes contend for the same connection. The `INSERT OR IGNORE` is idempotent under retries, but a task that fails to acquire the write pool within its timeout window silently drops edges. Test: verify edge write failures in the fire-and-forget task are logged at `warn!` level, not silently dropped.

**`EvalServiceLayer` stub fill-in and existing `from_profile()` callers**: Filling the W1-4 stub adds `NliServiceHandle` construction and 60s readiness wait to `from_profile()`. Any existing test that calls `from_profile()` without a model file will now wait 60s before getting a SKIPPED result. Test: verify `from_profile()` with `nli_enabled=false` completes immediately (no readiness wait).

**GRAPH_EDGES write path divergence**: Post-store NLI uses `write_pool_server()` directly (SR-02). Bootstrap promotion also uses `write_pool_server()` directly. These two paths must not both run concurrently on the same (source_id, target_id, Contradicts) edge during the startup window where bootstrap promotion has not yet completed. Test: if NLI is ready before the bootstrap promotion marker is set, a `context_store` for an entry adjacent to a bootstrap-only edge could race with the promotion task. Verify `INSERT OR IGNORE` handles this race without creating duplicate rows.

---

## Edge Cases

- **Empty candidate pool after quarantine filter**: HNSW retrieves `nli_top_k=20` candidates; all 20 are quarantined or filtered. NLI batch receives an empty pairs list. `score_batch(&[])` must return `Ok(vec![])`, not an ORT session error from a zero-batch inference call.
- **Single-word query vs full document passage**: NLI model behavior on extreme length asymmetry. Per-side truncation allows a 1-token query paired with a 512-token passage. Test: assert valid `NliScores` for this case.
- **`nli_top_k` smaller than the requested top-K results**: If `nli_top_k=5` but the caller requests `k=10`, the re-ranking pool has only 5 candidates. After re-ranking and truncation to top-5, the response contains fewer results than requested. Assert the response is valid (fewer results, not an error).
- **UTF-8 multibyte characters and token count divergence**: "~2000 chars" and "512 tokens" may fire at very different string lengths for CJK or emoji-heavy input. The truncation must enforce whichever fires first. Test: 1000-token CJK string that is under 2000 bytes — must be truncated at token count, not byte count.
- **Softmax overflow for extreme logits**: The ONNX model produces raw logits; softmax is applied in `NliProvider`. If logits are very large (e.g., [100.0, -50.0, -50.0]), naive softmax overflows to inf/NaN before normalization. Test: inject a mock session returning extreme logits; assert softmax produces a valid probability vector (not NaN).
- **`max_contradicts_per_tick = 1` with a pair above both thresholds**: A pair may simultaneously satisfy `entailment > threshold` AND `contradiction > threshold` (if both thresholds are set very low). The pipeline would write both a Supports and a Contradicts edge for the same pair. With `max_contradicts_per_tick=1`, only one edge is written. Which type takes priority? Assert the priority order is documented and consistently implemented.
- **Bootstrap promotion task deferral across server restarts**: NLI takes 30s to load; the server restarts every 25s (restart loop); the bootstrap promotion task is perpetually deferred. Test: verify the task eventually runs when NLI becomes ready regardless of restart count (the counter check + NLI readiness check handles this correctly).

---

## Security Risks

**Untrusted input via `context_store` content field**: Any MCP caller can store arbitrary text. This text becomes the "passage" in NLI inference pairs. Adversarial inputs designed to maximize OOM (extremely long sequences) or cause ONNX session panics are the primary attack surface.
- **Blast radius**: A crafted passage that causes an ONNX panic poisons the `Mutex<Session>`, triggering `NliServiceHandle → Failed` + retry. During retry (backoff period), all search re-ranking falls back to cosine. A sustained attack could keep NLI permanently in the retry cycle.
- **Mitigation adequacy**: Per-side 512-token / ~2000-char truncation (NFR-08, FR-06) must be enforced in `NliProvider` itself, not at call sites. Test: verify truncation is applied inside `NliProvider.score_batch()` before the `Mutex<Session>` is acquired — an oversized input must never reach the ONNX runtime.
- **Test scenario**: Store an entry with `content` = 100,000 characters; trigger post-store NLI; assert `NliServiceHandle` remains in `Ready` state after the task completes (no panic, no poisoning).

**Model integrity (SHA-256 pinning absent)**: As identified in R-05, absent hash verification allows model substitution attacks. The attack surface extends to the model download CLI: `unimatrix model-download --nli` downloads from HuggingFace Hub over HTTPS. A MITM or HF compromise could substitute the model file. Hash pinning is the operator's defense; the tool must make it straightforward to pin. Test: verify the hash printed by `model-download --nli` matches the actual file SHA-256 (not a cached or hardcoded value).

**Prompt injection via NLI pair construction**: NLI cross-encoders are not generative models; they classify. Prompt injection in the classical sense does not apply. However, if a stored entry contains structured text designed to shift the NLI score toward `contradiction` for any query, it could artificially generate Contradicts edges. The `max_contradicts_per_tick` circuit breaker is the primary mitigation. Test: verify the cap prevents a single crafted entry from generating more than `max_contradicts_per_tick` Contradicts edges.

**GRAPH_EDGES metadata column injection**: The `metadata` column stores `{"nli_entailment": f32, "nli_contradiction": f32}` as a JSON string. The f32 values come from ONNX softmax output — they are numeric, not user-controlled strings. No SQL injection risk in the metadata value. However, if `metadata` is ever rendered or parsed by downstream tooling (e.g., W3-1 GNN), it must remain valid JSON. Test: assert `serde_json::to_string` is used for metadata serialization (not string concatenation).

---

## Failure Modes

**NLI model absent or hash-invalid at startup**: `NliServiceHandle` transitions to `Failed`; one `tracing::warn!` emitted; all MCP tools operate on cosine fallback. Expected: no user-visible error, no MCP tool failures, no startup abort.

**NLI model loading in progress (Loading/Retrying state)**: `get_provider()` returns `Err(NliNotReady)`; search falls back to `rerank_score`; post-store task exits immediately. Expected: correct results (cosine-ranked), no errors to callers.

**ONNX session panic during inference**: Rayon thread panics; `RayonError::Cancelled` propagated to the tokio task; search uses cosine fallback for that request; mutex is poisoned; next `get_provider()` detects poison, transitions to `Failed`, initiates retry. Expected: single-request degradation, then automatic recovery after retry window.

**Write pool timeout during NLI edge write**: Fire-and-forget task fails to acquire write pool within timeout; edge write is logged at `warn!` level and dropped. Expected: `GRAPH_EDGES` is not updated for that store call; no MCP error; no retry (fire-and-forget).

**Background tick collision with bootstrap promotion**: The tick's graph rebuild reads `GRAPH_EDGES` via the read pool (Arc<RwLock<TypedRelationGraph>>); the bootstrap promotion writes to `GRAPH_EDGES` via the write pool. These do not conflict (separate connections, WAL mode). The graph rebuild picks up the promoted edges on the next tick. Expected: one-tick delay in bootstrap edges becoming active in the graph.

**`context_search` timeout during NLI batch**: `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` fires; the search result is the cosine-ranked fallback; the MCP response is returned without error. The rayon task continues and eventually completes (no cooperative cancellation). Expected: correct (cosine-ranked) results returned within the timeout window.

**Eval model file absent on CI**: `EvalServiceLayer::from_profile()` waits 60s; handle reaches `Failed`; profile is marked SKIPPED; eval report contains SKIPPED entry; exit code reflects partial results. Expected: baseline profile still produces results; SKIPPED is visible to reviewer.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Deberta ONNX unavailable | R-18 | ADR-003 implements `NliDebertaV3Small` unconditionally; if ONNX absent, `NliServiceHandle → Failed`; 2-profile eval is documented as valid fallback |
| SR-02: Shared rayon pool saturation | R-01, R-02, R-04 | ADR-001 raises pool floor to 6 when NLI enabled; `spawn_with_timeout` fallback on timeout; R-01 coverage requires load test confirming embedding path does not starve |
| SR-03: `Mutex<Session>` serializes 20-pair batch | R-04, R-13 | ADR-001 accepts serialization as adequate for sequential MCP workload; R-04 covers timeout behavior; R-13 covers poison detection |
| SR-04: Eval gate waivable for zero-history deployments | R-14 | ADR-006 SKIPPED behavior; FR-29 mandates AC-01 still passes under waiver; R-14 tests the waiver documentation requirement |
| SR-05: NLI score regression on terse entries | R-03, R-17 | ADR-002 accepts pure replacement with eval gate as guard; R-03 tests stable sort under uniform scores; R-17 tests status penalty pipeline ordering |
| SR-06: `max_contradicts_per_tick` per-call vs per-tick ambiguity | R-09 | FR-22 resolves: per-call semantic; AC-23 tests the cap is per-call; R-09 tests all edge types counted toward cap |
| SR-07: Bootstrap promotion non-idempotent | R-11, R-12 | ADR-005 uses COUNTERS table durable marker; R-11 covers transaction failure + re-run; R-12 verifies no HNSW dependency |
| SR-08: Eval CI missing model | R-14 | ADR-006 skip-not-fail behavior; R-14 tests SKIPPED annotation and AC-01 independence |
| SR-09: Embedding handoff contract | R-07, R-08 | ADR-004 move semantics after HNSW insert; R-07 tests correct embedding reaches NLI task; R-08 covers HNSW failure silent degradation |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01, R-03, R-05, R-09, R-10) | 15 scenarios minimum; load test mandatory for R-01; cascade test mandatory for R-10 |
| High | 12 (R-02, R-04, R-06, R-07, R-11, R-13, R-14, R-15, R-16, R-17, R-18, R-19) | 3 scenarios each minimum; mutex poison test (R-13) and tokenizer pairing test (R-18) are not optional |
| Med | 5 (R-08, R-12, R-20, R-21, R-22) | 2 scenarios each; R-20 (bootstrap vs post-store conflict) must be documented even if tested only via code review |
| Low | 0 (R-22 reclassified Med above) | — |

**Non-negotiable tests** (feature must not ship without them):
1. R-01: Concurrent load test at 3 simultaneous NLI searches — pool saturation validation
2. R-03: Stable sort under identical NLI scores — tie-breaking determinism
3. R-05: Hash mismatch → `Failed` + "security"+"hash mismatch" in logs + cosine fallback confirmed
4. R-09: Cap enforcement across both Supports and Contradicts edge types
5. R-10: Miscalibration cascade end-to-end: store → edges → tick → no auto-quarantine
6. R-13: Mutex poison detection at `get_provider()` boundary → `Failed` → retry

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #1203, #1204 (gate validation cascading rework, test plan pseudocode cross-reference); #683 (gate reports must be committed before signal).
- Queried: `/uni-knowledge-search` for "risk pattern ONNX rayon pool contention MCP timeout" — found entry #735 (spawn_blocking pool saturation from fire-and-forget — directly elevated R-01 and R-16 severity); entry #1367 (spawn_blocking_with_timeout for mutex acquisition — confirms timeout pattern); entry #2700 (ADR-001 crt-023 session concurrency — confirms architectural decision).
- Queried: `/uni-knowledge-search` for "outcome rework ONNX embedding contradiction" — found entry #1542 (background tick error semantics — elevated R-11); entry #685 (MCP embedding init failure degraded mode — confirmed R-13 importance).
- Queried: `/uni-knowledge-search` for "SQLite write pool contention graph edges integrity write" — found entry #2130 (write_pool max_connections=1 to prevent SQLITE_BUSY — directly created R-16); entry #2270 (dual-pool WAL architecture — confirmed write pool constraint is architectural invariant).
- Queried: `/uni-knowledge-search` for "confidence scoring search reranking regression eval harness" — found entry #724 (behavior-based ranking tests assert ordering not scores — applied to R-03 and R-17 test design).
- Queried: `/uni-knowledge-search` for "Mutex poison EmbedServiceHandle state machine retry" — found entry #770 (non-reentrant mutex deadlock — elevated R-13 to High); entry #685 (degraded mode requirement).
- Stored: nothing novel to store at this time — R-16 (SQLite write pool contention from fire-and-forget NLI tasks) and R-09 (circuit breaker edge-type specificity) are crt-023-specific. Pattern for "shared inference pool + background fire-and-forget edge writes combining under single write pool" will be stored after implementation reveals whether this materializes as a real issue.
