# Pseudocode: wire-protocol

## Purpose

Activate the CompactPayload and BriefingContent wire types (remove dead_code attrs) and add session_id to ContextSearch for injection tracking.

## Changes to wire.rs

### 1. Remove #[allow(dead_code)] from CompactPayload

```
// BEFORE:
#[allow(dead_code)]
CompactPayload { session_id, injected_entry_ids, role, feature, token_limit }

// AFTER:
CompactPayload { session_id, injected_entry_ids, role, feature, token_limit }
```

### 2. Remove #[allow(dead_code)] from BriefingContent

```
// BEFORE:
#[allow(dead_code)]
BriefingContent { content, token_count }

// AFTER:
BriefingContent { content, token_count }
```

### 3. Add session_id to ContextSearch

```
ContextSearch {
    query: String,
    #[serde(default)]
    session_id: Option<String>,  // NEW -- col-008
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
}
```

The `#[serde(default)]` ensures backward compatibility: existing hooks without session_id deserialize it as None.

## Error Handling

No new error paths. All changes are additive to existing serde types.

## Key Test Scenarios

1. ContextSearch round-trip with session_id present
2. ContextSearch round-trip with session_id absent (backward compat)
3. CompactPayload round-trip serialization/deserialization
4. BriefingContent round-trip serialization/deserialization
5. ContextSearch from JSON without session_id field deserializes to None
