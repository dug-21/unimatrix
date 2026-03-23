# crt-028: hook.rs Pseudocode

## Purpose

Add transcript extraction and prepend to the PreCompact hook path in
`crates/unimatrix-server/src/uds/hook.rs`. Provides task continuity to agents
after context-window compaction by reading the session transcript locally (no
server round-trip), parsing recent exchanges, and prepending a structured
restoration block before the `IndexBriefingService` output.

All I/O is `std::io` only. No tokio. Hook always exits 0 (FR-03.7).

---

## New Constants

Add immediately after existing `MIN_QUERY_WORDS` constant:

```
/// Maximum byte budget for the PreCompact transcript restoration block (~750 tokens).
/// Separate from MAX_INJECTION_BYTES (1400) per D-4 and AC-10.
/// TUNABLE: future config.toml pass may make this runtime-configurable (FR-04.4, SR-03).
const MAX_PRECOMPACT_BYTES: usize = 3000;

/// Tail-bytes window multiplier. Raw JSONL is ~4x larger than extracted text
/// due to JSON overhead, thinking blocks, and verbose tool_result content.
/// TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 12,000 bytes (ADR-001).
const TAIL_MULTIPLIER: usize = 4;

/// Per-tool-result snippet truncation budget (D-3, FR-03.4).
const TOOL_RESULT_SNIPPET_BYTES: usize = 300;

/// Key-param truncation budget for tool compact representation (OQ-3).
const TOOL_KEY_PARAM_BYTES: usize = 120;
```

---

## New Enum: `ExchangeTurn`

Add before the new functions, not exported:

```
/// A single typed turn extracted from the JSONL transcript window.
/// Internal to hook.rs — not exported or used by other modules.
enum ExchangeTurn {
    UserText(String),
    AssistantText(String),
    ToolPair {
        name: String,
        key_param: String,
        result_snippet: String,
    },
}
```

---

## New Function: `extract_key_param`

```
/// Return the most-identifying input field value for a tool call.
///
/// Hardcoded map for 10 known Claude Code tools (OQ-3 settled).
/// Fallback: first string-valued field in the input object.
/// Result truncated to TOOL_KEY_PARAM_BYTES via truncate_utf8.
///
/// Known mappings:
///   Bash       -> "command"
///   Read       -> "file_path"
///   Edit       -> "file_path"
///   Write      -> "file_path"
///   Glob       -> "pattern"
///   Grep       -> "pattern"
///   MultiEdit  -> "file_path"
///   Task       -> "description"
///   WebFetch   -> "url"
///   WebSearch  -> "query"
///   _          -> first string field (fallback)
///
/// NOTE (R-09): The first-string-field fallback may select a sensitive field
/// for unknown tools. A future denylist pass (api_key, token, secret, password)
/// is recommended before production. Documented as a known limitation.
fn extract_key_param(tool_name: &str, input: &serde_json::Value) -> String {
    // 1. Map tool_name to the field name via exhaustive match
    let field_name: &str = match tool_name {
        "Bash"      => "command",
        "Read"      => "file_path",
        "Edit"      => "file_path",
        "Write"     => "file_path",
        "Glob"      => "pattern",
        "Grep"      => "pattern",
        "MultiEdit" => "file_path",
        "Task"      => "description",
        "WebFetch"  => "url",
        "WebSearch" => "query",
        _           => "",          // signals: use fallback
    };

    // 2. If mapped: attempt to extract from input object
    if !field_name.is_empty() {
        if let Some(val) = input.get(field_name).and_then(|v| v.as_str()) {
            return truncate_utf8(val, TOOL_KEY_PARAM_BYTES).to_string();
        }
        // Field absent or not a string: fall through to fallback
    }

    // 3. Fallback: iterate input object fields, return first string value
    if let Some(obj) = input.as_object() {
        for (_key, val) in obj {
            if let Some(s) = val.as_str() {
                return truncate_utf8(s, TOOL_KEY_PARAM_BYTES).to_string();
            }
        }
    }

    // 4. No string field found: return empty string (FR-03.3)
    String::new()
}
```

---

## New Function: `build_exchange_pairs`

