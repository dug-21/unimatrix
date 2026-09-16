"use strict";

/**
 * Codex installer — `lib/codex-install.js` (nan-023, C14 codex-cli parity).
 *
 * WAVE A (this file, ADR-002 / toml-surgical.md): the minimal, dependency-free
 * SURGICAL TOML block writer. It manages EXACTLY ONE owned table
 * (`[mcp_servers.unimatrix]`) as text and copies every byte outside the owned
 * region verbatim. Foreign `[mcp_servers.*]` tables, comments, blank lines, key
 * order, quoting and CRLF/LF endings survive BY CONSTRUCTION — they are never
 * parsed, normalized, reordered or re-quoted. This is the structural advantage
 * over a round-trip TOML library that ADR-002 rejects. No new dependency.
 *
 * These are PURE string functions: no fs access, no throw on normal input. An
 * ambiguous file (unterminated header / multi-line string) is signalled with a
 * structured `{ malformed: true }` result — never an exception, never a partial
 * write. The fs/skip decision lives in the writers (Wave B), which turn a
 * malformed signal into a `skipped-malformed` WireLeg with the file preserved.
 *
 * WAVE B (added later in this same file — DO NOT remove the Wave A exports):
 *   maybeWireCodex, writeCodexMcpToml, writeCodexHooks — the fs-touching writers
 *   that return WireLegs. `buildTomlBody` (Wave B) MUST build every string value
 *   through `tomlString` below (R-10 command-injection guard) — never naive
 *   concatenation of a raw path into `command = ...`.
 */

const OWNED_TABLE = "mcp_servers.unimatrix";

// ---------------------------------------------------------------------------
// Line / ending primitives — preserve the file's exact bytes.
// ---------------------------------------------------------------------------

/**
 * Split `raw` into lines, each still carrying its own trailing "\n"/"\r\n" (the
 * final line may carry none). `splitKeepEndings(raw).join("") === raw` for all
 * inputs, including "" -> []. This is what makes verbatim re-join possible.
 */
function splitKeepEndings(raw) {
  const lines = [];
  let start = 0;
  for (let i = 0; i < raw.length; i++) {
    if (raw[i] === "\n") {
      lines.push(raw.slice(start, i + 1));
      start = i + 1;
    }
  }
  if (start < raw.length) lines.push(raw.slice(start));
  return lines;
}

/** Strip a single trailing "\r\n" or "\n" from a line (leaves inner bytes). */
function stripLineEnding(line) {
  return line.replace(/\r?\n$/, "");
}

/** Dominant line ending: "\r\n" if the file is CRLF-majority, else "\n". */
function dominantLineEnding(raw) {
  const crlf = (raw.match(/\r\n/g) || []).length;
  const lf = (raw.match(/\n/g) || []).length - crlf; // bare-LF count
  return crlf > lf ? "\r\n" : "\n";
}

/** True iff `s` ends with a blank line (two consecutive newlines at the tail). */
function endsWithBlankLine(s) {
  return /(?:\r?\n)[ \t]*(?:\r?\n)$/.test(s);
}

// ---------------------------------------------------------------------------
// Boundary scanner (multi-line-string aware).
// ---------------------------------------------------------------------------

/**
 * Advance triple-quoted-string state across one line's content (ending already
 * stripped). Returns the new state: null | '"""' | "'''". Minimal but correct:
 * the only job is to avoid treating a `[` that lives inside a multi-line string
 * as a table header. Outside a string, an unescaped `#` starts a comment to
 * end-of-line, so any triple-quote-looking bytes after it are ignored (TOML).
 */
function updateTripleState(content, state) {
  let inTriple = state;
  let i = 0;
  while (i < content.length) {
    if (inTriple === null) {
      if (content.startsWith('"""', i)) { inTriple = '"""'; i += 3; continue; }
      if (content.startsWith("'''", i)) { inTriple = "'''"; i += 3; continue; }
      if (content[i] === "#") break; // comment to EOL — nothing after it opens a string
      i += 1;
    } else {
      if (content.startsWith(inTriple, i)) { inTriple = null; i += 3; continue; }
      i += 1;
    }
  }
  return inTriple;
}

