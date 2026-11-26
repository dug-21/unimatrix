"use strict";

/**
 * Codex installer (nan-023 C14, nan-024 D-026/D-060/D-062/D-081/D-084/D-085).
 * Hooks -> `$CODEX_HOME/hooks.json` (default ~/.codex), MCP ->
 * `$CODEX_HOME/config.toml [mcp_servers.unimatrix]`; owned entries are removed
 * from the project `.codex/` files and `$CODEX_HOME/config.toml [hooks]`.
 * Fail-safe: nothing throws out of `maybeWireCodex`; malformed files are left
 * byte-intact. Surgical TOML helpers live in `codex-toml.js` (re-exported).
 */

const fs = require("fs");
const path = require("path");
const { isWithinProject, readJsonSafe, detectIndent } = require("./opencode-install.js");
const {
  buildShimCommand,
  isUnimatrixHook,
  removeOwnedHooks,
  EVENT_MATCHERS,
} = require("./merge-settings.js");
const { isUnimatrixCommand, isUnimatrixMcpEntry } = require("./hook-ownership.js");
const { buildMcpEntry } = require("./mcp-entry.js");
const { atomicWriteText } = require("./claude-json.js");
const toml = require("./codex-toml.js");

const {
  readTomlTable,
  upsertTomlTable,
  removeTomlTable,
  removeTablePositions,
  tomlTableToEntry,
  tomlString,
  scanTables,
  ownedRegion,
  splitKeepEndings,
  readTomlString,
} = toml;

const OWNED_TABLE = "mcp_servers.unimatrix";
const CODEX_HARNESS = "codex-cli";
const CODEX_PROVIDER = "codex-cli";
const TOML_MALFORMED_REASON = "config.toml has an ambiguous table boundary - preserved unchanged";
const CODEX_EVENTS = [
  "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse",
  "PreCompact", "SubagentStart", "Stop",
];

function readTextSafe(p) {
  try {
    return fs.readFileSync(p, "utf8");
  } catch (_) {
    return "";
  }
}

function isDir(p) {
  try {
    return fs.statSync(p).isDirectory();
  } catch (_) {
    return false;
  }
}

function isFile(p) {
  try {
    return fs.statSync(p).isFile();
  } catch (_) {
    return false;
  }
}

function leg(surface, action, legPath, extra) {
  return Object.assign({ harness: CODEX_HARNESS, surface, action, path: legPath }, extra || {});
}

/** Constant text printed by init and status (no trust hash is ever written). */
function codexApprovalStep() {
  return "Codex: run /hooks in Codex and approve the Unimatrix hooks (they stay inactive until approved)";
}

/** D-081/D-085: the Codex home dir; invalid/relative $CODEX_HOME is ignored with a warning. */
function codexHome(env, home) {
  const c = env && env.CODEX_HOME;
  if (typeof c === "string" && c !== "") {
    if (path.isAbsolute(c) && !c.includes("\0")) return { dir: c };
    return {
      dir: path.join(home, ".codex"),
      warning: "unimatrix: CODEX_HOME is not an absolute path; ignoring it and using ~/.codex",
    };
  }
  return { dir: path.join(home, ".codex") };
}

function onPath(name, pathVar, platform) {
  const win = platform === "win32";
  const dirs = String(pathVar || "").split(win ? ";" : ":").filter(Boolean);
  const names = win ? [name + ".exe", name + ".cmd", name + ".bat"] : [name];
  return dirs.some((d) => names.some((n) => isFile(path.join(d, n))));
}

/** D-081: any one trigger suffices. */
function detectsCodex(o) {
  const env = o.env || {};
  return (
    o.harnessSel === CODEX_HARNESS ||
    isDir(path.join(o.root, ".codex")) ||
    isDir(codexHome(env, o.home).dir) ||
    (typeof o.pathLookup === "function"
      ? !!o.pathLookup("codex")
      : onPath("codex", env.PATH, o.platform || process.platform))
  );
}

function requireProviderFlag(command) {
  if (typeof command !== "string" || !command.includes("--provider " + CODEX_PROVIDER)) {
    throw new Error("codex hook command missing --provider codex-cli (fail-loud, NFR-07/C-04)");
  }
  return command;
}

