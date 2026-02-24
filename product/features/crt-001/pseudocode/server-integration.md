# Pseudocode: C6 Server Integration

## File: crates/unimatrix-server/src/server.rs

### UnimatrixServer State Addition

```
pub struct UnimatrixServer {
    // ... existing fields ...
    /// Session-scoped usage deduplication.
    pub(crate) usage_dedup: Arc<UsageDedup>,
}
```

### Constructor Update

Add to `new()`:
```
usage_dedup: Arc::new(UsageDedup::new()),
```

### record_usage_for_entries Method

```
impl UnimatrixServer {
    /// Record usage for a set of retrieved entries with dedup and trust gating.
    ///
    /// Fire-and-forget: errors are logged but never propagated.
    pub(crate) async fn record_usage_for_entries(
        &self,
        agent_id: &str,
        trust_level: TrustLevel,
        entry_ids: &[u64],
        helpful: Option<bool>,
        feature: Option<&str>,
    ) {
        if entry_ids.is_empty() {
            return;
        }

        // Step 1: Determine which entries need access_count increment
        let access_ids = self.usage_dedup.filter_access(agent_id, entry_ids);

        // Step 2: Determine vote actions (if helpful param provided)
        let mut helpful_ids = Vec::new();
        let mut unhelpful_ids = Vec::new();
        let mut decrement_helpful_ids = Vec::new();
        let mut decrement_unhelpful_ids = Vec::new();

        if let Some(helpful_value) = helpful {
            let vote_actions = self.usage_dedup.check_votes(agent_id, entry_ids, helpful_value);
            for (id, action) in vote_actions {
                match action {
                    VoteAction::NewVote => {
                        if helpful_value {
                            helpful_ids.push(id);
                        } else {
                            unhelpful_ids.push(id);
                        }
                    }
                    VoteAction::CorrectedVote => {
                        // Changing vote: increment new, decrement old
                        if helpful_value {
                            // Was unhelpful, now helpful
                            helpful_ids.push(id);
                            decrement_unhelpful_ids.push(id);
                        } else {
                            // Was helpful, now unhelpful
                            unhelpful_ids.push(id);
                            decrement_helpful_ids.push(id);
                        }
                    }
                    VoteAction::NoOp => {}
                }
            }
        }

        // Step 3: Record usage via Store (spawn_blocking)
        let store = Arc::clone(&self.store);
        let all_ids = entry_ids.to_vec();
        let access_ids_owned = access_ids;
        let helpful_owned = helpful_ids;
        let unhelpful_owned = unhelpful_ids;
        let dec_helpful_owned = decrement_helpful_ids;
        let dec_unhelpful_owned = decrement_unhelpful_ids;

        let usage_result = tokio::task::spawn_blocking(move || {
            store.record_usage(
                &all_ids,
                &access_ids_owned,
                &helpful_owned,
                &unhelpful_owned,
                &dec_helpful_owned,
                &dec_unhelpful_owned,
            )
        }).await;

        match usage_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::warn!("usage recording failed: {e}");
            }
            Err(e) => {
                tracing::warn!("usage recording task failed: {e}");
            }
        }

        // Step 4: Record feature entries if applicable (trust gating)
        if let Some(feature_str) = feature {
            if trust_level >= TrustLevel::Internal {
                let store = Arc::clone(&self.store);
                let feature_owned = feature_str.to_string();
                let ids = entry_ids.to_vec();

                let feature_result = tokio::task::spawn_blocking(move || {
                    store.record_feature_entries(&feature_owned, &ids)
                }).await;

                match feature_result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("feature entry recording failed: {e}");
                    }
                    Err(e) => {
                        tracing::warn!("feature entry recording task failed: {e}");
                    }
                }
            }
            // Restricted agents' feature params silently ignored (AC-17)
        }
    }
}
```

## File: crates/unimatrix-server/src/tools.rs

### Tool Parameter Extensions

Add to SearchParams, LookupParams, GetParams:
```
/// Feature context for usage tracking.
pub feature: Option<String>,
/// Whether the returned entries were helpful.
pub helpful: Option<bool>,
```

Add to BriefingParams (only helpful -- feature already exists):
```
/// Whether the returned entries were helpful.
pub helpful: Option<bool>,
```

### Tool Handler Modifications

For each of the 4 retrieval tools, AFTER the audit log call and BEFORE returning:

```
// Usage recording (fire-and-forget)
let entry_ids: Vec<u64> = results.iter().map(|r| r.id).collect();
self.record_usage_for_entries(
    &identity.agent_id,
    identity.trust_level,
    &entry_ids,
    params.helpful,
    params.feature.as_deref(),
).await;
```

For context_briefing: Collect the unique set of entry IDs from the final assembled result (deduped across lookup + search phases) before calling record_usage_for_entries.

## File: crates/unimatrix-server/src/validation.rs

### New Validation Functions

```
pub fn validate_feature(feature: &Option<String>) -> Result<(), ServerError> {
    if let Some(f) = feature {
        check_length("feature", f, MAX_FEATURE_LEN)?;
        check_control_chars("feature", f, false)?;
    }
    Ok(())
}

pub fn validate_helpful(helpful: &Option<bool>) -> Result<(), ServerError> {
    // No validation needed -- Option<bool> is self-validating from deserialization
    let _ = helpful;
    Ok(())
}
```

Add validate_feature/validate_helpful calls in validate_search_params, validate_lookup_params, validate_get_params, validate_briefing_params.

## TrustLevel Comparison

TrustLevel needs PartialOrd or a helper for `>= Internal`:
```
// TrustLevel is ordered: System > Privileged > Internal > Restricted
// Check: trust_level >= Internal means NOT Restricted
matches!(trust_level, TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal)
```
