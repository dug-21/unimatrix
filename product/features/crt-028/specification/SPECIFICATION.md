# SPECIFICATION: crt-028 — WA-5 PreCompact Transcript Restoration

## Objective

When Claude Code compacts the context window, the PreCompact hook already delivers
Unimatrix knowledge to the agent via `IndexBriefingService` (crt-027), but provides
no continuity of the immediate working context — what was being asked, what tools were
called, what was found. This feature reads the session transcript locally in the hook
process and prepends a structured transcript restoration block to the compaction output,
closing the continuity gap. It also delivers two security/test fixes bundled from the
crt-027 security review (GH #354, GH #355).

---

## Functional Requirements

### FR-01: Transcript Extraction

**FR-01.1** — When `input.transcript_path` is a non-empty `Some(String)`, the PreCompact
arm in `build_request` (or the response handler) MUST attempt to open and read that file
before writing stdout. The transcript is read locally in the hook process; no transcript
content is sent to the server.

**FR-01.2** — The extraction MUST perform a type-aware reverse scan: iterate over JSONL
lines from the end of the file toward the beginning. Lines are parsed one at a time. The
scan stops when the byte budget is filled or the file is exhausted.

**FR-01.3** — During the reverse scan, records with `type: "assistant"` MUST be collected
for assistant turn content. Records with `type: "user"` MUST be collected for user turn
content. All other `type` values (including `"system"`, `"summary"`, unknown types) MUST
be skipped silently without error.

**FR-01.4** — An exchange pair is formed when a `user` record and an `assistant` record
are paired: the assistant record that immediately precedes the user record (in document
order). When a valid pair is identified during the reverse scan, the pair is appended to
the extraction result. Unpaired user or assistant records at the file boundary are
discarded.

**FR-01.5** — Extraction MUST stop as soon as the formatted transcript block reaches or
would exceed `MAX_PRECOMPACT_BYTES`. The result is the set of complete exchange pairs that
fit within the budget, in most-recent-first order.

**FR-01.6** — To avoid reading an entire large file, the scan MUST read only the last
`MAX_PRECOMPACT_BYTES * 4` bytes of the file (a `seek_from_end` equivalent using
`std::io::Seek`) before parsing. This bounds file I/O to a fixed byte window regardless
of transcript length. If the file is smaller than this window, the entire file is read.
This addresses SR-02.

**FR-01.7** — `input.transcript_path` is read from `HookInput.transcript_path:
Option<String>`. No changes to `HookInput` or `wire.rs` are needed; the field is already
present and fully deserialized.

### FR-02: Output Format

**FR-02.1** — The transcript restoration block MUST begin with the header:
```
=== Recent conversation (last N exchanges) ===
```
where N is the count of complete exchange pairs included.

**FR-02.2** — Exchange pairs MUST be ordered most-recent first (the last exchange in the
session is exchange 1). This ordering reflects the priority model: the most recent context
is most valuable.

**FR-02.3** — Each exchange pair MUST be rendered as:
```
[User] {user text}
[Assistant] {assistant text}
```
with a blank line between pairs. User text is the concatenation of all `type: "text"`
content block texts from the user message, joined by a single newline if multiple blocks
are present.

**FR-02.4** — Assistant text within a pair MUST be rendered as: the concatenation of all
`type: "text"` content blocks from the assistant message, followed by each tool pair on
its own line in the compact format defined in FR-03.

**FR-02.5** — The transcript block MUST end with the footer:
```
=== End recent conversation ===
```

**FR-02.6** — When `BriefingContent.content` is empty and a transcript block is present,
the transcript block is still emitted as the sole stdout content. A section separator
(blank line) MUST appear between the transcript block and the briefing content when both
are non-empty. This addresses SR-04.

**FR-02.7** — The transcript block is prepended; it is never appended or interleaved with
the briefing content.

### FR-03: Tool Representation

**FR-03.1** — Each `type: "tool_use"` block in an assistant message MUST be represented
as a tool pair: a compact inline string on one line in the format:
```
[tool: {name}({key_param}) → {snippet}]
```

**FR-03.2** — `{name}` is the tool name from the `tool_use` block's `name` field.

**FR-03.3** — `{key_param}` is the value of the "key parameter" for the tool, identified
by a priority lookup map:

| Tool name | Key parameter field |
|-----------|---------------------|
| `Bash`    | `command`           |
| `Read`    | `file_path`         |
| `Edit`    | `file_path`         |
| `Write`   | `file_path`         |
| `Glob`    | `pattern`           |
| `Grep`    | `pattern`           |

For any tool name not in this map, the key parameter MUST be the first string-valued
field found when iterating over `input` object keys. If no string field exists, the key
param is the empty string.

**FR-03.4** — `{snippet}` is extracted from the `tool_result` record that corresponds to
the `tool_use` block, matched by `tool_use_id`. The snippet is the first `type: "text"`
content block from the tool result, truncated to 300 bytes at a valid UTF-8 character
boundary. If the tool result content block is itself shorter than 300 bytes, it is
included verbatim.

**FR-03.5** — For Grep and Glob tools, results tend to be short structured text. They are
included verbatim (subject to the 300-byte cap) — no further summarization is applied.

**FR-03.6** — `type: "tool_result"` content blocks in `user` message records MUST NOT be
extracted as user text (FR-02.3). They are consumed only for tool pair snippet extraction
when matched by `tool_use_id`.

**FR-03.7** — A `type: "thinking"` block in an assistant message MUST be skipped silently.
It MUST NOT appear in the transcript block.

**FR-03.8** — When a `tool_use` block has no matching `tool_result` in the surrounding
context (e.g., the result was in a prior exchange that was not scanned), the snippet
MUST be the empty string: `[tool: {name}({key_param}) → ]`.

### FR-04: Budget

**FR-04.1** — A new compile-time constant `MAX_PRECOMPACT_BYTES: usize = 3000` MUST be
defined in `uds/hook.rs`. This constant is distinct from `MAX_INJECTION_BYTES` (1400)
and MUST NOT reuse it.

**FR-04.2** — The transcript block is filled in priority order: most-recent exchange first.
A complete exchange pair is added only if it fits within the remaining budget. Once adding
the next pair would exceed the budget, the scan stops. Partial pairs are not emitted.

**FR-04.3** — The budget applies to the transcript block portion only. The briefing
content returned by the server is appended after the transcript block without additional
truncation from this feature. The existing `MAX_COMPACTION_BYTES` enforcement in
`handle_compact_payload` (server side) already bounds the briefing content independently.

**FR-04.4** — The constant location (`uds/hook.rs`) MUST be documented with a comment
identifying it as the tunable for PreCompact injection size, noting the current value
reflects ~750 tokens at 4 bytes/token, and pointing to `config.toml` as the future
surface for runtime override. This addresses SR-03.

### FR-05: Prepend Behavior

**FR-05.1** — The transcript block MUST always be prepended to `BriefingContent.content`,
never replace it. The final stdout output for a PreCompact event with a non-empty
transcript and non-empty briefing MUST be:
```
{transcript_block}\n\n{briefing_content}
```

**FR-05.2** — If the transcript block is empty (no pairs extracted, or graceful skip),
the output is `{briefing_content}` unchanged — the same as the pre-feature behavior.

**FR-05.3** — If `BriefingContent.content` is empty but the transcript block is
non-empty, stdout MUST contain only the transcript block (no trailing blank line after
the footer).

**FR-05.4** — The prepend logic MUST be implemented in the response handling code path
(after `transport.request()` returns, before `write_stdout` is called), not inside
`write_stdout` itself. This keeps `write_stdout` unmodified for non-PreCompact events
(AC-14 invariant).

**FR-05.5** — A dedicated helper function (e.g., `read_transcript_block`) MUST encapsulate
all transcript file I/O and JSONL parsing. It accepts `Option<&str>` (the path) and
returns `Option<String>` (the formatted block, or `None` on any failure). This
decomposition makes the extraction independently testable (SR-01 + SR-07 recommendation).

### FR-06: Graceful Degradation

**FR-06.1** — When `input.transcript_path` is `None`, transcript extraction is skipped.
`BriefingContent` is written normally. No error is logged.

**FR-06.2** — When the file at `transcript_path` does not exist or cannot be opened
(permissions, path not found, I/O error), the error is silently swallowed. No `eprintln!`
or stderr output for this case. `BriefingContent` is written normally.

**FR-06.3** — When a JSONL line in the transcript file fails to parse as JSON, the line
is skipped silently. The scan continues to the next line. Parseable lines are used
normally.

**FR-06.4** — When a parsed JSONL record does not have the expected shape (missing
`message`, missing `content` array, missing `type` fields), the record is skipped
silently. Unknown `type` values are skipped, not rejected (SR-01: fail-open on unknown
record shapes).

**FR-06.5** — When the reverse scan yields zero complete exchange pairs (empty file,
post-compaction stale file, all records malformed), the transcript block is `None`. The
hook writes only `BriefingContent`. Exit code is 0.

**FR-06.6** — The `read_transcript_block` function MUST never propagate a Rust error
(`?` / `Result`) to its caller. All error paths MUST be handled internally, returning
`None`. This is the degradation boundary: only the transcript block is skipped; the
briefing content path is never gated on transcript success.

**FR-06.7** — An explicit test MUST verify that when transcript extraction fails (e.g.,
file not found path is passed), the hook stdout is still non-empty and contains the
briefing content. This directly addresses SR-07.

### FR-07: GH #354 — Source Field Allowlist

**FR-07.1** — In `crates/unimatrix-server/src/uds/listener.rs`, in the `dispatch_request`
function's `ContextSearch` arm, the `source` field from `HookRequest::ContextSearch` MUST
be validated against an allowlist before being written to the observations `hook` column.

**FR-07.2** — The allowlist contains exactly two values: `"UserPromptSubmit"` and
`"SubagentStart"`. Any `source` value not in this allowlist (including excessively long
strings, control characters, or unknown event names) MUST fall back to `"UserPromptSubmit"`.

**FR-07.3** — The current code `source.as_deref().unwrap_or("UserPromptSubmit").to_string()`
MUST be replaced with allowlist validation. A helper constant, inline match, or short
function is acceptable. The replacement must produce `"UserPromptSubmit"` for `None` and
for any unrecognized string.

**FR-07.4** — No length cap beyond the allowlist membership check is required: the
allowlist itself bounds value length to the longer of the two known strings
(`"UserPromptSubmit"` = 17 chars).

**FR-07.5** — This change is a single-site modification in `listener.rs`. No wire
protocol changes are needed — `source` continues to arrive as `Option<String>`.

**FR-07.6** — A dedicated unit test MUST verify: (a) `Some("SubagentStart")` →
`"SubagentStart"` in the `hook` column; (b) `None` → `"UserPromptSubmit"`; (c) an
unknown value such as `Some("Injected\nEvil")` → `"UserPromptSubmit"`. This test is
distinct from the transcript extraction tests.

### FR-08: GH #355 — IndexBriefingService Quarantine Test + Doc Comment

**FR-08.1** — A regression test MUST be added to
`crates/unimatrix-server/src/services/index_briefing.rs` that: stores an entry with
`status: Quarantined` in a test database, calls `IndexBriefingService::index()`, and
asserts the quarantined entry is absent from the returned `Vec<IndexEntry>`.

**FR-08.2** — The test MUST exercise the post-filter path (the `status == Active` check
inside `index()`) directly, not just rely on `SearchService` internal behavior. If the
post-filter were removed, the test MUST fail.

**FR-08.3** — A doc comment MUST be added to `IndexBriefingService::index()` stating
(verbatim or equivalent): "Input validation is delegated to `SearchService.search()`
which calls `self.gateway.validate_search_query()` (S3, length ≤ 10,000 chars, control
characters rejected, k bounds enforced). Do not remove the SearchService delegation
without adding an equivalent validation call."

**FR-08.4** — The test mirrors the deleted `T-BS-08` from `BriefingService`, restoring
test coverage for the quarantine exclusion invariant.

---

## Non-Functional Requirements

**NFR-01: Hook exit code** — The hook process MUST always exit 0. No transcript read
outcome (missing path, file not found, malformed JSONL, empty result, seek error) may
cause exit code 1. This is a hard invariant (FR-03.7 in hook.rs; preserved from prior
features).

**NFR-02: Synchronous I/O only** — All file I/O in `hook.rs` MUST use `std::fs::File`,
`std::io::BufReader`, `std::io::Seek`, and `std::io::BufRead` from the standard library.
No `tokio` async primitives, no `spawn_blocking`, no thread spawning. This preserves
ADR-002 (hook process has no tokio runtime).

**NFR-03: No server protocol changes** — `HookRequest::CompactPayload` wire format is
unchanged. `HookInput` struct is unchanged. No new UDS request variants are introduced.
The server is unaware of transcript extraction.

**NFR-04: Transcript read latency** — The transcript file read (seek + buffered read of
`MAX_PRECOMPACT_BYTES * 4` bytes) MUST complete within 50ms on typical hardware for
files up to 10MB. The `HOOK_TIMEOUT` (40ms) applies only to the server round-trip; local
file I/O is not covered by it. However, the total PreCompact wall time (local I/O +
server round-trip) MUST not exceed 50ms under normal conditions. The file scan byte cap
(FR-01.6) is the primary mitigation.

**NFR-05: UTF-8 safety** — All string truncation (FR-03.4: 300-byte tool result snippet;
FR-04.2: budget-filling) MUST land on valid UTF-8 character boundaries. The existing
`truncate_utf8` helper in `hook.rs` MUST be reused for all truncation operations in this
feature.

**NFR-06: Compile-time constant visibility** — `MAX_PRECOMPACT_BYTES` MUST be defined at
module level in `hook.rs` alongside `MAX_INJECTION_BYTES` and `MIN_QUERY_WORDS`, with
a doc comment (FR-04.4). It MUST NOT be defined inline or as a magic number.

**NFR-07: Test coverage non-regression** — All existing `hook.rs` tests MUST pass
unchanged. The test count for `hook.rs` MUST be non-decreasing. No existing test
behavior for non-PreCompact events is modified.

---

## Acceptance Criteria

### AC-01: Transcript block prepended on valid path

When `input.transcript_path` is set and the file contains at least one user/assistant
exchange pair, the PreCompact hook stdout MUST begin with the transcript restoration
block header (`=== Recent conversation`), followed by at least one `[User]`/`[Assistant]`
pair, followed by the footer, followed by the briefing content.

**Verification**: Unit test in `hook.rs` — mock `BriefingContent` response with non-empty
content, mock a readable transcript JSONL file with one exchange pair, assert stdout
starts with `=== Recent conversation` and contains the briefing content after the footer.

### AC-02: Most-recent exchanges first, text blocks extracted correctly

When the transcript contains multiple exchange pairs, the restoration block includes them
in most-recent-first order. User text from `type: "text"` content blocks is extracted.
Assistant `type: "text"` blocks are extracted.

**Verification**: Unit test — JSONL with 3 exchange pairs, assert pair 3 appears before
pair 2 appears before pair 1 in stdout.

### AC-03: Tool pairs in compact format

When an assistant message contains `type: "tool_use"` blocks, each is rendered as
`[tool: {name}({key_param}) → {snippet}]`. Snippet is truncated to 300 bytes. The
`tool_use` block's corresponding `tool_result` provides the snippet.

**Verification**: Unit test — assistant message with `Bash` tool_use (command=`"ls /"`)
and matching tool_result, assert stdout contains `[tool: Bash(ls /) → ...]` with snippet
≤ 300 bytes.

### AC-04: tool_result user blocks skipped

`type: "tool_result"` content blocks in `user` messages are not included in the
`[User]` text output.

**Verification**: Unit test — user message with `type: "tool_result"` block (no
`type: "text"` blocks), assert `[User]` section in output is empty or the pair is
omitted (no user text means no pair).

### AC-05: Transcript block respects MAX_PRECOMPACT_BYTES

When the transcript contains more exchange pairs than fit in `MAX_PRECOMPACT_BYTES`,
only the most-recent pairs that fit are included. The transcript block byte length MUST
be ≤ `MAX_PRECOMPACT_BYTES`.

**Verification**: Unit test — JSONL with 20 exchange pairs, each with ~200-byte user
text. Assert output transcript block length ≤ 3000 bytes. Assert most-recent pair is
present; earliest pair is absent.

### AC-06: None transcript_path — silent skip, briefing unchanged

When `input.transcript_path` is `None`, stdout contains only the briefing content.
No error is written to stderr. Exit code is 0.

**Verification**: Unit test — `transcript_path: None`, mock `BriefingContent` response,
assert stdout equals briefing content only.

### AC-07: Missing file — silent skip, briefing written, exit 0

When `transcript_path` is `Some(path)` but the file does not exist, the hook silently
skips transcript extraction, writes only the briefing content, and exits 0.

**Verification**: Unit test — pass a path to a non-existent file. Assert stdout is
non-empty (briefing content present). Assert stderr does not contain a file-not-found
error. Assert exit code 0.

### AC-08: Malformed JSONL — parseable lines used, malformed lines skipped, exit 0

When the JSONL file contains a mix of valid and invalid lines, valid lines are parsed
normally, invalid lines are silently skipped. Exit code is 0.

**Verification**: Unit test — JSONL file with 3 lines: one valid exchange, one line
of `"garbage\x00not json"`, one valid exchange. Assert output contains both valid
exchanges. Assert exit code 0.

### AC-09: Empty/no-pair transcript — block omitted, briefing written

When the JSONL file contains no extractable user/assistant pairs (all system messages,
post-compaction summary records, or empty file), stdout is the briefing content only.
No transcript block appears.

**Verification**: Unit test — JSONL file containing only `type: "system"` records.
Assert stdout does not contain `=== Recent conversation`. Assert stdout contains
briefing content.

### AC-10: MAX_PRECOMPACT_BYTES constant defined and distinct

`uds/hook.rs` defines `MAX_PRECOMPACT_BYTES: usize = 3000` as a named constant at module
level. This constant does not alias `MAX_INJECTION_BYTES` (which remains 1400).

**Verification**: Code inspection — `grep -n "MAX_PRECOMPACT_BYTES" uds/hook.rs` returns
a `const` declaration with value `3000`. `MAX_INJECTION_BYTES` is still present with
value `1400`.

### AC-11: Source field allowlist in listener.rs

In `listener.rs`, the `ObservationRow.hook` field is set by allowlist lookup:
`"UserPromptSubmit"` and `"SubagentStart"` are accepted; any other value (including
`None`) falls back to `"UserPromptSubmit"`.

**Verification**: Unit tests in `listener.rs`:
- (a) `source = Some("SubagentStart")` → `hook = "SubagentStart"` in observations table.
- (b) `source = None` → `hook = "UserPromptSubmit"`.
- (c) `source = Some("Injected\nEvil; DROP TABLE--")` → `hook = "UserPromptSubmit"`.

### AC-12: Quarantine exclusion regression test in index_briefing.rs

A test exists in `index_briefing.rs` that stores a `Quarantined` entry, calls
`IndexBriefingService::index()`, and asserts the entry is absent from the result.
Removing the `status == Active` post-filter would cause this test to fail.

**Verification**: Test present in `index_briefing.rs` test module. If the post-filter
line is commented out, the test fails (verified by code inspection).

### AC-13: Doc comment on IndexBriefingService::index()

`IndexBriefingService::index()` has a doc comment stating that query validation is
delegated to `SearchService.search()` → `self.gateway.validate_search_query()`, and
warning not to remove the delegation without adding equivalent validation.

**Verification**: Code inspection — `grep -A 5 "fn index" services/index_briefing.rs`
shows the doc comment.

### AC-14: Non-PreCompact hook behavior unchanged

All existing `hook.rs` tests pass without modification. `write_stdout` behavior for
`UserPromptSubmit`, `SubagentStart`, and other non-PreCompact events is identical to
the pre-feature baseline.

**Verification**: `cargo test` passes. Test count in `hook.rs` module is non-decreasing.
No existing assertion is modified.

### AC-15: Hook always exits 0 regardless of transcript outcome

For all transcript read outcomes (None path, file not found, malformed JSONL, empty
result, seek error), the hook exits 0.

**Verification**: Unit tests for AC-06, AC-07, AC-08, AC-09 all assert exit code 0 (or
`Ok(())` return from `run()`). No `process::exit(1)` or `Result::Err` propagation from
the transcript read path.

---

## Domain Models

### TranscriptRecord

A single parsed JSONL line from the session transcript file. Not all fields are present
on every record.

```
TranscriptRecord {
    type:    String,          // "user", "assistant", "system", "summary", unknown
    message: MessageContent,  // present on user/assistant records
}

MessageContent {
    content: Vec<ContentBlock>,
}

ContentBlock (enum, discriminated by "type" field):
    TextBlock    { type: "text",        text: String }
    ToolUse      { type: "tool_use",    id: String, name: String, input: JsonObject }
    ToolResult   { type: "tool_result", tool_use_id: String, content: Vec<ContentBlock> }
    ThinkingBlock { type: "thinking",   ... }  // skipped
    UnknownBlock  { type: <other> }             // skipped
```

These are internal parsing structs, not persisted. They MAY be anonymous (using
`serde_json::Value` field access) rather than named Rust types. If named structs are
used, they MUST be private to the extraction module.

### ExchangePair

A user/assistant exchange: one user turn paired with the assistant turn that immediately
precedes it in document order.

```
ExchangePair {
    user_text:  String,       // Concatenation of text blocks from user message
    asst_text:  String,       // Concatenation of text blocks from assistant message
    tool_pairs: Vec<ToolPair>,// One entry per tool_use block in the assistant message
}
```

The domain term "exchange pair" always refers to a (user, assistant) unit. There is no
single-turn half-pair in the output. A user message with no text content (only
`tool_result` blocks) is not combined with an assistant turn to form an exchange pair —
both user text and assistant text must be non-empty for a pair to be emitted.

### ToolPair

A paired tool call and its result, extracted from an exchange pair.

```
ToolPair {
    name:      String,   // tool_use.name
    key_param: String,   // value from tool_use.input using the key-param map (FR-03.3)
    snippet:   String,   // truncated tool_result text (≤ 300 bytes, UTF-8 boundary safe)
}
```

The formatted representation is: `[tool: {name}({key_param}) → {snippet}]`

### Transcript Block

The formatted string prepended to `BriefingContent`. Composed of:
1. Header: `=== Recent conversation (last N exchanges) ===`
2. N exchange pairs in most-recent-first order
3. Footer: `=== End recent conversation ===`

`N` is the count of `ExchangePair` values that fit within `MAX_PRECOMPACT_BYTES`.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **exchange pair** | A (user turn, assistant turn) unit extracted from the transcript. User turn precedes assistant turn in document order. |
| **tool pair** | A `tool_use` block from an assistant turn paired with its `tool_result`, formatted as the compact `[tool: name(key_param) → snippet]` string. |
| **key param** | The single most-identifying input parameter of a tool call. Resolved via a hardcoded tool-name→field map; first string field fallback for unknown tools. |
| **transcript block** | The formatted restoration string: header + N exchange pairs + footer. Prepended to `BriefingContent`. |
| **transcript restoration** | The act of reading the session transcript at PreCompact time and injecting recent context into the compaction output. |
| **graceful skip** | When any transcript read failure (missing file, malformed JSON, empty result) causes silent omission of the transcript block. `BriefingContent` is always written. |
| **reverse scan** | Iterating JSONL lines from end-of-file toward the beginning. Most-recent records encountered first. |
| **budget** | `MAX_PRECOMPACT_BYTES` (3000) — the byte ceiling for the transcript block portion of the PreCompact output. |
| **BriefingContent** | The `HookResponse::BriefingContent { content, token_count }` returned by `handle_compact_payload`. The `content` field contains the `IndexBriefingService` flat table plus session context (crt-027). |

---

## User Workflows

### PreCompact: agent receives continuity + knowledge

1. Claude Code fires the `PreCompact` hook before context compaction.
2. Hook reads `input.transcript_path` from stdin JSON.
3. If `transcript_path` is present, `read_transcript_block(path)` opens the file,
   seeks to `EOF - MAX_PRECOMPACT_BYTES * 4`, reads and parses JSONL lines in reverse.
4. Extraction builds `Vec<ExchangePair>` by pairing user/assistant records,
   filling the `MAX_PRECOMPACT_BYTES` budget most-recent first.
5. The hook builds `HookRequest::CompactPayload` and sends it to the server.
6. Server returns `HookResponse::BriefingContent` with the flat index table (crt-027).
7. Hook prepends the transcript block (if non-empty) to `BriefingContent.content`.
8. Hook writes combined output to stdout. Exit code 0.
9. Post-compaction, the agent's context window contains: recent exchanges (what was
   happening), followed by the Unimatrix knowledge index (what knowledge is available).

