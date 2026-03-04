# Pseudocode: usage-service

## File: `crates/unimatrix-server/src/services/usage.rs`

### Struct Definition

```
use std::sync::Arc;
use tokio::task::spawn_blocking;
use unimatrix_store::Store;
use crate::infra::usage_dedup::{UsageDedup, VoteAction};
use crate::infra::registry::TrustLevel;
use crate::services::strip_session_prefix;

pub(crate) struct UsageService {
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
}

pub(crate) enum AccessSource {
    McpTool,
    HookInjection,
    Briefing,
}

pub(crate) struct UsageContext {
    pub session_id: Option<String>,    // prefixed
    pub agent_id: Option<String>,
    pub helpful: Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level: Option<TrustLevel>,
}
```

### Constructor

```
impl UsageService {
    pub(crate) fn new(store: Arc<Store>, usage_dedup: Arc<UsageDedup>) -> Self {
        UsageService { store, usage_dedup }
    }
}
```

### record_access

```
pub(crate) fn record_access(
    &self,
    entry_ids: &[u64],
    source: AccessSource,
    ctx: UsageContext,
) {
    IF entry_ids is empty THEN
        RETURN immediately
    END IF

    MATCH source {
        AccessSource::McpTool => self.record_mcp_usage(entry_ids, ctx),
        AccessSource::HookInjection => self.record_hook_injection(entry_ids, ctx),
        AccessSource::Briefing => self.record_briefing_usage(entry_ids, ctx),
    }
}
```

### record_mcp_usage (internal)

This is a direct move of the body of `UnimatrixBackend::record_usage_for_entries()` from server.rs.

```
fn record_mcp_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    LET agent_id = ctx.agent_id.unwrap_or_default()

    // Step 1: Dedup access counts
    LET access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids)

    // Step 2: Determine vote actions
    LET mut helpful_ids = Vec::new()
    LET mut unhelpful_ids = Vec::new()
    LET mut dec_helpful_ids = Vec::new()
    LET mut dec_unhelpful_ids = Vec::new()

    IF let Some(helpful_value) = ctx.helpful THEN
        LET vote_actions = self.usage_dedup.check_votes(&agent_id, entry_ids, helpful_value)
        FOR each (id, action) in vote_actions:
            MATCH action {
                NewVote => IF helpful_value THEN helpful_ids.push(id) ELSE unhelpful_ids.push(id),
                CorrectedVote => IF helpful_value THEN {
                    helpful_ids.push(id); dec_unhelpful_ids.push(id)
                } ELSE {
                    unhelpful_ids.push(id); dec_helpful_ids.push(id)
                },
                NoOp => {},
            }
        END FOR
    END IF

    // Step 3: Record usage with confidence (spawn_blocking, fire-and-forget)
    LET store = Arc::clone(&self.store)
    LET all_ids = entry_ids.to_vec()
    // Clone all vecs for move into closure
    spawn_blocking(move || {
        store.record_usage_with_confidence(
            &all_ids, &access_ids, &helpful_ids, &unhelpful_ids,
            &dec_helpful_ids, &dec_unhelpful_ids,
            Some(&crate::confidence::compute_confidence),
        )
    })
    // Drop the JoinHandle (fire-and-forget). Log on error via match.

    // Step 4: Record feature entries if applicable (trust gating)
    IF let Some(feature_str) = ctx.feature_cycle THEN
        IF trust_level is System|Privileged|Internal THEN
            LET store = Arc::clone(&self.store)
            spawn_blocking(move || store.record_feature_entries(&feature_str, &ids))
            // Fire-and-forget
        END IF
    END IF
}
```

### record_hook_injection (internal)

Move of inline injection/co-access recording from `uds/listener.rs`.

