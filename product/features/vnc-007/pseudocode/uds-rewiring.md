# Pseudocode: UDS Rewiring (uds_listener.rs)

## Overview

Two changes in uds_listener.rs:

1. **CompactPayload handler**: Replace `handle_compact_payload` inline assembly with BriefingService delegation. Retain session state resolution, byte-to-token conversion, compaction count increment, response formatting.

2. **HookRequest::Briefing handler**: Wire the currently-unimplemented `HookRequest::Briefing` variant to BriefingService.

## CompactPayload Rewiring

### Current code: `handle_compact_payload` (lines 735-798) + `primary_path` + `fallback_path` + `format_compaction_payload` + `format_category_section`

These functions total ~300 lines. After rewiring, `handle_compact_payload` becomes ~50 lines. The helper functions `primary_path`, `fallback_path` are removed (logic moves to BriefingService). The formatting functions `format_compaction_payload`, `format_category_section`, `truncate_utf8` are RETAINED in uds_listener.rs because they are transport-specific formatting.

### Pseudocode

```rust
async fn handle_compact_payload(
    session_id: &str,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    session_registry: &SessionRegistry,
    services: &ServiceLayer,
) -> HookResponse {
    // 1. Byte/token budget (RETAINED — transport concern)
    let max_bytes = match token_limit {
        Some(limit) => ((limit as usize) * 4).min(MAX_COMPACTION_BYTES),
        None => MAX_COMPACTION_BYTES,
    };
    let max_tokens = max_bytes / 4;  // Convert bytes to tokens for BriefingService

    // 2. Session state resolution (RETAINED — transport concern)
    let session_state = session_registry.get_state(session_id);
    let effective_role = session_state.as_ref()
        .and_then(|s| s.role.clone()).or(role);
    let effective_feature = session_state.as_ref()
        .and_then(|s| s.feature.clone()).or(feature);
    let compaction_count = session_state.as_ref()
        .map(|s| s.compaction_count).unwrap_or(0);

    // 3. Determine path (RETAINED — transport concern)
    let has_injection_history = session_state.as_ref()
        .is_some_and(|s| !s.injection_history.is_empty());

    // 4. Build AuditContext (TRANSPORT-SPECIFIC)
    let audit_ctx = AuditContext {
        source: AuditSource::Uds {
            uid: 0, pid: None,
            session_id: session_id.to_string(),
        },
        caller_id: "uds-compact".to_string(),
        session_id: Some(session_id.to_string()),
        feature_cycle: None,
    };

    // 5. Build BriefingParams (NEW — replaces primary_path/fallback_path decision)
    let injection_history = if has_injection_history {
        let session = session_state.as_ref().unwrap();
        Some(session.injection_history.iter().map(|r| {
            services::briefing::InjectionEntry {
                entry_id: r.entry_id,
                confidence: r.confidence,
            }
        }).collect())
    } else {
        None
    };

    let briefing_params = services::briefing::BriefingParams {
        role: effective_role.clone(),
        task: None,
        feature: effective_feature.clone(),
        max_tokens,
        include_conventions: !has_injection_history,  // fallback includes conventions
        include_semantic: false,  // CRITICAL: no embedding, no vector search
        injection_history,
    };

    // 6. Delegate to BriefingService (NEW — replaces primary_path/fallback_path calls)
    let result = match services.briefing.assemble(briefing_params, &audit_ctx).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("compact payload assembly failed: {e}");
            return HookResponse::BriefingContent {
                content: String::new(),
                token_count: 0,
            };
        }
    };

    // 7. Convert BriefingResult to CompactionCategories for formatting (RETAINED)
    let categories = CompactionCategories {
        decisions: result.injection_sections.decisions,
        injections: result.injection_sections.injections,
        conventions: if has_injection_history {
            result.injection_sections.conventions
        } else {
            // Fallback path: conventions from BriefingResult.conventions
            result.conventions.into_iter()
                .map(|e| { let c = e.confidence; (e, c) })
                .collect()
        },
    };

    // 8. Format payload (RETAINED — transport-specific formatting)
    let content = format_compaction_payload(
        &categories, effective_role.as_deref(),
        effective_feature.as_deref(), compaction_count, max_bytes,
    );

    // 9. Increment compaction count (RETAINED — transport concern)
    session_registry.increment_compaction(session_id);

    let token_count = content.as_ref()
        .map(|c| (c.len() / 4) as u32).unwrap_or(0);

    HookResponse::BriefingContent {
        content: content.unwrap_or_default(),
        token_count,
    }
}
```