### Graceful skip: failed transcript read

1. Hook reads `input.transcript_path`.
2. `read_transcript_block` attempts to open the file; file not found (or malformed).
3. All errors are caught internally. `read_transcript_block` returns `None`.
4. Server round-trip proceeds normally.
5. `BriefingContent` is written to stdout unmodified. Exit code 0.

### Security: source field write (GH #354)

1. A ContextSearch hook request arrives at the server with `source: Some("unknown_value")`.
2. `dispatch_request` validates against the allowlist `{"UserPromptSubmit", "SubagentStart"}`.
3. `"unknown_value"` is not in the allowlist. Falls back to `"UserPromptSubmit"`.
4. `ObservationRow.hook` is written as `"UserPromptSubmit"`. No raw client-controlled
   string enters the database column.

---

## Constraints

**C-01** — crt-027 MUST be merged before crt-028 delivery begins. The `IndexBriefingService`,
`IndexEntry`, `format_index_table`, and the migrated `handle_compact_payload` are all
required by crt-028. The exact crt-027 symbols consumed:
`IndexBriefingService::index`, `format_index_table`, `HookResponse::BriefingContent`.
Any renaming of these symbols post-merge is a breaking change to crt-028 (SR-06).

**C-02** — No tokio runtime in `hook.rs`. All I/O uses `std::fs::File`,
`std::io::BufReader`, `std::io::Seek`, `std::io::BufRead`. No async code.

