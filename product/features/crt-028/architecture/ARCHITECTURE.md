# crt-028: WA-5 PreCompact Transcript Restoration — Architecture

## System Overview

crt-028 completes the WA-5 PreCompact transcript restoration path that crt-027 prepared.
crt-027 (GH #350) replaced `BriefingService` with `IndexBriefingService` and migrated
`handle_compact_payload` to emit a flat indexed table — expressly to give WA-5 a clean,
typed prepend surface (`IndexEntry` / `format_index_table`, ADR-005 crt-027).

This feature adds the hook-side transcript extraction that fills that surface, fixes two
security gaps left open by the crt-027 security review (GH #354, GH #355), and introduces
a named `MAX_PRECOMPACT_BYTES` constant to establish a separate injection budget for the
highest-value hook path.

No server schema changes, no new crates, no wire protocol changes. All transcript work is
local to the hook process.

---

## Component Breakdown

### 1. `uds/hook.rs` — Three new/modified functions

The only file that gains substantial new logic. Three functions are added or modified:

**a) `extract_transcript_block(path: &str) -> Option<String>`** (new)

Reads the transcript file using the tail-bytes strategy (ADR-001), parses the JSONL window
into typed turn structs, extracts user-text and assistant-text/tool-use pairs via
`build_exchange_pairs`, and formats them as a restoration block within `MAX_PRECOMPACT_BYTES`.
Returns `None` on any I/O or parse failure (SR-07 / ADR-003 degradation contract).

**b) `build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>`** (new, pure function)

Parses each JSONL line defensively (unknown `type` values skipped — SR-01 fail-open), builds
typed `ExchangeTurn` values (user text, assistant text, tool-use/result pairs), and returns
them in reverse-chronological order (most-recent first) so the budget-fill loop in
`extract_transcript_block` fills from the most recent exchanges outward.

Tool-use/result pairing is done via adjacent-record scan (ADR-002): `tool_use` blocks are
collected from an assistant message, then the immediately following user message is scanned
for `tool_result` blocks that match by `tool_use_id`. This works because Claude Code emits
the canonical `[assistant: tool_use] → [user: tool_result]` sequence in JSONL order.

**c) `write_stdout` — modified PreCompact branch**

The `HookResponse::BriefingContent` arm in `write_stdout` is unchanged structurally, but
the PreCompact path is modified: the caller (`run()`) reads the transcript before the
`transport.request()` call, holds the optional transcript block as `Option<String>`, and
prepends it to `content` in the `BriefingContent` handler. The transcript block is always
prepended before briefing content (D-5). If `content` is empty and a transcript block
exists, only the transcript block is written (with a section separator). See SR-04
resolution below.

### 2. `uds/listener.rs` — GH #354 source field allowlist (single-line change)

The `ObservationRow` construction in the `dispatch_request` `ContextSearch` arm currently
writes the `source` field verbatim:

```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

GH #354 fix: replace with an allowlist validation helper (ADR-004). The allowlist is
`{"UserPromptSubmit", "SubagentStart"}`; any other value (including excessively long values)
falls back to `"UserPromptSubmit"`. This is a server-side defense — the `source` field
arrives over the UDS wire from the hook process.

### 3. `services/index_briefing.rs` — GH #355 quarantine exclusion test + doc comment

Two changes to `IndexBriefingService`:

1. Add a doc comment on `index()` documenting that input validation is delegated to
   `SearchService.search()` → `gateway.validate_search_query()`. Guards: S3 (query
   content), length ≤ 10,000 chars, control characters rejected, k bounds enforced.
   Purpose: prevent future removal of the `SearchService` delegation without realizing
   validation disappears with it.

2. Add regression test `index_briefing_excludes_quarantined_entry`: stores an entry with
   `status: Quarantined`, runs `index()`, asserts the entry does not appear in the result.
   This mirrors the deleted T-BS-08 test from `BriefingService` (verified by the
   `se.entry.status == Status::Active` post-filter in `index()`).

### 4. `unimatrix-engine/src/wire.rs` — No change

`HookInput.transcript_path: Option<String>` is already present with `#[serde(default)]`.
No modifications required.

