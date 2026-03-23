# crt-027: WA-4 Proactive Knowledge Delivery — Architecture

## System Overview

crt-027 delivers two interrelated improvements to Unimatrix's knowledge delivery surfaces:

**WA-4a** routes the `SubagentStart` hook event through the existing `HookRequest::ContextSearch`
pipeline, so subagents receive injected knowledge from stdout before their first token — identical
to the `UserPromptSubmit` path that already exists.

**WA-4b** replaces `BriefingService` with a new `IndexBriefingService` that returns a flat indexed
table (active-only, high-k=20, compact per-entry format) used by both the `context_briefing` MCP
tool and the `handle_compact_payload` UDS path. This lays the clean surface WA-5 (PreCompact
transcript prepend) depends on.

The feature also adds a minimum word-count guard on `UserPromptSubmit` routing to suppress
injection noise from short prompts ("yes", "ok", "approve").

---

## Component Breakdown

### 1. `unimatrix-engine/src/wire.rs` — Wire Protocol Extension

Adds an optional `source` field to `HookRequest::ContextSearch`:

```rust
ContextSearch {
    query: String,
    #[serde(default)]
    session_id: Option<String>,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
    #[serde(default)]
    source: Option<String>,   // NEW: "SubagentStart" | None => "UserPromptSubmit"
}
```

`#[serde(default)]` on `source` ensures backward compatibility: all existing callers that omit
`source` deserialize to `None`, and `dispatch_request` treats `None` as `"UserPromptSubmit"`.
This is a purely additive wire protocol change. See ADR-001.

### 2. `unimatrix-server/src/uds/hook.rs` — Hook Routing

Two changes:

**a) SubagentStart arm** — added before the `_` fallthrough in `build_request`:

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
            session_id: input.session_id.clone(),
            source: Some("SubagentStart".to_string()),
            role: None, task: None, feature: None, k: None, max_tokens: None,
        }
    }
}
```

`input.session_id` is the parent session (SubagentStart fires in the parent session context
before the subagent starts), so WA-2 histogram boost applies automatically via the existing
`get_category_histogram(session_id)` call in `handle_context_search`. See ADR-002.

**b) UserPromptSubmit word-count guard** — added inside the `"UserPromptSubmit"` arm:

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
    HookRequest::ContextSearch { query, session_id: input.session_id.clone(), source: None, ... }
}
```

Both guards use `.trim()` before evaluation: `query.trim().is_empty()` for the empty
check, and `query.trim().split_whitespace().count()` for word counting. A prompt
consisting entirely of whitespace is treated as empty; leading/trailing whitespace does
not inflate the word count.

`MIN_QUERY_WORDS` is a named compile-time constant (not a magic number) so future config
exposure is straightforward. SubagentStart is unaffected — it retains only the
`query.trim().is_empty()` guard. See ADR-002.

### 3. `unimatrix-server/src/uds/listener.rs` — Observation Tagging + CompactPayload Migration

**a) dispatch_request ContextSearch arm** — replaces the hardcoded `"UserPromptSubmit"` literal
in the `ObservationRow` construction with the `source` field value:

```rust
// Before:
hook: "UserPromptSubmit".to_string(),

// After:
hook: req_source.unwrap_or_else(|| "UserPromptSubmit".to_string()),
```

`req_source` is destructured from `HookRequest::ContextSearch { source, .. }`. The default
preserves backward compatibility for all existing paths that omit `source`.

**b) handle_compact_payload migration** — replaces the `BriefingService::assemble()` call with
a call to the new `IndexBriefingService::index()`. The `CompactionCategories` struct is removed;
`format_compaction_payload` is rewritten to accept `Vec<IndexEntry>` and emit the flat indexed
table format. The histogram block and session-context header are preserved (see SR-04
analysis below). See ADR-004.

### 4. `unimatrix-server/src/services/briefing.rs` — Full Replacement

`BriefingService` struct, `BriefingParams`, `BriefingResult`, `InjectionSections`,
`InjectionEntry`, `parse_semantic_k()`, and all their methods and tests are **deleted**.

The file is replaced entirely with `IndexBriefingService` — a service that:
- Accepts: `topic` (required), `session_id` (optional, for histogram boost), `k` (default 20)
- Queries `status = Active` entries only (deprecated entries suppressed at query time)
- Delegates to `SearchService` for embedding + fused scoring (WA-2 histogram boost included
  when `session_id` is resolved to a histogram)
