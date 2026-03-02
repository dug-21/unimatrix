# Pseudocode Overview: col-008 Compaction Resilience

## Components

| Component | Module | Purpose |
|-----------|--------|---------|
| wire-protocol | `unimatrix-engine/src/wire.rs` | Activate CompactPayload/BriefingContent, add session_id to ContextSearch |
| session-registry | `unimatrix-server/src/session.rs` | Unified per-session state: injection history, co-access dedup, metadata |
| hook-handler | `unimatrix-server/src/hook.rs` | PreCompact arm, fire-and-forget exclusion, BriefingContent stdout |
| injection-tracking | `unimatrix-server/src/uds_listener.rs` | ContextSearch injection recording, SessionRegister/Close lifecycle, CoAccessDedup replacement |
| compact-dispatch | `unimatrix-server/src/uds_listener.rs` | CompactPayload handler, budget allocation, formatting, fallback |

## Data Flow

```
[Claude Code: PreCompact event]
    |
    v
[hook.rs: build_request("PreCompact", input)]
    -> HookRequest::CompactPayload { session_id, injected_entry_ids: [], ... }
    |
    v
[hook.rs: transport.request() -- synchronous, NOT fire-and-forget]
    |
    v
[uds_listener.rs: dispatch_request() -> CompactPayload arm]
    |
    v
[session_registry.get_state(session_id)]
    |
    +-- has injection_history --> PRIMARY PATH
    |     fetch entries by ID via entry_store.get()
    |     partition by category (decision/convention/other)
    |     allocate budget per ADR-003
    |     format_compaction_payload()
    |
    +-- no history --> FALLBACK PATH
          entry_store.query_by_category("decision")
          entry_store.query_by_category("convention")
          filter by status == Active
          sort by confidence desc
          format_compaction_payload()
    |
    v
[session_registry.increment_compaction(session_id)]
    |
    v
[HookResponse::BriefingContent { content, token_count }]
    |
    v
[hook.rs: write_stdout() -> print content to stdout]
```

## Injection Tracking Flow (ContextSearch extension)

```
[ContextSearch { query, session_id, ... }]
    |
    v
[handle_context_search() -- existing pipeline unchanged]
    |
    v
[After building HookResponse::Entries]
    |
    +-- session_id present and non-empty
    |     session_registry.record_injection(session_id, entry_ids_with_confidence)
    |
    +-- session_id absent or empty
          no-op (backward compatible)
    |
    v
[Co-access pair recording: use session_id from request instead of "hook-injection"]
    session_registry.check_and_insert_coaccess(session_id, entry_ids)
```

## Shared Types

- `SessionState` -- per-session state container (new, in session.rs)
- `InjectionRecord` -- single injection event (new, in session.rs)
- `SessionRegistry` -- thread-safe session state manager (new, in session.rs)
- `HookRequest::ContextSearch.session_id` -- new `Option<String>` field (wire.rs)

## Build Order

1. **wire-protocol** -- no dependencies, enables all other components
2. **session-registry** -- no runtime dependencies, standalone module
3. **hook-handler** -- depends on wire-protocol changes
4. **injection-tracking** -- depends on wire-protocol + session-registry
5. **compact-dispatch** -- depends on wire-protocol + session-registry + injection-tracking