**C-03** — Hook exit code is always 0. Any error path in transcript extraction MUST
be caught and swallowed within `read_transcript_block`. No `?` propagation across the
degradation boundary.

**C-04** — No server protocol changes. `HookRequest::CompactPayload` wire format is
unchanged. `HookInput` is unchanged. Server is unaware of local transcript extraction.

**C-05** — `MAX_PRECOMPACT_BYTES` is a separate constant. It MUST NOT alias or reuse
`MAX_INJECTION_BYTES`. They serve different hook paths with different budget rationale.

**C-06** — Transcript JSONL format is controlled by Claude Code and may change silently.
The parser MUST fail-open: unknown `type` values and missing fields are skipped, never
rejected with an error.

**C-07** — GH #354 write site is in `listener.rs`, not `hook.rs`. The allowlist check
is server-side at the write point, not client-side in the hook.

**C-08** — Server-side transcript storage is explicitly out of scope. The transcript
path is not sent to the server. No schema changes. No new UDS variants.

**C-09** — PostCompact is out of scope. Transcript extraction occurs only in the
`"PreCompact"` arm.

**C-10** — `write_stdout` (the existing function for non-PreCompact events) MUST NOT
be modified. The transcript prepend is implemented in the PreCompact-specific response
handling branch. This preserves AC-14.