---

## Data Flow: Transcript Path → Extraction → Prepend → Stdout

```
PreCompact hook fires
  │
  ▼
run() in hook.rs
  │
  ├─ Step 5: build_request("PreCompact", &hook_input)
  │    └─ HookRequest::CompactPayload { session_id, injected_entry_ids: vec![], ... }
  │
  ├─ Step 5c (NEW): extract transcript BEFORE transport.request()
  │    └─ if let Some(ref path) = hook_input.transcript_path {
  │         transcript_block = extract_transcript_block(path)  // Option<String>
  │       } else {
  │         transcript_block = None
  │       }
  │    NOTE: all errors within extract_transcript_block → None (AC-07/08, ADR-003)
  │
  ├─ Step 7: transport.request(&request, HOOK_TIMEOUT)
  │    └─ UDS → listener.rs: handle_compact_payload()
  │         → IndexBriefingService::index(derived_query, session_id, k=20)
  │         → format_compaction_payload(entries, ...) → flat indexed table
  │         → HookResponse::BriefingContent { content, token_count }
  │
  └─ Response handling:
       match response {
         HookResponse::BriefingContent { content, .. } => {
           // Prepend transcript block (D-5)
           let full_output = prepend_transcript(transcript_block.as_deref(), &content);
           if !full_output.is_empty() { println!("{full_output}"); }
         }
         ...
       }
```

The transcript read happens before the server round-trip to keep the logic sequential and
avoid needing to pass `transcript_path` into the response handler. The transport timeout
(40ms) applies only to the server request; the transcript read has its own implicit bound
via the tail-bytes read cap.

### Output format when both blocks are present

```
=== Recent conversation (last N exchanges) ===
[User] <text>
[Assistant] <text>
[tool: ReadTool(file_path=/foo/bar.rs) → <300-byte snippet>]
...
=== End recent conversation ===
<flat indexed table from handle_compact_payload>
```

When `content` (briefing) is empty and transcript block is non-empty, only the transcript
block is written (section header present, no merged/ambiguous block). When both are empty,
no stdout is written (FR-01.4 invariant preserved).

---

## Three Functions: Signatures and Responsibilities

### `extract_transcript_block(path: &str) -> Option<String>`

```rust
/// Read the last MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER bytes of the transcript
/// file at `path`, parse as JSONL, extract exchange pairs, and format as a
/// restoration block within MAX_PRECOMPACT_BYTES.
///
/// Returns None on:
/// - path is empty
/// - file open/read error (AC-07)
/// - seek error
/// - no parseable user/assistant pairs (AC-09)
///
/// Never panics. Never propagates errors (ADR-003 degradation contract).
fn extract_transcript_block(path: &str) -> Option<String>
```

Internal steps:
1. Open `std::fs::File::open(path)` — `?`-propagation wrapped in closure returning `None`
2. Get file length via `file.metadata()?.len()`
3. Compute read window: `min(file_len, MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER)` bytes from end
4. `file.seek(SeekFrom::End(-(window as i64)))` or `SeekFrom::Start(0)` if window >= file_len
5. `BufReader::new(file)` — iterate lines, collect into `Vec<String>`
6. Call `build_exchange_pairs(&lines)` → `Vec<ExchangeTurn>`
7. Fill output buffer up to `MAX_PRECOMPACT_BYTES`, most-recent turns first
8. Return `Some(formatted_block)` if at least one turn was included, else `None`

**`TAIL_MULTIPLIER`**: compile-time constant = 4. Rationale: raw JSONL is approximately 4×
larger than the extracted text (JSON overhead, skipped blocks, tool result verbosity). Reading
`MAX_PRECOMPACT_BYTES * 4 = 12,000 bytes` from the end of the file provides the source
window. Combined with the ~40ms hook budget and typical SSD I/O latency (~1ms/12KB), this
stays well within budget (SR-02 mitigation, ADR-001).

### `build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>`

