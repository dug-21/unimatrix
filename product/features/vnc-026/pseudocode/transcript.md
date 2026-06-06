# transcript.js — JSONL Tail-Parse (SubagentStart Query Derivation, RQ-6)

## Purpose
Port of `uds/transcript_block.rs` (entire module, the path-variant front-end):
`extractTranscriptBlock(path) -> string|null`. Used ONLY by the SubagentStart fallback in
index.js — the single permitted transcript read on a sync spawn (FR-09 exception). Also
exports `truncateUtf8` (shared by build-request.js goal truncation and delta.js trims).

## Constants (PINNED — transcript_block.rs:18-29)
```
MAX_PRECOMPACT_BYTES = 3000
TAIL_MULTIPLIER = 4                      // window = 12,000 bytes
TOOL_RESULT_SNIPPET_BYTES = 300
TOOL_KEY_PARAM_BYTES = 120
KEY_PARAM_FIELDS = { Bash:"command", Read:"file_path", Edit:"file_path",
  Write:"file_path", Glob:"pattern", Grep:"pattern", MultiEdit:"file_path",
  Task:"description", WebFetch:"url", WebSearch:"query" }
```

## Functions

### truncateUtf8(str, maxBytes) -> string  (transcript_block.rs:45-56)
JS strings are UTF-16 — operate on the UTF-8 byte image:
```
function truncateUtf8(s, maxBytes):
  buf = Buffer.from(s, "utf8")
  if buf.length <= maxBytes: return s
  end = maxBytes
  while end > 0 and isUtf8Continuation(buf[end]): end -= 1   // byte & 0xC0 === 0x80
  return buf.subarray(0, end).toString("utf8")
  // Backing to a UTF-8 char boundary = whole code points = no split surrogate pairs.
```

### extractTranscriptBlock(path) -> string|null  (transcript_block.rs:358-378)
```
function extractTranscriptBlock(path):
  try:
    fd = fs.openSync(path, "r")
    try:
      fileLen = fs.fstatSync(fd).size
      window = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER          // 12,000
      seekBack = Math.min(window, fileLen)
      buf = Buffer.alloc(seekBack)
      fs.readSync(fd, buf, 0, seekBack, fileLen - seekBack)    // positioned read
    finally: fs.closeSync(fd)
    lines = splitLinesLikeBufRead(buf)
    return blockFromLines(lines)
  catch: return null                       // any failure → None (degradation contract)

function splitLinesLikeBufRead(buf):       // BufRead::lines() parity
  segs = split buf on 0x0A at the BYTE level (drop the \n; strip one trailing 0x0D per line)
  out = []
  for seg of segs:
    s = seg.toString("utf8")
    if Buffer.from(s, "utf8").equals(seg): out.push(s)   // valid-UTF-8 round-trip check
    // else: DROP the line — Rust String::from_utf8 errs per line and filter_map skips it.
    // JS lossy decode would keep U+FFFD lines; dropping preserves oracle parity.
  return out
```

