# crt-027 Pseudocode Overview — WA-4 Proactive Knowledge Delivery

## Feature Summary

crt-027 delivers two interrelated improvements:

- **WA-4a**: Routes `SubagentStart` hook events through the existing `ContextSearch` pipeline so subagents receive injected knowledge before their first token via a `hookSpecificOutput` JSON envelope.
- **WA-4b**: Replaces `BriefingService` with `IndexBriefingService` that returns active-only entries in a flat indexed table (k=20 default), consumed by both the `context_briefing` MCP tool and `handle_compact_payload` UDS path.

---

## Components Involved

| Component | File | Change Type |
|-----------|------|-------------|
| wire-source-field | `crates/unimatrix-engine/src/wire.rs` | Additive field |
| hook-routing | `crates/unimatrix-server/src/uds/hook.rs` | New arm + new constant + new helper |
| listener-dispatch | `crates/unimatrix-server/src/uds/listener.rs` | Source field wiring + CompactPayload migration |
| index-briefing-service | `crates/unimatrix-server/src/services/index_briefing.rs` | New file (replaces briefing.rs) |
| service-layer-wiring | `crates/unimatrix-server/src/services/mod.rs` | Field rename + construction update |
| context-briefing-handler | `crates/unimatrix-server/src/mcp/tools.rs` | Handler replacement |
| index-entry-formatter | `crates/unimatrix-server/src/mcp/response/briefing.rs` | Type + function replacement |
| protocol-update | `.claude/protocols/uni/uni-delivery-protocol.md` | Text insertion × 6 |

---

## Sequencing Constraints

1. `wire-source-field` must be implemented first — `hook-routing` and `listener-dispatch` depend on the `source` field.
2. `index-entry-formatter` must be implemented before `index-briefing-service` — `IndexEntry` type is defined there and used by the service.
3. `index-briefing-service` must be implemented before `service-layer-wiring` — wiring requires the type to exist.
4. `service-layer-wiring` must be implemented before `listener-dispatch` and `context-briefing-handler` — both consume `services.briefing` which becomes `IndexBriefingService`.
5. `hook-routing` and `listener-dispatch` can be implemented in parallel after dependencies are met.
6. `protocol-update` is independent — text file edit, no code dependency.

Wave planning implication:
- Wave 1: wire-source-field + index-entry-formatter + protocol-update (independent)
- Wave 2: index-briefing-service (depends on IndexEntry from wave 1)
- Wave 3: service-layer-wiring (depends on IndexBriefingService from wave 2)
- Wave 4: hook-routing + listener-dispatch + context-briefing-handler (depend on wave 3)

---

## Shared Types

All types below are cross-component contracts. Their canonical definitions are in the files listed. No component should redefine them.

### `IndexEntry` — WA-5 contract type
**Defined in**: `crates/unimatrix-server/src/mcp/response/briefing.rs`
**Consumed by**: `IndexBriefingService::index()` return type, `format_compaction_payload`, `context_briefing` handler, `format_index_table`

```
pub struct IndexEntry {
    pub id: u64,
    pub topic: String,       // entry.topic — direct field, no join
    pub category: String,    // e.g., "decision", "pattern", "convention"
    pub confidence: f64,     // fused score: similarity + confidence + WA-2 boost
    pub snippet: String,     // first SNIPPET_CHARS chars of entry.content, UTF-8 char boundary safe
}

pub const SNIPPET_CHARS: usize = 150;
```

### `IndexBriefingParams`
**Defined in**: `crates/unimatrix-server/src/services/index_briefing.rs`
**Consumed by**: `IndexBriefingService::index()`, MCP handler, `handle_compact_payload`

```
pub(crate) struct IndexBriefingParams {
    pub query: String,
    pub k: usize,                    // default 20; not from UNIMATRIX_BRIEFING_K
    pub session_id: Option<String>,  // for WA-2 histogram boost via ServiceSearchParams
    pub max_tokens: Option<usize>,
}
```

### `IndexBriefingService`
**Defined in**: `crates/unimatrix-server/src/services/index_briefing.rs`
**Consumed by**: `ServiceLayer.briefing` field, `handle_compact_payload`, `context_briefing` handler

```
pub(crate) struct IndexBriefingService {
    entry_store: Arc<Store>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    default_k: usize,                              // 20, hardcoded
    effectiveness_state: EffectivenessStateHandle, // required, non-optional
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}
```