---

## Dependencies

| Dependency | Version / Location | Notes |
|---|---|---|
| crt-027 | GH #350 — must be merged first | Provides `BriefingContent` response, `IndexBriefingService`, `format_index_table` |
| `std::fs::File` | stdlib | Transcript file open |
| `std::io::{BufReader, BufRead, Seek, SeekFrom}` | stdlib | Buffered line reading + seek-from-end |
| `serde_json` | workspace | JSONL line parsing (already a dependency) |
| `unimatrix_engine::wire::HookInput` | workspace | `transcript_path: Option<String>` field (no change needed) |
| `unimatrix_engine::wire::HookResponse::BriefingContent` | workspace | Response variant whose `content` field is prepended to |
| `dirs` crate | workspace | Already used for home dir resolution in `hook.rs` |

Existing files modified:
- `crates/unimatrix-server/src/uds/hook.rs` — Add `MAX_PRECOMPACT_BYTES`; add
  `read_transcript_block` and extraction helpers; modify PreCompact response handling
  to prepend transcript block.
- `crates/unimatrix-server/src/uds/listener.rs` — GH #354: allowlist `source` field.
- `crates/unimatrix-server/src/services/index_briefing.rs` — GH #355: quarantine test
  + doc comment.

No new crates. No schema changes. No migrations.