```
/// Parse JSONL lines from a tail window into typed exchange turns.
///
/// Fail-open: malformed lines and unknown type values are skipped silently
/// (SR-01, AC-08, FR-06.3, FR-06.4).
///
/// Tool-use/result pairing: adjacent-record scan (ADR-002).
/// When an assistant record has tool_use blocks, the immediately following
/// record is inspected. If it is a user record containing tool_result blocks,
/// those are matched by tool_use_id.
///
/// OQ-SPEC-1 rule:
/// - Assistant turn with >= 1 tool_use but zero type:"text" blocks:
///     emit ToolPair entries only (no AssistantText turn emitted).
///     [Assistant] header line is omitted in formatting.
/// - Assistant turn with zero type:"text" AND zero tool_use (e.g., only
///     type:"thinking"): suppress entirely — emit nothing for this turn.
///
/// Returns turns in reverse-chronological order (Vec reversed before return).
/// Callers use this ordering to fill the byte budget from most-recent first.
fn build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn> {
    let mut turns: Vec<ExchangeTurn> = Vec::new();

    // Iteration state: process records by index, sometimes consuming i+1
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // Skip empty lines
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Parse line as JSON; skip on failure (fail-open, AC-08)
        let record: serde_json::Value = match serde_json::from_str(line) {
            Ok(v)  => v,
            Err(_) => { i += 1; continue; }
        };

        // Extract the top-level "type" field
        // Claude Code JSONL records have shape:
        //   { "type": "user"|"assistant", "message": { "content": [...] }, ... }
        // Some older records may have content directly at top level — handle both shapes.
        let record_type = match record.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None    => { i += 1; continue; }  // no type field: skip
        };

        match record_type {
            "user" => {
                // Extract content blocks; skip tool_result blocks (FR-03.6)
                // User text = concatenation of type:"text" blocks with "\n" separator
                let content_arr = get_content_array(&record);
                let user_texts: Vec<&str> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                            block.get("text").and_then(|v| v.as_str())
                        } else {
                            None  // skip tool_result, image, and unknown block types
                        }
                    })
                    .collect();

                if !user_texts.is_empty() {
                    turns.push(ExchangeTurn::UserText(user_texts.join("\n")));
                }
                // tool_result blocks in user records are consumed during assistant pairing (below),
                // not here. A standalone user record with only tool_result and no text
                // produces no UserText turn (correct — FR-03.6).
                i += 1;
            }

            "assistant" => {
                // Step A: Collect type:"text" blocks → AssistantText
                let content_arr = get_content_array(&record);

                let asst_texts: Vec<&str> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                            block.get("text").and_then(|v| v.as_str())
                        } else {
                            None  // thinking, tool_use collected separately below
                        }
                    })
                    .collect();

                // Step B: Collect type:"tool_use" blocks
                // Each tool_use: { "type": "tool_use", "id": "tu_...", "name": "Read", "input": {...} }
                struct ToolUseInfo {
                    id: String,
                    name: String,
                    key_param: String,
                }
                let tool_uses: Vec<ToolUseInfo> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                            return None;
                        }
                        let id   = block.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = block.get("name").and_then(|v| v.as_str())?.to_string();
                        let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                        let key_param = extract_key_param(&name, &input);
                        Some(ToolUseInfo { id, name, key_param })
                    })
                    .collect();

                // OQ-SPEC-1: determine whether to emit AssistantText
                let has_text      = !asst_texts.is_empty();
                let has_tool_use  = !tool_uses.is_empty();

                // Pure thinking turn (no text, no tool_use): suppress entirely
                if !has_text && !has_tool_use {
                    i += 1;
                    continue;
                }

                // Emit AssistantText only if there is actual text (OQ-SPEC-1)
                if has_text {
                    turns.push(ExchangeTurn::AssistantText(asst_texts.join("\n")));
                }
                // When tool_use present but no text: no AssistantText emitted
                // (the ToolPair lines stand alone in the output block)

                // Step C: Adjacent-record look-ahead for tool_result pairing (ADR-002)
                // Only attempt pairing when this assistant record has tool_use blocks
                // AND the next record exists and is type:"user".
                let mut result_map: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();

                if has_tool_use && i + 1 < lines.len() {
                    let next_line = lines[i + 1];
                    if !next_line.trim().is_empty() {
                        if let Ok(next_record) = serde_json::from_str::<serde_json::Value>(next_line) {
                            if next_record.get("type").and_then(|v| v.as_str()) == Some("user") {
                                // Scan next record's content for tool_result blocks
                                let next_content = get_content_array(&next_record);
                                for block in next_content {
                                    if block.get("type").and_then(|v| v.as_str()) != Some("tool_result") {
                                        continue;
                                    }
                                    let tool_use_id = match block.get("tool_use_id").and_then(|v| v.as_str()) {
                                        Some(id) => id.to_string(),
                                        None     => continue,
                                    };
                                    // Extract first type:"text" content block from result (FR-03.4)
                                    let snippet = extract_tool_result_snippet(block);
                                    result_map.insert(tool_use_id, snippet);
                                }
                                // NOTE: the next user record is also processed at i+1 in the
                                // outer loop for its UserText content. The adjacent-record scan
                                // does NOT consume i+1 — the outer loop advances to it normally.
                                // This means tool_result-only user records still run through the
                                // "user" arm, which skips tool_result blocks (FR-03.6).
                            }
                        }
                    }
                }

                // Step D: Emit ToolPair for each tool_use; use result_map for snippet
                // Unmatched tool_use → empty snippet (ADR-002, FR-03.8)
                for tu in &tool_uses {
                    let result_snippet = result_map
                        .get(&tu.id)
                        .cloned()
                        .unwrap_or_default();
                    turns.push(ExchangeTurn::ToolPair {
                        name:           tu.name.clone(),
                        key_param:      tu.key_param.clone(),
                        result_snippet,
                    });
                }

                i += 1;
                // NOTE: do NOT skip i+1 here — the following user record is processed
                // by the outer loop both for its UserText AND was already read for
                // tool_result pairing. This is correct: the loop processes all records.
            }

            _ => {
                // Unknown type (system, summary, etc.): skip (FR-01.3)
                i += 1;
            }
        }
    }

    // Reverse for reverse-chronological order (most-recent first)
    // This is the ordering expected by the budget-fill loop in extract_transcript_block.
    turns.reverse();
    turns
}

/// Helper: extract the content array from a JSONL record.
/// Handles two shapes:
///   { "type": "...", "message": { "content": [...] } }  (Claude Code UX format)
///   { "type": "...", "content": [...] }                 (raw API format)
/// Returns empty slice if neither shape is present.
fn get_content_array(record: &serde_json::Value) -> &[serde_json::Value] {
    // Try message.content first
    if let Some(arr) = record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        return arr;
    }
    // Try top-level content
    if let Some(arr) = record.get("content").and_then(|c| c.as_array()) {
        return arr;
    }
    &[]
}

/// Helper: extract snippet text from a tool_result content block.
/// Returns first type:"text" block text truncated to TOOL_RESULT_SNIPPET_BYTES.
/// Returns empty string if no text block is found.
fn extract_tool_result_snippet(tool_result_block: &serde_json::Value) -> String {
    // tool_result content can be:
    //   - a String directly: { "content": "some text" }
    //   - an array of blocks: { "content": [{ "type": "text", "text": "..." }] }
    let content = tool_result_block.get("content");
    match content {
        Some(serde_json::Value::String(s)) => {
            truncate_utf8(s, TOOL_RESULT_SNIPPET_BYTES).to_string()
        }
        Some(serde_json::Value::Array(blocks)) => {
            for block in blocks {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        return truncate_utf8(text, TOOL_RESULT_SNIPPET_BYTES).to_string();
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}
```