### `HookRequest::ContextSearch` (extended)
**Defined in**: `crates/unimatrix-engine/src/wire.rs`
**Consumed by**: `build_request` (hook.rs), `dispatch_request` (listener.rs)

```
ContextSearch {
    query: String,
    #[serde(default)] session_id: Option<String>,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
    #[serde(default)] source: Option<String>,  // NEW — None => "UserPromptSubmit"
}
```

### `format_compaction_payload` (updated signature)
**Defined in**: `crates/unimatrix-server/src/uds/listener.rs`

```
fn format_compaction_payload(
    entries: &[IndexEntry],
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,
) -> Option<String>
```

---

## Data Flow Between Components

### WA-4a: SubagentStart Injection

```
SubagentStart hook
  → hook.rs: build_request("SubagentStart", input)
      → reads input.extra["prompt_snippet"]
      → if empty/whitespace → generic_record_event → HookRequest::RecordEvent
      → else → HookRequest::ContextSearch { query, session_id, source: Some("SubagentStart") }
  → hook.rs: is_fire_and_forget = false (ContextSearch is always synchronous)
  → transport.request() via UDS
  → listener.rs: dispatch_request() ContextSearch arm
      → destructures source from HookRequest::ContextSearch
      → ObservationRow { hook: source.unwrap_or("UserPromptSubmit") }
      → handle_context_search(query, session_id, k, ...) [unchanged]
      → returns HookResponse::Entries { items, total_tokens }
  → hook.rs: response received
      → checks source field (carried via the request built earlier)
      → source == "SubagentStart" → write_stdout_subagent_inject(formatted_text)
          → writes JSON: {"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": text}}
      → source != "SubagentStart" → write_stdout(formatted_text) [plain text, unchanged]
```

### WA-4b: IndexBriefingService MCP path

```
context_briefing MCP call
  → tools.rs: BriefingParams parsed (role ignored, task optional)
  → derive_briefing_query(task, session_state_from_registry, topic) → query string
  → services.briefing.index(IndexBriefingParams { query, k=20, session_id })
      → SearchService.search(ServiceSearchParams { session_id, category_histogram, ... })
      → filters: status=Active only
      → returns Vec<IndexEntry> sorted by fused score descending
  → format_index_table(entries) → flat table string
  → CallToolResult::success(text)
```

### WA-4b: IndexBriefingService CompactPayload path

```
PreCompact hook
  → hook.rs: HookRequest::CompactPayload
  → listener.rs: handle_compact_payload()
      → session_state = session_registry.get_state(session_id)  [already held]
      → derive_briefing_query(task=None, session_state.as_ref(), topic=feature) → query
      → services.briefing.index(IndexBriefingParams { query, k=20, session_id })
      → format_compaction_payload(entries, role, feature, compaction_count, max_bytes, histogram)
          → header block
          → format_index_table(entries)  [within budget]
          → histogram block (if non-empty)
      → HookResponse::BriefingContent { content, token_count }
```

---

## Deleted Structures (Do Not Reference in New Code)

- `BriefingService` — deleted entirely from `services/briefing.rs`
- `BriefingParams` (service-layer struct) — deleted with `BriefingService`
- `BriefingResult` — deleted
- `InjectionSections` — deleted
- `InjectionEntry` (service-layer struct) — deleted
- `parse_semantic_k()` — deleted
- `CompactionCategories` — deleted from `listener.rs`
- `format_category_section()` — deleted from `listener.rs`
- `Briefing` struct (response) — deleted from `mcp/response/briefing.rs`
- `format_briefing()` — deleted from `mcp/response/briefing.rs`
- Budget constants: `DECISION_BUDGET_BYTES`, `INJECTION_BUDGET_BYTES`, `CONVENTION_BUDGET_BYTES`, `CONTEXT_BUDGET_BYTES` — deleted from `listener.rs`

## Retained Structures

- `format_retrospective_report()` in `mcp/response/briefing.rs` — NOT deleted
- `HookRequest::Briefing` wire variant — NOT deleted (C-04)
- `dispatch_request` arm for `HookRequest::Briefing` — NOT deleted
- `truncate_utf8()` in `listener.rs` — retained, used by new `format_compaction_payload`
- `format_injection()` in `hook.rs` — retained for `write_stdout` plain-text path
- `write_stdout()` in `hook.rs` — retained (UserPromptSubmit path unchanged)
- `handle_context_search()` in `listener.rs` — unchanged
- `MAX_COMPACTION_BYTES` in `listener.rs` — retained