```rust
/// Parse JSONL lines (a tail window) into typed exchange turns.
///
/// Fail-open on unknown record types and malformed lines (SR-01, AC-08).
/// Returns turns in reverse-chronological order (last message first).
///
/// Tool-use/result pairing: each tool_use block in an assistant message is
/// matched against tool_result blocks in the immediately following user message
/// by tool_use_id (ADR-002).
fn build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>
```

**ExchangeTurn** enum (internal to hook.rs):

```rust
enum ExchangeTurn {
    UserText(String),
    AssistantText(String),
    ToolPair { name: String, key_param: String, result_snippet: String },
}
```

**Parsing algorithm**:
1. Parse each line as `serde_json::Value`; skip on error or empty line (AC-08)
2. Extract `type` field (skip if absent or not "user"/"assistant")
3. For `type: "user"`: collect `content[]` where `type == "text"` → `UserText(text)`
4. For `type: "assistant"`:
   - collect `content[]` where `type == "text"` → `AssistantText(text)`
   - collect `content[]` where `type == "tool_use"` → `{id, name, input}` structs
5. After building the assistant tool_use list, look ahead one record (the next line if
   `type == "user"`) for `tool_result` blocks matching by `tool_use_id`
6. For each matched pair: emit `ToolPair { name, key_param, result_snippet }`
7. Collect all turns in JSONL order, then reverse the whole Vec before returning

### `prepend_transcript(transcript: Option<&str>, briefing: &str) -> String`

```rust
/// Combine optional transcript block with briefing content.
///
/// Prepend rules (D-5, SR-04):
/// - Both present: transcript block + "\n" + briefing
/// - Transcript only (briefing empty): transcript block verbatim (includes own === headers)
/// - Briefing only (transcript None/empty): briefing verbatim
/// - Both empty: ""
///
/// No byte-cap applied here; transcript_block is already within MAX_PRECOMPACT_BYTES.
fn prepend_transcript(transcript: Option<&str>, briefing: &str) -> String
```

---

## Tail-Bytes Read Strategy (SR-02 / ADR-001)

The transcript file for a long session can be megabytes. Reading the entire file before
parsing would violate the sub-50ms hook budget.

Strategy: seek to `max(0, file_end - TAIL_WINDOW_BYTES)` and read forward from there.

```
TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 3000 * 4 = 12,000 bytes
```

The seek position may land mid-line. The first line of the read window is therefore
discarded (it is almost certainly truncated). `BufReader::lines()` handles this naturally:
the first line may be partial JSON and will fail `serde_json::from_str` — it is silently
skipped (fail-open). All subsequent lines are complete JSONL records.

This means: for a 1 MB transcript file, only 12 KB of I/O is performed. For a file smaller
than 12 KB, `SeekFrom::Start(0)` is used (no truncation risk). The first-line-discard
applies only when seeking mid-file.

**Why 4× multiplier?** A typical assistant message with tool calls contains:
- JSON envelope overhead: ~200 bytes per message
- `thinking` blocks (skipped): ~500–2000 bytes each
- `tool_result` full content (compressed to 300-byte snippet): 200–5000 bytes each
- Extracted text: 50–300 bytes per turn

A conservative 4× factor means 3000 bytes of extracted content requires ~12,000 bytes of
raw JSONL. Sessions with many thinking blocks may require more, but those are rare and the
degradation path (return fewer turns) is acceptable (AC-09).

---

## Tool-Use/Tool-Result Pairing (ADR-002)

Claude Code JSONL follows a strict interleaving contract:

```
[record N]   type: "assistant", content: [{type: "tool_use", id: "tu_abc", name: "Read", input: {...}}]
[record N+1] type: "user",      content: [{type: "tool_result", tool_use_id: "tu_abc", content: "..."}]
```

Adjacent-record scan: when `build_exchange_pairs` encounters an assistant message with
`tool_use` blocks, it looks ahead to record N+1. If record N+1 is `type: "user"` and
contains `tool_result` blocks, they are matched against the tool_use list by `tool_use_id`.

