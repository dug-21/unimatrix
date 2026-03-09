# col-018: Specification

## Functional Requirements

### FR-01: Observation Persistence for UserPromptSubmit

When the server receives a `HookRequest::ContextSearch` via UDS with a non-empty `query` and a present `session_id`, it MUST persist one observation row to the `observations` table before returning search results.

### FR-02: Observation Field Values

The observation row MUST have the following field values:

| Field | Value | Source |
|-------|-------|--------|
| `session_id` | From ContextSearch request | `session_id` field (required for observation) |
| `ts_millis` | Current time in milliseconds | `unix_now_secs() * 1000` |
| `hook` | `"UserPromptSubmit"` | Literal string |
| `tool` | `NULL` | Not a tool event |
| `input` | Prompt text, truncated to 4096 chars | `query` field from ContextSearch |
| `response_size` | `NULL` | Not applicable |
| `response_snippet` | `NULL` | Not applicable |
| `topic_signal` | Result of `extract_topic_signal(&query)` | Server-side extraction |

### FR-03: Server-Side Topic Extraction

The server MUST call `unimatrix_observe::extract_topic_signal(&query)` to extract a topic signal from the prompt text. This populates the `topic_signal` column. If no topic signal is found (function returns `None`), the column is stored as `NULL`.

### FR-04: Topic Signal Accumulation

When a topic signal is successfully extracted (non-None), the server MUST call `session_registry.record_topic_signal()` to accumulate the signal for col-017 session-level attribution. The call pattern MUST match the existing RecordEvent path:

```
session_registry.record_topic_signal(&session_id, signal, timestamp);
```

### FR-05: Fire-and-Forget Persistence

The observation write MUST be fire-and-forget using `spawn_blocking_fire_and_forget`. The search pipeline execution MUST NOT be blocked by the observation write. Failures MUST be logged via `tracing::error!`.

### FR-06: Search Pipeline Unaffected

The search pipeline (`handle_context_search()`) MUST continue to function identically. The observation write is a side effect that does not alter the search response.

### FR-07: Session ID Guard

When `session_id` is `None` on the `ContextSearch` request, the observation write and topic signal accumulation MUST be skipped. The search pipeline still executes normally.

### FR-08: Empty Query Guard

When `query` is empty, no observation is written. (In practice, empty-prompt UserPromptSubmit does not reach ContextSearch -- it falls through to RecordEvent on the hook side. This guard is defense-in-depth.)

### FR-09: Backward Compatibility

Existing behavior of all other `HookRequest` variants is unchanged. The `ContextSearch` response format is unchanged. No wire protocol changes.

## Domain Model

### ObservationRow (existing, col-012 + col-017)

```
struct ObservationRow {
    session_id: String,          // NOT NULL
    ts_millis: i64,              // Unix millis
    hook: String,                // Event type name
    tool: Option<String>,        // Tool name (NULL for non-tool events)
    input: Option<String>,       // Event input data
    response_size: Option<i64>,  // Response size in bytes
    response_snippet: Option<String>, // Response preview
    topic_signal: Option<String>,     // col-017: topic attribution signal
}
```

For col-018 UserPromptSubmit observations: `hook = "UserPromptSubmit"`, `tool = None`, `input = query text`, `response_size = None`, `response_snippet = None`, `topic_signal = extract_topic_signal(&query)`.

### observations table (existing, schema v10)

```sql
CREATE TABLE observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    ts_millis INTEGER NOT NULL,
    hook TEXT NOT NULL,
    tool TEXT,
    input TEXT,
    response_size INTEGER,
    response_snippet TEXT,
    topic_signal TEXT           -- col-017: added in v10
);
```

No schema changes required.

### HookRequest::ContextSearch (existing, unchanged)

```
HookRequest::ContextSearch {
    query: String,
    session_id: Option<String>,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
}
```

No changes to this variant.

## Acceptance Criteria

### AC-01: Observation Created

Given a `ContextSearch` request arrives via UDS with `query = "implement col-018 feature"` and `session_id = Some("sess-1")`, when the dispatch processes it, then one row is inserted into `observations` with `session_id = "sess-1"`, `hook = "UserPromptSubmit"`, `input` containing the query text.

### AC-02: Topic Signal Populated

Given a `ContextSearch` request with `query = "work on col-018 design"`, when the observation is written, then `topic_signal = "col-018"` (extracted by `extract_topic_signal`).

### AC-03: Topic Signal Accumulated

Given a `ContextSearch` request with `query = "implement col-018"` and `session_id = Some("sess-1")`, when dispatch processes it, then `session_registry.record_topic_signal("sess-1", "col-018", timestamp)` is called.

### AC-04: Search Results Returned

Given a `ContextSearch` request with valid query and session_id, when dispatch processes it, then the response is `HookResponse::Entries` (or empty entries on no matches) -- identical to pre-col-018 behavior.

### AC-05: No Latency Impact

Given a `ContextSearch` request, when the observation write is performed, then it runs in `spawn_blocking_fire_and_forget` and does not block the search pipeline.

### AC-06: Session ID None Skips Observation

Given a `ContextSearch` request with `session_id = None`, when dispatch processes it, then no observation row is inserted and the search pipeline executes normally.

### AC-07: Empty Query No Observation

Given a `ContextSearch` request with `query = ""` and `session_id = Some("sess-1")`, when dispatch processes it, then no observation row is inserted.

### AC-08: Input Truncation

Given a `ContextSearch` request with a query longer than 4096 characters, when the observation is written, then the `input` field is truncated to 4096 characters.

### AC-09: No Topic Signal for Generic Prompts

Given a `ContextSearch` request with `query = "help me fix the bug"` (no feature ID pattern), when the observation is written, then `topic_signal = NULL`.

### AC-10: Empty Prompt Path Unchanged

Given a `UserPromptSubmit` hook event with empty prompt, the hook side maps it to `RecordEvent` (not `ContextSearch`). This path is unchanged. Existing tests continue to pass.
