# Component: Surgical TOML Helper — `lib/codex-install.js` internal (`readTomlTable`, `upsertTomlTable`)

> ADR-002 (minimal in-house surgical block writer, NOT a round-trip library). First non-JSON writer for the installer. Byte-for-byte foreign preservation by construction; zero new dependency. Manages EXACTLY ONE owned table: `[mcp_servers.unimatrix]`.

## Purpose

Read or upsert a single owned TOML table as TEXT, copying every byte outside the owned region verbatim. Foreign `[mcp_servers.*]` tables, comments, blank lines, key order, quoting, and formatting are preserved because they are never parsed or re-serialized — only the owned region's body is replaced. Idempotent (body-equality → no change). Fail-safe (ambiguous boundary → caller skips, file preserved).

## Public (internal) signatures (Integration Surface — exact)

```
readTomlTable(raw: string, tablePath: string) -> { present: boolean, body: string }
upsertTomlTable(raw: string, tablePath: string, body: string[]) -> { text: string, changed: boolean }
```

`tablePath` is the dotted table name WITHOUT brackets, e.g. `"mcp_servers.unimatrix"`. `body` (upsert) is the array of key-line strings that belong under the header, e.g. `['command = "/abs/unimatrix"']` or `['command = "node"', 'args = ["/abs/mcp-bridge.js", "abc123"]']`.

## Region model

A table region is: the header line `[<tablePath>]` (allowing surrounding whitespace) through the byte just before the next **top-level table header** (a line whose first non-whitespace character is `[`) or EOF. The header's own line and everything up to (but not including) the next `[`-line is the region; the "body" is the region minus the header line.

Detection rules (be precise — these are the R-04 edge inventory):
- A line is a **table header** iff, after trimming leading whitespace, its first char is `[`. This includes `[table]` and `[[array.of.tables]]` and `[other.table]`.
- The OWNED header matches iff the bracket content equals `tablePath` exactly (after trimming inner whitespace is NOT allowed — codex writes canonical `[mcp_servers.unimatrix]`; match the exact bracketed string `[mcp_servers.unimatrix]` and also tolerate `[ mcp_servers.unimatrix ]`? — **No**: match the canonical form only; if a foreign file uses spaced/quoted dotted keys we do not own it, treat as absent and append our canonical table). Document this: we recognize our own canonical header; anything else is foreign.
- Distinguish `[mcp_servers.unimatrix]` from the parent `[mcp_servers]` and from a sibling `[mcp_servers.other]` — exact bracket-content equality prevents the off-by-one (a `[mcp_servers]` parent header is NOT our table).
- Lines INSIDE a multi-line basic string (`"""..."""`) or literal string (`'''...'''`) that happen to start with `[` must NOT be treated as headers. Track multi-line-string state while scanning (see below). This is the "table header inside a multi-line string" edge (ADR-002 Consequences).

## `readTomlTable(raw, tablePath)`

```
function readTomlTable(raw, tablePath):
  lines = splitKeepEndings(raw)         # preserve each line's original line ending (LF/CRLF)
  scan = scanTables(lines)              # -> array of { headerIndex, bracketContent, isArrayOfTables }
  if scan.error: return { present:false, body:"", malformed:true }   # ambiguous (see fail-safe)

  owned = scan.find(t => not t.isArrayOfTables AND t.bracketContent === tablePath)
  if owned is null:
    return { present:false, body:"" }

  regionStart = owned.headerIndex + 1
  regionEnd   = index of the next table header after owned.headerIndex, or lines.length
  bodyLines   = lines[regionStart .. regionEnd)
  return { present:true, body: join(bodyLines) }
```

## `upsertTomlTable(raw, tablePath, bodyLines)`

```
function upsertTomlTable(raw, tablePath, bodyLines):
  lines = splitKeepEndings(raw)
  scan  = scanTables(lines)
  if scan.error:
    return { text: raw, changed: false, malformed: true }   # ambiguous → caller skips (preserve file)

  nl = dominantLineEnding(raw)          # "\r\n" if the file is CRLF-majority, else "\n"
  desiredBody = bodyLines.map(l => l + nl)              # each key line + file's line ending
  header = "[" + tablePath + "]" + nl

  owned = scan.find(t => not t.isArrayOfTables AND t.bracketContent === tablePath)

  if owned exists:
    regionStart = owned.headerIndex + 1
    regionEnd   = next table header index after owned, or lines.length
    currentBody = lines[regionStart .. regionEnd)
    # IDEMPOTENCE (AC-04): compare the MEANINGFUL body, not raw whitespace noise.
    if bodyEquals(currentBody, desiredBody):
      return { text: raw, changed: false }
    newLines = lines[0 .. regionStart) + desiredBody + lines[regionEnd .. end)
    return { text: join(newLines), changed: true }
  else:
    # APPEND the owned table. Preserve every existing byte; add exactly ONE blank-line
    # separator iff the file is non-empty and does not already end in a blank line.
    prefix = raw
    sep = ""
    if raw.length > 0:
      if not raw.endsWith(nl): prefix = raw + nl                 # ensure the file ends with a newline first
      if not endsWithBlankLine(prefix): sep = nl                 # exactly one blank-line separator
    appended = prefix + sep + header + join(desiredBody)
    return { text: appended, changed: true }
```