Matching guarantees:
- An unmatched `tool_use` (no result in N+1) emits `ToolPair { result_snippet: "" }`
- An unmatched `tool_result` (no preceding tool_use for that id) is silently skipped
- Multiple tool_use blocks in one assistant message are all matched in a single pass
- If record N+1 is another assistant message (unusual), no pairing occurs

The `tool_result.content` is truncated to ~300 bytes using `truncate_utf8` before being
stored as `result_snippet` (D-3). This keeps the grep/glob output usable without bloating
the block.

---

## Key-Param Extraction Map (OQ-3 Settled)

For `ToolPair` entries the compact representation is `[tool: name(key_param) → snippet]`.
`key_param` is the most identifying input field for that tool type. Decision: hardcoded map
for known Claude Code tools, first-string-field fallback for unknowns.

```rust
fn extract_key_param(tool_name: &str, input: &serde_json::Value) -> String {
    let field_name = match tool_name {
        "Bash"         => "command",
        "Read"         => "file_path",
        "Edit"         => "file_path",
        "Write"        => "file_path",
        "Glob"         => "pattern",
        "Grep"         => "pattern",
        "MultiEdit"    => "file_path",
        "Task"         => "description",
        "WebFetch"     => "url",
        "WebSearch"    => "query",
        _              => "",   // fallback: first string field below
    };

    if !field_name.is_empty() {
        if let Some(v) = input.get(field_name).and_then(|v| v.as_str()) {
            return truncate_utf8(v, 120).to_string();
        }
    }

    // Fallback: first string field in the input object
    if let Some(obj) = input.as_object() {
        for (_, v) in obj {
            if let Some(s) = v.as_str() {
                return truncate_utf8(s, 120).to_string();
            }
        }
    }

    String::new()
}
```

The key-param is truncated to 120 bytes (separate from the 300-byte result snippet budget).

---

## Graceful Degradation Contract (SR-07 / ADR-003)

The degradation boundary is precisely scoped: **only the transcript block is skipped on
failure**. `BriefingContent` is always written.

```
Failure class                 | Behavior
------------------------------|------------------------------------------
transcript_path is None       | transcript_block = None; briefing emitted normally
File not found / unreadable   | transcript_block = None; briefing emitted normally
Seek error                    | transcript_block = None; briefing emitted normally
All lines malformed JSONL     | transcript_block = None; briefing emitted normally
No user/assistant pairs found | transcript_block = None; briefing emitted normally
BriefingContent content=""    | transcript_block emitted if Some; otherwise nothing
```

Implementation: `extract_transcript_block` uses an inner closure pattern (or a local
`fn inner() -> Option<String>` with `?` operators) that maps all errors to `None`. The
outer call site never propagates any error:

```rust
let transcript_block: Option<String> = hook_input
    .transcript_path
    .as_deref()
    .filter(|p| !p.is_empty())
    .and_then(|p| extract_transcript_block(p));
// Always continues regardless of transcript outcome
```

This construction makes it structurally impossible for a transcript failure to prevent
the `BriefingContent` write. The briefing path is independent of the transcript path.

**Test coverage required** (SR-07 explicit test): a test that calls the full `write_stdout`
path with a `BriefingContent` response where `transcript_block = None` must verify that
non-empty stdout is produced (briefing is always written regardless of transcript outcome).

---

## GH #354 Fix Design — `listener.rs` Source Field Allowlist

**Location**: `dispatch_request` ContextSearch arm, line ~813 of listener.rs.

