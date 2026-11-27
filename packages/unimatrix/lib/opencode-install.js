"use strict";

const fs = require("fs");
const path = require("path");
const { isUnimatrixMcpEntry, isLegacyOpencodePlugin } = require("./hook-ownership.js");
const { buildMcpArgv } = require("./mcp-entry.js");
const { updateFileSync } = require("./safe-write.js");

/**
 * OpenCode MCP wiring (nan-024 ADR-006, 1036 C10). Writes `mcp.unimatrix` to the
 * resolved user opencode.json (caller passes it from config-dirs), removes an
 * owned twin at the legacy ~/.config/opencode location, and removes an owned
 * project entry plus the legacy plugin (G3) in one mutate. Malformed user files
 * skip with a warning; a malformed project file throws (D-029); SafeWriteError
 * propagates.
 */

/**
 * Detect OpenCode by the presence of `opencode.json` or `.opencode/`
 * (ADR-006 §1). Fail-safe: any stat error is treated as "not present".
 *
 * @param {string} dir - Absolute project root.
 * @returns {boolean} true iff OpenCode markers are present.
 */
function detectsOpenCode(dir) {
  return exists(path.join(dir, "opencode.json")) || exists(path.join(dir, ".opencode"));
}

/**
 * Best-effort existence check (never throws).
 * @param {string} p
 * @returns {boolean}
 */
function exists(p) {
  try {
    return fs.existsSync(p);
  } catch (_err) {
    return false;
  }
}

/**
 * Detect the indentation of an existing JSON document so re-serialization does
 * not reformat preserved stanzas (ADR-006: mutate minimally). Defaults to two
 * spaces — the formatting the rest of init writes.
 *
 * @param {string} raw - Raw file contents.
 * @returns {number|string} Indent arg for JSON.stringify.
 */
function detectIndent(raw) {
  const m = /\n([ \t]+)"/.exec(raw);
  if (!m) {
    return 2;
  }
  return m[1].indexOf("\t") >= 0 ? "\t" : m[1].length;
}

/**
 * Parse raw JSON text WITHOUT throwing. null = absent; "" = empty object;
 * non-object or invalid = malformed.
 *
 * @param {string|null} raw
 * @returns {{ present:boolean, parsed:(object|null), raw:string, malformed:boolean }}
 */
function parseJsonRaw(raw) {
  if (raw === null || raw === undefined) return { present: false, parsed: null, raw: "", malformed: false };
  const trimmed = raw.trim();
  if (trimmed === "") return { present: true, parsed: {}, raw, malformed: false };
  try {
    const parsed = JSON.parse(trimmed);
    if (!isPlainObject(parsed)) return { present: true, parsed: null, raw, malformed: true };
    return { present: true, parsed, raw, malformed: false };
  } catch (_err) {
    return { present: true, parsed: null, raw, malformed: true };
  }
}

/** Read + parse a JSON file WITHOUT throwing (read-only callers). */
function readJsonSafe(filePath) {
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch (_err) {
    raw = null;
  }
  return parseJsonRaw(raw);
}

/**
 * Strict plain-object test (not null, not array). Used to reject a user-set
 * value we must not clobber (a scalar/array where an object is expected).
 *
 * @param {*} v
 * @returns {boolean}
 */
function isPlainObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

/**
 * Order-insensitive structural equality (idempotence check — AC-04). Small,
 * dependency-free; only the shapes this module emits (JSON-safe objects,
 * arrays, scalars) are ever compared.
 *
 * @param {*} a
 * @param {*} b
 * @returns {boolean}
 */
function deepEqual(a, b) {
  if (a === b) {
    return true;
  }
  if (typeof a !== typeof b || a === null || b === null || typeof a !== "object") {
    return false;
  }
  const aArr = Array.isArray(a);
  if (aArr !== Array.isArray(b)) {
    return false;
  }
  if (aArr) {
    if (a.length !== b.length) {
      return false;
    }
    for (let i = 0; i < a.length; i += 1) {
      if (!deepEqual(a[i], b[i])) {
        return false;
      }
    }
    return true;
  }
  const ak = Object.keys(a);
  const bk = Object.keys(b);
  if (ak.length !== bk.length) {
    return false;
  }
  for (const k of ak) {
    if (!Object.prototype.hasOwnProperty.call(b, k) || !deepEqual(a[k], b[k])) {
      return false;
    }
  }
  return true;
}

