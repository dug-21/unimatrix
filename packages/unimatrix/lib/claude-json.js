"use strict";
// Claude MCP in the resolved .claude.json and project .mcp.json owned-entry
// removal (nan-024 D-060, D-065; #1036 C7). Pure mutates through updateFileSync;
// only the caller-resolved file is touched (D-028). Parse failure aborts untouched.

const fs = require("fs");
const path = require("path");
const { isUnimatrixMcpEntry } = require("./hook-ownership.js");
const { detectIndent } = require("./opencode-install.js");
const { updateFileSync } = require("./safe-write.js");

class ClaudeJsonError extends Error {}

const plain = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const deepEqual = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const NUM = /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y;

// Number tokens JSON.parse would not round-trip (e.g. 9007199254740993, 1.0)
// are swapped for unique string sentinels so stringify restores their text.
function protectNumbers(raw) {
  let tag = "\\u0001n"; // escaped in JSON text; stringify re-emits it escaped
  while (raw.includes(tag)) tag += "n";
  const toks = [];
  let out = "", last = 0;
  for (let i = 0; i < raw.length; i++) {
    const c = raw[i];
    if (c === '"') {
      for (i++; i < raw.length && raw[i] !== '"'; i++) if (raw[i] === "\\") i++;
    } else if (c === "-" || (c >= "0" && c <= "9")) {
      NUM.lastIndex = i;
      const m = NUM.exec(raw);
      if (!m) continue;
      const t = m[0];
      if (JSON.stringify(Number(t)) !== t) {
        out += raw.slice(last, i) + '"' + tag + toks.length + '"';
        toks.push(t);
        last = i + t.length;
      }
      i += t.length - 1;
    }
  }
  return { text: out + raw.slice(last), tag, toks };
}

function restoreNumbers(text, p) {
  if (p.toks.length === 0) return text;
  return text.replace(new RegExp('"\\' + p.tag + '(\\d+)"', "g"), (_m, n) => p.toks[n]);
}

function parseJson(file, raw) {
  const p = protectNumbers(raw);
  let j;
  try {
    j = JSON.parse(p.text);
  } catch (_) {
    throw new ClaudeJsonError(file + " is not valid JSON; fix it manually (not modified)");
  }
  return { j, p };
}

function parseClaudeJson(file, raw) {
  if (raw === null || raw.trim() === "") return { j: {}, p: { toks: [] } };
  const r = parseJson(file, raw);
  if (!plain(r.j)) throw new ClaudeJsonError(file + " is not a JSON object; fix it manually (not modified)");
  return r;
}

function normalizeKey(k, platform) {
  let s = String(k);
  try { s = fs.realpathSync(s); } catch (_) { /* keep as-is */ }
  if (platform === "win32") {
    s = s.replace(/\\/g, "/");
    s = s.replace(/^([A-Za-z]):/, (_m, d) => d.toLowerCase() + ":");
  }
  return s.length > 1 ? s.replace(/[/\\]+$/, "") : s;
}

/** Reuse Claude's own key spelling when an existing key names the same root. */
function resolveProjectKey(projects, root, platform) {
  const plat = platform || process.platform;
  let real = root;
  try { real = fs.realpathSync(root); } catch (_) { /* keep */ }
  const want = normalizeKey(real, plat);
  if (plain(projects)) {
    for (const k of Object.keys(projects)) {
      if (normalizeKey(k, plat) === want) return k;
    }
  }
  return plat === "win32" ? real.replace(/\\/g, "/") : real;
}

const localEntry = (parsed, key) => {
  const proj = plain(parsed.projects) ? parsed.projects[key] : undefined;
  const servers = plain(proj) ? proj.mcpServers : undefined;
  return plain(servers) ? servers.unimatrix : undefined;
};

/** Pure: destination write (W2) + owned user-scope removal (R1/G2) in one mutate. */
function computeClaudeMcp(raw, o) {
  const { j: parsed, p } = parseClaudeJson(o.file, raw);
  const key = resolveProjectKey(parsed.projects, o.projectRoot, o.platform);
  const existing = localEntry(parsed, key);
  if (existing !== undefined && !isUnimatrixMcpEntry("unimatrix", existing)) {
    const warning =
      "unimatrix: " + o.file + ' key projects["' + key +
      '"].mcpServers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written';
    return { text: null, detail: { key, skip: { action: "skipped-foreign", warning } } };
  }
  let changed = false;
  const mcpAction = existing === undefined ? "created" : deepEqual(existing, o.entry) ? "unchanged" : "updated";
  if (mcpAction !== "unchanged") {
    if (!plain(parsed.projects)) parsed.projects = {};
    if (!plain(parsed.projects[key])) parsed.projects[key] = {};
    if (!plain(parsed.projects[key].mcpServers)) parsed.projects[key].mcpServers = {};
    parsed.projects[key].mcpServers.unimatrix = o.entry;
    changed = true;
  }
  const detail = { key, mcpAction, removedUserScope: false };
  const top = plain(parsed.mcpServers) ? parsed.mcpServers.unimatrix : undefined;
  if (top !== undefined) {
    if (isUnimatrixMcpEntry("unimatrix", top)) {
      delete parsed.mcpServers.unimatrix;
      detail.removedUserScope = changed = true;
    } else {
      detail.userScopeForeignWarning = "unimatrix: " + o.file + " key mcpServers.unimatrix holds a non-Unimatrix entry; left unchanged";
    }
  }
  const text = changed ? restoreNumbers(JSON.stringify(parsed, null, raw ? detectIndent(raw) : 2), p) + "\n" : null;
  return { text, detail };
}