---

## New Function: `extract_transcript_block`

```
/// Read the tail of the transcript file at `path`, parse as JSONL, and format
/// a restoration block within MAX_PRECOMPACT_BYTES.
///
/// Returns None on any failure (ADR-003 degradation contract):
///   - path empty (caller filters before calling)
///   - file open / metadata / seek error
///   - no parseable user/assistant pairs in the tail window
///   - budget fills with zero complete turns
///
/// Never panics. Never propagates errors. No stderr output (FR-06.2, NFR-03).
/// All I/O is std::io — no tokio (constraint: hook has no tokio runtime).
fn extract_transcript_block(path: &str) -> Option<String> {
    // Inner closure: all ? operators contained here; maps failures to None (ADR-003)
    let inner = || -> Option<String> {
        // Step 1: Open file read-only
        let mut file = std::fs::File::open(path).ok()?;

        // Step 2: Get file length for seek calculation
        let file_len: u64 = file.metadata().ok()?.len();

        // Step 3: Compute tail-bytes seek position (ADR-001, OQ-SPEC-2 mitigation)
        // seek_back = min(TAIL_WINDOW_BYTES, file_len)
        // When seek_back == 0 (zero-byte file): no seek, BufReader yields no lines → None
        let window: u64 = (MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER) as u64;
        let seek_back: u64 = window.min(file_len);

        if seek_back > 0 {
            // SeekFrom::End(-seek_back): always valid because seek_back <= file_len
            // This positions the read cursor at max(0, file_end - seek_back).
            // For files smaller than the window: seek_back == file_len → positions at 0
            // (equivalent to SeekFrom::Start(0), no data discarded).
            // For files larger than the window: positions seek_back bytes before EOF.
            // The first line may be truncated JSON — silently skipped by fail-open parser.
            file.seek(std::io::SeekFrom::End(-(seek_back as i64))).ok()?;
        }
        // When seek_back == 0: file is empty; BufReader yields no lines; function returns None.

        // Step 4: Collect all lines from this position forward
        let reader = std::io::BufReader::new(file);
        let raw_lines: Vec<String> = {
            use std::io::BufRead;
            reader.lines().filter_map(|l| l.ok()).collect()
        };

        // Step 5: Build reference slices for build_exchange_pairs
        let line_refs: Vec<&str> = raw_lines.iter().map(|s| s.as_str()).collect();

        // Step 6: Parse into exchange turns (reverse-chronological order)
        let turns: Vec<ExchangeTurn> = build_exchange_pairs(&line_refs);

        // Step 7: Fill byte budget most-recent-first, building output lines
        // Each complete exchange group is either included entirely or not at all (FR-04.2).
        // We format turns into text lines, then group into exchange units for budget check.
        // Strategy: format all turns into individual text lines, accumulate with budget guard.
        //
        // Budget accounting is done at the formatted-string level, not per turn enum,
        // because a single exchange group may contain multiple turns (UserText + ToolPairs).
        // We accumulate formatted lines in a buffer and stop when adding the next
        // "exchange unit" (one UserText + associated AssistantText/ToolPairs) would
        // exceed MAX_PRECOMPACT_BYTES.
        //
        // Simple implementation: collect formatted text per turn, then accumulate
        // in reverse (turns are already reverse-chronological from build_exchange_pairs).
        // Emit N turns that fit in MAX_PRECOMPACT_BYTES.

        let mut output_parts: Vec<String> = Vec::new();
        let mut bytes_used: usize = 0;
        let mut exchange_count: usize = 0;

        for turn in &turns {
            let turn_text = format_turn(turn);
            let turn_bytes = turn_text.len();
            if bytes_used + turn_bytes > MAX_PRECOMPACT_BYTES {
                break;
            }
            bytes_used += turn_bytes;
            if matches!(turn, ExchangeTurn::UserText(_)) {
                exchange_count += 1;
            }
            output_parts.push(turn_text);
        }

        // Step 8: Require at least one turn to produce output (AC-09)
        if output_parts.is_empty() {
            return None;
        }

        // Step 9: Wrap with section headers (FR-02.1, FR-02.5)
        let header = format!("=== Recent conversation (last {} exchanges) ===", exchange_count);
        let footer = "=== End recent conversation ===".to_string();

        // Join turns with blank line separators between exchange pairs.
        // Blank line logic: insert blank line between UserText and the next UserText
        // (i.e., between exchange pairs). ToolPairs and AssistantText are grouped
        // with their preceding turn — no extra blank line within an exchange.
        // Implementation: insert "\n" between each formatted turn for readability.
        // A blank line between pairs is natural because format_turn ends each user
        // block with a newline; the separator between exchange pairs adds one more.
        let body = output_parts.join("\n");

        Some(format!("{}\n{}\n{}", header, body, footer))
    };

    inner()
}

/// Helper: format a single ExchangeTurn as a text line.
/// UserText    → "[User] {text}"
/// AssistantText → "[Assistant] {text}"
/// ToolPair    → "[tool: {name}({key_param}) → {snippet}]"
fn format_turn(turn: &ExchangeTurn) -> String {
    match turn {
        ExchangeTurn::UserText(text) => {
            format!("[User] {}", text)
        }
        ExchangeTurn::AssistantText(text) => {
            format!("[Assistant] {}", text)
        }
        ExchangeTurn::ToolPair { name, key_param, result_snippet } => {
            format!("[tool: {}({}) → {}]", name, key_param, result_snippet)
        }
    }
}
```

