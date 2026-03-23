## ADR-002: Tool-Use/Tool-Result Pairing Strategy

### Context

Claude Code JSONL records the conversation as a sequence of `type: "user"` and
`type: "assistant"` records. Tool interactions are split across two adjacent records:

```jsonl
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu_abc","name":"Read","input":{"file_path":"/foo.rs"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu_abc","content":"fn main() {..."}]}}
```

The `tool_use` block appears in an assistant record; the `tool_result` block appears in the
immediately following user record. This is the canonical Claude API message structure.

WA-5 must produce compact `[tool: name(key_param) → snippet]` pairs (D-2, D-3). To do
this, each `tool_use` must be matched with its corresponding `tool_result` to produce the
result snippet.

Two strategies were considered:

**Option A: Two-pass scan.** First pass: collect all `(tool_use_id, tool_use)` pairs into
a HashMap. Second pass: match `tool_result` blocks by `tool_use_id`. Works regardless of
how far apart the use and result records are.

Rejected: Claude Code's API guarantees that `tool_result` follows `tool_use` in the
immediately adjacent user record. A two-pass strategy would require holding the entire
parsed window in memory as two HashMaps. The added complexity is not warranted given the
guarantee. Additionally, two-pass requires reading all lines before any pairing can occur —
same asymptotic cost as one-pass but with a constant factor overhead.

**Option B: Adjacent-record scan (one-pass, look-ahead by one record).** When parsing an
assistant record with `tool_use` blocks, peek at the next record in the JSONL line sequence.
If the next record is `type: "user"` and contains `tool_result` blocks, match them against
the collected `tool_use` list by `tool_use_id`. Consume both records in a single iteration
step.

Selected. This matches Claude Code's structure directly. The look-ahead is a single-record
peek (array index `i+1`), not a full buffer scan.

**Edge cases handled**:
- Multiple `tool_use` blocks in one assistant record: all are collected, all matched against
  the same following user record's `tool_result` list.
- `tool_use` with no matching `tool_result` in N+1: emits `ToolPair { result_snippet: "" }`.
  The pair is still included in the restoration block (D-3 requires compact pairs, not
  omission).
- `tool_result` with no preceding `tool_use` (orphaned): silently skipped. This can occur
  when the tail-bytes window starts mid-conversation and the corresponding `tool_use` was
  before the window.
- Next record is another assistant record (back-to-back assistant turns, unusual but valid):
  no pairing occurs for this `tool_use` set. The tool_use blocks are emitted as
  `ToolPair { result_snippet: "" }`.

**`tool_use_id` format**: Claude Code generates `tool_use_id` values like `"tu_01AbCd..."`.
The matching is a simple string equality check — no parsing of the ID format is required.

**Result truncation**: `tool_result.content` is truncated to `TOOL_RESULT_SNIPPET_BYTES`
(300 bytes) using `truncate_utf8` before storing as `result_snippet`. This preserves enough
context for the agent to decide whether to re-fetch (D-3) without bloating the block.

### Decision

Use adjacent-record scan: when building exchange pairs from the JSONL window, process lines
in order. When an assistant record has `tool_use` blocks, look ahead one record. If the
next record is a user record, scan its `tool_result` blocks and match by `tool_use_id`.
Both records are consumed in a single iteration step (index advances by 2 for the
assistant+user pair when tool_use is present).

`build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>` implements this algorithm and
returns turns in reverse-chronological order (Vec reversed before return) so the
budget-fill loop fills from most-recent turns first.

The `tool_use` content is extracted via `extract_key_param(tool_name, &input)` using a
hardcoded map for known Claude Code tools (OQ-3 settled, see ARCHITECTURE.md key-param map).

### Consequences

- Pairing is correct for the canonical Claude Code JSONL structure. If Claude Code ever
  emits `tool_result` records non-adjacently (currently not observed), pairing will
  silently produce empty result snippets — degradation, not error.
- Tool calls at the very end of the tail-bytes window (where the following user record is
  outside the window) will have empty result snippets. Acceptable — the agent sees the tool
  name and key_param even without the result.
- The one-pass approach keeps memory proportional to the window size (not the full file).
- Orphaned `tool_result` blocks (e.g., the window starts mid-tool-call) are silently
  skipped — no spurious output, no error.
- `thinking` blocks in assistant content (`type: "thinking"`) are silently skipped by the
  parser (not in the extracted types list) — correct behavior per D-2.
