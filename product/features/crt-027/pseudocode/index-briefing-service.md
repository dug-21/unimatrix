# IndexBriefingService — Pseudocode
# File: crates/unimatrix-server/src/services/index_briefing.rs (NEW FILE)

## Purpose

This is a new file that entirely replaces the content of `services/briefing.rs`.
The old `briefing.rs` is deleted; this new `index_briefing.rs` is created in its place.

`IndexBriefingService` provides a unified knowledge index query used by:
1. `context_briefing` MCP tool handler (`mcp/tools.rs`)
2. `handle_compact_payload` UDS path (`uds/listener.rs`)

It always returns `Vec<IndexEntry>` with `status=Active` entries only, sorted by fused
score descending, using the existing `SearchService` for embedding + ranking.

`derive_briefing_query` is a shared free function that encapsulates the three-step
query derivation priority. Both callers must use this function (not independent logic).

---

## File Structure

This file has three public-within-crate items:
1. `IndexBriefingParams` struct
2. `IndexBriefingService` struct + `impl` block
3. `derive_briefing_query` free function

Target file size: ~250-300 lines including tests. Well within the 500-line limit.

---

## Types

### `IndexBriefingParams`

```rust
/// Parameters for IndexBriefingService::index().
pub(crate) struct IndexBriefingParams {
    /// The search query derived by derive_briefing_query.
    pub query: String,
    /// Max entries to return. Defaults to IndexBriefingService.default_k (20).
    pub k: usize,
    /// Session ID for WA-2 category histogram boost (optional).
    pub session_id: Option<String>,
    /// Approximate token budget (used for future ranked truncation; not enforced here).
    pub max_tokens: Option<usize>,
}
```

### `IndexBriefingService`

```rust
/// Replaces BriefingService (deleted in crt-027).
/// Returns Vec<IndexEntry> with status=Active entries, sorted by fused score descending.
/// Uses SearchService for embedding + WA-2 histogram boost + effectiveness ranking.
#[derive(Clone)]
pub(crate) struct IndexBriefingService {
    entry_store: Arc<Store>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    /// Default k for index() calls that do not specify k.
    /// Hardcoded to 20. UNIMATRIX_BRIEFING_K is not read (ADR-003 crt-027).
    default_k: usize,
    /// crt-018b (ADR-004): effectiveness classification handle.
    /// Required parameter — missing wiring is a compile error.
    effectiveness_state: EffectivenessStateHandle,
    /// crt-018b (ADR-001): generation-cached snapshot shared across clones.
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}
```

---

## Constructor: `IndexBriefingService::new`

```rust
impl IndexBriefingService {
    /// Construct a new IndexBriefingService.
    ///
    /// `effectiveness_state` is a required, non-optional parameter (ADR-004 crt-018b pattern).
    /// Missing this parameter at the call site is a compile error — NOT a runtime degradation.
    ///
    /// # crt-027 Note
    /// `UNIMATRIX_BRIEFING_K` env var is deprecated and NOT read here.
    /// The default k=20 is hardcoded and cannot be reduced via environment variable.
    /// Callers that need a different k should pass it explicitly via IndexBriefingParams.k.
    pub(crate) fn new(
        entry_store: Arc<Store>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
        effectiveness_state: EffectivenessStateHandle,  // required, non-optional
    ) -> Self {
        IndexBriefingService {
            entry_store,
            search,
            gateway,
            default_k: 20,  // hardcoded, not from env var (ADR-003 crt-027)
            effectiveness_state,
            // Initialize cached snapshot internally (same pattern as BriefingService)
            cached_snapshot: EffectivenessSnapshot::new_shared(),
        }
    }
}
```

---

## Primary Method: `IndexBriefingService::index`