---

## New Function: `prepend_transcript`

```
/// Combine optional transcript block with briefing content.
///
/// Four explicit cases (FR-05, SR-04):
///
/// 1. Both present (transcript Some and non-empty, briefing non-empty):
///       "{transcript}\n\n{briefing}"
///    The double newline is the section separator (FR-02.6, FR-05.1).
///
/// 2. Transcript only (briefing is empty string):
///       "{transcript}"   (verbatim — headers already present in block)
///    No trailing blank line (FR-05.3).
///
/// 3. Briefing only (transcript is None):
///       "{briefing}"     (verbatim — no header injected, FR-05.2)
///
/// 4. Both empty (transcript None, briefing ""):
///       ""               (FR-01.4 invariant — nothing written to stdout)
///
/// No byte-cap applied here. The transcript block was already capped at
/// MAX_PRECOMPACT_BYTES inside extract_transcript_block.
fn prepend_transcript(transcript: Option<&str>, briefing: &str) -> String {
    let briefing_empty = briefing.is_empty();

    match (transcript, briefing_empty) {
        (Some(t), false) => {
            // Case 1: both present
            format!("{}\n\n{}", t, briefing)
        }
        (Some(t), true) => {
            // Case 2: transcript only
            t.to_string()
        }
        (None, false) => {
            // Case 3: briefing only
            briefing.to_string()
        }
        (None, true) => {
            // Case 4: both empty
            String::new()
        }
    }
}
```