**Current code**:
```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

**Problem**: `source` is `Option<String>` from `HookRequest::ContextSearch`. It travels
over the UDS wire from the hook process. Any string content is written verbatim to the
`hook TEXT NOT NULL` column in the observations table — including adversarially long strings
or unexpected values.

**Fix** (ADR-004): Replace the inline expression with a helper call:

```rust
hook: sanitize_observation_source(source.as_deref()),
```

Where:

```rust
/// Allowlist-validate the `source` field before writing to the observations hook column.
///
/// Known values: "UserPromptSubmit", "SubagentStart".
/// Any other value (including None, empty, or excessively long strings) falls back
/// to "UserPromptSubmit" (GH #354, ADR-004 crt-028).
///
/// This is the sole write site — all validation happens here.
fn sanitize_observation_source(source: Option<&str>) -> String {
    match source {
        Some("UserPromptSubmit") => "UserPromptSubmit".to_string(),
        Some("SubagentStart")    => "SubagentStart".to_string(),
        _                        => "UserPromptSubmit".to_string(),
    }
}
```

No length cap is needed because the allowlist match exhausts all valid values — any value
not in the allowlist falls to the default regardless of length (ADR-004 rationale).

**Security note (SR-05)**: The `source` field on `HookRequest::ContextSearch` is set by
the hook process. In the crt-027 implementation the only callers are:
- `UserPromptSubmit` arm: `source: None`
- `SubagentStart` arm: `source: Some("SubagentStart".to_string())`

The wire is a local Unix domain socket. An adversary would need code execution in the same
user account to inject an unexpected `source` value. The allowlist is defense-in-depth, not
a primary attack surface mitigation — but it is the correct engineering practice for any
field written to persistent storage.

---

## GH #355 Fix Design — `index_briefing.rs` Quarantine Exclusion Test + Doc Comment

**Doc comment on `index()`** (to be added immediately above `pub(crate) async fn index`):

```
/// Input validation is delegated to `SearchService.search()` which calls
/// `self.gateway.validate_search_query()` (S3, length ≤ 10,000 chars,
/// control characters rejected, k bounds enforced).
/// Do not remove the SearchService delegation without adding an equivalent
/// validation call here.
```

**Regression test** — mirrors the deleted T-BS-08 from BriefingService:

```rust
#[tokio::test]
async fn index_briefing_excludes_quarantined_entry() {
    // Store an entry with status=Quarantined
    // Call index() with a query that would match it
    // Assert: the entry ID does not appear in the result Vec<IndexEntry>
    // Rationale: verifies the `se.entry.status == Status::Active` post-filter
    //            in index() step 5 (FR-08, AC-12).
}
```

The test uses the existing test infrastructure in `index_briefing.rs` (in-memory store,
mock search service or real service against test DB). The quarantined entry must be stored
via the real store path to exercise the post-filter, not mocked away.

---

## Constants in hook.rs

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_INJECTION_BYTES` | 1400 (existing) | Injection budget for UserPromptSubmit/SubagentStart |
| `MAX_PRECOMPACT_BYTES` | 3000 (new) | Transcript block budget at PreCompact — separate constant (D-4, AC-10) |
| `TAIL_MULTIPLIER` | 4 (new) | Raw-to-extracted ratio for tail-bytes window sizing (ADR-001) |
| `TOOL_RESULT_SNIPPET_BYTES` | 300 (new) | Per-tool-result truncation budget (D-3) |
| `TOOL_KEY_PARAM_BYTES` | 120 (new) | Key-param truncation in compact tool representation |
| `MIN_QUERY_WORDS` | 5 (existing, crt-027) | UserPromptSubmit word-count guard |
| `HOOK_TIMEOUT` | 40ms (existing) | Transport timeout |

`MAX_PRECOMPACT_BYTES` must not reuse or alias `MAX_INJECTION_BYTES` — they serve different
injection surfaces with different budget requirements (Constraint 5 from SCOPE.md). The
constant should carry a comment noting it is a tunable for a future config pass (SR-03
acknowledgment).

---

## Integration Points

### No-change boundaries