/** Owned-scoped matcher-group upsert for one event; dedups extra owned hooks. */
function upsertHookEntry(hooksObj, event, matcher, command) {
  const newEntry = { type: "command", command };
  if (!Array.isArray(hooksObj[event])) hooksObj[event] = [];
  let stripped = false;
  for (const g of hooksObj[event]) {
    if (!g || g.matcher === matcher || !Array.isArray(g.hooks)) continue;
    const n = g.hooks.length;
    g.hooks = g.hooks.filter((h) => !isUnimatrixHook(h));
    if (g.hooks.length !== n) stripped = true;
  }
  if (stripped) hooksObj[event] = hooksObj[event].filter((g) => g && !(Array.isArray(g.hooks) && g.hooks.length === 0));
  const groups = hooksObj[event];
  for (const group of groups) {
    if (!group || group.matcher !== matcher) continue;
    if (!Array.isArray(group.hooks)) group.hooks = [];
    let idx = -1;
    const dups = [];
    for (let i = 0; i < group.hooks.length; i++) {
      if (!isUnimatrixHook(group.hooks[i])) continue;
      if (idx === -1) idx = i;
      else dups.push(i);
    }
    let changed = stripped;
    for (let j = dups.length - 1; j >= 0; j--) {
      group.hooks.splice(dups[j], 1);
      changed = true;
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
  groups.push({ matcher, hooks: [newEntry] });
  return true;
}

/** One WireLeg per event; hooks only ever land in `<codexHome>/hooks.json`. */
function writeCodexHooks(o) {
  const { home, env, platform, dryRun } = o;
  const hooksPath = path.join(codexHome(env || {}, home).dir, "hooks.json");
  const skip = (reason) => [leg("hooks", "skipped-malformed", hooksPath, { reason })];
  const read = readJsonSafe(hooksPath);
  if (read.malformed) return skip("hooks.json is not valid JSON - preserved unchanged");
  const content = read.parsed || {};
  if (content.hooks !== undefined && (typeof content.hooks !== "object" || content.hooks === null || Array.isArray(content.hooks))) {
    return skip("`hooks` key is not an object - preserved unchanged");
  }
  if (content.hooks === undefined) content.hooks = {};
  let changed = false;
  const commands = [];
  for (const event of CODEX_EVENTS) {
    const command = requireProviderFlag(buildShimCommand(event, CODEX_PROVIDER, platform, home));
    commands.push(command);
    if (upsertHookEntry(content.hooks, event, EVENT_MATCHERS[event], command)) changed = true;
  }
  const action = read.present ? (changed ? "updated" : "unchanged") : "created";
  if (!dryRun && action !== "unchanged") {
    atomicWriteText(hooksPath, JSON.stringify(content, null, detectIndent(read.raw || "")) + "\n", 0o600);
  }
  return commands.map((command) => leg("hooks", action, hooksPath, { command }));
}

/**
 * Remove owned `hooks.*` tables from a Codex config.toml text. A table is owned
 * iff it has a `command = "<string>"` line satisfying isUnimatrixCommand. Emptied
 * `[[hooks.X]]` parent groups (no children, only `matcher`) are removed too.
 * @returns {{text: string, changed: boolean, malformed?: boolean}}
 */
function removeOwnedHooksToml(raw) {
  const lines = splitKeepEndings(raw);
  const scan = scanTables(lines);
  if (scan.error) return { text: raw, changed: false, malformed: true };
  const tables = scan.tables;
  const isHooks = (t) => t.bracketContent === "hooks" || t.bracketContent.startsWith("hooks.");
  const owned = new Set();
  tables.forEach((t, pos) => {
    if (!isHooks(t)) return;
    const { start, end } = ownedRegion(tables, pos, lines.length);
    for (let i = start; i < end; i++) {
      const m = /^[ \t]*command[ \t]*=[ \t]*(.*?)[ \t]*(#.*)?\r?\n?$/.exec(lines[i]);
      const r = m && readTomlString(m[1], 0);
      if (r && isUnimatrixCommand(r.value)) { owned.add(pos); break; }
    }
  });
  if (owned.size === 0) return { text: raw, changed: false };
  // Drop emptied parent array-of-tables groups.
  for (const pos of Array.from(owned)) {
    const segs = tables[pos].bracketContent.split(".");
    if (segs.length < 3) continue;
    const parentPath = segs.slice(0, -1).join(".");
    for (let p = pos - 1; p >= 0; p--) {
      if (!tables[p].bracketContent.startsWith(parentPath)) continue;
      if (tables[p].bracketContent !== parentPath || !tables[p].isArrayOfTables) break;
      let alive = false;
      for (let q = p + 1; q < tables.length && tables[q].bracketContent.startsWith(parentPath + "."); q++) {
        if (!owned.has(q)) alive = true;
      }
      const { start, end } = ownedRegion(tables, p, lines.length);
      const onlyMatcher = lines.slice(start, end).every((l) => /^[ \t]*(matcher[ \t]*=.*)?(#.*)?\r?\n?$/.test(l));
      if (!alive && onlyMatcher) owned.add(p);
      break;
    }
  }
  return { text: removeTablePositions(lines, tables, Array.from(owned)), changed: true };
}

function pre(dryRun, s) {
  return dryRun ? "[dry-run] " + s : s;
}

/** Remove owned hooks from project `.codex/hooks.json` and `<codexHome>/config.toml [hooks]`. */
function removeOwnedCodexHooks(o) {
  const { home, env, projectRoot, dryRun } = o;
  const actions = [];
  const legs = [];
  const homeDir = codexHome(env || {}, home).dir;
  const userHooks = path.join(homeDir, "hooks.json");
  const projHooks = path.join(projectRoot, ".codex", "hooks.json");
  // Never strip the user-level file we just wrote (project root == home).
  if (path.resolve(projHooks) !== path.resolve(userHooks) && isWithinProject(projectRoot, projHooks)) {
    try {
      const r = removeOwnedHooks(projHooks, { dryRun });
      for (const a of r.actions) actions.push(pre(dryRun, a));
      if (r.changed) legs.push(leg("hooks", "removed", projHooks, { reason: "owned project hooks removed" }));
    } catch (e) {
      legs.push(leg("hooks", "skipped-malformed", projHooks, { reason: "hooks.json is not valid JSON - preserved unchanged" }));
    }
  }
  const cfg = path.join(homeDir, "config.toml");
  const raw = readTextSafe(cfg);
  if (raw) {
    const r = removeOwnedHooksToml(raw);
    if (r.malformed) {
      legs.push(leg("hooks", "skipped-malformed", cfg, { reason: TOML_MALFORMED_REASON }));
    } else if (r.changed) {
      if (!dryRun) atomicWriteText(cfg, r.text);
      actions.push(pre(dryRun, "Removed unimatrix hooks from " + cfg + " [hooks]"));
      legs.push(leg("hooks", "removed", cfg, { reason: "owned [hooks] tables removed" }));
    }
  }
  return { actions, legs };
}

function foreignWarning(cfg) {
  return "unimatrix: " + cfg + " key mcp_servers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written";
}

/** MCP at `<codexHome>/config.toml`; written whenever Codex is detected (D-084). */
function writeCodexMcpToml(o) {
  const { home, env, platform, node, dryRun } = o;
  const cfg = path.join(codexHome(env || {}, home).dir, "config.toml");
  const raw = readTextSafe(cfg);
  const existing = readTomlTable(raw, OWNED_TABLE);
  if (existing.malformed) return leg("mcp", "skipped-malformed", cfg, { reason: TOML_MALFORMED_REASON });
  if (existing.present) {
    const cur = tomlTableToEntry(existing.body);
    if (!cur || !isUnimatrixMcpEntry("unimatrix", cur)) {
      return leg("mcp", "skipped-foreign", cfg, { reason: foreignWarning(cfg) });
    }
  }
  const e = buildMcpEntry(platform, home, node);
  const entry = { command: e.command, args: e.args };
  const body = [
    "command = " + tomlString(entry.command),
    "args = [" + entry.args.map(tomlString).join(", ") + "]",
  ];
  const result = upsertTomlTable(raw, OWNED_TABLE, body);
  if (result.malformed) return leg("mcp", "skipped-malformed", cfg, { reason: TOML_MALFORMED_REASON });
  if (!result.changed) return leg("mcp", "unchanged", cfg, { entry });
  if (!dryRun) atomicWriteText(cfg, result.text, 0o600);
  return leg("mcp", existing.present ? "updated" : "created", cfg, { entry });
}

/** Remove an owned `[mcp_servers.unimatrix]` from project `.codex/config.toml`. */
function removeOwnedCodexProjectMcp(projectRoot, dryRun, userCfg) {
  const out = { actions: [], warnings: [] };
  const cfg = path.join(projectRoot, ".codex", "config.toml");
  if (userCfg && path.resolve(cfg) === path.resolve(userCfg)) return out;
  if (!isWithinProject(projectRoot, cfg)) return out;
  const raw = readTextSafe(cfg);
  if (!raw) return out;
  const existing = readTomlTable(raw, OWNED_TABLE);
  if (existing.malformed) {
    out.warnings.push("unimatrix: " + cfg + " is not parseable TOML; left unchanged");
    return out;
  }
  if (!existing.present) return out;
  const cur = tomlTableToEntry(existing.body);
  if (!cur || !isUnimatrixMcpEntry("unimatrix", cur)) {
    out.warnings.push(foreignWarning(cfg));
    return out;
  }
  const r = removeTomlTable(raw, OWNED_TABLE);
  if (!r.changed) return out;
  if (!dryRun) atomicWriteText(cfg, r.text);
  out.actions.push(pre(dryRun, "Removed unimatrix MCP entry from " + cfg));
  return out;
}

/**
 * Top-level Codex fan-out. Not detected -> one skipped-not-detected leg and no
 * file touched. Never throws; warnings go to stderr once and onto leg reasons.
 * @returns {object[]} WireLeg[]
 */
function maybeWireCodex(opts) {
  const o = opts || {};
  const env = o.env || process.env;
  const home = o.home;
  const platform = o.platform || process.platform;
  const dryRun = !!o.dryRun;
  let legs = [];
  const warnings = [];
  const ch = codexHome(env, home);
  if (ch.warning) warnings.push(ch.warning);
  try {
    if (!detectsCodex({ root: o.projectRoot, env, home, harnessSel: o.harnessSel, platform, pathLookup: o.pathLookup })) {
      legs.push(leg("codex", "skipped-not-detected", ch.dir, {
        reason: "no Codex signal (.codex/, $CODEX_HOME dir, codex on PATH, --harness codex-cli)",
      }));
    } else {
      const base = { home, env, platform, node: o.node, dryRun };
      legs = legs.concat(writeCodexHooks(base));
      legs.push(writeCodexMcpToml(base));
      const rh = removeOwnedCodexHooks({ home, env, projectRoot: o.projectRoot, dryRun });
      legs = legs.concat(rh.legs);
      const pm = removeOwnedCodexProjectMcp(o.projectRoot, dryRun, path.join(ch.dir, "config.toml"));
      for (const a of pm.actions) legs.push(leg("mcp", "removed", path.join(o.projectRoot, ".codex", "config.toml"), { reason: a }));
      for (const w of pm.warnings) warnings.push(w);
    }
  } catch (e) {
    legs.push(leg("mcp", "skipped-malformed", path.join(ch.dir, "config.toml"), { reason: "codex wiring error: " + e.message }));
  }
  for (const l of legs) if (l.action === "skipped-foreign" && l.reason) warnings.push(l.reason);
  if (ch.warning) for (const l of legs) l.reason = l.reason ? l.reason + "; " + ch.warning : ch.warning;
  for (const w of warnings) {
    try { process.stderr.write(w + "\n"); } catch (_) { /* best effort */ }
  }
  return legs;
}

module.exports = {
  upsertTomlTable,
  readTomlTable,
  removeTomlTable,
  tomlTableToEntry,
  tomlString,
  OWNED_TABLE,
  codexHome,
  detectsCodex,
  maybeWireCodex,
  writeCodexMcpToml,
  writeCodexHooks,
  removeOwnedCodexHooks,
  removeOwnedHooksToml,
  removeOwnedCodexProjectMcp,
  codexApprovalStep,
  requireProviderFlag,
  CODEX_EVENTS,
  CODEX_PROVIDER,
  CODEX_HARNESS,
};