---

## Modification: `run()` — PreCompact arm

The `run()` function requires two targeted changes. Both are within the existing
`else` branch that handles synchronous (non-fire-and-forget) requests.

### Change 1: Extract transcript BEFORE `transport.request()`

After the existing `Step 5b` (extract `req_source`), add Step 5c:

```
// Step 5b: (existing) Extract source for response routing
let req_source: Option<String> = match &request {
    HookRequest::ContextSearch { source, .. } => source.clone(),
    _ => None,
};

// Step 5c: (NEW) Extract transcript block before server round-trip (OQ-2 resolved)
// Read only for PreCompact; other events do not use transcript_path.
// Failure → None; BriefingContent is always written regardless (ADR-003).
let transcript_block: Option<String> = if matches!(request, HookRequest::CompactPayload { .. }) {
    hook_input
        .transcript_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .and_then(|p| extract_transcript_block(p))
} else {
    None
};
```

### Change 2: Prepend in the `BriefingContent` response handler

Inside the `Ok(response)` arm of `transport.request()`, change the existing
`write_stdout` call site. The current code is:

```
let write_result = if req_source.as_deref() == Some("SubagentStart") {
    write_stdout_subagent_inject_response(&response)
} else {
    write_stdout(&response)           // <-- existing
};
```

Replace only the `else` branch. The SubagentStart path is unchanged.
For non-SubagentStart responses, intercept `BriefingContent` to prepend:

```
let write_result = if req_source.as_deref() == Some("SubagentStart") {
    write_stdout_subagent_inject_response(&response)
} else {
    // Modified: for BriefingContent responses, prepend transcript block (D-5)
    // For all other response types, delegate to write_stdout unchanged (AC-14).
    match &response {
        HookResponse::BriefingContent { content, .. } => {
            let full_output = prepend_transcript(transcript_block.as_deref(), content);
            if !full_output.is_empty() {
                println!("{full_output}");
            }
            Ok(())
        }
        _ => write_stdout(&response),
    }
};
```

