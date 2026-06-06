# normalize.js — Event Canonicalization

## Purpose
Pure string maps porting `hook.rs:50-105` verbatim: `mapToCanonical(event)` and
`normalizeEventName(event) -> [canonical, provider]`. No I/O, no allocation concerns —
just exact table parity. F3 has no `--provider` flag, so index.js uses ONLY
`normalizeEventName` (inference path); `mapToCanonical` is exported for completeness and
future hint-path use (kept because the parity corpus enumerates it).

## Functions

### mapToCanonical(event) -> string
```
function mapToCanonical(event):
  switch event:
    "BeforeTool"        -> "PreToolUse"        // Gemini
    "AfterTool"         -> "PostToolUse"       // Gemini
    "SessionEnd"        -> "Stop"              // Gemini
    "PreToolUse"        -> "PreToolUse"
    "PostToolUse"       -> "PostToolUse"
    "SessionStart"      -> "SessionStart"
    "Stop"              -> "Stop"
    "TaskCompleted"     -> "TaskCompleted"
    "Ping"              -> "Ping"
    "UserPromptSubmit"  -> "UserPromptSubmit"
    "PreCompact"        -> "PreCompact"
    "PostToolUseFailure"-> "PostToolUseFailure"
    "SubagentStart"     -> "SubagentStart"
    "SubagentStop"      -> "SubagentStop"
    default             -> "__unknown__"       // caller substitutes the raw event name
```

### normalizeEventName(event) -> [canonical, provider]
```
function normalizeEventName(event):
  switch event:
    "BeforeTool"  -> ["PreToolUse",  "gemini-cli"]
    "AfterTool"   -> ["PostToolUse", "gemini-cli"]
    "SessionEnd"  -> ["Stop",        "gemini-cli"]
    // all 11 canonical names         -> [name, "claude-code"]
    default       -> ["__unknown__", "unknown"]
// Exact-match, case-sensitive, no trimming — byte-for-byte parity with the Rust match.
```

## Error Handling
None — total functions over strings. Non-string input cannot reach here (argv[2] is a
string or ""; "" hits the default arm exactly as in Rust).

## Key Test Scenarios
1. Full 14-name table (11 canonical + 3 Gemini) returns the exact pairs above.
2. Unknown name → `["__unknown__","unknown"]`; index.js preserves the RAW name as
   `effectiveEvent` (corpus: unknown-event passthrough case asserts the raw string in
   the resulting RecordEvent `event_type`).
3. Case sensitivity: `"pretooluse"` → unknown (no normalization beyond the table).
4. Empty string → unknown.