```rust
impl IndexBriefingService {
    /// Query the knowledge index and return a ranked, active-only result set.
    ///
    /// Steps:
    /// 1. Determine effective k (params.k, clamp to minimum 1 to guard EC-03)
    /// 2. Resolve category histogram if session_id is present (for WA-2 boost)
    /// 3. Build ServiceSearchParams with no filters except status=Active
    /// 4. Delegate to SearchService.search()
    /// 5. Map SearchResult entries to IndexEntry (snippet = first SNIPPET_CHARS chars)
    /// 6. Return Vec<IndexEntry> sorted by fused score descending
    ///
    /// Returns Ok(vec![]) on no results (not an error).
    pub(crate) async fn index(
        &self,
        params: IndexBriefingParams,
        audit_ctx: &AuditContext,
        caller_id: Option<&CallerId>,
    ) -> Result<Vec<IndexEntry>, ServiceError> {
        // Step 1: Effective k (EC-03: clamp k to minimum 1)
        let effective_k = if params.k == 0 { self.default_k } else { params.k };

        // Step 2: Security gateway check (SR-B: all query paths go through SecurityGateway)
        // The gateway is consulted via SearchService which wraps all calls internally.
        // No explicit gateway.check() needed here — SearchService handles it.

        // Step 3: Resolve category histogram for WA-2 boost
        // IndexBriefingService does NOT have access to SessionRegistry directly.
        // The session_id is passed to ServiceSearchParams.session_id, and
        // SearchService handles the histogram lookup internally (or the caller pre-resolves it).
        //
        // Decision: pass session_id to ServiceSearchParams; SearchService
        // resolves histogram via its own session_registry reference.
        // This is consistent with how handle_context_search works (line 990-991 in listener.rs).
        //
        // Note: If SearchService does NOT have a direct SessionRegistry reference,
        // the caller must pre-resolve the histogram and pass it via category_histogram.
        // Inspect SearchService::search() signature for ServiceSearchParams.category_histogram.
        // Current ServiceSearchParams has: category_histogram: Option<HashMap<String, u32>>
        // This means the caller pre-resolves; IndexBriefingService must receive a
        // session_registry reference OR the caller passes pre-resolved histogram.
        //
        // RESOLUTION (per existing codebase pattern):
        // IndexBriefingService does NOT hold SessionRegistry. The callers:
        // - MCP handler: pre-resolves histogram via SessionRegistry.get_category_histogram(session_id)
        // - UDS handler: pre-resolves histogram via session_registry.get_category_histogram(session_id)
        // Both pass it via IndexBriefingParams (add histogram field) OR via ServiceSearchParams directly.
        //
        // Simplest approach: add category_histogram to IndexBriefingParams, let callers pre-resolve.
        // This is consistent with the existing pattern in handle_context_search (lines 974-977).

        // Step 4: Build ServiceSearchParams
        // Filters include status=Active to exclude deprecated entries (FR-08)
        let service_params = ServiceSearchParams {
            query: params.query.clone(),
            k: effective_k,
            // Filter to Active status only (FR-08, AC-06)
            // If ServiceSearchParams supports a status filter, set it here.
            // If not, post-filter after SearchService returns.
            filters: None,  // See note below on status filtering
            similarity_floor: None,   // No floor for briefing (return all ranked results)
            confidence_floor: None,   // No floor for briefing
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Strict,
            session_id: params.session_id.clone(),
            category_histogram: params.category_histogram,  // pre-resolved by caller
        };

        // Step 5: Delegate to SearchService
        let uds_caller = caller_id
            .cloned()
            .unwrap_or_else(|| CallerId::UdsSession("index-briefing".to_string()));

        let search_results = self
            .search
            .search(service_params, audit_ctx, &uds_caller)
            .await?;

        // Step 6: Post-filter: status=Active only
        // If SearchService does not guarantee Active-only results (it may not),
        // post-filter here. Check existing SearchService behavior.
        // Defensive: always filter Active status regardless of SearchService behavior.
        let active_entries: Vec<_> = search_results.entries
            .into_iter()
            .filter(|se| se.entry.status == Status::Active)
            .collect();

        // Step 7: Map to IndexEntry
        // confidence = fused score (similarity + confidence + WA-2 boost) — from SearchEntry
        // snippet = first SNIPPET_CHARS chars, UTF-8 char boundary safe (ADR-005)
        let index_entries: Vec<IndexEntry> = active_entries
            .into_iter()
            .map(|se| {
                let snippet = se.entry.content
                    .chars()
                    .take(SNIPPET_CHARS)
                    .collect::<String>();
                IndexEntry {
                    id: se.entry.id,
                    topic: se.entry.topic.clone(),
                    category: se.entry.category.clone(),
                    // Use se.similarity as the fused score (SearchService already applies
                    // confidence + co-access + WA-2 boost in its output similarity field)
                    confidence: se.similarity,
                    snippet,
                }
            })
            .collect();

        // Step 8: Sort by fused score descending (SearchService may already sort, but ensure it)
        // Note: SearchService returns results sorted by fused score per existing behavior.
        // Re-sort here to be defensive and ensure contract is met.
        let mut sorted = index_entries;
        sorted.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        Ok(sorted)
    }
}
```

