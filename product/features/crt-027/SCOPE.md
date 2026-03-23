# crt-027: WA-4 Proactive Knowledge Delivery

## Problem Statement

The two primary knowledge delivery surfaces — SubagentStart hook injection and
`context_briefing` — are currently passive and unaware of session phase context,
which causes two distinct problems:

**WA-4a (SubagentStart):** When the SM spawns a subagent, the SubagentStart hook
fires but is routed to `generic_record_event`, producing only a fire-and-forget
`RecordEvent` with no response. The subagent receives zero knowledge from Unimatrix
before its first token. By the time the subagent calls `context_briefing` or a user
prompt triggers `UserPromptSubmit`, the subagent has already begun working without
relevant lessons, patterns, and decisions that were available in the parent session.

**WA-4b (`context_briefing`):** The current implementation is role-and-task-oriented
(`role`, `task` required fields), returns a small default k=3 semantic search over
all categories, includes conventions alongside context, and embeds a `role` framing
that was designed for human-readable orientation briefings. There is no index-format
output, no suppression of deprecated entries (the service relies on `status: Active`
filtering only at the store layer), and no high-k comprehensive scan. The SM protocol
has no prescribed call to `context_briefing` after phase transitions. The CompactPayload
path also calls `BriefingService::assemble()` and would benefit from a consistent
index format for the upcoming WA-5 transcript prepend.

## Goals

1. Route SubagentStart hook events to `HookRequest::ContextSearch` (same path as
   UserPromptSubmit), using `prompt_snippet` from the hook input as the query, and
   the parent session_id so WA-2 histogram boost applies.
2. Replace `BriefingService` as the backend for the `context_briefing` MCP tool with
   a new index implementation: active-only entries (deprecated suppressed), high default
   k=20, compact per-entry format `{id, name, category, confidence, topic, snippet}`.
3. Update the CompactPayload path in `handle_compact_payload` to use the new index
   format instead of calling `BriefingService::assemble()`, providing WA-5 a clean
   surface for transcript prepend.
4. Update the SM delivery protocol to call `context_briefing(topic)` immediately after
   each `context_cycle(type: "phase-end", ...)` call.
5. Add `MIN_QUERY_WORDS: usize = 5` compile-time constant in `hook.rs`. UserPromptSubmit
   with fewer than 5 trimmed words produces no injection.

## Non-Goals

- **WA-4a is NOT a phase-transition candidate cache.** The product vision's WA-4a
  description (candidate set rebuilt on phase transition, drawn from on PreToolUse)
  is deferred. This feature routes SubagentStart to ContextSearch only.
