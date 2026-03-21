# Agent Report: crt-023-agent-6-search

**Feature**: crt-023 — NLI Re-ranking
**Role**: Rust Developer — SearchService NLI Integration (Wave 3)
**Agent ID**: crt-023-agent-6-search

---

## Files Modified

- `crates/unimatrix-server/src/services/search.rs` — primary: NLI fields, hnsw_k expansion, try_nli_rerank, apply_nli_sort, 12 unit tests
- `crates/unimatrix-server/src/services/mod.rs` — ServiceLayer::new + with_rate_config signatures extended with nli_handle, nli_top_k, nli_enabled, inference_config
- `crates/unimatrix-server/src/services/briefing.rs` — test helper SearchService::new call updated
- `crates/unimatrix-server/src/test_support.rs` — TestHarness ServiceLayer::with_rate_config call updated
- `crates/unimatrix-server/src/server.rs` — ServiceLayer::new call updated
- `crates/unimatrix-server/src/infra/shutdown.rs` — both ServiceLayer::new calls updated
- `crates/unimatrix-server/src/uds/listener.rs` — test helper ServiceLayer::new call updated
- `crates/unimatrix-server/src/eval/profile/layer.rs` — inference_config arg added to with_rate_config
- `crates/unimatrix-server/src/main.rs` — NLI handle construction + ServiceLayer::new calls updated
- `crates/unimatrix-server/src/main_tests.rs` — fixed Command::ModelDownload pattern match, added 2 NLI flag tests

---

## Implementation Summary

### search.rs changes

**New fields on SearchService:**
- `nli_handle: Arc<NliServiceHandle>` — lazy-loading handle for CrossEncoderProvider
- `nli_top_k: usize` — expanded HNSW candidate pool size when NLI active
- `nli_enabled: bool` — fast gate; when false, NLI path never attempted

**Step 5 (HNSW search):** Expanded candidate pool to `hnsw_k = max(nli_top_k, params.k)` when `nli_enabled`, so NLI has sufficient candidates to re-rank before final truncation.

**Step 7 (sort):** Replaced unconditional `rerank_score` sort with conditional NLI path:
- Calls `try_nli_rerank(...)` when `nli_enabled`
- On success: replaces `results_with_scores` with NLI-sorted results, truncated to `params.k`
- On any failure (handle not ready, rayon timeout, rayon panic, score_batch error): silent `debug!` log, falls back to existing `rerank_score` sort (ADR-002)

**`try_nli_rerank` (async, private):**
- Gets provider via `nli_handle.get_provider().await` — returns None on Err
- Collects passages into owned `Vec<String>` (Send requirement for rayon closure)
- Dispatches `provider.score_batch(&pairs)` via `rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`
- Returns None on empty candidates, rayon error, or score_batch error
- Delegates sort to `apply_nli_sort` on success

**`apply_nli_sort` (pub(crate), pure):**
- Computes `effective_entailment = nli_scores.entailment * penalty` (status penalty from penalty_map)
- Stable sort: `effective_entailment DESC`, tiebreak by original HNSW rank ASC (R-03, deterministic)
- Truncates to `top_k`
- Returns `Vec<(EntryRecord, f64)>` preserving the existing pipeline tuple type

### ServiceLayer signature changes

Both `ServiceLayer::new` and `ServiceLayer::with_rate_config` extended with:
- `nli_handle: Arc<NliServiceHandle>`
- `nli_top_k: usize`
- `nli_enabled: bool`
- `inference_config: Arc<InferenceConfig>`

All call sites updated across server.rs, shutdown.rs, listener.rs, test_support.rs, briefing.rs, eval/profile/layer.rs, main.rs.

---

## Tests

**Total**: 1689 passed, 3 failed (pre-existing failures in nli_detection.rs from another agent)

**My tests (services::search::tests):**
- `test_nli_sort_stable_identical_scores_preserves_original_order` — R-03 tiebreak
- `test_nli_sort_orders_by_entailment_descending` — primary sort key
- `test_nli_sort_nan_entailment_treated_as_equal` — NaN edge case
- `test_nli_sort_truncates_to_top_k` — truncation
- `test_nli_sort_penalty_depresses_effective_entailment` — status penalty interaction
- `test_nli_fallback_when_handle_not_ready` — Loading state fallback
- `test_nli_fallback_when_handle_exhausted` — async timeout fallback (tokio)
- `test_nli_fallback_on_empty_candidates` — empty candidate guard
- `test_nli_top_k_drives_hnsw_expansion` — Step 5 hnsw_k logic
- `test_nli_disabled_uses_params_k` — nli_enabled=false path
- `test_nli_hnsw_k_never_below_params_k` — nli_top_k < params.k guard
- All 12 pass.

**Pre-existing failures (not caused by this agent):**
- `services::nli_detection::tests::test_bootstrap_promotion_confirms_above_threshold`
- `services::nli_detection::tests::test_bootstrap_promotion_refutes_below_threshold`
- `services::nli_detection::tests::test_bootstrap_promotion_nli_inference_runs_on_rayon_thread`

These are in `nli_detection.rs` (untracked file, another agent's work). The `maybe_bootstrap_promotion` implementation does not satisfy the test assertions — not caused by search.rs changes.

---

## Issues / Blockers

None for this agent's scope. The 3 failing tests belong to another agent's bootstrap promotion implementation.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for unimatrix-server NLI rayon patterns — found ADRs and W1-2 compliance test pattern (#2728), no existing pattern for owned-data-before-spawn or pure-function test extraction.
- Stored: entry #2742 "Collect owned data before rayon_pool.spawn_with_timeout — &[T] is not Send across spawn boundary" via `/uni-store-pattern`
- Stored: entry #2743 "Extract NLI sort logic as pub(crate) pure function to enable unit tests without ONNX model" via `/uni-store-pattern`
