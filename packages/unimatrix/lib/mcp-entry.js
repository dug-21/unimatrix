"use strict";
// Shared MCP entry builder (nan-024 D-040, D-047, D-072). Pure: no fs.

const { isUnimatrixMcpEntry } = require("./hook-ownership.js");

function toFwd(p) {
  return String(p).replace(/\\/g, "/");
}

/** {command, args, env} for the per-machine Claude/Codex MCP entry. */
function buildMcpEntry(platform, home, node) {
  if (platform === "win32") {
    return {
      command: node,
      args: [toFwd(home) + "/.unimatrix/bin/shim-entry.js", "mcp-stdio"],
      env: {},
    };
  }
  return {
    command: "sh",
    args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'],
    env: {},
  };
}

/** Flat argv form (OpenCode). */
function buildMcpArgv(platform, home, node) {
  const e = buildMcpEntry(platform, home, node);
  return [e.command].concat(e.args);
}

/** D-047 fallback shape: node shim-entry.js mcp-stdio. */
function isFallbackMcpEntry(entry) {
  try {
    const a = entry && Array.isArray(entry.args) ? entry.args : [];
    return (
      typeof a[0] === "string" &&
      /\.unimatrix[/\\]bin[/\\]shim-entry\.js$/.test(a[0]) &&
      a[1] === "mcp-stdio"
    );
  } catch (_) {
    return false;
  }
}

module.exports = { buildMcpEntry, buildMcpArgv, isFallbackMcpEntry, isUnimatrixMcpEntry };