function runClaudeMcp(o) {
  const res = updateFileSync(o.file, (raw) => computeClaudeMcp(raw, o).text, { dryRun: !!o.dryRun, newMode: 0o600, platform: o.platform });
  return computeClaudeMcp(res.before, o).detail;
}

/** W2 + R1 legs. o: {file, projectRoot, entry, platform, dryRun} */
function claudeMcpLegs(o) {
  const d = runClaudeMcp(o);
  const base = { harness: "claude-code", surface: "mcp", path: o.file };
  if (d.skip) return [{ ...base, action: "skipped-foreign", reason: d.skip.warning, warning: d.skip.warning }];
  const legs = [{ ...base, action: d.mcpAction, entry: o.entry }];
  if (d.removedUserScope) legs.push({ ...base, action: "removed", reason: "owned user-scope MCP entry removed" });
  if (d.userScopeForeignWarning) legs[0].warning = d.userScopeForeignWarning;
  return legs;
}

/** Thin wrapper (§6): {action, file, key, warning?, removedUserScope}. */
function writeClaudeMcp(o) {
  const d = runClaudeMcp(o);
  const r = { action: d.skip ? "skipped-foreign" : d.mcpAction, file: o.file, key: d.key, removedUserScope: !!d.removedUserScope };
  const w = d.skip ? d.skip.warning : d.userScopeForeignWarning;
  if (w) r.warning = w;
  return r;
}

/** Pure: delete owned servers from a project .mcp.json. */
function computeMcpJsonRemoval(raw, file) {
  const none = { text: null, detail: { changed: false, warnings: [] } };
  if (raw === null || raw.trim() === "") return none;
  const { j: parsed, p } = parseJson(file, raw);
  const servers = plain(parsed) ? parsed.mcpServers : undefined;
  if (!plain(servers)) return none;
  const warnings = [];
  const owned = Object.keys(servers).filter((k) => isUnimatrixMcpEntry(k, servers[k]));
  if (servers.unimatrix !== undefined && !owned.includes("unimatrix")) {
    warnings.push("unimatrix: " + file + ' has a non-Unimatrix server named "unimatrix"; left unchanged');
  }
  if (owned.length === 0) return { text: null, detail: { changed: false, warnings } };
  for (const k of owned) delete servers[k];
  return { text: restoreNumbers(JSON.stringify(parsed, null, detectIndent(raw)), p) + "\n", detail: { changed: true, warnings } };
}

function removeOwnedMcpJson(projectRoot, dryRun) {
  const file = path.join(projectRoot, ".mcp.json");
  const res = updateFileSync(file, (raw) => computeMcpJsonRemoval(raw, file).text, { dryRun: !!dryRun, containWithin: projectRoot });
  const d = computeMcpJsonRemoval(res.before, file).detail;
  const msg = "Removed Unimatrix MCP entry from .mcp.json";
  return { actions: d.changed ? [dryRun ? "[dry-run] " + msg : msg] : [], warnings: d.warnings, changed: d.changed };
}

/** Legs for R6; leg-less warnings ride on legs.warnings. */
function removeOwnedMcpJsonLeg(root, dryRun) {
  const r = removeOwnedMcpJson(root, dryRun);
  const legs = r.changed
    ? [{ harness: "claude-code", surface: "mcp", action: "removed", path: path.join(root, ".mcp.json"), reason: "owned project MCP entry removed" }]
    : [];
  return Object.assign(legs, { warnings: r.warnings });
}

/** Read-only status probe; never throws. */
function readClaudeMcpState(file, projectRoot, platform) {
  let raw;
  try {
    raw = fs.readFileSync(file, "utf8");
  } catch (_) {
    return { status: "missing", file, key: resolveProjectKey(undefined, projectRoot, platform) };
  }
  let j;
  try {
    j = raw.trim() === "" ? {} : JSON.parse(raw);
  } catch (_) {
    return { status: "unreadable", file, key: null };
  }
  if (!plain(j)) return { status: "unreadable", file, key: null };
  const key = resolveProjectKey(j.projects, projectRoot, platform);
  const e = localEntry(j, key);
  if (e === undefined) return { status: "missing", file, key };
  return { status: isUnimatrixMcpEntry("unimatrix", e) ? "wired" : "foreign", file, key };
}

module.exports = {
  writeClaudeMcp,
  claudeMcpLegs,
  computeClaudeMcp,
  removeOwnedMcpJson,
  removeOwnedMcpJsonLeg,
  computeMcpJsonRemoval,
  readClaudeMcpState,
  resolveProjectKey,
  ClaudeJsonError,
};
