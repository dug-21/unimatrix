# col-018: context-search-observation Pseudocode

## Purpose

Add observation persistence and topic signal accumulation as a side-effect in the `HookRequest::ContextSearch` dispatch arm. When a ContextSearch request arrives with a valid session_id and non-empty query, write the prompt as an observation before executing the search pipeline.

## Modified Function: dispatch_request() -- ContextSearch arm

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Location**: Inside the `HookRequest::ContextSearch` match arm, after session_id validation and before `handle_context_search()`.

### Pseudocode

```
// Existing: capability check and session_id validation (lines 644-658)
// ... unchanged ...

// NEW: col-018 observation side-effect
let topic_signal = unimatrix_observe::extract_topic_signal(&query);

if let Some(ref sid) = session_id {
    if !query.is_empty() {
        // Accumulate topic signal in session registry (matches RecordEvent pattern)
        if let Some(ref signal) = topic_signal {
            session_registry.record_topic_signal(
                sid,
                signal.clone(),
                unix_now_secs(),
            );
        }

        // Build observation row directly (ADR-018-001: no synthetic ImplantEvent)
        let truncated_input: String = query.chars().take(4096).collect();
        let obs = ObservationRow {
            session_id: sid.clone(),
            ts_millis: (unix_now_secs() as i64).saturating_mul(1000),
            hook: "UserPromptSubmit".to_string(),
            tool: None,
            input: Some(truncated_input),
            response_size: None,
            response_snippet: None,
            topic_signal: topic_signal.clone(),
        };

        // Fire-and-forget persistence (matches RecordEvent pattern at lines 592-598)
        let store_for_obs = Arc::clone(store);
        spawn_blocking_fire_and_forget(move || {
            if let Err(e) = insert_observation(&store_for_obs, &obs) {
                tracing::error!(error = %e, "col-018: UserPromptSubmit observation write failed");
            }
        });
    }
}

// Existing: handle_context_search(...).await (unchanged)
```

## Guard Conditions

1. **session_id is None**: Skip observation write entirely. Search pipeline still runs. (ADR-018-002)
2. **query is empty**: Skip observation write. Defense-in-depth -- empty prompts map to RecordEvent on hook side, not ContextSearch.

## Error Handling

- `insert_observation()` failure: Logged via `tracing::error!`, not propagated. Fire-and-forget pattern (col-012 established).
- `extract_topic_signal()` returns `None` for generic prompts: `topic_signal` stored as NULL in DB. No error.
- Truncation: `.chars().take(4096).collect()` -- safe for multi-byte UTF-8, no panic possible.

## Key Test Scenarios

1. ContextSearch with valid session_id + query containing feature ID produces observation row with topic_signal
2. ContextSearch with valid session_id + generic query produces observation row with topic_signal = NULL
3. ContextSearch with session_id = None produces no observation row
4. ContextSearch with empty query produces no observation row
5. Long query (>4096 chars) produces observation with truncated input
6. Search results unchanged after observation side-effect added
7. Topic signal accumulated in session registry after dispatch
