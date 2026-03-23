# Context Briefing Handler — Pseudocode
# File: crates/unimatrix-server/src/mcp/tools.rs

## Purpose

Update the `context_briefing` async method inside the `#[cfg(feature = "mcp-briefing")]`
block to:
1. Replace `BriefingService::assemble()` call with `IndexBriefingService::index()`.
2. Add three-step query derivation via `derive_briefing_query`.
3. Replace `format_briefing()` with `format_index_table()`.
4. Resolve session state from `SessionRegistry` for step 2 of query derivation.
5. Pre-resolve category histogram for WA-2 boost.
6. Remove `Briefing` struct construction and `format_briefing` import.

The `BriefingParams` MCP schema struct and its fields are unchanged for backward compatibility.
`role` is parsed but ignored. `task` is used as step 1 of query derivation if present.

---

## MCP Schema Struct: `BriefingParams` — UNCHANGED

```rust
/// Parameters for getting an orientation briefing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BriefingParams {
    /// Role to get briefed on (e.g., "architect", "developer").
    pub role: String,                  // retained for backward compat; ignored by new handler
    /// Task description for context retrieval (used as query step 1).
    pub task: String,                  // used by derive_briefing_query step 1
    /// Feature tag / topic (used as query step 3 fallback).
    pub feature: Option<String>,
    /// Max output tokens (default: 3000, range: 500-10000).
    pub max_tokens: Option<i64>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Whether the returned entries were helpful.
    pub helpful: Option<bool>,
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}
```

No changes to this struct. `role` remains required by the schema (callers that pass `role`
continue to work without error). The new handler simply ignores it.

---

## Modified Handler: `context_briefing`

**Existing signature** (unchanged):
```rust
async fn context_briefing(
    &self,
    #[allow(unused_variables)] Parameters(params): Parameters<BriefingParams>,
) -> Result<CallToolResult, rmcp::ErrorData>
```

**New body** (inside `#[cfg(feature = "mcp-briefing")]` block):

```rust
#[cfg(feature = "mcp-briefing")]
{
    // 1. Identity + capability check (unchanged)
    let ctx = self
        .build_context(&params.agent_id, &params.format, &params.session_id)
        .await?;
    self.require_cap(&ctx.agent_id, Capability::Read).await?;

    // 2. Validation
    validate_briefing_params(&params).map_err(rmcp::ErrorData::from)?;
    validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

    // 3. Validate max_tokens
    let max_tokens = validated_max_tokens(params.max_tokens).map_err(rmcp::ErrorData::from)?;

    // 4. Resolve session state for step 2 of query derivation
    // MCP path: must look up SessionRegistry by session_id (AC-10)
    let session_state: Option<crate::infra::session::SessionState> =
        params.session_id.as_deref().and_then(|sid| {
            self.session_registry.get_state(sid)
        });

    // 5. Pre-resolve category histogram for WA-2 boost (crt-026 pattern)
    let category_histogram: Option<std::collections::HashMap<String, u32>> =
        params.session_id.as_deref().and_then(|sid| {
            let h = self.session_registry.get_category_histogram(sid);
            if h.is_empty() { None } else { Some(h) }
        });

    // 6. Three-step query derivation (FR-11, AC-09)
    // Step 1: task param if non-empty
    // Step 2: synthesized from feature_cycle + top 3 topic_signals (from session_state)
    // Step 3: feature/topic fallback
    let topic = params.feature.as_deref().unwrap_or(&params.role);
    // Note: using params.role as final fallback only if params.feature is also absent.
    // FR-11 specifies "topic param" — which is `params.feature` when present, else `params.role`.
    // Implementation should use params.feature if present, else params.task if non-empty,
    // else params.role. The SPEC says step 3 uses "the topic param (e.g., crt-027)".
    // BriefingParams has `feature: Option<String>`. When feature is None, fall back to role.

    let query = crate::services::index_briefing::derive_briefing_query(
        Some(&params.task),            // step 1: task param (may be empty string, handled by derive_briefing_query)
        session_state.as_ref(),        // step 2: session signals
        topic,                         // step 3: feature or role as topic
    );

    // 7. Build IndexBriefingParams
    let briefing_params = crate::services::index_briefing::IndexBriefingParams {
        query,
        k: 20,          // default k (FR-13: not from UNIMATRIX_BRIEFING_K)
        session_id: params.session_id.clone(),
        max_tokens: Some(max_tokens),
        category_histogram,             // pre-resolved for WA-2 boost
    };

    // 8. Delegate to IndexBriefingService
    let entries = self
        .services
        .briefing
        .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
        .await
        .map_err(rmcp::ErrorData::from)?;

    // 9. Collect entry IDs for audit + usage recording
    let entry_ids: Vec<u64> = entries.iter().map(|e| e.id).collect();

    // 10. Format response as flat indexed table (FR-12, AC-08)
    use crate::mcp::response::briefing::format_index_table;
    let table_text = format_index_table(&entries);

    // 11. Audit (unchanged pattern)
    self.audit_fire_and_forget(AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: ctx.agent_id.clone(),
        operation: "context_briefing".to_string(),
        target_ids: entry_ids.clone(),
        outcome: Outcome::Success,
        detail: format!(
            "index briefing: query derived, {} entries returned",
            entries.len()
        ),
    });

    // 12. Usage recording (unchanged pattern, using Briefing access source)
    self.services.usage.record_access(
        &entry_ids,
        AccessSource::Briefing,
        UsageContext {
            session_id: ctx.audit_ctx.session_id.clone(),
            agent_id: Some(ctx.agent_id.clone()),
            helpful: params.helpful,
            feature_cycle: params.feature.clone(),
            trust_level: Some(ctx.trust_level),
            access_weight: 1,
            current_phase: None,
        },
    );

    // 13. Return flat indexed table
    Ok(CallToolResult::success(vec![Content::text(table_text)]))
}
```

