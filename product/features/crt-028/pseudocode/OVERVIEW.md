# crt-028: Pseudocode Overview — WA-5 PreCompact Transcript Restoration

## Components Affected

| Component | File | Change Type |
|-----------|------|-------------|
| hook.rs | `crates/unimatrix-server/src/uds/hook.rs` | Modify — new functions + PreCompact arm |
| listener.rs | `crates/unimatrix-server/src/uds/listener.rs` | Modify — GH #354 source allowlist |
| index_briefing.rs | `crates/unimatrix-server/src/services/index_briefing.rs` | Modify — GH #355 doc + test |

## Data Flow

```
PreCompact hook fires (Claude Code calls hook binary)
  │
  ▼
run() in hook.rs
  │
  ├─ build_request("PreCompact", &hook_input)
  │    → HookRequest::CompactPayload { session_id, injected_entry_ids: vec![], ... }
  │
  ├─ [NEW] extract transcript BEFORE transport.request()
  │    let transcript_block: Option<String> =
  │        hook_input.transcript_path
  │            .as_deref()
  │            .filter(|p| !p.is_empty())
  │            .and_then(|p| extract_transcript_block(p));
  │    // All I/O errors → None; never propagates (ADR-003)
  │
  ├─ transport.request(&request, HOOK_TIMEOUT)
  │    → UDS → listener.rs handle_compact_payload()
  │       → IndexBriefingService::index(query, session_id, k=20)
  │       → format_compaction_payload(entries, ...) → flat indexed table
  │       → HookResponse::BriefingContent { content, token_count }
  │
  └─ Response handling (modified BriefingContent arm):
       let full_output = prepend_transcript(transcript_block.as_deref(), &content);
       if !full_output.is_empty() { println!("{full_output}"); }
```

## Shared Types Introduced

### `ExchangeTurn` (internal to hook.rs, not exported)

```
enum ExchangeTurn {
    UserText(String),
    AssistantText(String),
    ToolPair { name: String, key_param: String, result_snippet: String },
}
```

Produced by `build_exchange_pairs`, consumed by `extract_transcript_block`.

## New Constants (hook.rs)

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_PRECOMPACT_BYTES` | 3000 | Transcript block budget — separate from `MAX_INJECTION_BYTES` (D-4) |
| `TAIL_MULTIPLIER` | 4 | Tail-bytes window multiplier: `TAIL_WINDOW_BYTES = 3000 * 4 = 12,000` |
| `TOOL_RESULT_SNIPPET_BYTES` | 300 | Per-tool-result truncation limit |
| `TOOL_KEY_PARAM_BYTES` | 120 | Key-param truncation limit |

## New Functions (hook.rs)

| Function | Signature | Pure? |
|----------|-----------|-------|
| `extract_transcript_block` | `fn(path: &str) -> Option<String>` | No (file I/O) |
| `build_exchange_pairs` | `fn(lines: &[&str]) -> Vec<ExchangeTurn>` | Yes |
| `prepend_transcript` | `fn(transcript: Option<&str>, briefing: &str) -> String` | Yes |
| `extract_key_param` | `fn(tool_name: &str, input: &serde_json::Value) -> String` | Yes |

## New Functions (listener.rs)

| Function | Signature |
|----------|-----------|
| `sanitize_observation_source` | `fn(source: Option<&str>) -> String` |

## Output Format Contract

```
=== Recent conversation (last N exchanges) ===
[User] {user text}
[Assistant] {assistant text}
[tool: {name}({key_param}) → {snippet}]

[User] {next user text}
[tool: {name}({key_param}) → {snippet}]

=== End recent conversation ===

{BriefingContent from IndexBriefingService}
```

When briefing is empty and transcript present: transcript block only (no trailing blank line).
When transcript is None and briefing is non-empty: briefing verbatim (no header injected).
When both empty: empty stdout (FR-01.4 invariant).

## Sequencing Constraints

1. crt-027 must be merged before crt-028 delivery begins (provides `IndexBriefingService`).
2. `hook.rs` changes are independent of `listener.rs` and `index_briefing.rs` changes.
3. `build_exchange_pairs` and `extract_key_param` must exist before `extract_transcript_block`.
4. `prepend_transcript` must exist before the `run()` modification.
5. `sanitize_observation_source` in `listener.rs` is fully independent of all hook.rs changes.

## Integration Surface (no-change boundaries)

- `unimatrix-engine/src/wire.rs` — `HookInput.transcript_path: Option<String>` already present
- `HookResponse::BriefingContent { content, token_count }` — existing variant, no change
- `handle_compact_payload` in listener.rs — no change beyond GH #354 fix
- `write_stdout` function — structurally unchanged; PreCompact prepend happens before the call
