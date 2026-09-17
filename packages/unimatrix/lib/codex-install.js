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

const fs = require("fs");
const path = require("path");
// Wave B reuse (do not reinvent — codex-install.md §Module constants / reuse):
const { isWithinProject, readJsonSafe, detectIndent } = require("./opencode-install.js");
const {
  buildHookClientCommand,
  isUnimatrixHook,
  EVENT_MATCHERS,
} = require("./merge-settings.js");

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

// ===========================================================================
// WAVE B — fs-touching writers (codex-install.md; ADR-001/003/006).
//
// Bring codex-cli to C14 parity: write `[mcp_servers.unimatrix]` into
// `.codex/config.toml` (surgical, foreign-preserving) and Claude-like hooks into
// `.codex/hooks.json` targeting the JS hook client with `--provider codex-cli` on
// EVERY command. Fail-safe: warn-and-skip, NEVER throw out of `maybeWireCodex`
// (AC-15). Every TOML value routes through the Wave A `tomlString` escaper (R-10).
// ===========================================================================

const CONFIG_FILE = ".codex/config.toml";
const HOOKS_FILE = ".codex/hooks.json";
const CODEX_HARNESS = "codex-cli";
const CODEX_PROVIDER = "codex-cli"; // mandatory hook hint (C-04, NFR-07)
const TOML_MALFORMED_REASON =
  "config.toml has an ambiguous table boundary — preserved unchanged";

// ADR-003 §3 / Q4 — the 7 emitted events. Excludes PostToolUseFailure
// (Claude-specific arm) and SubagentStop (opt-in). Matchers reused from
// EVENT_MATCHERS so codex and claude stay in step (PreToolUse = cycle matcher).
const CODEX_EVENTS = [
  "SessionStart",
  "UserPromptSubmit",
  "PreToolUse",
  "PostToolUse",
  "PreCompact",
  "SubagentStart",
  "Stop",
];

/** Best-effort text read; a missing/unreadable file is treated as "" (fail-safe). */
function readTextSafe(filePath) {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch (_err) {
    return "";
  }
}

/**
 * Surfaced trust precondition (ADR-006 §3, NFR-09). `.codex/` config + hooks
 * only load for a TRUSTED layer; wiring cannot make it trusted, so we state the
 * conditionality on every codex leg — never a silent inert pass.
 */
function codexTrustNote() {
  return "codex wiring is active only in a trusted .codex/ layer (mark .codex/ trusted in Codex)";
}

/** Structured WireLeg builder; `extra` carries command/entry/reason/note. */
function leg(surface, action, legPath, extra) {
  return Object.assign(
    { harness: CODEX_HARNESS, surface: surface, action: action, path: legPath },
    extra || {}
  );
}

/**
 * Resolve the emit shape. Q1: cloud is the token-free stdio bridge; a bearer
 * token / `url =` is NEVER written to TOML (Principle 8, R-03). Prefers an
 * explicit `transport` descriptor from the orchestrator, then `binaryPath`
 * (local), then `bridgePath` + `projectHash` (cloud). `url` is intentionally
 * NOT mapped to a `url =` entry — the orchestrator resolves cloud to a bridge.
 * @returns {{kind:string, binaryPath?:string, bridgePath?:string, projectHash?:string}}
 */
function resolveCodexTransport(opts) {
  const o = opts || {};
  if (o.transport && typeof o.transport.kind === "string") {
    return o.transport;
  }
  if (typeof o.binaryPath === "string" && o.binaryPath) {
    return { kind: "stdio-binary", binaryPath: o.binaryPath };
  }
  if (
    typeof o.bridgePath === "string" &&
    o.bridgePath &&
    typeof o.projectHash === "string" &&
    o.projectHash
  ) {
    return { kind: "stdio-bridge", bridgePath: o.bridgePath, projectHash: o.projectHash };
  }
  return { kind: "none" };
}

/**
 * Owned-table key lines (TOML-escaped via `tomlString`, R-10) plus a structured
 * `entry` mirror for the WireLeg (the C14 verifier spawns/connects from it).
 * Returns null for `kind === "none"` (caller emits a skipped leg).
 */
