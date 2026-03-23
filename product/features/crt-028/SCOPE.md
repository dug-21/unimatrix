# crt-028: WA-5 PreCompact Transcript Restoration

## Problem Statement

When Claude Code compacts the context window, the agent loses its working context — what
it was doing, what files it was touching, what tool calls it had made. The `PreCompact` hook
fires before compaction and already receives `transcript_path` in its stdin JSON, but this
field is parsed by `HookInput` and then completely ignored. The hook sends a `CompactPayload`
request to the server, which returns a Unimatrix knowledge briefing — but no recent
conversation history. The agent resumes post-compaction with Unimatrix knowledge but no
continuity of the immediate task context.

This feature reads the transcript locally in the hook process and prepends a structured
context restoration block to the compaction output, eliminating as much of the continuity
gap as possible. It also fixes two small security/test gaps left open by the crt-027 security
review (GH #354 and #355).

## Goals

1. Extract the last k user/assistant exchange pairs from the session transcript at PreCompact
   time, using a type-aware reverse-scan strategy (D-2).
2. Prepend the extracted transcript block to the server's `BriefingContent` response before
   writing stdout, giving the agent both task continuity and knowledge context (D-5).
3. Operate with a separate `MAX_PRECOMPACT_BYTES` budget (~3000 bytes) for the transcript
   block, distinct from the general `MAX_INJECTION_BYTES` (1400) used on other hook paths (D-4).
4. Degrade silently on all transcript read failures — the hook must always exit 0 (D-6).
5. Replace the full-content briefing at compaction with the flat indexed table format via
   `IndexBriefingService`, the WA-5 contract surface established by crt-027 (D-8).
6. Fix GH #354: allowlist/length-cap the `source` field before writing to the `hook` column
   in `listener.rs` (D-9).
7. Fix GH #355: add quarantine exclusion regression test to `IndexBriefingService` and doc
   comment on validation chain delegation (D-9).

## Non-Goals

- Server-side transcript storage: the transcript is read locally by the hook process only.
  No `transcript_path` is sent to the server; no server schema changes are required.
- Persistent transcript summaries: the extracted block is ephemeral, prepended once to the
  compaction output. It is not stored in Unimatrix or in any analytics table.
- PostCompact hook changes: restoration only happens at PreCompact. PostCompact is out of
  scope.
- Full verbatim replay: tool_result blocks from user turns are skipped; only text blocks and
  compact tool_use+result pairs are extracted (D-2).
- Configurable k or budget: `MAX_PRECOMPACT_BYTES` is a compile-time constant; k is derived
  by filling the budget in priority order. No runtime configuration.
- GH #303, #305, or any other pre-existing open issues not listed in D-9.

## Background Research

### Existing PreCompact path (`uds/hook.rs`)

The `"PreCompact"` arm in `build_request()` currently produces:

```rust
"PreCompact" => HookRequest::CompactPayload {
    session_id,
    injected_entry_ids: vec![],
    role: None,
    feature: None,
    token_limit: None,
},
```

`input.transcript_path` is available at this point as `Option<String>` on `HookInput` (field
present, `#[serde(default)]`, fully parsed). It is not read or used anywhere in the current
hook code. All downstream processing ignores it.

The hook process has no tokio runtime (ADR-002 — sub-50ms latency requirement); all I/O is
synchronous `std::io`.

### HookInput struct (`unimatrix-engine/src/wire.rs`)

`HookInput.transcript_path: Option<String>` — field exists, `#[serde(default)]`,
fully deserialized. No changes to this struct are needed.

### CompactPayload server response (`uds/listener.rs` — `handle_compact_payload`)

As of crt-027 (must be merged before this feature), `handle_compact_payload`:
- Calls `IndexBriefingService::index()` (replaced from `BriefingService::assemble()`)
- Returns `HookResponse::BriefingContent { content, token_count }`
- `content` is a flat indexed table via `format_index_table(&entries)` plus session context
  header and histogram block

`write_stdout()` in `hook.rs` handles `HookResponse::BriefingContent` by printing `content`
if non-empty. WA-5 must prepend the transcript block to `content` before printing.

### IndexBriefingService contract (crt-027 ADR-005)

`IndexEntry { id, topic, category, confidence, snippet }` — typed WA-5 contract surface.
`format_index_table(entries: &[IndexEntry]) -> String` — canonical formatter, always compiled
(not gated by `mcp-briefing` feature flag). WA-5 prepends transcript content as a string
before or around the call to `write_stdout()` — it does not parse the rendered table.

### Transcript JSONL format (ASS-028 Recommendation 2)

File: `~/.claude/projects/{project-slug}/{session-uuid}.jsonl`
Timing: PreCompact fires before compaction; file is intact at hook execution time.

Record types relevant to extraction:
- `type: "user"` — `message.content` array with `type: "text"` blocks (human prompt) and
  `type: "tool_result"` blocks (tool responses). Only `type: "text"` blocks are extracted.
- `type: "assistant"` — `message.content` array with `type: "text"` blocks (response text),
  `type: "tool_use"` blocks (tool calls with `name` and `input`), `type: "thinking"` blocks
  (skipped).

### GH #354 — `source` field write site

Location: `listener.rs` `dispatch_request` ContextSearch arm, `ObservationRow` construction:

```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

`source` is `Option<String>` from `HookRequest::ContextSearch`. No length or content
validation before writing to the observations `hook TEXT NOT NULL` column. A long or
adversarial value is written verbatim. Fix: allowlist `{"UserPromptSubmit", "SubagentStart"}`
with a fallback to `"UserPromptSubmit"` for unknown/missing values. This is a single-site
change in `listener.rs`.

### GH #355 — `IndexBriefingService` quarantine exclusion regression test + doc comment

Location: `services/index_briefing.rs`, `index()` method.

The deleted `BriefingService` had an explicit test (T-BS-08) for quarantine exclusion.
`IndexBriefingService` relies on `RetrievalMode::Strict` (via `SearchService`) plus a
post-filter `se.entry.status == Status::Active`. The behavior is correct but no test in
`index_briefing.rs` directly verifies that a `ScoredEntry` with `status: Quarantined` is
excluded by the post-filter at the `index()` level.

Additionally, `IndexBriefingService::index()` does not document that input validation is
delegated to `SearchService.search()` → `gateway.validate_search_query()`. A future
developer removing the `SearchService` call could believe validation is elsewhere.

Fix:
1. Add a test confirming quarantine exclusion through the post-filter.
2. Add a doc comment on `index()` noting that query validation is delegated to SearchService.

### Constants in scope

| Constant | Current value | Location |
|----------|--------------|----------|
| `MAX_INJECTION_BYTES` | 1400 | `uds/hook.rs` |
| `MAX_COMPACTION_BYTES` | 8000 | `uds/listener.rs` |
| `MAX_PRECOMPACT_BYTES` | NEW ~3000 | `uds/hook.rs` (to add) |
| `SNIPPET_CHARS` | 150 | `mcp/response/briefing.rs` (crt-027) |

## Settled Design Decisions

These decisions were made in prior human conversation and are not open for re-discussion.

**D-1**: Transcript restoration is the primary compaction output. The hook reads
`input.transcript_path` locally before sending `CompactPayload`. No server round-trip for
transcript content.

**D-2**: Type-aware extraction — not verbatim message replay:
- User turns: `type: "text"` content blocks verbatim; `type: "tool_result"` blocks skipped.
- Assistant turns: `type: "text"` blocks verbatim + each `type: "tool_use"` (name + key
  param) paired with its `tool_result` truncated to ~300 bytes.
- Most-recent turns first; fill budget in priority order.

**D-3**: Compact tool results, not omitted. ~300-byte snippet per result. Grep/Glob results
are compact enough to survive whole. Gives the agent enough context to decide whether to
re-fetch.

**D-4**: Separate `MAX_PRECOMPACT_BYTES` constant (~3000 bytes) for the transcript block —
distinct from `MAX_INJECTION_BYTES` (1400) on other hook paths.

**D-5**: Transcript block prepends the server response. Task/work continuity before knowledge
context.

**D-6**: Graceful degradation on all transcript read failures — missing path, unreadable file,
malformed JSONL all produce silent skip. Hook never fails (FR-03.7 invariant preserved).

**D-7**: All transcript extraction logic in `hook.rs`. No server changes required for this
part.

**D-8**: The Unimatrix knowledge component at compaction moves to index format (not full
content). `CompactPayload` routes through `IndexBriefingService` (built in crt-027).
Full-content briefing is replaced. This is the server-side change — already delivered by
crt-027 and consumed here.

**D-9**: GH #354 (allowlist/length-cap `source` field before `hook` column write) and GH #355
(quarantine exclusion regression test + doc comment on validation chain delegation) are in
scope.

## Acceptance Criteria

- AC-01: When `input.transcript_path` is present and readable, the PreCompact hook stdout
  begins with a transcript restoration block before the Unimatrix briefing content.
- AC-02: Transcript restoration block includes user text turns and assistant text turns from
  the last k exchanges, most-recent first.
- AC-03: Tool use entries from assistant turns are included as compact `[tool: name(key_param)
  → snippet]` pairs truncated to ~300 bytes.
- AC-04: `type: "tool_result"` blocks in user turns are skipped (not included in restoration
  block).
- AC-05: The combined transcript + briefing output does not exceed `MAX_PRECOMPACT_BYTES` for
  the transcript portion.
- AC-06: When `input.transcript_path` is `None`, the hook behaves as if the transcript block
  is absent — no error, no output change, briefing content is written normally.
- AC-07: When the transcript file does not exist or cannot be opened, the hook silently skips
  transcript restoration and writes only the briefing content. Exit code is 0.
- AC-08: When the transcript file contains malformed JSONL (unparseable lines), those lines
  are skipped silently; any parseable lines are used. Exit code is 0.
- AC-09: When the transcript file contains no user/assistant pairs (e.g., post-compaction
  empty file), the transcript block is omitted and only the briefing content is written.
- AC-10: `MAX_PRECOMPACT_BYTES` is a named compile-time constant in `hook.rs`, distinct from
  `MAX_INJECTION_BYTES`.
- AC-11: The `source` field in `HookRequest::ContextSearch` is validated against an allowlist
  `{"UserPromptSubmit", "SubagentStart"}` before being written to the observations `hook`
  column; unrecognized values fall back to `"UserPromptSubmit"`.
- AC-12: A regression test in `index_briefing.rs` verifies that a `ScoredEntry` with
  `status: Quarantined` is excluded by the `index()` post-filter (status == Active check).
- AC-13: `IndexBriefingService::index()` has a doc comment stating that query validation is
  delegated to `SearchService.search()` → `gateway.validate_search_query()`.
- AC-14: All existing hook.rs tests continue to pass. No `write_stdout` behavior changes for
  non-PreCompact events.
- AC-15: Hook always exits 0 regardless of transcript read outcome (FR-03.7 invariant).

## GH #354 Scope

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Location**: `dispatch_request` ContextSearch arm, `ObservationRow` construction.
**Current code**:
```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```
**Fix**: Replace with allowlist validation. Known values: `"UserPromptSubmit"`,
`"SubagentStart"`. Any other value (including excessively long values) falls back to
`"UserPromptSubmit"`. A helper constant or inline match is acceptable.

## GH #355 Scope

**File**: `crates/unimatrix-server/src/services/index_briefing.rs`
**Changes**:
1. Add test: store an entry with `status: Quarantined`, run the post-filter step, assert
   the entry does not appear in `index()` output. This mirrors the deleted T-BS-08 from
   `BriefingService`.
2. Add doc comment on `index()`: "Input validation is delegated to `SearchService.search()`
   which calls `self.gateway.validate_search_query()` (S3, length ≤ 10,000 chars, control
   characters rejected, k bounds enforced). Do not remove the SearchService delegation
   without adding an equivalent validation call."

## Component Inventory

| File | Change type | Reason |
|------|-------------|--------|
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | Add `MAX_PRECOMPACT_BYTES` constant; add transcript read + extraction in PreCompact arm; prepend transcript block to `BriefingContent` response in `write_stdout` or a new `write_stdout_precompact` helper |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | GH #354: allowlist `source` field before `hook` column write |
| `crates/unimatrix-server/src/services/index_briefing.rs` | Modify | GH #355: quarantine exclusion regression test + doc comment on validation delegation |
| `crates/unimatrix-engine/src/wire.rs` | No change | `HookInput.transcript_path` already present |
| `crates/unimatrix-server/src/uds/listener.rs` — `handle_compact_payload` | No change | Already migrated to `IndexBriefingService` in crt-027 |

No new crates. No schema changes. No migrations.

## Constraints

1. **crt-027 must be merged first.** WA-5 depends on `IndexBriefingService` and the
   `IndexEntry`/`format_index_table` contract introduced in crt-027 (ADR-005). Building on
   the old `BriefingService` is not viable.
2. **No tokio runtime in hook process.** All I/O in `hook.rs` is synchronous `std::io`.
   Transcript file reading must use `std::fs::File` and `std::io::BufReader`. No async
   primitives.
3. **Hook must always exit 0.** FR-03.7 is a hard invariant. Any transcript read path that
   can fail must be wrapped in graceful degradation (D-6).
4. **No server protocol changes.** `HookRequest::CompactPayload` is unchanged. The transcript
   extraction is entirely hook-side.
5. **MAX_PRECOMPACT_BYTES is a separate constant.** It must not reuse `MAX_INJECTION_BYTES`
   (1400) — PreCompact is the highest-value injection point and requires a larger budget (D-4).
6. **Transcript file format is JSONL, may contain post-compaction empty lines.** The parser
   must handle empty lines and malformed records gracefully (AC-08, AC-09).
7. **GH #354 write site is in `listener.rs`, not `hook.rs`.** The `source` field travels
   over the UDS wire from hook to server; validation happens server-side at the write site.

## Open Questions

**OQ-1**: Index ranking strategy at compaction time — session-injected entries first vs.
pure fused score. `handle_compact_payload` currently uses `derive_briefing_query` with
topic_signals for query derivation, then returns entries sorted by fused score (confidence-
descending). Should entries that were previously injected into the session (tracked via
`InjectionLogRecord`) get a ranking boost at compaction time, since the agent has already
seen them? crt-027 ADR-004 explicitly removed the `include_semantic: false` injection
history path and notes the new index search "returns the top-20 active entries ranked by
fused score — this is a broader (not narrower) result set." The question is whether WA-5
should re-introduce session-injection affinity as a tie-breaking signal, or whether pure
fused score is sufficient. This does not affect the transcript extraction scope (hook.rs)
but would affect `handle_compact_payload` in `listener.rs`. **Not blocking delivery; pure
fused score is acceptable for the initial implementation.**

**OQ-2**: Should `write_stdout` be split into a `write_stdout_precompact` helper that
handles the transcript prepend, or should the prepend happen in the `PreCompact` branch of
`build_request` before the server round-trip? The cleanest approach is to read the transcript
before the `transport.request()` call (since it's local file I/O), store the block, and then
prepend to the response in the response handling branch. **Recommend: read transcript before
`transport.request()`, prepend in the response handler.**

**OQ-3**: How to identify the "key param" of a tool call for compact representation (D-2,
D-3). For `Bash` the key param is `command`; for `Read`/`Edit`/`Write` it is `file_path`;
for `Grep` it is `pattern`. Should this be a hardcoded map of tool-name → key-param-field,
or always the first string field in `input`? **Recommend: small hardcoded map for common
Claude Code tools (Bash, Read, Edit, Write, Glob, Grep); fallback to first string field.**

## Dependencies

- **crt-027** (GH #350) — must be merged before crt-028 delivery begins. Provides
  `IndexBriefingService`, `IndexEntry`, `format_index_table`, and the migrated
  `handle_compact_payload` that returns flat indexed table format.
- **No other feature dependencies.**

## Tracking

GH Issues: #354 (source field allowlist), #355 (quarantine exclusion test + doc comment).
Primary tracking issue: https://github.com/dug-21/unimatrix/issues/356