---

## NOT in Scope

- **Server-side transcript storage**: transcript content is hook-local only.
- **Persistent transcript summaries**: the restoration block is ephemeral, not stored.
- **PostCompact hook**: restoration is PreCompact-only.
- **Full verbatim replay**: `tool_result` user-turn blocks are skipped; only text blocks
  and compact tool pairs are extracted.
- **Runtime-configurable k or budget**: `MAX_PRECOMPACT_BYTES` is compile-time only.
- **GH #303, #305, or any open issue not listed in D-9**: explicitly excluded by SCOPE.md.
- **Session-injection affinity ranking at compaction**: OQ-1 from SCOPE.md — pure fused
  score is acceptable; re-introducing session-injection affinity is deferred.
- **OQ-2 resolution as `write_stdout_precompact`**: the prepend logic goes in the
  PreCompact response handler branch (FR-05.4); `write_stdout` is not split.
- **Configurable tool key-param map**: the map in FR-03.3 is hardcoded.
- **Any changes to the MCP tool API** (no new MCP tools, no signature changes).
- **Histogram block changes**: the histogram block in `BriefingContent` (from crt-026
  WA-2) is carried through unchanged.

---

## Scope Risk Traceability

| SR ID | Risk | Spec Mitigation |
|-------|------|-----------------|
| SR-01 | Transcript JSONL schema may change silently | FR-06.4: unknown `type` values and missing fields are skipped silently (fail-open). `read_transcript_block` returns `None` rather than erroring. |
| SR-02 | Large JSONL file may read megabytes on sync I/O hook path | FR-01.6: seek to `EOF - MAX_PRECOMPACT_BYTES * 4` before parsing. File I/O is bounded to ~12KB regardless of transcript size. |
| SR-03 | `MAX_PRECOMPACT_BYTES` is buried compile-time constant with no runtime override | FR-04.4: doc comment on the constant documents location and points to `config.toml` as future override surface. NFR-06: constant defined at module level alongside peers. |
| SR-04 | When `BriefingContent.content` is empty, output may be ambiguous | FR-02.6: explicit output format when briefing is empty — transcript block is still emitted; separator only when both are non-empty. AC-01 verifies combined non-empty output. |
| SR-05 | GH #354 allowlist fix may be under-reviewed alongside new extraction logic | FR-07 is a standalone section with its own ACs (AC-11). Three explicit test cases in AC-11 cover the security property. |
| SR-06 | crt-027 API change post-merge breaks crt-028 integration | C-01 names the exact crt-027 symbols consumed. Any deviation is a breaking change requiring re-scoping. |
| SR-07 | Mis-scoped graceful degradation silently drops `BriefingContent` | FR-06.6: `read_transcript_block` never propagates errors to caller. FR-05.4: transcript and briefing paths are independent. AC-07 (FR-06.7): explicit test that failure path still produces non-empty stdout. |