/**
 * Scan `lines` for TOML table headers. A line is a header iff — when it does NOT
 * begin inside a multi-line string — its first non-whitespace char is `[`. Both
 * `[table]` and `[[array.of.tables]]` bound a region. `bracketContent` is the
 * exact inner string (NO normalization): the owned table is matched with `===`,
 * so spaced/quoted foreign dotted keys are treated as not-ours (safe: we append
 * our canonical table). Returns `{ tables }` or `{ error }` when the file cannot
 * be parsed unambiguously (fail-safe → caller skips, file preserved).
 */
function scanTables(lines) {
  const tables = [];
  let inTriple = null;
  for (let i = 0; i < lines.length; i++) {
    const content = stripLineEnding(lines[i]);
    const beganInString = inTriple !== null;
    inTriple = updateTripleState(content, inTriple);
    if (beganInString) continue; // a `[` inside a multi-line string is not a header
    const trimmed = content.replace(/^[ \t]+/, "");
    if (trimmed.startsWith("[[")) {
      const close = trimmed.indexOf("]]");
      if (close < 0) return { error: "unterminated array-of-tables header" };
      tables.push({ headerIndex: i, bracketContent: trimmed.slice(2, close), isArrayOfTables: true });
    } else if (trimmed.startsWith("[")) {
      const close = trimmed.indexOf("]");
      if (close < 0) return { error: "unterminated table header" };
      tables.push({ headerIndex: i, bracketContent: trimmed.slice(1, close), isArrayOfTables: false });
    }
  }
  if (inTriple !== null) return { error: "unterminated multi-line string" };
  return { tables };
}

/** Index into `tables` of the owned (non-array) table, or -1 if absent. */
function findOwned(tables, tablePath) {
  for (let i = 0; i < tables.length; i++) {
    if (!tables[i].isArrayOfTables && tables[i].bracketContent === tablePath) return i;
  }
  return -1;
}

/**
 * Region for the owned table at `tables[pos]`: [start, end) line indices. The
 * region body is every line after the header up to (but not including) the next
 * table header of ANY kind, or EOF.
 */
function ownedRegion(tables, pos, lineCount) {
  const start = tables[pos].headerIndex + 1;
  const end = pos + 1 < tables.length ? tables[pos + 1].headerIndex : lineCount;
  return { start, end };
}

// ---------------------------------------------------------------------------
// Body equality (idempotence) — meaningful content, not whitespace noise.
// ---------------------------------------------------------------------------

/** Normalize a body: strip each line's trailing whitespace/ending, drop trailing blanks. */
function normalizeBody(lines) {
  const arr = lines.map((l) => l.replace(/\s+$/, ""));
  while (arr.length && arr[arr.length - 1] === "") arr.pop();
  return arr;
}

/**
 * Compare two bodies after normalizing trailing whitespace/endings and ignoring
 * trailing all-blank lines on both sides. Key ORDER is significant (our body is
 * fixed-order, so order stability is part of idempotence).
 */
function bodyEquals(a, b) {
  const na = normalizeBody(a);
  const nb = normalizeBody(b);
  if (na.length !== nb.length) return false;
  for (let i = 0; i < na.length; i++) if (na[i] !== nb[i]) return false;
  return true;
}

// ---------------------------------------------------------------------------
// Public (internal) surgical helpers — ADR-002 integration surface.
// ---------------------------------------------------------------------------

/**
 * Read the owned table's body as text, copying nothing else.
 * @param {string} raw       full file contents ("" for a missing/empty file)
 * @param {string} tablePath dotted table name without brackets, e.g. "mcp_servers.unimatrix"
 * @returns {{present: boolean, body: string, malformed?: boolean}}
 */
