## ADR-002: SubagentStart Routing to ContextSearch and UserPromptSubmit Word-Count Guard

### Context

**SubagentStart routing:**

`SubagentStart` currently falls through to `generic_record_event` in `build_request`, producing
a fire-and-forget `RecordEvent` with no response content. The subagent starts without any
Unimatrix knowledge injection.

The `prompt_snippet` field in SubagentStart input (already extracted by
`extract_event_topic_signal` for topic tracking) contains the spawning prompt — a rich signal
for knowledge retrieval. Routing SubagentStart to `ContextSearch` reuses the entire existing
retrieval pipeline at zero additional server-side cost.

The `is_fire_and_forget` predicate in `hook.rs` (lines 58-64) enumerates only
`SessionRegister | SessionClose | RecordEvent | RecordEvents`. `ContextSearch` is NOT in
this set, so it is already synchronous. No code change to the transport or dispatch layer
is needed to make SubagentStart synchronous.

The hook response for ContextSearch is `HookResponse::Entries`, written to stdout by the
hook process. Whether Claude Code reads SubagentStart hook stdout and injects it into the
subagent context is unconfirmed (SR-01). The architecture degrades gracefully: if stdout is
ignored, the observation row is still written and topic_signal is still recorded. No error
occurs. Exit code is always 0 (FR-03.7).

**UserPromptSubmit word-count guard:**

The existing UserPromptSubmit routing has an empty-string guard but no minimum-length guard.
Short prompts ("yes", "approve", "ok continue") generate injection that is irrelevant to the
prompt content, wasting the 1400-byte MAX_INJECTION_BYTES budget and inserting noise into
the context window. These short prompts do not contain enough semantic signal for meaningful
retrieval.

A compile-time constant `MIN_QUERY_WORDS` (not a runtime parameter) is sufficient for the
current use case. Configuration exposure can be added if operational need arises.

The guard applies exclusively to `UserPromptSubmit`. SubagentStart retains only the existing
empty-string guard because `prompt_snippet` for a SubagentStart event is typically the full
spawning prompt (rich signal); a word-count guard here would suppress legitimate short-but-
specific role descriptions.

### Decision

**SubagentStart:** Add a `"SubagentStart"` arm in `build_request` before the `_` fallthrough:

```rust
"SubagentStart" => {
    let query = input.extra
        .get("prompt_snippet")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if query.trim().is_empty() {
        generic_record_event(event, session_id, input)
    } else {
        HookRequest::ContextSearch {
            query,
            session_id: input.session_id.clone(),  // parent session → WA-2 boost applies
            source: Some("SubagentStart".to_string()),
            role: None, task: None, feature: None, k: None, max_tokens: None,
        }
    }
}
```

`input.session_id` (the parent session) is passed as `session_id`. SubagentStart fires in
the parent session before the subagent is created, so this is the correct session for WA-2
histogram lookup.

**UserPromptSubmit word guard:**

```rust
const MIN_QUERY_WORDS: usize = 5;

"UserPromptSubmit" => {
    let query = input.prompt.clone().unwrap_or_default();
    if query.trim().is_empty() {
        return generic_record_event(event, session_id, input);
    }
    let word_count = query.trim().split_whitespace().count();
    if word_count < MIN_QUERY_WORDS {
        return generic_record_event(event, session_id, input);
    }
    HookRequest::ContextSearch {
        query,
        session_id: input.session_id.clone(),
        source: None,
        ...
    }
}
```

`MIN_QUERY_WORDS = 5` chosen as the minimum meaningful query. A 5-word prompt provides
enough tokens for embedding retrieval to return relevant results. The constant is public
within the crate so tests can reference it without hardcoding the magic number.

Both guards use `.trim()` before evaluation: `query.trim().is_empty()` for the empty
check, and `query.trim().split_whitespace().count()` for word counting. This ensures
a prompt consisting entirely of whitespace is treated as empty by both guards, and that
leading/trailing whitespace does not inflate the word count. The implementation value
held in `query` is the original (untrimmed) string — trimming is evaluation-only.

### Consequences

- SubagentStart now participates in knowledge injection. Value delivered even if Claude Code
  does not read stdout (observation and topic_signal are still recorded).
- The hook process never produces a non-zero exit code or panic from this change. The
  empty-string fallback to `generic_record_event` preserves FR-03.7.
- Short UserPromptSubmit prompts (< 5 words) no longer generate injection. This reduces
  noise for common single-word or short-phrase confirmations. Any prompt of >= 5 words
  routes exactly as before.
- `MIN_QUERY_WORDS` is named and visible — easy to locate if config exposure is needed.
- AC-02b requires unit tests on the word-count threshold (4-word prompt → RecordEvent,
  5-word prompt → ContextSearch).
