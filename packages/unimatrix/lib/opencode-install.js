"use strict";

const fs = require("fs");
const path = require("path");
const { isUnimatrixMcpEntry } = require("./hook-ownership.js");
const { buildMcpArgv } = require("./mcp-entry.js");

/**
 * OpenCode MCP wiring (nan-024 ADR-006). `init` provisions no OpenCode plugin;
 * `wire --harness opencode` writes `mcp.unimatrix` to the per-machine
 * ~/.config/opencode/opencode.json and removes an owned project entry (D-064).
 * Fail-safe legs: malformed/foreign/fs errors are skipped, never thrown (the
 * sole exception is a malformed project opencode.json during removal).
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
 * R-15 containment guard: resolve `target` and assert it lives inside `dir`.
 * Blocks a symlinked or traversal-crafted path from escaping the project. The
 * installer only ever writes to fixed constant subpaths, so a failure here is a
 * hostile/misconfigured tree — skip that write, never follow it.
 *
 * @param {string} dir - Absolute project root.
 * @param {string} target - Path to be written.
 * @returns {boolean} true iff `target` is within `dir`.
 */
function isWithinProject(dir, target) {
  const root = path.resolve(dir);
  const resolved = path.resolve(target);
  return resolved === root || resolved.startsWith(root + path.sep);
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
 * Read + parse a JSON file WITHOUT throwing (best-effort). Distinguishes
 * absent (ok, empty) from malformed (skip that leg, preserve the file).
 *
 * @param {string} filePath
 * @returns {{ present:boolean, parsed:(object|null), raw:string, malformed:boolean }}
 */
function readJsonSafe(filePath) {
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch (_err) {
    return { present: false, parsed: null, raw: "", malformed: false };
  }
  const trimmed = raw.trim();
  if (trimmed === "") {
    return { present: true, parsed: {}, raw: raw, malformed: false };
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { present: true, parsed: null, raw: raw, malformed: true };
    }
    return { present: true, parsed: parsed, raw: raw, malformed: false };
  } catch (_err) {
    return { present: true, parsed: null, raw: raw, malformed: true };
  }
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

function atomicWriteJson(file, obj, indent) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = file + "." + process.pid + ".tmp";
  try {
    fs.writeFileSync(tmp, JSON.stringify(obj, null, indent) + "\n", "utf8");
    fs.renameSync(tmp, file);
  } catch (e) {
    try { fs.unlinkSync(tmp); } catch (_) { /* best effort */ }
    throw e;
  }
}

/** Per-machine writer: ~/.config/opencode/opencode.json mcp.unimatrix only. */
function writeOpencodeMcp(o) {
  const cfgPath = path.join(o.home, ".config", "opencode", "opencode.json");
  const read = readJsonSafe(cfgPath);
  if (read.malformed) {
    return opencodeLeg("skipped-malformed", cfgPath, {
      reason: "opencode.json is not valid JSON — preserved unchanged",
    });
  }
  const cfg = read.parsed || {};
  if (cfg.mcp !== undefined && !isPlainObject(cfg.mcp)) {
    return opencodeLeg("skipped-malformed", cfgPath, {
      reason: "`mcp` key is not an object — preserved unchanged",
    });
  }
  const existing = cfg.mcp ? cfg.mcp.unimatrix : undefined;
  if (existing !== undefined) {
    if (!isUnimatrixMcpEntry("unimatrix", existing)) {
      return opencodeLeg("skipped-foreign", cfgPath, {
        reason: "mcp.unimatrix holds a non-Unimatrix entry; left unchanged",
        warning:
          "unimatrix: " + cfgPath +
          " key mcp.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written",
      });
    }
  } else if (o.harnessSelected !== true) {
    return opencodeLeg("skipped-intent", cfgPath, {
      reason: "new mcp.unimatrix entry requires explicit --harness opencode opt-in",
    });
  }
  const entry = buildOpencodeEntry(o);
  if (existing !== undefined && deepEqual(existing, entry)) {
    return opencodeLeg("unchanged", cfgPath, { entry });
  }
  const action = existing !== undefined ? "updated" : "created";
  if (o.dryRun) return opencodeLeg(action, cfgPath, { entry });
  cfg.mcp = cfg.mcp || {};
  cfg.mcp.unimatrix = entry;
  try {
    atomicWriteJson(cfgPath, cfg, read.present ? detectIndent(read.raw) : 2);
  } catch (e) {
    return opencodeLeg("skipped", cfgPath, { reason: "write failed: " + e.message });
  }
  return opencodeLeg(action, cfgPath, { entry });
}

/** Remove an owned mcp.unimatrix from the project opencode.json (D-064). */
function removeOwnedOpencodeProjectMcp(projectRoot, dryRun) {
  const file = path.join(projectRoot, "opencode.json");
  const none = { actions: [], warnings: [], changed: false };
  if (!isWithinProject(projectRoot, file)) return none;
  const read = readJsonSafe(file);
  if (!read.present) return none;
  if (read.malformed) {
    throw new Error(file + " is not valid JSON; fix it manually (not modified)");
  }
  const mcp = read.parsed && read.parsed.mcp;
  if (!isPlainObject(mcp) || mcp.unimatrix === undefined) return none;
  if (!isUnimatrixMcpEntry("unimatrix", mcp.unimatrix)) {
    return {
      actions: [],
      warnings: ["unimatrix: " + file + ' has a non-Unimatrix mcp entry named "unimatrix"; left unchanged'],
      changed: false,
    };
  }
  delete mcp.unimatrix;
  if (!dryRun) atomicWriteJson(file, read.parsed, detectIndent(read.raw));
  const msg = "Removed Unimatrix MCP entry from opencode.json";
  return { actions: [dryRun ? "[dry-run] " + msg : msg], warnings: [], changed: true };
}

module.exports = {
  detectsOpenCode,
  writeOpencodeMcp,
  removeOwnedOpencodeProjectMcp,
  buildOpencodeEntry,
  opencodeLeg,
  isWithinProject,
  detectIndent,
  readJsonSafe,
};
