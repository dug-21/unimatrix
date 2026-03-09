# col-018: context-search-observation Test Plan

## Unit Test Expectations

All tests use `dispatch_request()` directly with constructed `HookRequest::ContextSearch` values. Observation verification queries the SQLite observations table via the test store.

### T-01: Observation created for ContextSearch with valid session_id (R-01, AC-01)

```
Arrange: make_store(), make_registry(), standard deps
Act:     dispatch_request(ContextSearch { query: "implement col-018 feature", session_id: Some("sess-1"), ... })
         tokio::task::yield_now().await  // allow spawn_blocking to complete
Assert:  SELECT * FROM observations WHERE session_id = 'sess-1'
         - row exists
         - hook = "UserPromptSubmit"
         - input contains "implement col-018 feature"
         - ts_millis > 0
```

### T-03: Topic signal extracted from feature ID prompt (R-02, AC-02)

```
Arrange: make_store(), make_registry()
Act:     dispatch_request(ContextSearch { query: "work on col-018 design", session_id: Some("sess-1"), ... })
         yield
Assert:  SELECT topic_signal FROM observations WHERE session_id = 'sess-1'
         - topic_signal = "col-018"
```

### T-04: Topic signal NULL for generic prompt (R-02, AC-09)

```
Arrange: make_store(), make_registry()
Act:     dispatch_request(ContextSearch { query: "help me fix the bug", session_id: Some("sess-1"), ... })
         yield
Assert:  SELECT topic_signal FROM observations WHERE session_id = 'sess-1'
         - topic_signal IS NULL
```

### T-05: Topic signal from file path in prompt (R-02, AC-02)

```
Arrange: make_store(), make_registry()
Act:     dispatch_request(ContextSearch { query: "work on product/features/col-018/SCOPE.md", session_id: Some("sess-1"), ... })
         yield
Assert:  topic_signal = "col-018"
```

### T-06: Long prompt truncated to 4096 chars (R-03, AC-08)

```
Arrange: make_store(), make_registry()
         query = "a".repeat(5000)
Act:     dispatch_request(ContextSearch { query, session_id: Some("sess-1"), ... })
         yield
Assert:  SELECT input FROM observations WHERE session_id = 'sess-1'
         - input.len() == 4096
```

### T-07: Prompt at 4096 chars stored without truncation (R-03, AC-08)

```
Arrange: query = "a".repeat(4096)
Act:     dispatch_request(...)
Assert:  input.len() == 4096 (stored fully, not truncated further)
```

### T-08: session_id=None skips observation (R-04, AC-06)

```
Arrange: make_store(), make_registry()
Act:     dispatch_request(ContextSearch { query: "test query", session_id: None, ... })
Assert:  SELECT COUNT(*) FROM observations = 0
         response is HookResponse::Entries (search still works)
```

### T-09: Empty query skips observation (R-04, AC-07)

```
Arrange: make_store(), make_registry()
Act:     dispatch_request(ContextSearch { query: "", session_id: Some("sess-1"), ... })
Assert:  SELECT COUNT(*) FROM observations = 0
```

### T-10/T-11: Search results unchanged (R-05, AC-04)

```
Arrange: make_store() with no entries (embed not ready)
Act:     dispatch_request(ContextSearch { query: "test", session_id: Some("sess-1"), ... })
Assert:  response matches HookResponse::Entries { items: [], total_tokens: 0 }
         (same behavior as existing test dispatch_context_search_embed_not_ready)
```

### T-12: Topic signal accumulated in session registry (R-06, AC-03)

```
Arrange: make_store(), registry = SessionRegistry::new()
         registry.register_session("sess-1", None, None)
Act:     dispatch_request(ContextSearch { query: "implement col-018", session_id: Some("sess-1"), ... })
Assert:  registry.get_session_state("sess-1").topic_signals contains "col-018"
```

## Edge Cases

- T-02 (write failure logging): Structural -- the fire-and-forget pattern with `tracing::error!` matches col-012. Not independently testable without mocking the store.
- Empty string session_id: Rejected by `sanitize_session_id()` before observation logic (existing guard).

## Test Infrastructure

Reuse existing helpers: `make_store()`, `make_registry()`, `make_embed_service()`, `make_dispatch_deps()`, `make_pending()`, `make_services()`. No new test infrastructure needed.

After `dispatch_request()` calls that include observation writes, use `tokio::task::yield_now().await` followed by a small delay or direct synchronous query to allow `spawn_blocking` tasks to complete.