- `unimatrix-engine/src/wire.rs` — `HookInput.transcript_path` already present; no edits
- `handle_compact_payload` in `listener.rs` — already migrated to `IndexBriefingService` in crt-027; no edits in crt-028 (beyond GH #354)
- `HookRequest::CompactPayload` wire format — unchanged
- `IndexBriefingService::index()` method signature — consumed as-is
- `format_index_table()` / `IndexEntry` — consumed as-is (crt-027 ADR-005 contract)
- `write_stdout_subagent_inject_response()` — unchanged
- All non-PreCompact hook event paths — unchanged

### crt-027 symbols consumed by crt-028

| Symbol | Source | How crt-028 uses it |
|--------|--------|---------------------|
| `IndexBriefingService` | `services/index_briefing.rs` | Unchanged; `handle_compact_payload` already calls it |
| `IndexEntry` | `mcp/response/briefing.rs` | WA-5 contract type — crt-028 does not construct IndexEntry directly |
| `format_index_table` | `mcp/response/briefing.rs` | Called by `format_compaction_payload` (already), not by hook.rs |
| `SNIPPET_CHARS` | `mcp/response/briefing.rs` | Available for reference; hook.rs uses its own TOOL_RESULT_SNIPPET_BYTES |

Any renaming of these symbols in crt-027 after merge is a compile-time breaking change that
surfaces immediately.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `extract_transcript_block` | `fn(path: &str) -> Option<String>` | `uds/hook.rs` (new) |
| `build_exchange_pairs` | `fn(lines: &[&str]) -> Vec<ExchangeTurn>` | `uds/hook.rs` (new) |
| `prepend_transcript` | `fn(transcript: Option<&str>, briefing: &str) -> String` | `uds/hook.rs` (new) |
| `extract_key_param` | `fn(tool_name: &str, input: &serde_json::Value) -> String` | `uds/hook.rs` (new, private) |
| `sanitize_observation_source` | `fn(source: Option<&str>) -> String` | `uds/listener.rs` (new, private) |
| `MAX_PRECOMPACT_BYTES` | `const usize = 3000` | `uds/hook.rs` |
| `TAIL_MULTIPLIER` | `const usize = 4` | `uds/hook.rs` |
| `TOOL_RESULT_SNIPPET_BYTES` | `const usize = 300` | `uds/hook.rs` |
| `HookInput.transcript_path` | `Option<String>`, `#[serde(default)]` | `unimatrix-engine/src/wire.rs` (existing, no change) |
| `HookResponse::BriefingContent { content, token_count }` | existing variant | `unimatrix-engine/src/wire.rs` (existing, no change) |

---

## SR-04 Resolution: Output Format When Briefing Content is Empty

When `BriefingContent.content` is empty (e.g., no Unimatrix entries matched the query) but
a transcript block was extracted:

- Output: transcript block verbatim — already includes `=== Recent conversation ===` header
  and `=== End recent conversation ===` footer. No additional section separator needed.
- Briefing separator is omitted when briefing is empty.
- When transcript block is also empty (or None): nothing written to stdout (FR-01.4).

This is handled entirely in `prepend_transcript()` with explicit case analysis — no
conditional in `write_stdout` itself.

---

## SR-05: Transcript File Security

The `transcript_path` value comes from Claude Code itself via stdin JSON (`HookInput`).
The hook process cannot alter this value after Claude Code writes it. The path points to
`~/.claude/projects/{slug}/{session-uuid}.jsonl` — a local file in the user's home
directory that the hook process already has read access to.

No path injection is possible in the intended use: the path is written by Claude Code, not
by any user-supplied parameter. The hook process reads the file as read-only
(`File::open`, not `OpenOptions::write`). No path sanitization is needed beyond confirming
the path is non-empty (the `filter(|p| !p.is_empty())` check on `transcript_path`).

This is not a security mitigation boundary — it is documentation of why no additional
sanitization is warranted.

---

## Open Questions

**OQ-1** (Not blocking): Index ranking at compaction time — should previously-injected
entries get a ranking boost? SCOPE.md resolves this as deferred: pure fused score is
acceptable for the initial implementation. A future feature may reintroduce session-injection
affinity as a tie-breaking signal via `handle_compact_payload`.

**OQ-2** (Resolved): Transcript read happens before `transport.request()` in `run()`,
stored as `Option<String>`, passed to the response handler by the caller. This avoids
needing a second parameter on `write_stdout` — the prepend is done by the `run()` caller
before passing to `write_stdout`, or via a local closure. Either approach is acceptable to
the implementer; the key invariant is that `write_stdout` is not responsible for reading
the file.

**OQ-3** (Resolved — see Key-Param Extraction Map above): Hardcoded map for 10 known
Claude Code tools, first-string-field fallback.