function readTomlTable(raw, tablePath) {
  const lines = splitKeepEndings(raw);
  const scan = scanTables(lines);
  if (scan.error) return { present: false, body: "", malformed: true };

  const pos = findOwned(scan.tables, tablePath);
  if (pos < 0) return { present: false, body: "" };

  const { start, end } = ownedRegion(scan.tables, pos, lines.length);
  return { present: true, body: lines.slice(start, end).join("") };
}

/**
 * Upsert the owned table. Present → replace ONLY its body (idempotent when the
 * desired body already matches). Absent → append the table, preceded by exactly
 * one blank-line separator iff the file is non-empty and does not already end in
 * a blank line. Every byte outside the owned region is preserved verbatim.
 * @param {string}   raw        full file contents ("" for missing/empty)
 * @param {string}   tablePath  dotted table name without brackets
 * @param {string[]} bodyLines  key-line strings for under the header (no endings)
 * @returns {{text: string, changed: boolean, malformed?: boolean}}
 */
function upsertTomlTable(raw, tablePath, bodyLines) {
  const lines = splitKeepEndings(raw);
  const scan = scanTables(lines);
  if (scan.error) return { text: raw, changed: false, malformed: true };

  const nl = dominantLineEnding(raw);
  const desiredBody = bodyLines.map((l) => l + nl);
  const header = "[" + tablePath + "]" + nl;

  const pos = findOwned(scan.tables, tablePath);
  if (pos >= 0) {
    const { start, end } = ownedRegion(scan.tables, pos, lines.length);
    const currentBody = lines.slice(start, end);
    if (bodyEquals(currentBody, desiredBody)) return { text: raw, changed: false };
    const newLines = lines.slice(0, start).concat(desiredBody, lines.slice(end));
    return { text: newLines.join(""), changed: true };
  }

  // Append the owned table, preserving every existing byte.
  let prefix = raw;
  let sep = "";
  if (raw.length > 0) {
    if (!prefix.endsWith(nl)) prefix += nl; // ensure the file ends with a newline first
    if (!endsWithBlankLine(prefix)) sep = nl; // exactly one blank-line separator
  }
  return { text: prefix + sep + header + desiredBody.join(""), changed: true };
}

/**
 * Emit a TOML BASIC string with proper escaping — the security-critical guard
 * (R-10). A path/value containing spaces, quotes, backslashes or control chars
 * produces a valid TOML string that re-parses to the exact intended value.
 * Wave B's `buildTomlBody` MUST route every emitted value through this; never
 * naive-concatenate a raw path into `command = ...`.
 * @param {string} s
 * @returns {string} the value wrapped in double quotes, escaped per TOML spec
 */
function tomlString(s) {
  let out = '"';
  for (const ch of String(s)) {
    const code = ch.codePointAt(0);
    if (ch === "\\") out += "\\\\";
    else if (ch === '"') out += '\\"';
    else if (ch === "\b") out += "\\b";
    else if (ch === "\t") out += "\\t";
    else if (ch === "\n") out += "\\n";
    else if (ch === "\f") out += "\\f";
    else if (ch === "\r") out += "\\r";
    else if (code < 0x20 || code === 0x7f) out += "\\u" + code.toString(16).padStart(4, "0");
    else out += ch;
  }
  return out + '"';
}

module.exports = {
  // Wave A — surgical TOML helpers (ADR-002, toml-surgical.md). DO NOT remove.
  upsertTomlTable,
  readTomlTable,
  tomlString,
  OWNED_TABLE,
  // ---- Wave B export seam ----------------------------------------------------
  // Wave B (codex-install.md) adds the fs-touching writers below and to this
  // object WITHOUT altering the Wave A helpers above:
  //   maybeWireCodex, writeCodexMcpToml, writeCodexHooks
  // buildTomlBody (Wave B) must build values via `tomlString` (R-10).
};
