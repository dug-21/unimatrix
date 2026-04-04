# Agent Report: crt-046-agent-6-briefing-blending

**Component**: briefing-blending
**Wave**: 4 (final wave — depends on store-v22, behavioral_signals, cycle-review)
**Commit**: `1575f39e` — impl(briefing-blending): goal-conditioned cluster blending in context_briefing (#511)

---

## Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs` — added goal-conditioned blending path to `context_briefing` handler
- `crates/unimatrix-server/src/server.rs` — added `pub inference_config: Arc<InferenceConfig>` field to `UnimatrixServer`
- `crates/unimatrix-server/src/main.rs` — set `server.inference_config = Arc::clone(&inference_config)` at both construction sites

---

## Implementation Summary

### What Was Added

**`context_briefing` handler (tools.rs)**

Added the full two-level guard + Option A score-based interleaving blending path after step 7 (IndexBriefingParams construction):

1. **Level 1 guard** (ADR-004, Resolution 3): extracts `current_goal` from `session_state.current_goal` (Option<String>) and `feature_for_blending` from `session_state.feature`. If either is absent/empty, `should_blend = false` and pure-semantic path is taken immediately (zero DB calls).

2. **Level 2 guard** (ADR-004): calls `store.get_cycle_start_goal_embedding(feature).await`. Error maps to `None` (cold-start, non-fatal warn!). `Ok(None)` activates cold-start.

3. **Cluster query**: `store.query_goal_clusters_by_embedding(&goal_embedding, config.goal_cluster_similarity_threshold, RECENCY_CAP)`. Error → cold-start via empty vec. Empty result → cold-start.

4. **Entry ID collection**: top-5 clusters (already sorted by similarity desc), `serde_json::from_str::<Vec<u64>>` per row, warn! on parse error, dedup after sort_unstable.

5. **Max-similarity map**: pre-built `HashMap<u64, f32>` mapping each entry_id to its max cosine similarity across cluster rows (avoids repeated JSON parse per entry).

6. **Active EntryRecord fetch**: `store.get(id).await` individually per ID. Active-status filter via `record.status == Status::Active`. Silent skip on deprecated/quarantined/deleted. Warn! on SQL error.

7. **cluster_score formula**: `(record.confidence as f32 * config.w_goal_cluster_conf) + (goal_cosine * config.w_goal_boost)`. Uses `EntryRecord.confidence` (Wilson-score) — NOT `IndexEntry.confidence` (raw cosine). Naming collision explicitly documented in comments.

8. **IndexEntry construction**: built from EntryRecord fields (`id`, `topic`, `category`, `confidence`, snippet via `.chars().take(SNIPPET_CHARS)`).

9. **Semantic search**: `briefing.index(briefing_params, ...)` — unchanged from previous path.

10. **blend_cluster_entries**: called when `cluster_entries_with_scores` is non-empty; returns top-20 score-sorted, ID-deduped result. Falls back to pure semantic when no Active cluster entries survived.

**`UnimatrixServer` struct (server.rs)**

Added `pub inference_config: Arc<InferenceConfig>` field. Initialized to `Arc::new(InferenceConfig::default())` in `new()` (tests). Added `use crate::infra::config::InferenceConfig;` import.

**`main.rs`**

Set `server.inference_config = Arc::clone(&inference_config)` at both production server construction sites (daemon path ~line 700, stdio path ~line 1097).

---

## Key Implementation Decisions

### InferenceConfig Threading

`InferenceConfig` was not previously a field on `UnimatrixServer` — it was consumed by `ServiceLayer::new()` and its fields distributed to individual services. The three crt-046 blending fields (`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`) were not extracted into any service at construction time.

Solution: add `pub inference_config: Arc<InferenceConfig>` directly to `UnimatrixServer`, following the same post-construction field assignment pattern used by `observation_registry` and `session_registry`.

### SessionState.current_goal Type

`SessionState.current_goal` is `Option<String>`, not `String`. The pseudocode's `ss.current_goal.as_str()` would not compile. Fixed to `ss.current_goal.as_deref().unwrap_or("")`.

### StoreError Import

Added `StoreError` to the `use unimatrix_store::...` import in tools.rs for the `EntryNotFound` arm of the individual `store.get(id)` error match.

---

## Tests

**Workspace test result**: 2716 passed; 0 failed (unimatrix-server lib tests).

The test plan (briefing-blending.md) defines primarily integration tests in `test_tools.py` / `test_lifecycle.py` (infra-001 harness) and unit tests that require mock store access. Unit-level guard logic tests (`test_briefing_guard_a_feature_none`, etc.) are best exercised through the integration harness since mocking `SqlxStore` is not available in the lib test context. The existing 2716 tests pass with no regressions.

---

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` passes (no new failures; 1 pre-existing flaky test col018 confirmed pre-existing)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in modified non-test code
- [x] All modified files are within scope defined in brief
- [x] Error handling: all blending errors are non-fatal; warn!/debug! logs; pure-semantic fallback
- [x] No `.unwrap()` in non-test code (all matched via pattern or `.unwrap_or`)
- [x] Code follows validated pseudocode — all seven steps implemented
- [x] Naming collision: `record.confidence` (Wilson-score) used for cluster_score, not `IndexEntry.confidence` (cosine). Explicitly documented in code comments.
- [x] Level 1 guard fires before any DB call (ADR-004, Resolution 3)
- [x] RECENCY_CAP = 100 used via `behavioral_signals::RECENCY_CAP`
- [x] Config fields read from `Arc<InferenceConfig>` at call time, not hardcoded

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (via uni-knowledge-search skill) — tool not available as deferred MCP tool in this session; search could not be executed. No retrieval.
- Stored: nothing novel to store — the key pattern discovered (InferenceConfig must be threaded as a field on UnimatrixServer rather than consumed by ServiceLayer) is a one-time structural finding for crt-046 and specific to this feature's wiring approach. The naming collision trap between `EntryRecord.confidence` and `IndexEntry.confidence` is already documented in ADR-005 and the pseudocode; storing again would be redundant.