function buildTomlEmit(transport) {
  if (transport.kind === "stdio-binary") {
    return {
      bodyLines: ["command = " + tomlString(transport.binaryPath)],
      entry: { command: transport.binaryPath },
    };
  }
  if (transport.kind === "stdio-bridge") {
    return {
      bodyLines: [
        'command = "node"',
        "args = [" +
          tomlString(transport.bridgePath) +
          ", " +
          tomlString(transport.projectHash) +
          "]",
      ],
      entry: { command: "node", args: [transport.bridgePath, transport.projectHash] },
    };
  }
  return null;
}

/**
 * Surgical TOML MCP writer. Self-gates the intent decision (AC-11) and never
 * throws on malformed input — the file is byte-preserved and the leg is skipped
 * (AC-15). Malformed is checked BEFORE the intent gate.
 * @returns {object} a single WireLeg
 */
function writeCodexMcpToml(dir, opts, dryRun) {
  const options = opts || {};
  const cfgPath = path.join(dir, CONFIG_FILE);
  const S = "mcp";

  if (!isWithinProject(dir, cfgPath)) {
    return leg(S, "skipped-undetected", cfgPath, { reason: "path escapes project root" }); // AC-12
  }

  const raw = readTextSafe(cfgPath);
  const existing = readTomlTable(raw, OWNED_TABLE);
  if (existing.malformed) {
    return leg(S, "skipped-malformed", cfgPath, { reason: TOML_MALFORMED_REASON });
  }

  // Intent gate (self-gating): a NEW entry into a user-owned config needs opt-in.
  if (!existing.present && options.harnessSel !== CODEX_HARNESS) {
    return leg(S, "skipped-intent", cfgPath, {
      reason: "new codex entry needs opt-in: run `unimatrix wire --harness codex-cli`",
    }); // AC-11
  }

  const emit = buildTomlEmit(options.transport || { kind: "none" });
  if (!emit) {
    return leg(S, "skipped-undetected", cfgPath, {
      reason: "no transport source (binaryPath / bridge absent) — nothing to wire",
    });
  }

  const result = upsertTomlTable(raw, OWNED_TABLE, emit.bodyLines);
  if (result.malformed) {
    return leg(S, "skipped-malformed", cfgPath, { reason: TOML_MALFORMED_REASON });
  }
  if (!result.changed) {
    return leg(S, "unchanged", cfgPath, { entry: emit.entry }); // idempotent — no write (AC-04)
  }

  const action = existing.present ? "updated" : "created";
  if (!dryRun) {
    fs.mkdirSync(path.dirname(cfgPath), { recursive: true });
    fs.writeFileSync(cfgPath, result.text, "utf8");
  }
  return leg(S, action, cfgPath, { entry: emit.entry });
}

/**
 * Fail-loud invariant (NFR-07, C-04): a codex hook command MUST carry
 * `--provider codex-cli`. A missing flag is a defect, not a silent degrade.
 */
function requireProviderFlag(command) {
  if (typeof command !== "string" || !command.includes("--provider " + CODEX_PROVIDER)) {
    throw new Error(
      "codex hook command missing --provider codex-cli (fail-loud, NFR-07/C-04)"
    );
  }
  return command;
}

/**
 * Non-clobbering matcher-group upsert for one event, scoped to Unimatrix-owned
 * entries (isUnimatrixHook). Mirrors mergeSettings Step 3: find the matcher
 * group, update/append the uni-owned hook, dedup extra uni hooks. Foreign hook
 * entries and foreign matcher groups are NEVER touched. Mutates `hooksObj`;
 * returns true iff it changed.
 */
function upsertHookEntry(hooksObj, event, matcher, command) {
  const newEntry = { type: "command", command: command };
  if (!Array.isArray(hooksObj[event])) {
    hooksObj[event] = [];
  }
  const groups = hooksObj[event];
  for (const group of groups) {
    if (!group || group.matcher !== matcher) {
      continue;
    }
    if (!Array.isArray(group.hooks)) {
      group.hooks = [];
    }
    let idx = -1;
    const dups = [];
    for (let i = 0; i < group.hooks.length; i++) {
      if (isUnimatrixHook(group.hooks[i])) {
        if (idx === -1) {
          idx = i;
        } else {
          dups.push(i);
        }
      }
    }
    let changed = false;
    for (let j = dups.length - 1; j >= 0; j--) {
      group.hooks.splice(dups[j], 1);
      changed = true; // dedup on re-run
    }
    if (idx >= 0) {
      const cur = group.hooks[idx];
      if (cur.type !== "command" || cur.command !== command) {
        group.hooks[idx] = newEntry;
        changed = true;
      }
    } else {
      group.hooks.push(newEntry);
      changed = true;
    }
    return changed;
  }
  groups.push({ matcher: matcher, hooks: [newEntry] });
  return true;
}

