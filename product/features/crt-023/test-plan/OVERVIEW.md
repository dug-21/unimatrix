# Test Plan Overview: crt-023 — NLI + Cross-Encoder Re-ranking (W1-4)

## Test Strategy

crt-023 introduces ONNX NLI inference into two hot paths (search re-ranking, post-store
detection) and three background paths (bootstrap promotion, auto-quarantine threshold,
eval gate). The test strategy is organized in three tiers:

**Tier 1 — Unit tests** (unimatrix-embed, unimatrix-server): Fast, mock-able, deterministic.
Cover `NliProvider` inference correctness, `NliModel` enum methods, config validation, and
`NliServiceHandle` state machine transitions via controlled stubs.

**Tier 2 — Integration unit tests** (within cargo test, using test fixtures): Cover the
inter-component wiring — post-store fire-and-forget task, bootstrap promotion pipeline,
circuit breaker enforcement, and auto-quarantine threshold logic. Use mock
`CrossEncoderProvider` implementations to make ONNX optional.

**Tier 3 — Integration harness tests** (infra-001): Validate end-to-end behavior through
the MCP JSON-RPC interface. NLI is exercised where the model is present; graceful
degradation is tested in all configurations.

## Risk-to-Test Mapping

| Risk ID | Priority | Description (short) | Test Location | Test Function(s) |
|---------|----------|---------------------|---------------|-----------------|
| R-01 | Critical | Rayon pool starvation under 3 concurrent NLI searches | nli-service-handle.md + harness lifecycle | `test_concurrent_nli_search_pool_saturation`, `test_nli_search_concurrent_embedding_not_starved` |
| R-02 | High | Pool floor raise race at startup | nli-service-handle.md | `test_pool_floor_raised_nli_enabled`, `test_pool_floor_not_raised_nli_disabled` |
| R-03 | Critical | NLI score tie-breaking instability — unstable sort | search-reranking.md | `test_nli_sort_stable_identical_scores`, `test_nli_sort_deterministic_repeated_calls` |
| R-04 | High | MCP_HANDLER_TIMEOUT fires mid-batch — mutex blockage | search-reranking.md | `test_nli_timeout_falls_back_to_cosine`, `test_nli_handle_not_failed_after_timeout` |
| R-05 | Critical | Hash verification absent — no warn when sha256=None | nli-service-handle.md | `test_hash_missing_emits_warn`, `test_hash_mismatch_emits_security_error`, `test_hash_correct_reaches_ready` |
| R-06 | High | Partial model file causes panic instead of Failed | nli-service-handle.md | `test_truncated_model_file_transitions_to_failed`, `test_corrupt_onnx_header_transitions_to_failed` |
| R-07 | High | Embedding consumed before NLI hand-off point | post-store-detection.md | `test_post_store_embedding_reaches_nli_task`, `test_post_store_empty_embedding_skips_nli` |
| R-08 | Med | HNSW insert failure — orphaned entry, no NLI edges | post-store-detection.md | `test_hnsw_failure_nli_task_still_spawned`, `test_hnsw_failure_store_returns_ok` |
| R-09 | Critical | Circuit breaker counts only Contradicts, not all edges | post-store-detection.md | `test_circuit_breaker_counts_all_edge_types`, `test_circuit_breaker_stops_at_cap` |
| R-10 | Critical | NLI miscalibration cascade to auto-quarantine | auto-quarantine-threshold.md + harness lifecycle | `test_miscalibration_cascade_no_auto_quarantine`, `test_auto_quarantine_threshold_enforced` |
| R-11 | High | Bootstrap promotion partial transaction failure | bootstrap-promotion.md | `test_bootstrap_promotion_idempotent_on_write_error`, `test_bootstrap_promotion_marker_set` |
| R-12 | Med | Bootstrap promotion runs before HNSW warmup | bootstrap-promotion.md | `test_bootstrap_promotion_no_hnsw_dependency`, `test_bootstrap_promotion_cold_index` |
| R-13 | High | Mutex poison not detected between calls | nli-service-handle.md | `test_mutex_poison_detected_at_get_provider`, `test_mutex_poison_transitions_to_failed` |
| R-14 | High | Eval SKIPPED profiles misread as gate pass | eval-integration.md | `test_eval_skipped_profile_annotation`, `test_eval_skipped_exit_code_nonzero` |
| R-15 | High | Invalid nli_model_name reaches runtime | config-extension.md | `test_invalid_model_name_fails_validate`, `test_nli_model_from_config_name_unknown` |
| R-16 | High | Post-store NLI write contention on SQLite write pool | post-store-detection.md | `test_burst_stores_all_edges_written`, `test_write_pool_error_not_propagated_to_mcp` |
| R-17 | High | Status penalty applied as entailment score multiplier | search-reranking.md | `test_deprecated_entry_penalty_not_applied_to_nli_score`, `test_raw_nli_scores_in_metadata` |
| R-18 | High | Deberta tokenizer incompatible with MiniLM2 path | nli-provider.md | `test_nli_model_cache_subdirs_distinct`, `test_deberta_score_pair_valid_on_obvious_entailment` |
| R-19 | Med | Combined sequence exceeds position embedding limit | nli-provider.md | `test_score_pair_511_plus_10_tokens_valid`, `test_score_pair_512_plus_512_tokens_no_panic` |
| R-20 | Med | INSERT OR IGNORE silently preserves bootstrap edge | bootstrap-promotion.md | `test_post_store_bootstrap_edge_conflict_documented`, `test_bootstrap_promotion_runs_before_post_store_for_adjacent_entries` |
| R-21 | Med | Eval latency contaminated by background NLI tasks | eval-integration.md | `test_eval_layer_no_store_during_replay` |
| R-22 | Med | sha2 crate absent from server dependencies | config-extension.md | Build gate: `cargo check -p unimatrix-server` |