---

## Open Questions

**OQ-1** (from SCOPE.md, not blocking): Session-injection affinity as a ranking boost
at compaction time. Pure fused score is acceptable for initial delivery. Deferred.

**OQ-3** (from SCOPE.md, resolved in spec): Key-param identification strategy. FR-03.3
specifies a hardcoded map for known Claude Code tools with a first-string-field fallback
for unknowns. This is the recommended approach from SCOPE.md OQ-3.

**OQ-SPEC-1** (resolved by risk strategist): When an assistant turn has no `type: "text"`
blocks (only `tool_use` + `thinking`), the exchange pair MUST be emitted if at least one
`ToolPair` is present. Suppression applies only when both `asst_text` and `tool_pairs`
are empty (pure-`thinking` turn). This is the dominant pattern in delivery runs — tool-only
assistant turns must not be silently dropped. FR-02.4 is authoritative.

**OQ-SPEC-2** (new): File seek behavior when `transcript_path` points to a file smaller
than `MAX_PRECOMPACT_BYTES * 4`. FR-01.6 states "if the file is smaller than this window,
the entire file is read." The implementation should handle `SeekFrom::End` with an
offset larger than the file size by clamping to file start (`seek` returns
`Ok(0)` in this case on most platforms, but the architect should verify the stdlib
behavior and handle it explicitly).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `hook PreCompact transcript extraction graceful degradation sync IO` — found entry #3331 (PreCompact hook: read transcript_path locally before server round-trip, prepend to BriefingContent), confirming the hook-local extraction pattern is already recorded as a convention for crt-028.
- Queried: `/uni-query-patterns` for `JSONL file reading reverse scan byte budget hook latency` — found entry #243 (ADR-002: Hook Process Uses Blocking std I/O), confirming sync I/O constraint is established.
- Queried: `/uni-query-patterns` for `acceptance criteria patterns security allowlist source field write site` — no directly applicable AC patterns for allowlist security. GH #354 fix is scoped as a standalone security AC per SR-05 recommendation.