This keeps `write_stdout` structurally unmodified (AC-14 invariant: non-PreCompact
events are not affected). The PreCompact interception is explicit and localized.

---

## Imports to Add

At the top of hook.rs, in the existing `use std::io::Read;` block, add:

```
use std::io::{BufRead, BufReader, Seek, SeekFrom};
```

`serde_json` is already in the crate dependency tree; no new crate dependency.

---

## Error Handling

| Site | Error | Handling |
|------|-------|----------|
| `File::open(path)` | NotFound, PermissionDenied, other | `.ok()?` inside inner closure → None |
| `file.metadata()` | OS error | `.ok()?` → None |
| `file.seek(...)` | Seek error (named pipe etc.) | `.ok()?` → None |
| `serde_json::from_str(line)` | Malformed JSON | Skip line, continue loop (fail-open) |
| `build_exchange_pairs` returns `vec![]` | No parseable turns | `output_parts.is_empty()` → None |
| Byte budget reached before first turn | All turns too large | `output_parts.is_empty()` → None |
| `prepend_transcript` called with None | Transcript unavailable | Returns briefing verbatim |

No error type escapes `extract_transcript_block`. The outer call chain uses `and_then`
which short-circuits on None without error propagation (ADR-003 contract).

---

## Key Test Scenarios

These scenarios guide the tester; they are not exhaustive (see test-plan/hook.md).

### R-01 (Critical): Degradation boundary — briefing always written

1. `transcript_path` = non-existent path: assert `prepend_transcript(None, "briefing")` = "briefing"
2. `transcript_path` = file with all malformed JSONL: `extract_transcript_block` returns None; briefing written
3. `transcript_path` = None: `and_then` short-circuits; briefing written unchanged
4. Zero-byte file: `seek_back = 0`, no seek, no lines, `build_exchange_pairs([])` = `[]`, returns None

### R-03 (Critical): SeekFrom::End clamp

1. File of 100 bytes (window = 12000): `seek_back = 100`, `SeekFrom::End(-100)` = `SeekFrom::Start(0)`, all lines read
2. File of exactly 12000 bytes: `seek_back = 12000`, `SeekFrom::End(-12000)` = position 0
3. File of 12001 bytes: `seek_back = 12000`, `SeekFrom::End(-12000)`, first line may be partial → skipped

### R-05 (High): Reversal order

1. JSONL with exchanges A, B, C (oldest → newest): `build_exchange_pairs` returns C, B, A
2. Budget fills after 2 exchanges: output contains C and B, not A

### R-10 (High): OQ-SPEC-1 tool-only turns

1. Assistant turn with tool_use + thinking, no text: `has_text=false, has_tool_use=true` → ToolPair emitted, no AssistantText
2. Assistant turn with only thinking: `has_text=false, has_tool_use=false` → turn suppressed entirely
3. Session of all tool-call-only turns: at least one ToolPair emitted (not zero restoration)

### R-12 (Low): prepend_transcript output format

1. `prepend_transcript(Some("block"), "")` — contains `"=== Recent conversation"` (already in block)
2. `prepend_transcript(None, "")` — returns `""`
3. `prepend_transcript(Some("block"), "briefing")` — contains both, `"\n\n"` separator present
4. `prepend_transcript(None, "briefing")` — returns `"briefing"` verbatim

### R-06 (High): truncate_utf8 reuse

1. Tool result with 4-byte CJK char at byte 300: snippet is 297 bytes (char boundary respected)
2. Key-param with 4-byte emoji at byte 120: key_param is 116 bytes (char boundary respected)

### build_exchange_pairs: get_content_array shape handling

1. `message.content` shape (Claude Code UX): content extracted correctly
2. `content` top-level shape (raw API): content extracted correctly
3. Missing both: empty slice returned, no panic

### extract_key_param: all 10 known tools

Each of: Bash, Read, Edit, Write, Glob, Grep, MultiEdit, Task, WebFetch, WebSearch
→ verify correct field extracted and truncated to 120 bytes.

Unknown tool with `{"command": "ls"}` → "command" value returned (first string field).