### buildExchangePairs(lines) -> ExchangeTurn[]  (transcript_block.rs:140-293)
ExchangeTurn = `{kind:"user", text}` | `{kind:"assistant", text}` |
`{kind:"tool", name, keyParam, resultSnippet}`.
```
function buildExchangePairs(lines):
  turns = []; i = 0
  while i < lines.length:
    line = lines[i]
    if line.trim() === "": i++; continue
    try: record = JSON.parse(line) catch: i++; continue        // malformed → skip silently
    t = (record is object) ? record.type : undefined
    if typeof t !== "string": i++; continue
    if t === "user":
      texts = textBlocks(getContentArray(record))              // type:"text" → .text strings
      if texts.length: turns.push({kind:"user", text: texts.join("\n")})
      i++
    else if t === "assistant":
      arr = getContentArray(record)
      texts = textBlocks(arr)
      toolUses = arr.filter(b => b?.type === "tool_use" and typeof b.id === "string"
                                 and typeof b.name === "string")
                    .map(b => ({ id:b.id, name:b.name,
                                 keyParam: extractKeyParam(b.name, b.input ?? null) }))
      if texts.length === 0 and toolUses.length === 0: i++; continue   // thinking-only: suppress
      if texts.length: turns.push({kind:"assistant", text: texts.join("\n")})
      resultMap = {}
      if toolUses.length and i+1 < lines.length:               // adjacent-record look-ahead ONLY
        next = lines[i+1]
        if next.trim() !== "":
          try nextRec = JSON.parse(next):
            if nextRec?.type === "user":
              for block of getContentArray(nextRec):
                if block?.type === "tool_result" and typeof block.tool_use_id === "string":
                  resultMap[block.tool_use_id] = extractToolResultSnippet(block)
      for tu of toolUses:
        turns.push({kind:"tool", name:tu.name, keyParam:tu.keyParam,
                    resultSnippet: resultMap[tu.id] ?? ""})
      i++
    else: i++                                                  // unknown type: skip
  return turns.reverse()                                       // reverse-chronological

function getContentArray(record):          // transcript_block.rs:99-111 (two shapes)
  if Array.isArray(record?.message?.content): return record.message.content
  if Array.isArray(record?.content): return record.content
  return []

function extractKeyParam(toolName, input): // transcript_block.rs:63-93
  field = KEY_PARAM_FIELDS[toolName] ?? ""
  if field !== "" and typeof input?.[field] === "string":
    return truncateUtf8(input[field], TOOL_KEY_PARAM_BYTES)
  if input is plain object:
    for value of Object.values(input):     // insertion order ≙ preserve_order iteration
      if typeof value === "string": return truncateUtf8(value, TOOL_KEY_PARAM_BYTES)
  return ""

function extractToolResultSnippet(block):  // transcript_block.rs:115-133
  c = block.content
  if typeof c === "string": return truncateUtf8(c, TOOL_RESULT_SNIPPET_BYTES)
  if Array.isArray(c):
    for b of c: if b?.type === "text" and typeof b.text === "string":
      return truncateUtf8(b.text, TOOL_RESULT_SNIPPET_BYTES)
  return ""
```

### blockFromLines(lines) -> string|null  (transcript_block.rs:319-351)
```
function blockFromLines(lines):
  turns = buildExchangePairs(lines)
  parts = []; bytesUsed = 0; exchangeCount = 0
  for turn of turns:
    text = formatTurn(turn)
    tb = Buffer.byteLength(text, "utf8")        // BYTE budget, not .length
    if bytesUsed + tb > MAX_PRECOMPACT_BYTES: break
    bytesUsed += tb
    if turn.kind === "user": exchangeCount += 1
    parts.push(text)
  if parts.length === 0: return null
  return "=== Recent conversation (last " + exchangeCount + " exchanges) ===\n"
       + parts.join("\n")
       + "\n=== End recent conversation ==="

function formatTurn(turn):                      // transcript_block.rs:296-311
  user      -> "[User] " + text
  assistant -> "[Assistant] " + text
  tool      -> "[tool: " + name + "(" + keyParam + ") → " + resultSnippet + "]"
```

## Error Handling
- `extractTranscriptBlock` returns null on ANY failure (missing file, dir path, perms,
  read error) — wrapped end-to-end; never throws to index.js.
- Malformed JSONL lines, unknown record types, invalid-UTF-8 lines: skipped silently.

## Key Test Scenarios (corpus SubagentStart variants — R-01 scenario 2)
1. Window starting mid-line (partial first line fails JSON.parse → filtered).
2. Multi-byte char split at the 12,000-byte window edge (first line dropped/parse-fails;
   identical block to Rust golden).
3. Thinking-only assistant turns suppressed; tool_use/tool_result adjacent pairing;
   tool_result two content shapes (string + text-block array); missing tool_use_id.
4. Budget loop: turn that would exceed 3000 B excluded; zero-fitting-turns → null;
   exchange count counts user turns only.
5. Missing file / empty `transcript_path` / directory path → null → SubagentStart stays
   RecordEvent.
6. Key-param fallback: unknown tool → first string-valued input field; non-string
   `input` → "".
7. Invalid-UTF-8 line mid-window dropped (Rust lines() parity), neighbors still parsed.