/**
 * Construct a well-formed opencode retrieval `WireLeg` (ADR-001 §3). `entry` is
 * attached only for a real write action; `reason` only for a skip.
 *
 * @param {string} action
 * @param {string} filePath
 * @param {object} [extra] - { entry?, reason? }
 * @returns {object} WireLeg
 */
function opencodeLeg(action, filePath, extra) {
  const legObj = { harness: "opencode", surface: "retrieval", action: action, path: filePath };
  if (extra && extra.entry !== undefined) {
    legObj.entry = extra.entry;
  }
  if (extra && extra.reason !== undefined) {
    legObj.reason = extra.reason;
  }
  if (extra && extra.warning !== undefined) {
    legObj.warning = extra.warning;
  }
  return legObj;
}

/** Per-machine MCP entry: argv form from the shared builder (D-040, D-072). */
function buildOpencodeEntry(o) {
  return { type: "local", command: buildMcpArgv(o.platform, o.home, o.node) };
}

const MALFORMED_REASON = "opencode.json is not valid JSON — preserved unchanged";
const foreignWarning = (file) =>
  "unimatrix: " + file + ' has a non-Unimatrix mcp entry named "unimatrix"; left unchanged';
const ser = (obj, raw) => JSON.stringify(obj, null, raw ? detectIndent(raw) : 2) + "\n";
const pre = (dryRun, msg) => (dryRun ? "[dry-run] " + msg : msg);

/** Pure W5 decision over the user file's raw text. */
function computeUserMcp(raw, o) {
  const read = parseJsonRaw(raw);
  const skip = (action, reason, warning) => ({ text: null, detail: { skip: { action, reason, warning } } });
  const badWarn = "unimatrix: " + o.file + " is not valid JSON; OpenCode left without Unimatrix wiring until it is fixed";
  if (read.malformed) return skip("skipped-malformed", MALFORMED_REASON, badWarn);
  const cfg = read.parsed || {};
  if (cfg.mcp !== undefined && !isPlainObject(cfg.mcp)) {
    return skip("skipped-malformed", "`mcp` key is not an object — preserved unchanged", badWarn);
  }
  const existing = cfg.mcp ? cfg.mcp.unimatrix : undefined;
  if (existing !== undefined && !isUnimatrixMcpEntry("unimatrix", existing)) {
    return skip("skipped-foreign", "mcp.unimatrix holds a non-Unimatrix entry; left unchanged",
      "unimatrix: " + o.file + " key mcp.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written");
  }
  if (existing === undefined && o.harnessSelected !== true && !o.priorOptIn) {
    return skip("skipped-intent", "new mcp.unimatrix entry requires explicit --harness opencode opt-in");
  }
  const entry = buildOpencodeEntry(o);
  if (existing !== undefined && deepEqual(existing, entry)) return { text: null, detail: { entry, entryAction: "unchanged" } };
  cfg.mcp = cfg.mcp || {};
  cfg.mcp.unimatrix = entry;
  return { text: ser(cfg, read.present ? raw : ""), detail: { entry, entryAction: existing !== undefined ? "updated" : "created" } };
}

/** Read-only: does `file` hold an owned mcp.unimatrix? Never throws. */
function ownedAt(file) {
  const r = readJsonSafe(file);
  return !!(r.parsed && isPlainObject(r.parsed.mcp) && isUnimatrixMcpEntry("unimatrix", r.parsed.mcp.unimatrix));
}

/**
 * W5. o: {file, legacyFile?, platform, home, node, harnessSelected, dryRun}.
 * `file` is the resolved user opencode.json (config-dirs); an owned entry at
 * `legacyFile` counts as prior opt-in (D-015). SafeWriteError propagates.
 */
function writeOpencodeMcp(o) {
  const opts = Object.assign({}, o, { priorOptIn: o.legacyFile ? ownedAt(o.legacyFile) : false });
  const res = updateFileSync(opts.file, (raw) => computeUserMcp(raw, opts).text,
    { dryRun: !!opts.dryRun, newMode: 0o600, platform: opts.platform });
  const d = computeUserMcp(res.before, opts).detail;
  if (d.skip) return opencodeLeg(d.skip.action, opts.file, { reason: d.skip.reason, warning: d.skip.warning });
  return opencodeLeg(d.entryAction, opts.file, { entry: d.entry });
}