- **No phase-to-category config mapping.** Phase-conditioned ranking (e.g., "spec phase
  boosts pattern entries") is deferred to W3-1.
- **No `feature_cycle` ranking boost formula.** W3-1 owns the scoring changes.
- **No injection_history filter on `context_briefing`.** The new briefing is the
  comprehensive "entering a phase" package with no dedup.
- **No successor pointer display for orphaned deprecated entries.** Post-WA-4 refinement.
- **WA-5 (PreCompact transcript extraction).** WA-5 reads the transcript file in the
  hook process and prepends it before the server response. This feature delivers the
  index format surface WA-5 needs; WA-5 itself is a separate feature.
- **No changes to the existing `context_briefing` MCP tool signature.** The `role` and
  `task` fields remain present for backward compatibility; they are ignored by the new
  index path. Query derivation uses `task` if present, otherwise session state, otherwise `topic` — `role` is ignored.

## Background Research

### Q1: What does SubagentStart currently produce in hook.rs?

SubagentStart falls through to `generic_record_event` (hook.rs line 365 `_ =>` arm).
The event name does not match any explicit case in `build_request`'s match. The result
is `HookRequest::RecordEvent` — a fire-and-forget event. The server receives it, records
an `ImplantEvent` with topic_signal extracted from `prompt_snippet`, and returns
`HookResponse::Ack`. No content is returned to stdout. The subagent starts with nothing.

`extract_event_topic_signal` does have a SubagentStart arm (hook.rs lines 192-199) that
correctly extracts `input.extra["prompt_snippet"]` — but this is only called by
`generic_record_event` for topic tracking, not to build a ContextSearch request.

### Q2: What fields does HookRequest::ContextSearch carry?

From `unimatrix-engine/src/wire.rs`:
```rust
ContextSearch {
    query: String,
    session_id: Option<String>,   // #[serde(default)]
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
}
```

This maps cleanly from SubagentStart input:
- `query` = `input.extra["prompt_snippet"]` as str (already extracted in
  `extract_event_topic_signal`)
- `session_id` = `input.session_id` (the parent session — SubagentStart fires in the
  parent session context before the subagent is created)
- `role`, `task`, `feature`, `k`, `max_tokens` = None (defaults)

**Sync path confirmation:** `is_fire_and_forget` in hook.rs lines 58-64 only matches
`SessionRegister | SessionClose | RecordEvent | RecordEvents`. `ContextSearch` is NOT
in this set, so it is already synchronous (uses `transport.request()`). No code change
needed to make SubagentStart sync.

### Q3: What does handle_compact_payload consume from BriefingService::assemble()?

`handle_compact_payload` builds `BriefingParams` with `include_semantic: false`,
`include_conventions: !has_injection_history`, and optionally `injection_history`.
It then calls `BriefingService::assemble()` and consumes `result.injection_sections`
(three fields: `decisions`, `injections`, `conventions` — each `Vec<(EntryRecord, f64)>`)
plus `result.conventions` on the fallback path.

These are consumed by `CompactionCategories` and passed to `format_compaction_payload`.
The `format_compaction_payload` function produces the compaction block text.

The migration to index format here means: instead of calling `BriefingService::assemble()`,
`handle_compact_payload` calls the new index service and formats the result using an
updated `format_compaction_payload` that consumes index entries. This is the WA-5
dependency surface.

### Q4: Are there other callers of BriefingService::assemble() beyond tools.rs and handle_compact_payload?

Confirmed two callers only:
1. `mcp/tools.rs` — `context_briefing` MCP tool handler (line ~942)
2. `uds/listener.rs` `handle_compact_payload` — CompactPayload UDS path (line ~1209)

No other callers. The `BriefingService` struct and its `assemble()` method can be fully
replaced once both callers are migrated.

### Q5: What does the MCP context_briefing tool currently accept?

From `mcp/tools.rs` `BriefingParams` struct (lines 206-225):
```rust
pub struct BriefingParams {
    pub role: String,           // required
    pub task: String,           // required
    pub feature: Option<String>,
    pub max_tokens: Option<i64>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
    pub helpful: Option<bool>,
    pub session_id: Option<String>,  // #[serde(default)]
}
```

`session_id` is already present in the MCP tool params (added in a prior feature).
The tool does NOT currently use it for histogram boost — it passes `None` for
`injection_history` and calls `BriefingService::assemble()` with
`include_semantic: true`. The new index implementation will use `session_id` when
present to feed the WA-2 histogram boost through `ServiceSearchParams`.

### Q6: Does the SM delivery protocol have existing context_cycle(type: "phase-end") calls?

Yes. The delivery protocol in `uni-delivery-protocol.md` has phase-end calls at five
points:
- `phase-end, phase: "spec"` → after Stage 3a agents complete
- `phase-end, phase: "spec-review"` → on Gate 3a PASS
- `phase-end, phase: "develop"` → on Gate 3b PASS
- `phase-end, phase: "test"` → on Gate 3c PASS
- `phase-end, phase: "pr-review"` → after Phase 4 returns

The WA-4 update adds `context_briefing(topic)` immediately after each phase-end call
(before spawning the next phase's agents), plus after `context_cycle(type: "start")`.

### Q7: Does EntryRecord carry a `topic` field directly?

Yes. `unimatrix-store/src/schema.rs` `EntryRecord` struct (line 53):
```rust
pub topic: String,
```
`topic` is a first-class field on `EntryRecord`, stored in the ENTRIES table. No join
to FEATURE_ENTRIES or any other table is needed to retrieve it.

### Additional finding: observation recording side effect

The `dispatch_request` handler for `HookRequest::ContextSearch` hardcodes
`hook: "UserPromptSubmit"` in the observation row (listener.rs line 821). When
SubagentStart is routed to ContextSearch, the observation must be tagged correctly.

**Resolution (OQ-1):** Thread a `source: String` optional field through
`HookRequest::ContextSearch` with `#[serde(default)]` defaulting to `"UserPromptSubmit"`
for backward compatibility. SubagentStart-sourced requests set `source: "SubagentStart"`.
`dispatch_request` uses this field for the observation `hook` column instead of the
hardcoded literal. This is a wire protocol addition — all existing callers that omit
`source` continue to tag observations as `"UserPromptSubmit"` unchanged.

### Additional finding: empty prompt_snippet handling

If `prompt_snippet` is absent or empty in SubagentStart input, the query would be an
empty string. The existing UserPromptSubmit path already handles this: an empty `query`
causes `build_request` to fall through to `generic_record_event` (hook.rs lines 255-257).
The same guard applies for SubagentStart.

## Proposed Approach

### WA-4a: SubagentStart Hook Injection

In `build_request` (hook.rs), add a `"SubagentStart"` match arm before the `_` fallthrough:

```rust
"SubagentStart" => {
    let query = input.extra
        .get("prompt_snippet")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if query.is_empty() {
        generic_record_event(event, session_id, input)
    } else {
        HookRequest::ContextSearch {
            query,
            session_id: input.session_id.clone(),
            source: Some("SubagentStart".to_string()),
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
        }
    }
}
```

`HookRequest::ContextSearch` gains an optional `source` field (defaulting to
`"UserPromptSubmit"`) — the SubagentStart arm sets `source: "SubagentStart".to_string()`.
`dispatch_request` uses `source` for the observation `hook` column. No other changes
to `dispatch_request`, `handle_context_search`, or the transport layer.
The parent session_id flows in via `input.session_id`, which maps to the existing
`category_histogram` lookup in `handle_context_search`. WA-2 histogram boost applies
automatically.

The hook response for SubagentStart will be `HookResponse::Entries` (same as
UserPromptSubmit), which the hook process writes to stdout. Claude Code reads stdout
from the SubagentStart hook and injects it into the subagent context before the first
token.

**UserPromptSubmit minimum word count guard:**
Add a `MIN_QUERY_WORDS: usize = 5` compile-time constant in `hook.rs`. Before routing
UserPromptSubmit to `ContextSearch`, count whitespace-delimited words in the query string.
If `word_count < MIN_QUERY_WORDS`, fall through to `generic_record_event` (fire-and-forget,
no injection). Short prompts like "approve", "yes", "ok continue" produce no injection noise.
This guard applies to UserPromptSubmit only — SubagentStart retains its existing empty-string
guard unchanged. The constant is named for easy identification when config exposure is needed.

### WA-4b: context_briefing as Index

Replace `BriefingService::assemble()` in both callers with a new index service:

**New `IndexBriefingService`** (replaces `BriefingService` entirely — see OQ-4 resolution):
- Accepts: `topic` (required), `session_id` (optional, for histogram boost), `k` (default 20)
- Queries: `status = Active` entries only — deprecated suppressed at query time
- Ranking: existing fused score (similarity + confidence + WA-2 histogram boost) — no new phase weighting
- Returns: `Vec<IndexEntry>` where each entry is `{id, topic, category, confidence, snippet}`
  - `snippet` = first 150 chars of `entry.content`
  - `topic` = `entry.topic` (direct field, no join)

**Output format (resolved — OQ-2):** Flat indexed table, no section headers. Both the
MCP `context_briefing` response and `CompactPayload` use this exact format:
```
#    id   topic               cat             conf   snippet
─────────────────────────────────────────────────────────────────────────────────────
 1   2    product-vision      decision        0.60   Unimatrix is a self-learning context engine...
```
Columns: row number, id, topic, category, confidence (2 decimal places), 150-char snippet.
Active entries only. Deprecated suppressed. No section headers (Decisions / Injections /
Conventions are removed from the output).

**MCP tool (`context_briefing`) — query derivation (resolved — OQ-3):**
Priority order:
1. If `task` is explicitly provided: use it as the search query directly.
2. If no `task`: synthesize from session state — concatenate `feature_cycle` + top 3
   `topic_signals` by vote count, looked up from `SessionRegistry` using the `session_id`
   parameter.
3. If no session state or empty `topic_signals`: fall back to the `topic` parameter string
   (e.g., `"crt-027"`).

`session_id` passed into `ServiceSearchParams.session_id` for histogram boost.

**CompactPayload path (`handle_compact_payload`) — query derivation (resolved — OQ-3):**
Same three-step priority order. The UDS path already holds `session` state, so step 2
is available directly without a registry lookup. Both call sites share the same derivation
logic (extract to a shared function).

`format_compaction_payload` updated to consume `Vec<IndexEntry>` and emit the flat indexed
table format. The existing section structure (decisions / injections / conventions) is
removed — WA-5 reads the flat table directly.

**SM delivery protocol update (`uni-delivery-protocol.md`):**
- After each `context_cycle(type: "phase-end", ...)`, SM calls `context_briefing(topic="{feature-id}")`
- After `context_cycle(type: "start", ...)`, SM calls `context_briefing(topic="{feature-id}")`
- The briefing result is included in each spawned agent's context as a knowledge package

## Acceptance Criteria

- AC-01: SubagentStart hook with non-empty `prompt_snippet` produces `HookRequest::ContextSearch` in `build_request` (unit test on `build_request`).
- AC-02: SubagentStart hook with empty or absent `prompt_snippet` falls through to `generic_record_event` (fire-and-forget RecordEvent, no ContextSearch).
- AC-02b: `hook.rs` defines `MIN_QUERY_WORDS: usize = 5`. UserPromptSubmit with fewer than 5 whitespace-delimited words falls through to `generic_record_event` — no ContextSearch, no injection. UserPromptSubmit with 5 or more words routes to ContextSearch as before. SubagentStart is unaffected by this constant.
- AC-03: SubagentStart ContextSearch uses `input.session_id` (the parent session) — not the ppid fallback — so WA-2 histogram boost applies when a session is registered.
- AC-04: SubagentStart ContextSearch is synchronous (response written to stdout) — confirmed by the existing `is_fire_and_forget` logic which does not include ContextSearch.
- AC-05: `HookRequest::ContextSearch` carries an optional `source` field (default `"UserPromptSubmit"`). SubagentStart-sourced requests set `source: "SubagentStart"`. `dispatch_request` uses this field for the observation `hook` column (not a hardcoded literal). All existing callers that omit `source` remain unaffected.
- AC-06: `context_briefing` MCP tool returns only `status=Active` entries — deprecated entries do not appear in the response.
- AC-07: `context_briefing` default k is 20 (not 3); `UNIMATRIX_BRIEFING_K` env var no longer controls this path (or is overridden to 20 minimum for briefing).
- AC-08: Both `context_briefing` and `handle_compact_payload` output a flat indexed table with columns: row number, id, topic, category, confidence (2 decimal places), 150-char snippet. No section headers.
- AC-09: `context_briefing` query derivation follows the three-step priority: (1) explicit `task` param, (2) synthesized from `feature_cycle` + top 3 `topic_signals` via `SessionRegistry` when `session_id` is provided, (3) `topic` param fallback.
- AC-10: `handle_compact_payload` query derivation follows the same three-step priority using the already-held session state (no registry lookup needed for step 2).
- AC-11: `context_briefing` `session_id` parameter, when provided, routes histogram boost through `ServiceSearchParams.category_histogram`.
- AC-12: `handle_compact_payload` no longer calls `BriefingService::assemble()`; it calls the new index path and produces `HookResponse::BriefingContent` with the flat indexed table format.
- AC-13: `BriefingService` struct, all its methods, its tests, and any re-exports are deleted — no dead code remains.
- AC-14: `uni-delivery-protocol.md` includes `context_briefing(topic="{feature-id}")` call immediately after every `context_cycle(type: "phase-end", ...)` call and after `context_cycle(type: "start", ...)`.
- AC-15: All existing tests in `listener.rs`, `hook.rs`, `tools.rs` (briefing path) continue to pass.

## Constraints

**WA-5 dependency (D-8):** WA-5 (PreCompact transcript restoration) depends on this
feature completing the CompactPayload migration (AC-12). WA-5 needs a clean
`BriefingContent` response from the server that it can prepend the transcript block to.
With WA-4 delivering the flat indexed table on the CompactPayload path, WA-5 can prepend
without parsing section structure. WA-4 must land before WA-5 is implemented.

**W3-1 handoff:** Phase-conditioned ranking (phase-to-category affinity scoring) is
explicitly deferred to W3-1. The new `context_briefing` index path must be designed
so ranking can be extended without replacing it — `ServiceSearchParams` is already
extensible.

**Hook exit-0 contract:** The SubagentStart hook must never write a non-zero exit code
and must never crash. If `prompt_snippet` is absent, empty, or produces a search error,
the hook must degrade gracefully (empty response or generic_record_event fallthrough).
This is the existing invariant (FR-03.7 in hook.rs comments).

**BriefingService removal (resolved — OQ-4):** `BriefingService` is deleted in full in
this feature once both callers are migrated — no deferral, no dead code. The existing
`HookRequest::Briefing` variant (in wire.rs) is marked `#[allow(dead_code)]` and is
handled in `dispatch_request` but not used by any hook; it is separate from
`BriefingService` and does not need to be removed in this feature.

**mcp-briefing feature flag:** The `context_briefing` MCP tool is conditionally compiled
behind `#[cfg(feature = "mcp-briefing")]`. The new index implementation must respect
this guard.

**`UNIMATRIX_BRIEFING_K` env var:** Currently governs semantic k for the old BriefingService
(default 3, clamped to [1, 20]). The new briefing index has default k=20. The env var
relationship should be documented or superceded — not silently inherited.

## Design Decisions

All open questions from scoping are resolved. The resolutions are incorporated inline
in the Proposed Approach and Acceptance Criteria above. Summary:

- **OQ-1 (SubagentStart observation tagging):** Thread optional `source: String` through
  `HookRequest::ContextSearch` (`#[serde(default)]` = `"UserPromptSubmit"`). SubagentStart
  sets `source: "SubagentStart"`. `dispatch_request` uses the field instead of the
  hardcoded literal. Backward-compatible wire protocol addition.

- **OQ-2 (CompactPayload format):** Go flat. Both `context_briefing` and `CompactPayload`
  output a flat indexed table (row, id, topic, category, confidence at 2 decimals, 150-char
  snippet). Section headers (Decisions / Injections / Conventions) are removed. Active
  entries only.

- **OQ-3 (Briefing query when `task` absent):** Three-step priority: (1) explicit `task`
  param, (2) synthesize from `feature_cycle` + top 3 `topic_signals` by vote count from
  session state, (3) fall back to `topic` param string. Same logic in both call sites
  (MCP handler looks up `SessionRegistry`; UDS path uses held session state directly).

- **OQ-4 (`BriefingService` removal):** Remove in this feature. Struct, methods, tests,
  and re-exports all deleted once both callers are migrated. No dead code or deferred
  cleanup.

## Tracking

https://github.com/dug-21/unimatrix/issues/349
