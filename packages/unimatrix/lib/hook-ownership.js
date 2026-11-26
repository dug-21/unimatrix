"use strict";
// Single ownership predicate for hook commands and MCP entries (nan-024 ADR-003).
// Pure: no fs, never throws.

const CLAUDE_EVENTS = [
  "SessionStart", "Stop", "UserPromptSubmit", "PreToolUse", "PostToolUse",
  "PostToolUseFailure", "PreCompact", "SubagentStart", "SubagentStop",
];
// Mirrors codex-install CODEX_EVENTS (defined here to avoid a require cycle).
const CODEX_EVENTS = [
  "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse",
  "PreCompact", "SubagentStart", "Stop",
];
const KNOWN_EVENTS = Array.from(new Set(CLAUDE_EVENTS.concat(CODEX_EVENTS)));

const EV = "(?:" + KNOWN_EVENTS.join("|") + ")";
const ANYEV = "[A-Z][A-Za-z]+";
const PROV = "(?:\\s+--provider(?:\\s+|=)[\\w-]+)?";
const LDP = "(?:LD_LIBRARY_PATH=(?:\"[^\"]*\"|'[^']*'|\\S+)\\s+)?";
const SEP = "[/\\\\]";
const END = PROV + "\\s*$";
const SHIM = "\\.unimatrix" + SEP + "bin" + SEP + "unimatrix(?:\\.cmd)?";

const OWNED_FORMS = [
  { name: "L1-bare", re: new RegExp("^" + LDP + "unimatrix(?:-server)?\\s+hook\\s+" + EV + END) },
  {
    name: "L2-abs",
    re: new RegExp(
      "^" + LDP + "(?:\"[^\"]*|'[^']*|\\S*)" + SEP +
        "unimatrix(?:-server)?(?:\\.exe)?[\"']?\\s+hook\\s+" + EV + END
    ),
  },
  {
    name: "L3-node-client",
    re: new RegExp(
      "(^|[\\s\"'/\\\\])node(?:\\.exe)?\\s+(?:\"[^\"]*" + SEP + "hook-client" + SEP +
        "index\\.js\"|'[^']*" + SEP + "hook-client" + SEP + "index\\.js'|\\S+" + SEP +
        "hook-client" + SEP + "index\\.js)\\s+[A-Z]\\w*"
    ),
  },
  {
    name: "L5-npx",
    re: new RegExp(
      "^npx(?:\\s+-y)?\\s+(?:@dug-21/)?unimatrix(?:@\\S+)?\\s+hook\\s+" + EV + END
    ),
  },
  {
    name: "L6-home-shim",
    re: new RegExp(
      "^[\"']?(?:\\$HOME|\\$\\{HOME\\}|~)" + SEP + SHIM + "[\"']?\\s+hook\\s+" + ANYEV + END
    ),
  },
  {
    name: "abs-shim",
    re: new RegExp("^([\"'])[^\"']*?" + SEP + SHIM + "\\1\\s+hook\\s+" + ANYEV + END),
  },
];

function isUnimatrixCommand(cmd) {
  try {
    if (typeof cmd !== "string" || cmd === "") return false;
    const s = cmd.trim();
    return OWNED_FORMS.some((f) => f.re.test(s));
  } catch (_) {
    return false;
  }
}

function normalizeMcpArgv(entry) {
  try {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) return null;
    if (Array.isArray(entry.command)) return entry.command.map(String);
    if (typeof entry.command !== "string") return null;
    const args = Array.isArray(entry.args) ? entry.args.map(String) : [];
    return [entry.command].concat(args);
  } catch (_) {
    return null;
  }
}

function baseName(p) {
  const b = String(p).split(/[/\\]/).pop().toLowerCase();
  return b.replace(/\.exe$/, "");
}

const RE_BRIDGE = /[/\\]mcp-bridge\.js$/;
const RE_NPX_PKG = /^(?:@dug-21\/)?unimatrix(?:@\S+)?$/;
const RE_SH = /exec\s+"\$HOME\/\.unimatrix\/bin\/unimatrix"\s+mcp-stdio/;
const RE_CMD = /\.unimatrix\\bin\\unimatrix\.cmd"\s+mcp-stdio/i;
const RE_SHIM_ENTRY = /[/\\]\.unimatrix[/\\]bin[/\\]shim-entry\.js$/;

function isUnimatrixMcpEntry(name, entry) {
  try {
    if (name !== "unimatrix") return false;
    const argv = normalizeMcpArgv(entry);
    if (!argv || argv.length === 0) return false;
    const cmd = argv[0];
    const a = argv.slice(1);
    const base = baseName(cmd);
    if (base === "node" && a[0] !== undefined && RE_BRIDGE.test(a[0])) return true;
    if (base === "npx") {
      const i = a.findIndex((x) => RE_NPX_PKG.test(x));
      if (i >= 0 && a[i + 1] === "mcp-bridge") return true;
    }
    if ((base === "unimatrix" || base === "unimatrix-server") &&
        (a.length === 0 || a[0] === "serve" || a[0] === "mcp-stdio")) return true;
    if (base === "sh" && a[0] === "-c" && typeof a[1] === "string" && RE_SH.test(a[1])) return true;
    if (base === "cmd" && RE_CMD.test(a.join(" "))) return true;
    if (a[0] !== undefined && RE_SHIM_ENTRY.test(a[0]) && a[1] === "mcp-stdio") return true;
    return false;
  } catch (_) {
    return false;
  }
}

module.exports = {
  OWNED_FORMS, KNOWN_EVENTS, isUnimatrixCommand, isUnimatrixMcpEntry, normalizeMcpArgv,
};