```
fn record_hook_injection(&self, entry_ids: &[u64], ctx: UsageContext) {
    // Strip prefix before storage writes (ADR-004)
    LET raw_session_id = ctx.session_id
        .as_deref()
        .map(strip_session_prefix)
        .unwrap_or("")
        .to_string()

    LET agent_id = ctx.agent_id.clone()

    // 1. Injection log batch (needs InjectionLogRecord construction)
    // NOTE: Caller provides entry_ids only. Injection log needs confidence
    // and timestamp. Since UsageService doesn't have scores, the caller
    // constructs InjectionLogRecords before calling record_access.
    // REVISED: The injection path in listener.rs already writes injection
    // logs with confidence scores directly. UsageService handles ONLY
    // co-access pairs and feature entries for the hook injection path.
    //
    // ACTUALLY: Per ARCHITECTURE.md, HookInjection triggers:
    // insert_injection_log_batch + record_co_access_pairs + FEATURE_ENTRIES
    // But injection log needs per-entry confidence, which UsageService
    // doesn't receive. Solution: Keep injection log writing in listener.rs
    // (it needs per-entry similarity scores) and have UsageService handle
    // co-access + feature entries. OR: extend UsageContext with injection
    // records.
    //
    // DECISION: UsageService for HookInjection handles co-access pairs
    // and feature entries. Injection log batch remains in listener.rs
    // because it requires per-entry confidence data that UsageService
    // doesn't carry. This is a pragmatic split that matches SCOPE.md
    // AC-04 "UDS listener calls UsageService::record_access with
    // AccessSource::HookInjection" - record_access handles the post-
    // injection recording (co-access + feature), not the injection log itself.

    // Co-access pairs
    IF entry_ids.len() > 1 THEN
        LET pairs = generate_pairs(entry_ids, entry_ids.len())
        IF !pairs.is_empty() THEN
            LET store = Arc::clone(&self.store)
            spawn_blocking(move || {
                IF let Err(e) = store.record_co_access_pairs(&pairs) THEN
                    tracing::warn("co-access recording failed: {e}")
                END IF
            })
        END IF
    END IF

    // Feature entries
    IF let Some(feature_str) = ctx.feature_cycle THEN
        IF let Some(trust) = ctx.trust_level THEN
            IF trust is System|Privileged|Internal THEN
                LET store = Arc::clone(&self.store)
                LET ids = entry_ids.to_vec()
                spawn_blocking(move || store.record_feature_entries(&feature_str, &ids))
            END IF
        END IF
    END IF
}
```

### record_briefing_usage (internal)

```
fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    LET agent_id = ctx.agent_id.unwrap_or_default()

    // Dedup access count only (no votes for briefing)
    LET access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids)

    IF access_ids.is_empty() THEN
        RETURN
    END IF

    LET store = Arc::clone(&self.store)
    spawn_blocking(move || {
        // Increment access counts only, no votes
        store.record_usage_with_confidence(
            &access_ids, &access_ids,
            &[], &[], &[], &[],
            Some(&crate::confidence::compute_confidence),
        )
    })
    // Fire-and-forget
}
```

## File: `crates/unimatrix-server/src/services/mod.rs` (additions)

### UsageService integration

```
// Add module declaration
pub(crate) mod usage;
pub(crate) use usage::UsageService;

// Add to ServiceLayer
pub struct ServiceLayer {
    // ... existing fields ...
    pub(crate) usage: UsageService,
}

// In ServiceLayer::new(), after existing service construction:
LET usage = UsageService::new(
    Arc::clone(&store),
    // UsageDedup needs to be passed in or constructed here
    // It's currently created in server.rs (UnimatrixBackend::new)
    // Need to receive it as parameter to ServiceLayer::new()
);

// REVISED: Add usage_dedup: Arc<UsageDedup> as parameter to ServiceLayer::new()
```

## File: `crates/unimatrix-server/src/server.rs` (removals)

### Remove record_usage_for_entries

```
// DELETE the entire record_usage_for_entries method
// All callers migrate to services.usage.record_access()
```

## File: `crates/unimatrix-server/src/mcp/tools.rs` (changes)

### In context_search, context_lookup, context_get handlers

```
// BEFORE: self.record_usage_for_entries(...)
// AFTER:
self.services.usage.record_access(
    &entry_ids,
    AccessSource::McpTool,
    UsageContext {
        session_id: ctx.audit_ctx.session_id.clone(),
        agent_id: Some(ctx.agent_id.clone()),
        helpful: params.helpful,
        feature_cycle: params.feature.clone(),
        trust_level: Some(ctx.trust_level),
    },
);
```

### In context_briefing handler

```
self.services.usage.record_access(
    &briefing_result.entry_ids,
    AccessSource::Briefing,
    UsageContext {
        session_id: ctx.audit_ctx.session_id.clone(),
        agent_id: Some(ctx.agent_id.clone()),
        helpful: params.helpful,
        feature_cycle: params.feature.clone(),
        trust_level: Some(ctx.trust_level),
    },
);
```

## File: `crates/unimatrix-server/src/uds/listener.rs` (changes)

### In injection recording section

```
// Keep injection log batch write (needs per-entry confidence)
// REPLACE inline co-access and feature entry recording with:
usage_service.record_access(
    &entry_ids,
    AccessSource::HookInjection,
    UsageContext {
        session_id: Some(prefixed_session_id),
        agent_id: Some(agent_id),
        helpful: None,
        feature_cycle: feature.clone(),
        trust_level: Some(trust_level),
    },
);
```

## Open Questions

1. UsageDedup is currently owned by UnimatrixBackend in server.rs. It needs to be shared
   with UsageService. Solution: Pass `Arc<UsageDedup>` to ServiceLayer::new() and forward
   to UsageService. UnimatrixBackend keeps its own reference for any remaining uses.
2. The HookInjection path in listener.rs writes injection log records with per-entry
   confidence scores. UsageService record_access does not receive these scores. The
   injection log write stays in listener.rs; UsageService handles co-access + feature
   entries only for HookInjection source.