- Returns `Vec<IndexEntry>` where each entry is `{id, topic, category, confidence, snippet}`

**Construction signature** (SR-03 resolution):

```rust
pub(crate) struct IndexBriefingService {
    entry_store: Arc<Store>,
    search: SearchService,          // carries its own EffectivenessStateHandle
    gateway: Arc<SecurityGateway>,
    default_k: usize,               // default 20, not from UNIMATRIX_BRIEFING_K
    effectiveness_state: EffectivenessStateHandle,   // ADR-001 crt-027
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}

impl IndexBriefingService {
    pub(crate) fn new(
        entry_store: Arc<Store>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
        effectiveness_state: EffectivenessStateHandle,  // required, non-optional
    ) -> Self { ... }
}
```

`effectiveness_state` is a **required, non-optional parameter** following the pattern
established by ADR-004 crt-018b (#1546). Missing wiring is a compile error. The cached
snapshot is initialized internally (same pattern as `BriefingService::new`).

`default_k` is hardcoded to 20 inside `new()`. `UNIMATRIX_BRIEFING_K` is deprecated and
explicitly not read by `IndexBriefingService`. The env var is documented as deprecated in
the removal commit. See ADR-003.

### 5. `unimatrix-server/src/services/mod.rs` — ServiceLayer Wiring Update

`ServiceLayer` field `briefing: BriefingService` becomes `briefing: IndexBriefingService`.
`BriefingService` re-export is removed; `IndexBriefingService` is exported in its place.
The `ServiceLayer::with_rate_config()` construction block replaces `BriefingService::new()`
with `IndexBriefingService::new()`, passing `Arc::clone(&effectiveness_state)`.
`parse_semantic_k()` call is removed. See ADR-003.

### 6. `unimatrix-server/src/mcp/tools.rs` — context_briefing Handler Update

Inside the `#[cfg(feature = "mcp-briefing")]` block:
- Replaces `BriefingService::assemble()` call with `IndexBriefingService::index()`
- Query derivation follows the three-step priority (see Data Flow section)
- Returns flat indexed table via `format_index_table()`
- `BriefingParams.role` is ignored; `BriefingParams.task` is used as step 1 of query
  derivation when present
- `session_id` is passed to `ServiceSearchParams.category_histogram` for WA-2 boost

The `BriefingParams` MCP schema struct retains `role` and `task` fields unchanged (backward
compatibility: callers that pass `role` continue to work; `role` is simply ignored).

### 7. `unimatrix-server/src/mcp/response/briefing.rs` — Response Formatter Replacement

The `Briefing` struct and `format_briefing()` function are deleted. A new
`format_index_table()` function is added, used by both MCP and UDS callers.

```rust
pub struct IndexEntry {
    pub id: u64,
    pub topic: String,
    pub category: String,
    pub confidence: f64,
    pub snippet: String,   // first 150 chars of entry.content
}

pub fn format_index_table(entries: &[IndexEntry]) -> String {
    // Flat indexed table — no section headers
    // Columns: row#, id, topic, category, confidence (2 decimal), snippet
    // Header + separator line + one row per entry
}
```

This type is the WA-5 contract surface. See ADR-005 and SR-06 resolution.

### 8. `.claude/protocols/uni/uni-delivery-protocol.md` — SM Protocol Update

After every `context_cycle(type: "phase-end", ...)` call and after
`context_cycle(type: "start", ...)`, SM calls:

```
context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)
```

`max_tokens: 1000` caps SM context budget per briefing call (SR-09 mitigation).
The briefing result is included as a knowledge package in each spawned agent's prompt.

---

## Component Interactions

```
SubagentStart hook
  → hook.rs: build_request() → HookRequest::ContextSearch { source: "SubagentStart" }
  → hook.rs: is_fire_and_forget → false (ContextSearch not in fire-and-forget set)
  → transport.request() → UDS socket
  → listener.rs: dispatch_request() ContextSearch arm
      → dispatch_request extracts source from struct
      → ObservationRow { hook: source.unwrap_or("UserPromptSubmit") }
  → handle_context_search() [unchanged]
  → HookResponse::Entries returned to hook process
  → hook.rs: write_stdout_subagent_inject() wraps formatted entries in JSON envelope:
      { "hookSpecificOutput": { "hookEventName": "SubagentStart",
                                "additionalContext": "<formatted entries text>" } }
  → Claude Code reads JSON stdout → injects additionalContext into subagent context

UserPromptSubmit hook (>= 5 trimmed words)
  → hook.rs: query.trim().split_whitespace().count() >= MIN_QUERY_WORDS guard passes
  → HookRequest::ContextSearch { source: None } (backward compat)
  → [same dispatch path as above, observation tagged "UserPromptSubmit"]
  → hook.rs: write_stdout() writes formatted entries as plain text (no JSON envelope)

UserPromptSubmit hook (< 5 trimmed words, or all-whitespace)
  → falls through to generic_record_event → RecordEvent (fire-and-forget)
  → no injection, no observation

context_briefing MCP tool
  → tools.rs: BriefingParams parsed (role ignored, task optional)
  → query derived: task > session signals > topic fallback
  → SessionRegistry.get_session_state(session_id) for topic_signals (step 2)
  → IndexBriefingService::index(query, session_id, k=20)
      → SearchService.search(ServiceSearchParams { category_histogram: ... })
      → status=Active filter applied
      → Vec<IndexEntry> returned
  → format_index_table() → flat table text
  → CallToolResult::success

PreCompact hook
  → hook.rs: HookRequest::CompactPayload (unchanged)
  → listener.rs: handle_compact_payload()
      → session state resolved (feature_cycle + topic_signals for query derivation)
      → IndexBriefingService::index(derived_query, session_id, k=20)
      → format_compaction_payload_index(entries, session_ctx, histogram) → flat table
  → HookResponse::BriefingContent
```

---

## Data Flow: Query Derivation (Shared Logic)

Both `context_briefing` MCP handler and `handle_compact_payload` use the same three-step
priority to derive the search query. This logic is extracted to a shared function
`derive_briefing_query()`:

```
1. If `task` is explicitly provided and non-empty: use task string directly.
2. If `session_id` is present and session has topic_signals:
   synthesize: feature_cycle + " " + top 3 topic_signals by vote count
3. Fall back to `topic` param (e.g., "crt-027").
```

For MCP: step 2 requires a `SessionRegistry` lookup via `session_id`.
For UDS (`handle_compact_payload`): step 2 reads the already-held `session_state` directly
(no registry lookup needed).

---

## Technology Decisions

See ADR files for full rationale. Summary:

| Decision | Choice | ADR |
|----------|--------|-----|
| Wire protocol source field | `#[serde(default)]` optional `source` on ContextSearch | ADR-001 |
| SubagentStart routing | Route to ContextSearch, graceful fallback to RecordEvent when empty | ADR-002 |
| `.trim()` on empty/word-count guards | Both SubagentStart empty guard and UserPromptSubmit word-count guard use `.trim()` | ADR-002 |
| UNIMATRIX_BRIEFING_K fate | Deprecated, not read by IndexBriefingService; k=20 hardcoded | ADR-003 |
| IndexBriefingService dependencies | EffectivenessStateHandle required, non-optional (ADR-004 crt-018b pattern) | ADR-003 |
| format_compaction_payload migration | Flat indexed table replacing section structure; 10 test invariants rewritten | ADR-004 |
| IndexEntry as typed WA-5 contract | Typed struct rather than inline string format | ADR-005 |
| SubagentStart stdout JSON envelope | SubagentStart response uses `hookSpecificOutput` JSON wrapper; UserPromptSubmit uses plain text | ADR-006 (#3251) |

---

## Integration Points

### Existing components consumed unchanged

- `SearchService.search(ServiceSearchParams)` — `IndexBriefingService` delegates all
  ranking (embedding, fused score, WA-2 histogram boost) to this service
- `SessionRegistry.get_category_histogram(session_id)` — WA-2 histogram lookup
- `SessionRegistry.get_session_state(session_id)` — topic_signals for query derivation
- `handle_context_search()` — SubagentStart requests arrive at this handler unchanged
- `is_fire_and_forget()` in `hook.rs` — ContextSearch is already synchronous; no change

### New interfaces introduced

- `IndexBriefingService::new(entry_store, search, gateway, effectiveness_state)` — replaces BriefingService
- `IndexBriefingService::index(params: IndexBriefingParams) -> Vec<IndexEntry>` — new primary method
- `IndexEntry { id, topic, category, confidence, snippet }` — WA-5 contract type
- `format_index_table(entries: &[IndexEntry]) -> String` — shared table formatter
- `derive_briefing_query(task, session_state, topic) -> String` — shared query derivation

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `HookRequest::ContextSearch.source` | `Option<String>`, `#[serde(default)]`, default → "UserPromptSubmit" | `unimatrix-engine/src/wire.rs` |
| `IndexBriefingService::new` | `(Arc<Store>, SearchService, Arc<SecurityGateway>, EffectivenessStateHandle) -> Self` | `services/briefing.rs` |
| `IndexBriefingService::index` | `(IndexBriefingParams, &AuditContext, Option<&CallerId>) -> Result<Vec<IndexEntry>, ServiceError>` | `services/briefing.rs` |
| `IndexBriefingParams` | `{ query: String, k: usize, session_id: Option<String>, max_tokens: Option<usize> }` | `services/briefing.rs` |
| `IndexEntry` | `{ id: u64, topic: String, category: String, confidence: f64, snippet: String }` | `mcp/response/briefing.rs` |
| `format_index_table` | `(entries: &[IndexEntry]) -> String` | `mcp/response/briefing.rs` |
| `derive_briefing_query` | `(task: Option<&str>, session_state: Option<&SessionState>, topic: &str) -> String` | shared (location TBD: `services/briefing.rs` or new `services/query_derive.rs`) |
| `MIN_QUERY_WORDS` | `const usize = 5`, compile-time constant; word count evaluated on `query.trim().split_whitespace()` | `uds/hook.rs` |
| `write_stdout_subagent_inject` | `(entries_text: &str) -> io::Result<()>`; wraps text in `hookSpecificOutput` JSON envelope and writes to stdout | `uds/hook.rs` |
| `format_compaction_payload` (updated) | signature gains `entries: &[IndexEntry]`; drops `categories: &CompactionCategories` | `uds/listener.rs` |

---

## SR-01: SubagentStart stdout injection — Confirmed with JSON Envelope Requirement

**Status: Confirmed. SubagentStart stdout injection is supported by Claude Code, but requires
a specific JSON envelope format. See ADR-006.**

Claude Code documentation confirms SubagentStart supports context injection via stdout.
However, SubagentStart does NOT use plain text stdout like UserPromptSubmit — it requires
a specific JSON envelope:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "SubagentStart",
    "additionalContext": "injected text here"
  }
}
```

The `additionalContext` field contains the same formatted index/entries text that
`UserPromptSubmit` would write as plain text. The server returns `HookResponse::Entries`
unchanged — wrapping is a hook-process-only formatting concern. See ADR-006 for the
full decision and `write_stdout_subagent_inject` helper specification.

The architecture is designed so WA-4a degrades safely if writing the JSON envelope fails:

- If Claude Code ignores SubagentStart stdout: the hook still routes to `ContextSearch`,
  the server still records the `ObservationRow` with `hook: "SubagentStart"`, and the
  topic_signal still feeds `extract_event_topic_signal`. The only loss is the stdout
  injection — there is no error, no non-zero exit code, no crash.
- The `observation` write is fire-and-forget in a spawn_blocking task (same as the
  existing UserPromptSubmit observation path). The hook process exits 0 regardless of
  the observation write outcome (FR-03.7 invariant preserved).
- The hook response (HookResponse::Entries) is still written to stdout via the existing
  `write_stdout()` path. If Claude Code ignores it, it is ignored silently.

**Fallback behavior:** WA-4a degrades to "SubagentStart is recorded as an observation
with topic_signal, but no knowledge is injected into the subagent." This is strictly better
than the current state (currently SubagentStart produces only a fire-and-forget RecordEvent
with no observation). The implementation delivers value even if stdout injection is not
supported.

**Post-delivery validation:** The spec writer should include a manual test step: spawn a
subagent and verify in its initial context whether Unimatrix injection text appears. If it
does not, file a spike to confirm Claude Code SubagentStart hook stdout behavior.

---

## SR-03: IndexBriefingService EffectivenessStateHandle Wiring

**Status: Resolved. Construction signature specified explicitly.**

Following ADR-004 crt-018b (entry #1546): `EffectivenessStateHandle` is a **required,
non-optional parameter** on `IndexBriefingService::new()`. The handle is passed as
`Arc::clone(&effectiveness_state)` in `ServiceLayer::with_rate_config()`, the same
`effectiveness_state` Arc already passed to `SearchService::new()`. No new handle is created.

The `IndexBriefingService` uses the generation-cached snapshot pattern (same as
`BriefingService` did): it initializes `cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>`
internally via `EffectivenessSnapshot::new_shared()`. The generation check runs on every
`index()` call using the same lock-ordering protocol as `BriefingService::assemble()`.

The `ServiceLayer` field changes from `briefing: BriefingService` to
`briefing: IndexBriefingService`. The `effectiveness_state_handle()` accessor on
`ServiceLayer` is unchanged. `spawn_background_tick()` signature is unchanged (it receives
the handle from `server.effectiveness_state`, not from `ServiceLayer.briefing`).

---

## SR-04: format_compaction_payload Test Invariants That Must Survive

The 10 existing tests assert on invariants that remain valid in the flat index format.
The **test must be rewritten** — the function signature changes — but the underlying
invariants must be covered:

| Test name | Invariant | Survives? | New form |
|-----------|-----------|-----------|----------|
| `format_payload_empty_categories_returns_none` | Empty entry list → `None` | YES | Empty `Vec<IndexEntry>` → `None` |
| `format_payload_header_present` | Output starts with compaction header | YES | `"--- Unimatrix Compaction Context ---\n"` still present |
| `format_payload_decisions_before_injections` | Decision entries before injection entries | NO | Section ordering removed. Replaced by: entries are sorted by confidence descending (flat table invariant). |
| `format_payload_sorted_by_confidence` | High-confidence entries appear before low-confidence | YES | Flat table rows ordered by confidence descending |
| `format_payload_budget_enforcement` | Output length <= max_bytes | YES | `result.len() <= MAX_COMPACTION_BYTES` |
| `format_payload_multibyte_utf8` | UTF-8 boundary safe at budget limit | YES | `truncate_utf8` still used for snippet truncation and overall limit |
| `format_payload_session_context` | Role/Feature/Compaction# lines present when provided | YES | Session context header block retained in `format_compaction_payload_index` |
| `format_payload_deprecated_indicator` | Deprecated entries show `[deprecated]` marker | NO | Flat index shows only Active entries (deprecated suppressed). Test replaced with: all entries in output have `status=Active`. |
| `format_payload_entry_id_metadata` | Entry ID present in output (e.g. as `<!-- id:42 -->`) | YES | Entry ID present in flat table `id` column |
| `format_payload_token_limit_override` | Custom budget respected | YES | Output <= custom budget bytes |
| `test_compact_payload_histogram_block_present_and_absent` | Non-empty histogram → "Recent session activity:" block; empty histogram → no block | YES | Histogram block retained in updated formatter |

**Summary of invariant changes:**
- Section ordering (decisions before injections before conventions) is **removed**. Replaced
  by confidence-descending sort across all categories.
- The deprecated indicator test is **removed**. Replaced by: flat index contains only Active
  entries (suppression test).
- All budget, UTF-8, header, entry ID, session context, and histogram invariants **survive**.

The spec writer must enumerate these 10 replacements as explicit ACs, not delete the tests.

---

## Open Questions

None. All open questions from scoping (OQ-1 through OQ-4) are resolved in the SCOPE.md.
SR-01 (SubagentStart stdout behavior) is resolved: confirmed that SubagentStart stdout
injection is supported when the `hookSpecificOutput` JSON envelope is used (ADR-006, #3251).
SR-05 (UNIMATRIX_BRIEFING_K fate) is resolved by ADR-003 (deprecated).
SR-06 (WA-5 format contract) is resolved by the typed `IndexEntry` struct (ADR-005).
SR-08 (cold-state fallback) is handled by step 3 of `derive_briefing_query` — the `topic`
param string is always available as a minimum fallback. SR-09 (SM context budget) is resolved
by the `max_tokens: 1000` cap on SM-initiated briefing calls in the protocol update.
