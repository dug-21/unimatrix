"use strict";
// Per-machine Claude MCP in ~/.claude.json and project .mcp.json owned-entry
// removal (nan-024 D-060, D-065). Only projects[<root>].mcpServers.unimatrix is
// ever changed; every write is tmp+rename; parse failure aborts untouched.

const fs = require("fs");
const path = require("path");
const { isUnimatrixMcpEntry } = require("./hook-ownership.js");
const { detectIndent } = require("./opencode-install.js");
const { atomicWrite } = require("./atomic-write.js");

class ClaudeJsonError extends Error {}

const plain = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

function readOrNull(file) {
  try {
    return fs.readFileSync(file, "utf8");
  } catch (_) {
    return null;
  }
}

function statOrNull(file) {
  try {
    const s = fs.statSync(file);
    return { m: s.mtimeMs, z: s.size, mode: s.mode & 0o777 };
  } catch (_) {
    return null;
  }
}

const sameStat = (a, b) =>
  (a === null && b === null) || (a !== null && b !== null && a.m === b.m && a.z === b.z);

function deepEqual(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** tmp + rename preserving an existing file's mode; `mode` applies to new files only. */
const atomicWriteText = atomicWrite;

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

function writeClaudeMcp(o) {
  const file = path.join(o.home, ".claude.json");
  for (let attempt = 0; attempt < 3; attempt++) {
    const raw = readOrNull(file);
    const st1 = statOrNull(file);
    let parsed = {};
    if (raw !== null && raw.trim() !== "") {
      try {
        parsed = JSON.parse(raw);
      } catch (_) {
        throw new ClaudeJsonError(file + " is not valid JSON; fix it manually (not modified)");
      }
    }
    if (!plain(parsed)) {
      throw new ClaudeJsonError(file + " is not a JSON object; fix it manually (not modified)");
    }
    const key = resolveProjectKey(parsed.projects, o.projectRoot, o.platform);
    const proj = plain(parsed.projects) ? parsed.projects[key] : undefined;
    const servers = plain(proj) ? proj.mcpServers : undefined;
    const existing = plain(servers) ? servers.unimatrix : undefined;
    if (existing !== undefined && !isUnimatrixMcpEntry("unimatrix", existing)) {
      return {
        action: "skipped-foreign",
        file,
        key,
        warning:
          "unimatrix: " + file + ' key projects["' + key +
          '"].mcpServers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written',
      };
    }
    if (existing !== undefined && deepEqual(existing, o.entry)) {
      return { action: "unchanged", file, key };
    }
    const action = existing !== undefined ? "updated" : "created";
    if (o.dryRun) return { action, file, key };
    if (!plain(parsed.projects)) parsed.projects = {};
    if (!plain(parsed.projects[key])) parsed.projects[key] = {};
    if (!plain(parsed.projects[key].mcpServers)) parsed.projects[key].mcpServers = {};
    parsed.projects[key].mcpServers.unimatrix = o.entry;
    const out = JSON.stringify(parsed, null, raw ? detectIndent(raw) : 2) + "\n";
    if (!sameStat(st1, statOrNull(file))) continue; // Claude wrote meanwhile; re-read
    atomicWriteText(file, out, st1 ? st1.mode : 0o600);
    return { action, file, key };
  }
  throw new ClaudeJsonError("~/.claude.json changed while writing; re-run init");
}

function removeOwnedMcpJson(projectRoot, dryRun) {
  const file = path.join(projectRoot, ".mcp.json");
  const none = { actions: [], warnings: [], changed: false };
  const raw = readOrNull(file);
  if (raw === null || raw.trim() === "") return none;
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (_) {
    throw new ClaudeJsonError(file + " is not valid JSON; fix it manually (not modified)");
  }
  const servers = plain(parsed) ? parsed.mcpServers : undefined;
  if (!plain(servers)) return none;
  const warnings = [];
  const owned = Object.keys(servers).filter((k) => isUnimatrixMcpEntry(k, servers[k]));
  if (servers.unimatrix !== undefined && !owned.includes("unimatrix")) {
    warnings.push("unimatrix: " + file + ' has a non-Unimatrix server named "unimatrix"; left unchanged');
  }
  if (owned.length === 0) return { actions: [], warnings, changed: false };
  for (const k of owned) delete servers[k];
  if (!dryRun) {
    atomicWriteText(file, JSON.stringify(parsed, null, detectIndent(raw)) + "\n");
  }
  const msg = "Removed Unimatrix MCP entry from .mcp.json";
  return { actions: [dryRun ? "[dry-run] " + msg : msg], warnings, changed: true };
}

module.exports = {
  writeClaudeMcp,
  removeOwnedMcpJson,
  resolveProjectKey,
  atomicWriteText,
  ClaudeJsonError,
};