### `scanTables(lines)` — the boundary scanner (multi-line-string aware)

```
function scanTables(lines):
  tables = []
  inTriple = null                       # null | '"""' | "'''"
  for i, line in enumerate(lines):
    content = stripLineEnding(line)
    # Track multi-line string state so a `[` inside one is not a header.
    # (Scan the line char-by-char for triple-quote toggles; a single-line `"""x"""`
    #  opens and closes on the same line.) Keep this minimal but correct — the goal
    #  is only to avoid FALSE headers, so err toward treating ambiguous lines as
    #  in-string ONLY when a triple quote is unbalanced.
    updateTripleState(content, inTriple)   # mutates inTriple
    if inTriple is not null: continue
    trimmed = content.trimStart()
    if trimmed.startsWith("[["):
      tables.push({ headerIndex:i, bracketContent: inner("[[", "]]", trimmed), isArrayOfTables:true })
    else if trimmed.startsWith("["):
      close = index of "]" in trimmed
      if close < 0:
        return { error: "unterminated table header" }     # ambiguous → fail-safe skip
      tables.push({ headerIndex:i, bracketContent: trimmed[1..close], isArrayOfTables:false })
  if inTriple is not null:
    return { error: "unterminated multi-line string" }     # ambiguous → fail-safe skip
  return tables
```

`inner("[[","]]",s)` extracts the content between the double brackets. `bracketContent` is compared with `===` to `tablePath` (exact — no normalization), so foreign spaced/quoted dotted keys are treated as not-ours (safe: we append our canonical table).

### Helpers

```
splitKeepEndings(raw):   split into lines each still carrying its own trailing "\n"/"\r\n"/"" (last line may have none)
dominantLineEnding(raw): count "\r\n" vs bare "\n"; return the majority ("\n" default when none)
bodyEquals(a, b):        compare after stripping each line's trailing whitespace + line ending, ignoring
                         trailing all-blank lines on both sides (so idempotence is robust to a preserved
                         blank line between our table and the next). Do NOT ignore key ORDER — our body
                         is fixed-order, so order stability is part of idempotence.
endsWithBlankLine(s):    s ends with two consecutive newlines (an empty line)
```

## Fail-safe posture (ADR-002, AC-15, NFR-03)

- A file the scanner cannot parse unambiguously (unterminated header, unterminated multi-line string) → `{ changed:false, malformed:true }`. The caller (`writeCodexMcpToml`) turns this into `action:"skipped-malformed"`, preserving the file. NEVER a partial write.
- Empty/absent file → `raw = ""`; the owned table is appended with no separator. (`writeCodexMcpToml` handles the absent-file read as `""`.)
- These helpers are pure string functions: NO fs access, NO throw on normal input (only structured `{malformed:true}`). This keeps them trivially unit-testable and keeps the fs/skip decision in the writer.

## Foreign preservation guarantee (SR-01, R-04, NFR-08)

Bytes outside `[regionStart, regionEnd)` are sliced from the original `lines` array and re-joined verbatim — never parsed, normalized, reordered, or re-quoted. Therefore foreign `[mcp_servers.*]` tables, interleaved comments (including a comment immediately above/below the owned table — the adjacency off-by-one is handled by the header-line boundary being the region edge), non-alphabetical key order, and CRLF/LF endings all survive by construction. This is the structural advantage over a round-trip library (Option A) that ADR-002 rejects.

## Key test scenarios (hints)

- Foreign tables + interleaved comments + non-alpha key order → upsert → byte-diff of every foreign region = empty; owned table present (AC-06, R-04).
- Comment immediately above and below `[mcp_servers.unimatrix]` → preserved on update (adjacency off-by-one, R-04 scenario 3).
- Update-in-place: existing owned table with a stale `command` value → body replaced, surrounding bytes byte-identical (R-04 scenario 4).
- Idempotence: `upsertTomlTable` with the same body → `changed:false`, `text === raw` (AC-04).
- `[mcp_servers]` parent present but no `[mcp_servers.unimatrix]` → treated as absent → append (not a false match).
- `[mcp_servers.other]` sibling present → not matched; owned appended; sibling preserved.
- CRLF file → owned table written with CRLF endings; foreign CRLF preserved.
- A `[` inside a `"""..."""` value → NOT treated as a header (no false region boundary).
- Unterminated header / multi-line string → `malformed:true`, file preserved (AC-15).
- Empty file → owned table is the entire content, no leading blank line.