/**
 * Write `.codex/hooks.json` (same matcher-group shape as `.claude/settings.json`)
 * targeting the JS hook client — NEVER the unimatrix binary (C-03, AC-08) — with
 * `--provider codex-cli` on every command (C-04, AC-07). Returns ONE WireLeg per
 * event so the manifest carries every exact command string (SR-09, R-02).
 * @returns {object[]} WireLeg[]
 */
function writeCodexHooks(dir, opts) {
  const options = opts || {};
  const hooksPath = path.join(dir, HOOKS_FILE);
  const S = "hooks";

  if (!isWithinProject(dir, hooksPath)) {
    return [leg(S, "skipped-undetected", hooksPath, { reason: "path escapes project root" })]; // AC-12
  }

  const read = readJsonSafe(hooksPath);
  if (read.malformed) {
    return [
      leg(S, "skipped-malformed", hooksPath, {
        reason: ".codex/hooks.json is not valid JSON — preserved unchanged",
      }),
    ];
  }
  const content = read.parsed || {};
  if (
    content.hooks !== undefined &&
    (typeof content.hooks !== "object" || Array.isArray(content.hooks))
  ) {
    return [
      leg(S, "skipped-malformed", hooksPath, {
        reason: "`hooks` key is not an object — preserved unchanged",
      }),
    ];
  }
  if (content.hooks === undefined) {
    content.hooks = {};
  }

  let changed = false;
  const commands = [];
  for (const event of CODEX_EVENTS) {
    const matcher = EVENT_MATCHERS[event];
    const command = requireProviderFlag(
      buildHookClientCommand(options.clientPath, event, CODEX_PROVIDER)
    );
    commands.push({ event: event, command: command });
    if (upsertHookEntry(content.hooks, event, matcher, command)) {
      changed = true;
    }
  }

  let action = read.present ? "updated" : "created";
  if (!changed && read.present) {
    action = "unchanged";
  }

  if (!options.dryRun && action !== "unchanged") {
    fs.mkdirSync(path.dirname(hooksPath), { recursive: true });
    fs.writeFileSync(
      hooksPath,
      JSON.stringify(content, null, detectIndent(read.raw || "")) + "\n",
      "utf8"
    );
  }

  // One WireLeg per event — each carries its own exact command (manifest fidelity).
  return commands.map((c) => leg(S, action, hooksPath, { command: c.command }));
}

/**
 * Top-level codex fan-out: MCP TOML + hooks. Fail-safe — never throws out of the
 * wire layer (AC-15); an unexpected throw becomes a skipped leg. The trust
 * precondition is surfaced on every codex leg (NFR-09), never a silent no-op.
 * @returns {object[]} WireLeg[]
 */
function maybeWireCodex(dir, opts) {
  const options = opts || {};
  const transport = resolveCodexTransport(options);
  const legs = [];
  try {
    legs.push(
      writeCodexMcpToml(
        dir,
        { transport: transport, harnessSel: options.harnessSel },
        options.dryRun
      )
    );
    const hookLegs = writeCodexHooks(dir, {
      clientPath: options.clientPath,
      dryRun: options.dryRun,
    });
    for (const l of hookLegs) {
      legs.push(l);
    }
  } catch (e) {
    legs.push(
      leg("mcp", "skipped-malformed", path.join(dir, CONFIG_FILE), {
        reason: "codex wiring error: " + e.message,
      })
    );
  }
  const note = codexTrustNote();
  for (const l of legs) {
    l.note = note;
  }
  return legs;
}

module.exports = {
  // Wave A — surgical TOML helpers (ADR-002, toml-surgical.md). DO NOT remove.
  upsertTomlTable,
  readTomlTable,
  tomlString,
  OWNED_TABLE,
  // Wave B — fs-touching writers (codex-install.md, ADR-001/003/006).
  maybeWireCodex,
  writeCodexMcpToml,
  writeCodexHooks,
  resolveCodexTransport,
  requireProviderFlag,
  codexTrustNote,
  CODEX_EVENTS,
  CODEX_PROVIDER,
  CODEX_HARNESS,
  CONFIG_FILE,
  HOOKS_FILE,
};