## Non-Negotiable Tests (Feature Must Not Ship Without All Six)

1. **R-01**: `test_concurrent_nli_search_pool_saturation` — 3 simultaneous NLI searches,
   verify embedding path completes within 2x single-call baseline. Located in
   `nli_service_handle` integration tests.

2. **R-03**: `test_nli_sort_stable_identical_scores` — inject mock `CrossEncoderProvider`
   returning identical scores for all candidates; assert ordering is deterministic across
   10 repeated calls. Located in `search` module tests.

3. **R-05**: `test_hash_mismatch_transitions_to_failed` — valid model file + wrong 64-char
   hex hash; assert `Failed` state + log contains "security" + "hash mismatch" + server
   continues on cosine fallback. Located in `nli_handle` tests.

4. **R-09**: `test_circuit_breaker_counts_all_edge_types` — `max_contradicts_per_tick=2`,
   5 neighbors all above both thresholds; assert exactly 2 total edges written.
   Located in `nli_detection` integration tests.

5. **R-10**: `test_miscalibration_cascade_no_auto_quarantine` — store entry, write 10
   Contradicts edges via mock NLI, run background tick; assert no auto-quarantine fires
   when scores below `nli_auto_quarantine_threshold`. Located in background tick tests.

6. **R-13**: `test_mutex_poison_detected_at_get_provider` — poison `Mutex<Session>` via
   panicking mock; assert next `get_provider()` returns `Err(NliFailed)`, not `Ok`.
   Located in `nli_handle` tests.

## Component Test Plan Boundaries

| Test Plan File | Component Boundary | Primary Risks |
|----------------|--------------------|---------------|
| `nli-provider.md` | `unimatrix-embed/src/cross_encoder.rs` + `model.rs` + `download.rs` | R-18, R-19, AC-01 through AC-04 |
| `nli-service-handle.md` | `unimatrix-server/src/infra/nli_handle.rs` | R-01, R-02, R-05, R-06, R-13 |
| `config-extension.md` | `unimatrix-server/src/infra/config.rs` (NLI fields) | R-15, R-22, AC-07, AC-17 |
| `search-reranking.md` | `unimatrix-server/src/services/search.rs` (NLI path) | R-03, R-04, R-17, AC-08, AC-20 |
| `post-store-detection.md` | `unimatrix-server/src/services/nli_detection.rs` (run_post_store_nli) | R-07, R-08, R-09, R-16, AC-10, AC-11, AC-13 |
| `bootstrap-promotion.md` | `unimatrix-server/src/services/nli_detection.rs` (run_bootstrap_promotion) | R-11, R-12, R-20, AC-12, AC-24 |
| `eval-integration.md` | `unimatrix-server/src/services/eval.rs` (W1-4 stub) | R-14, R-21, AC-18, AC-22 |
| `model-download-cli.md` | CLI `model-download --nli` subcommand | AC-16 |
| `auto-quarantine-threshold.md` | `services/background_tick.rs` (NLI auto-quarantine path) | R-10, AC-25, ADR-007 |

---

## Integration Harness Plan

### Suite Selection

crt-023 touches: server tool logic (context_search, context_store), store/retrieval
behavior (GRAPH_EDGES writes), confidence/background tick (auto-quarantine), security
(hash verification, input truncation). The minimum gate is smoke; relevant full suites are:

| Suite | Justification |
|-------|---------------|
| `smoke` | Mandatory minimum gate — verify core tools still work with NLI enabled/disabled |
| `tools` | `context_search` and `context_store` behavior changes with NLI active; all existing tool tests must pass |
| `lifecycle` | Multi-step flows: store→NLI detection→search with NLI re-ranking; restart persistence of GRAPH_EDGES |
| `security` | Input truncation (NFR-08) is a security requirement; SHA-256 hash verification (NFR-09) |
| `contradiction` | NLI-written Contradicts edges are the primary contradiction-detection path for new stores |
| `edge_cases` | Extreme inputs through NLI (unicode, boundary values, empty candidate pool) |
| `confidence` | Auto-quarantine threshold (ADR-007) is triggered in the confidence/background path |