### Signature change

`handle_compact_payload` gains `services: &ServiceLayer` parameter. Loses `entry_store` (accessed via services.briefing).

### Removed functions

- `primary_path` — logic moved to BriefingService::process_injection_history
- `fallback_path` — logic moved to BriefingService convention query + feature sort

### Retained functions

- `format_compaction_payload` — transport-specific formatting
- `format_category_section` — used by format_compaction_payload
- `truncate_utf8` — used by format_compaction_payload
- `CompactionCategories` struct — used for formatting bridge

## HookRequest::Briefing Handler

### Current behavior: returns ERR_UNKNOWN_REQUEST (catch-all)

### New behavior: delegates to BriefingService

### dispatch_request changes

```rust
// In the match on request:

HookRequest::Briefing { role, task, feature, max_tokens } => {
    // Build AuditContext
    let audit_ctx = AuditContext {
        source: AuditSource::Uds {
            uid: 0, pid: None,
            session_id: String::new(),
        },
        caller_id: "uds-briefing".to_string(),
        session_id: None,
        feature_cycle: None,
    };

    // Default max_tokens to 3000 (same as MCP default)
    let effective_max_tokens = max_tokens.map(|v| v as usize).unwrap_or(3000);

    // Build BriefingParams
    let briefing_params = services::briefing::BriefingParams {
        role: Some(role),
        task: Some(task),
        feature,
        max_tokens: effective_max_tokens,
        include_conventions: true,
        include_semantic: true,
        injection_history: None,
    };

    match services.briefing.assemble(briefing_params, &audit_ctx).await {
        Ok(result) => {
            // Format as plain text (similar to MCP summary format)
            let mut content = String::new();
            // Conventions section
            if !result.conventions.is_empty() {
                content.push_str("## Conventions\n");
                for entry in &result.conventions {
                    content.push_str(&format!("- {}: {}\n", entry.title, entry.content));
                }
                content.push('\n');
            }
            // Relevant context section
            if !result.relevant_context.is_empty() {
                content.push_str("## Relevant Context\n");
                for (entry, score) in &result.relevant_context {
                    content.push_str(&format!("- {} ({:.2}): {}\n", entry.title, score, entry.content));
                }
            }
            let token_count = (content.len() / 4) as u32;
            HookResponse::BriefingContent { content, token_count }
        }
        Err(e) => HookResponse::Error {
            code: ERR_INVALID_PAYLOAD,
            message: format!("briefing failed: {e}"),
        },
    }
}
```

### dispatch_request catch-all update

The `_ =>` catch-all that returns ERR_UNKNOWN_REQUEST no longer catches `HookRequest::Briefing`. This is the desired behavior.

The test `dispatch_unknown_returns_error` currently sends `HookRequest::Briefing` to test the catch-all. After wiring, this test must use a different approach -- the test was already testing the catch-all, and Briefing was just a convenient unimplemented variant. Now all variants are handled, so the catch-all is dead code. The test can be updated to verify that all variants are handled (remove the catch-all test, or add a comment explaining all variants are now covered).

## Patterns

- CompactPayload handler follows same pattern as ContextSearch handler: session state -> build AuditContext -> delegate to service -> format response
- HookRequest::Briefing handler follows same pattern as ContextSearch handler
- CompactionCategories struct retained as formatting bridge (transport concern)