/** Pure R10 decision over the legacy user file's raw text. */
function computeLegacyRemoval(raw, file) {
  const read = parseJsonRaw(raw);
  if (!read.present) return { text: null, detail: {} };
  if (read.malformed) {
    return { text: null, detail: { skip: { reason: MALFORMED_REASON,
      warning: "unimatrix: " + file + " is not valid JSON; owned entry (if any) not removed" } } };
  }
  const mcp = read.parsed.mcp;
  if (!isPlainObject(mcp) || mcp.unimatrix === undefined) return { text: null, detail: {} };
  if (!isUnimatrixMcpEntry("unimatrix", mcp.unimatrix)) return { text: null, detail: { warning: foreignWarning(file) } };
  delete mcp.unimatrix;
  return { text: ser(read.parsed, raw), detail: { changed: true } };
}

/** R10 step body: remove an owned entry from the legacy ~/.config/opencode file. */
function removeOwnedOpencodeUserMcpLeg(file, dryRun) {
  const res = updateFileSync(file, (raw) => computeLegacyRemoval(raw, file).text, { dryRun: !!dryRun });
  const d = computeLegacyRemoval(res.before, file).detail;
  if (d.skip) return [opencodeLeg("skipped-malformed", file, d.skip)];
  if (d.changed) return [opencodeLeg("removed", file, { reason: "owned MCP entry removed (legacy location)" })];
  return Object.assign([], { warnings: d.warning ? [d.warning] : [] });
}

/** Pure R11+R12 decision over the project file's raw text. Malformed throws (D-029). */
function computeProjectRemoval(raw, file) {
  const detail = { mcpRemoved: false, removedPlugin: false, warnings: [] };
  const read = parseJsonRaw(raw);
  if (!read.present) return { text: null, detail };
  if (read.malformed) throw new Error(file + " is not valid JSON; fix it manually (not modified)");
  const p = read.parsed;
  if (isPlainObject(p.mcp) && p.mcp.unimatrix !== undefined) {
    if (isUnimatrixMcpEntry("unimatrix", p.mcp.unimatrix)) {
      delete p.mcp.unimatrix;
      detail.mcpRemoved = true;
    } else {
      detail.warnings.push(foreignWarning(file));
    }
  }
  if (Array.isArray(p.plugin)) {
    const kept = p.plugin.filter((el) => !isLegacyOpencodePlugin(el));
    if (kept.length !== p.plugin.length) {
      p.plugin = kept;
      detail.removedPlugin = true;
    }
  }
  return { text: detail.mcpRemoved || detail.removedPlugin ? ser(p, raw) : null, detail };
}

/** Remove owned mcp.unimatrix and legacy plugin[] entries from the project opencode.json (D-064, G3). */
function removeOwnedOpencodeProjectMcp(projectRoot, dryRun) {
  const file = path.join(projectRoot, "opencode.json");
  const res = updateFileSync(file, (raw) => computeProjectRemoval(raw, file).text,
    { dryRun: !!dryRun, containWithin: projectRoot });
  const d = computeProjectRemoval(res.before, file).detail;
  const actions = [];
  if (d.mcpRemoved) actions.push(pre(dryRun, "Removed Unimatrix MCP entry from opencode.json"));
  if (d.removedPlugin) actions.push(pre(dryRun, "Removed legacy Unimatrix plugin entry from opencode.json"));
  return { actions, warnings: d.warnings, changed: d.mcpRemoved, removedPlugin: d.removedPlugin };
}

/** R11+R12 step body. */
function removeOwnedOpencodeProjectLegs(projectRoot, dryRun) {
  const r = removeOwnedOpencodeProjectMcp(projectRoot, dryRun);
  const f = path.join(projectRoot, "opencode.json");
  const legs = [];
  if (r.changed) legs.push({ harness: "opencode", surface: "mcp", action: "removed", path: f, reason: "owned project MCP entry removed" });
  if (r.removedPlugin) legs.push({ harness: "opencode", surface: "mcp", action: "removed", path: f, reason: "legacy Unimatrix plugin entry removed" });
  return Object.assign(legs, { warnings: r.warnings });
}

module.exports = {
  detectsOpenCode,
  writeOpencodeMcp,
  removeOwnedOpencodeProjectMcp,
  removeOwnedOpencodeProjectLegs,
  removeOwnedOpencodeUserMcpLeg,
  computeUserMcp,
  computeLegacyRemoval,
  computeProjectRemoval,
  buildOpencodeEntry,
  opencodeLeg,
  detectIndent,
  readJsonSafe,
  parseJsonRaw,
};