Suites NOT needed: `volume` (no schema change, NLI edge count is capped), `adaptation`
(no category allowlist changes), `protocol` (no MCP wire format changes).

### New Integration Tests to Write (Stage 3c)

The following new tests are required in the infra-001 harness because the behaviors are
only observable through the MCP interface:

#### `suites/test_lifecycle.py` — new tests

```python
# Test NLI graceful degradation: store→search with NLI absent
def test_search_nli_absent_returns_cosine_results(server):
    # AC-14: server with no NLI model; context_search returns results
    # Fixture: `server` (default config, NLI model absent in CI)

# Test NLI post-store edge written (requires NLI model in env)
def test_post_store_nli_contradicts_edge_written(server):
    # AC-10: store entry with known contradictory neighbor, assert GRAPH_EDGES
    # Requires NLI model; mark @pytest.mark.skipif(not NLI_MODEL_AVAILABLE)

# Test bootstrap promotion restart no-op (AC-24)
def test_bootstrap_promotion_restart_noop(server):
    # Restart server after bootstrap promotion; assert no duplicate edges
    # Uses shared_server to simulate restart
```

#### `suites/test_tools.py` — new tests

```python
# Test context_search returns valid results when NLI not ready (AC-05, AC-14)
def test_search_nli_not_ready_fallback_results(server):
    # Assert: response schema unchanged, results returned, no error

# Test context_store not blocked by NLI detection (NFR-02)
def test_store_response_not_blocked_by_nli_task(server):
    # Assert: store MCP response returns before NLI task completes
    # Measure via timing (store response < 2s)
```

#### `suites/test_security.py` — new tests

```python
# Test large input truncation does not crash server (AC-03, NFR-08)
def test_store_large_content_nli_no_crash(server):
    # Store entry with 100,000-char content; assert server stays healthy
    # Assert context_search still works after

# Test hash mismatch degrades gracefully, no tool errors (AC-06, AC-14)
def test_nli_hash_mismatch_graceful_degradation(server):
    # Server started with wrong hash; assert context_search returns results
    # Requires config injection (server fixture variant or env var)
```

#### `suites/test_contradiction.py` — new tests

```python
# Test NLI Contradicts edge excludes entry from search via graph penalty (AC-10 + lifecycle)
def test_nli_contradicts_edge_depresses_search_rank(server):
    # Store two entries, NLI writes Contradicts edge, run tick, search
    # Assert contradicted entry ranks lower than before (or below threshold)
    # Requires NLI model or mock
```

### Fixture Guidance

- Use `server` fixture for NLI degradation tests (NLI absent = default in CI, no model cached)
- Use `admin_server` for tests that write graph edges or trigger auto-quarantine checks
- Tests requiring NLI model must use `@pytest.mark.skipif(not NLI_MODEL_AVAILABLE, reason="NLI model not cached")`
- Define `NLI_MODEL_AVAILABLE = os.path.exists(NLI_MODEL_CACHE_PATH)` in `conftest.py`

### Tests Already Covered by Existing Suites

The following behaviors are already exercised by existing tests and do NOT require new
integration tests:

- `context_store` basic roundtrip (tools suite)
- GRAPH_EDGES written by existing contradiction path (contradiction suite)
- Server restart persistence (lifecycle suite)
- Content scanning boundaries (security suite)
- Confidence scoring pipeline (confidence suite)

---

## Cross-Component Test Dependencies

| Dependency | How It Affects Tests |
|------------|---------------------|
| Mock `CrossEncoderProvider` | All component tests (except NliProvider itself) use a mock; the trait boundary makes this clean |
| `Arc<RayonPool>` availability | Tests exercising the rayon dispatch path need a real `RayonPool`; can be constructed in tests with `RayonPool::new(2)` |
| ONNX model file | `NliProvider` unit tests require model on disk; mark `#[cfg(feature = "nli_model_available")]` or `#[ignore]` with doc comment |
| `COUNTERS` table | Bootstrap promotion tests need a real in-memory SQLite DB; use `Store::open_in_memory()` |
| Write pool singleton | Post-store contention tests require real `Store` with WAL mode |

---

## W1-2 Compliance Verification

All three NLI inference paths (search re-ranking, post-store detection, bootstrap promotion)
must route through `rayon_pool.spawn()` or `rayon_pool.spawn_with_timeout()`. Tests verify
this at the unit level by asserting the mock provider is called from within a rayon thread
context (via `rayon::current_thread_index().is_some()`). This is a mandatory check for each
inference path — no `spawn_blocking` substitution is acceptable.