### Note on `ServiceSearchParams.category_histogram`

The `params` struct must carry `category_histogram: Option<HashMap<String, u32>>` to allow
the caller to pass the pre-resolved histogram. Update `IndexBriefingParams`:

```rust
pub(crate) struct IndexBriefingParams {
    pub query: String,
    pub k: usize,
    pub session_id: Option<String>,
    pub max_tokens: Option<usize>,
    pub category_histogram: Option<HashMap<String, u32>>,  // pre-resolved by caller
}
```

### Note on status filtering via SearchService

Inspect `ServiceSearchParams.filters` type. If it is `Option<QueryFilter>` and `QueryFilter`
supports a `status` field, set `filters: Some(QueryFilter { status: Some(Status::Active), .. })`.
If `SearchService` does not support status filtering via params, post-filter after return as
shown above. The post-filter approach is safe and correct regardless.

---

## Free Function: `derive_briefing_query`

```rust
/// Derive the search query for IndexBriefingService using three-step priority.
///
/// Step 1: If `task` is Some and non-empty, use it directly.
/// Step 2: If `session_state` is Some and has topic_signals, synthesize:
///         "{feature_cycle} {top_3_signals_by_vote_count}"
/// Step 3: Fall back to `topic` (always non-empty; e.g., "crt-027").
///
/// Used by both the MCP context_briefing handler (with session_state from
/// SessionRegistry lookup) and handle_compact_payload (with held session_state).
/// Both call sites MUST use this function to prevent divergence (AC-09, R-06).
pub(crate) fn derive_briefing_query(
    task: Option<&str>,
    session_state: Option<&SessionState>,
    topic: &str,
) -> String {
    // Step 1: explicit task overrides everything
    if let Some(t) = task {
        if !t.trim().is_empty() {
            return t.to_string();
        }
    }

    // Step 2: synthesize from session state if available
    if let Some(state) = session_state {
        let signals = extract_top_topic_signals(&state.topic_signals, 3);
        if !signals.is_empty() {
            let feature_cycle = state.feature.as_deref().unwrap_or("");
            if !feature_cycle.is_empty() {
                // "crt-027 briefing hook compaction"
                return format!("{} {}", feature_cycle, signals.join(" "));
            } else {
                // No feature_cycle but signals present: use signals only
                // Falls to step 3 if feature_cycle is empty AND signals are from an
                // unattributed session. This is intentional — topic fallback is safer.
                // Alternatively: if signals.len() >= 1, use them even without feature_cycle.
                // Per FR-11: "feature_cycle + top 3 topic_signals". If feature_cycle is absent,
                // fall to step 3.
                // DECISION: If feature_cycle is absent, skip to step 3.
                // Rationale: a query of just "briefing hook compaction" may return noise.
                // The feature-id in step 3 is more reliable.
            }
        }
    }

    // Step 3: topic fallback (always available — e.g., "crt-027")
    topic.to_string()
}

/// Extract top N topic signals sorted by vote count descending.
///
/// Returns signal strings only (not tallies).
/// If fewer than N signals exist, returns all available.
/// EC-06: handles fewer than 3 signals gracefully.
fn extract_top_topic_signals(
    topic_signals: &HashMap<String, TopicTally>,
    n: usize,
) -> Vec<String> {
    if topic_signals.is_empty() {
        return vec![];
    }

    let mut pairs: Vec<(&String, u32)> = topic_signals
        .iter()
        .map(|(k, v)| (k, v.count))
        .collect();

    // Sort by count descending
    pairs.sort_by(|a, b| b.1.cmp(&a.1));

    pairs.into_iter()
        .take(n)
        .map(|(k, _)| k.clone())
        .collect()
}
```