### Handling `params.task` as query step 1

The current `BriefingParams.task` field is `String` (required, not Option). When task is
an empty string `""`, `derive_briefing_query` treats it as absent and falls to step 2.
This is consistent with FR-11 ("non-empty" requirement).

---

## Removed Imports (from the context_briefing handler scope)

- `crate::services::briefing::BriefingParams` — removed (the service-layer struct, not the MCP params)
- `Briefing` (response struct) — removed
- `format_briefing` — removed

## Added Imports (to the context_briefing handler scope)

- `crate::services::index_briefing::IndexBriefingParams`
- `crate::services::index_briefing::derive_briefing_query`
- `crate::mcp::response::briefing::format_index_table`
- `crate::infra::session::SessionState`

These can be added as `use` statements at the top of `tools.rs` or inlined as full paths
within the handler. Follow the existing import style in `tools.rs`.

---

## Note on `self.session_registry`

The `context_briefing` handler accesses `self.session_registry` for `get_state()` and
`get_category_histogram()`. Verify that `self.session_registry` is available in the
MCP handler's `self` (the `UnimatrixServer` struct). If it is not currently held there,
inspect how `handle_context_search` accesses it in `listener.rs` — the UDS path passes
`session_registry` as a function parameter. The MCP handler may need the registry
passed via the server struct.

If `session_registry` is not in the MCP server struct, the session_state and histogram
lookups produce `None` (the code is `and_then` on an Option) and query derivation falls
to step 3 (topic fallback). This is correct graceful degradation — the handler still works
without session state; it just uses a weaker query.

However, for WA-2 histogram boost to work, `session_registry` must be accessible.
Check the existing MCP handler that calls `get_category_histogram` — if there is none,
this is a new integration that may require adding `session_registry` to the server struct.
Flag this as a potential implementation gap (see Open Questions below).

---

## Error Handling

- `IndexBriefingService::index()` returns `Err(ServiceError)`. Map via `rmcp::ErrorData::from()`
  (existing pattern: same as other handlers).
- Empty result (zero entries): `format_index_table(&[])` returns `""`. Return `CallToolResult::success`
  with empty content — this is NOT an error (R-10, AC-18 empty result is Ok).
- Session state lookup failure (session expired): `get_state()` returns `None` → `session_state = None`
  → `derive_briefing_query` falls to step 3. Silent degradation, no error.

---

## Key Test Scenarios

All in `tools.rs` `#[cfg(test)]` block or integration tests with `--features mcp-briefing`.

**T-CB-01** `context_briefing_active_only` (AC-06, R-08 requires `--features mcp-briefing`):
- Test database: one Active entry id=1, one Deprecated entry id=2, same topic
- Call context_briefing(role="dev", task="design the feature", feature="crt-027", ...)
- Assert: response text contains id=1 but NOT id=2
- Assert: response does NOT contain "[deprecated]"

**T-CB-02** `context_briefing_default_k_is_20` (AC-07):
- Insert 25 Active entries
- Call context_briefing with no k param
- Assert: result contains at most 20 entries (count rows in flat table)

**T-CB-03** `context_briefing_flat_table_no_section_headers` (AC-08):
- Call context_briefing
- Assert: result does NOT contain "## Decisions"
- Assert: result does NOT contain "## Injections"
- Assert: result does NOT contain "## Conventions"
- Assert: result does NOT contain "## Key Context"

**T-CB-04** `context_briefing_with_task_uses_task_as_query` (AC-09):
- Call context_briefing(task="implement spec writer for crt-027", ...)
- Log/trace assertion or mock: derive_briefing_query called with task="implement spec writer for crt-027"
- Or: insert an entry matching the task, assert it appears in top results

**T-CB-05** `context_briefing_session_id_applies_wa2_boost` (AC-11):
- Register session, accumulate category histogram for "decision" category
- Call context_briefing(session_id=<registered_sid>)
- Assert: "decision" category entries rank higher than without session_id
  (this requires careful test setup; may be an integration test rather than unit test)

**T-CB-06** `context_briefing_empty_result_returns_success` (R-10):
- Query with no matching entries
- Assert: CallToolResult is success (not error)
- Assert: content is empty string or minimal output

**T-CB-07** `context_briefing_role_field_ignored_no_error` (FR-14):
- Call context_briefing(role="architect", task="design auth", ...)
- Assert: no error (role is accepted but not used in query)
- Assert: response is well-formed flat table
