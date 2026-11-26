"use strict";
// Recorded-node path-exists preflight for `wire` / non-re-recording upgrades
// (nan-024 D-082, D-057, D-066). init never uses this: its re-record step is
// its own check (NodeNotRecordableError from install-store).

const fs = require("fs");
const path = require("path");
const { readInstallJson } = require("./install-store.js");
const { isFallbackMcpEntry } = require("./mcp-entry.js");

function isFile(p) {
  try {
    return fs.statSync(p).isFile();
  } catch (_) {
    return false;
  }
}

function readText(p) {
  try {
    return fs.readFileSync(p, "utf8");
  } catch (_) {
    return null;
  }
}

function onPath(env, platform) {
  const exe = platform === "win32" ? "node.exe" : "node";
  const parts = String((env && env.PATH) || "").split(platform === "win32" ? ";" : ":");
  return parts.some((d) => d && isFile(path.join(d, exe)));
}

/** Commands of owned D-047 fallback MCP entries (win32 has no PATH fallback). */
function fallbackCommands(home, env) {
  const out = [];
  const add = (entry) => {
    if (entry && isFallbackMcpEntry(entry) && typeof entry.command === "string") out.push(entry.command);
  };
  try {
    const cj = JSON.parse(readText(path.join(home, ".claude.json")) || "{}");
    for (const p of Object.values((cj && cj.projects) || {})) add(p && p.mcpServers && p.mcpServers.unimatrix);
  } catch (_) { /* unreadable: nothing to check */ }
  try {
    const oc = JSON.parse(readText(path.join(home, ".config", "opencode", "opencode.json")) || "{}");
    const c = oc && oc.mcp && oc.mcp.unimatrix && oc.mcp.unimatrix.command;
    if (Array.isArray(c)) add({ command: c[0], args: c.slice(1) });
  } catch (_) { /* unreadable */ }
  try {
    const { codexHome, readTomlTable, tomlTableToEntry, OWNED_TABLE } = require("./codex-install.js");
    const raw = readText(path.join(codexHome(env || {}, home).dir, "config.toml"));
    const t = raw && readTomlTable(raw, OWNED_TABLE);
    if (t && t.present) add(tomlTableToEntry(t.body));
  } catch (_) { /* unreadable */ }
  return out;
}

/**
 * @returns {{node?: string, warning?: string}} node = still-valid recorded node
 * (keep it); throws when the rule says exit 1.
 */
function preflightRecordedNode(o) {
  const platform = o.platform || process.platform;
  const inst = readInstallJson(o.home);
  if (!inst || typeof inst.node !== "string") return {};
  const fail = (p) => new Error("unimatrix: recorded node " + p + " missing; run 'npx unimatrix init'");
  if (platform === "win32") {
    if (!isFile(inst.node)) throw fail(inst.node);
    for (const c of fallbackCommands(o.home, o.env)) if (!isFile(c)) throw fail(c);
    return { node: inst.node };
  }
  if (isFile(inst.node)) return { node: inst.node };
  if (!onPath(o.env || process.env, platform)) throw fail(inst.node);
  return { warning: "unimatrix: recorded node " + inst.node + " missing; using the current node (run 'npx unimatrix init' to re-record)" };
}

module.exports = { preflightRecordedNode, onPath };