---

## Imports

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use unimatrix_core::{Status, Store};

use crate::infra::session::SessionState;
use crate::mcp::response::briefing::{IndexEntry, SNIPPET_CHARS};
use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
use crate::services::gateway::SecurityGateway;
use crate::services::search::{RetrievalMode, SearchService, ServiceSearchParams};
use crate::services::{AuditContext, CallerId, ServiceError};
```

The `SessionState` and `TopicTally` types are imported from `crate::infra::session`.
`SNIPPET_CHARS` is imported from `mcp::response::briefing` — the canonical definition location.

---

## Error Handling

- `index()` returns `Result<Vec<IndexEntry>, ServiceError>`.
- Empty result set (`Ok(vec![])`) is not an error. Callers must handle it (R-10, AC-18).
- `SearchService.search()` errors propagate via `?` operator as `ServiceError`.
- EC-03: `k=0` is clamped to `default_k` (20), not passed to SearchService as 0.

---

## Key Test Scenarios

All in `index_briefing.rs` `#[cfg(test)]` block.

**T-IB-01** `index_returns_only_active_entries` (AC-06, IR-02):
- Test database: one Active entry id=1, one Deprecated entry id=2, same topic
- Call: service.index(IndexBriefingParams { query: "test topic", k: 20, ... })
- Assert: result contains entry id=1
- Assert: result does NOT contain entry id=2

**T-IB-02** `index_default_k_is_20` (AC-07, R-09):
- Set UNIMATRIX_BRIEFING_K=3 in test env (should have no effect)
- Insert 25 Active entries
- Call: service.index(IndexBriefingParams { query: "test", k: 20, ... })
- Assert: result.len() <= 20 (not capped at 3)

**T-IB-03** `index_respects_k_param` (AC-07):
- Insert 25 Active entries
- Call with k=5
- Assert: result.len() <= 5

**T-IB-04** `index_empty_result_is_ok_not_error` (R-10, AC-18):
- Query: "nonexistent-feature-xyz"
- Assert: result == Ok(vec![])

**T-IB-05** `index_results_sorted_by_fused_score_descending` (AC-19):
- Insert 2+ Active entries with different confidence values
- Assert: result[0].confidence >= result[1].confidence

**T-IB-06** `index_effectiveness_state_influences_ranking` (R-02):
- Construct service with a mock EffectivenessStateHandle
- Two entries with same similarity but different effectiveness scores
- Assert: higher-effectiveness entry ranks first

**T-IB-07** `derive_briefing_query_step1_task_used_directly` (AC-09, R-06):
- Call: derive_briefing_query(Some("implement spec writer"), None, "crt-027")
- Assert: returns "implement spec writer"

**T-IB-08** `derive_briefing_query_step1_empty_task_falls_to_step3` (AC-09, R-06):
- Call: derive_briefing_query(Some(""), None, "crt-027")
- Assert: returns "crt-027" (empty task treated as absent)

**T-IB-09** `derive_briefing_query_step2_synthesized_from_session_signals` (AC-09, R-06):
- Build SessionState with feature="crt-027", topic_signals={"briefing": count=5, "hook": count=3, "compaction": count=2}
- Call: derive_briefing_query(None, Some(&state), "crt-027")
- Assert: returns "crt-027 briefing hook compaction"

**T-IB-10** `derive_briefing_query_step3_topic_fallback` (AC-09, R-06):
- Call: derive_briefing_query(None, None, "crt-027")
- Assert: returns "crt-027"

**T-IB-11** `derive_briefing_query_step2_fewer_than_3_signals` (EC-06):
- Build SessionState with feature="crt-027/spec", topic_signals={"briefing": count=5}
- Assert: returns "crt-027/spec briefing" (not "crt-027/spec briefing  " with trailing space)

**T-IB-12** `snippet_is_utf8_char_boundary_safe` (AC-17, NFR-04):
- Entry content: "\u{4e16}\u{754c}".repeat(200) (CJK chars, each 3 bytes)
- Call: index(...)
- For each IndexEntry in result: assert snippet.len() <= SNIPPET_CHARS * 3 (max bytes for all CJK)
- Assert: String::from_utf8(snippet.as_bytes().to_vec()).is_ok()
