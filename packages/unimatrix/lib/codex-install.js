"use strict";

/**
 * Codex installer (nan-023 C14, nan-024 D-026/D-060/D-062/D-081/D-084/D-085).
 * Hooks -> `$CODEX_HOME/hooks.json` (default ~/.codex), MCP ->
 * `$CODEX_HOME/config.toml [mcp_servers.unimatrix]`; owned entries are removed
 * from the project `.codex/` files and `$CODEX_HOME/config.toml [hooks]`.
 * Writes go through safe-write `updateFileSync`; only `SafeWriteError` escapes
 * `maybeWireCodex` (FR-31); malformed files are left byte-intact. Surgical TOML helpers live in `codex-toml.js` (re-exported).
 */

const fs = require("fs");
const path = require("path");
const { detectIndent } = require("./opencode-install.js");
const {
  buildShimCommand,
  isUnimatrixHook,
  removeOwnedHooks,
  EVENT_MATCHERS,
} = require("./merge-settings.js");
const { isUnimatrixCommand, isUnimatrixMcpEntry } = require("./hook-ownership.js");
const { buildMcpEntry } = require("./mcp-entry.js");
const { updateFileSync, SafeWriteError } = require("./safe-write.js");
const { codexHome, sameRealFile } = require("./config-dirs.js");
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

/** Parse JSON object text like opencode readJsonSafe (null = absent, blank = {}). */
function parseJsonObj(raw) {
  if (raw === null) return { present: false, parsed: {} };
  if (raw.trim() === "") return { present: true, parsed: {} };
  try {
    const v = JSON.parse(raw);
    if (v && typeof v === "object" && !Array.isArray(v)) return { present: true, parsed: v };
  } catch (_) { /* malformed */ }
  return { present: true, malformed: true };
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

const orNull = (t) => (typeof t === "string" ? t : null);

function leg(surface, action, legPath, extra) {
  return Object.assign({ harness: CODEX_HARNESS, surface, action, path: legPath }, extra || {});
}

/** Constant text printed by init and status (no trust hash is ever written). */
function codexApprovalStep() {
  return "Codex: run /hooks in Codex and approve the Unimatrix hooks (they stay inactive until approved)";
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

function codexPaths(o) {
  if (o.dirs && o.dirs.codex) return { hooks: o.dirs.codex.hooksJson, cfg: o.dirs.codex.configToml };
  const d = codexHome(o.env || {}, o.home).dir;
  return { hooks: path.join(d, "hooks.json"), cfg: path.join(d, "config.toml") };
}

/** One WireLeg per event; hooks only ever land in `<codexHome>/hooks.json`. */
function writeCodexHooks(o) {
  const { home, platform, dryRun } = o;
  const hooksPath = codexPaths(o).hooks;
  const commands = CODEX_EVENTS.map((ev) => requireProviderFlag(buildShimCommand(ev, CODEX_PROVIDER, platform, home)));
  const compute = (raw) => {
    const read = parseJsonObj(raw);
    if (read.malformed) return { skip: "hooks.json is not valid JSON - preserved unchanged" };
    const content = read.parsed;
    if (content.hooks !== undefined && (typeof content.hooks !== "object" || content.hooks === null || Array.isArray(content.hooks))) {
      return { skip: "`hooks` key is not an object - preserved unchanged" };
    }
    if (content.hooks === undefined) content.hooks = {};
    let changed = false;
    CODEX_EVENTS.forEach((ev, i) => {
      if (upsertHookEntry(content.hooks, ev, EVENT_MATCHERS[ev], commands[i])) changed = true;
    });
    return { text: changed || !read.present ? JSON.stringify(content, null, detectIndent(raw || "")) + "\n" : null };
  };
  const res = updateFileSync(hooksPath, (raw) => orNull(compute(raw).text), { dryRun, newMode: 0o600 });
  const d = compute(res.before);
  if (d.skip) return [leg("hooks", "skipped-malformed", hooksPath, { reason: d.skip })];
  return commands.map((command) => leg("hooks", res.action, hooksPath, { command }));
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
  const { projectRoot, dryRun } = o;
  const actions = [];
  const legs = [];
  const p = codexPaths(o);
  const projHooks = path.join(projectRoot, ".codex", "hooks.json");
  // Never strip the user-level file we just wrote (project root == home).
  if (!sameRealFile(projHooks, p.hooks)) {
    try {
      const r = removeOwnedHooks(projHooks, { dryRun, containWithin: projectRoot });
      for (const a of r.actions) actions.push(pre(dryRun, a));
      if (r.changed) legs.push(leg("hooks", "removed", projHooks, { reason: "owned project hooks removed" }));
    } catch (e) {
      if (e instanceof SafeWriteError) throw e;
      legs.push(leg("hooks", "skipped-malformed", projHooks, { reason: "hooks.json is not valid JSON - preserved unchanged" }));
    }
  }
  const cfg = p.cfg;
  const compute = (raw) => (raw ? removeOwnedHooksToml(raw) : { changed: false });
  const res = updateFileSync(cfg, (raw) => {
    const r = compute(raw);
    return r.changed ? r.text : null;
  }, { dryRun, newMode: 0o600 });
  const r = compute(res.before);
  if (r.malformed) {
    legs.push(leg("hooks", "skipped-malformed", cfg, { reason: TOML_MALFORMED_REASON }));
  } else if (r.changed) {
    actions.push(pre(dryRun, "Removed unimatrix hooks from " + cfg + " [hooks]"));
    legs.push(leg("hooks", "removed", cfg, { reason: "owned [hooks] tables removed" }));
  }
  return { actions, legs };
}

function foreignWarning(cfg) {
  return "unimatrix: " + cfg + " key mcp_servers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written";
}

/** MCP at `<codexHome>/config.toml`; written whenever Codex is detected (D-084). */
function writeCodexMcpToml(o) {
  const { home, platform, node, dryRun } = o;
  const cfg = codexPaths(o).cfg;
  const e = buildMcpEntry(platform, home, node);
  const entry = { command: e.command, args: e.args };
  const body = [
    "command = " + tomlString(entry.command),
    "args = [" + entry.args.map(tomlString).join(", ") + "]",
  ];
  const compute = (rawIn) => {
    const raw = rawIn || "";
    const existing = readTomlTable(raw, OWNED_TABLE);
    if (existing.malformed) return { skip: ["skipped-malformed", TOML_MALFORMED_REASON] };
    if (existing.present) {
      const cur = tomlTableToEntry(existing.body);
      if (!cur || !isUnimatrixMcpEntry("unimatrix", cur)) return { skip: ["skipped-foreign", foreignWarning(cfg)] };
    }
    const result = upsertTomlTable(raw, OWNED_TABLE, body);
    if (result.malformed) return { skip: ["skipped-malformed", TOML_MALFORMED_REASON] };
    return { text: result.changed ? result.text : null, tablePresent: existing.present };
  };
  const res = updateFileSync(cfg, (raw) => orNull(compute(raw).text), { dryRun, newMode: 0o600 });
  const d = compute(res.before);
  if (d.skip) return leg("mcp", d.skip[0], cfg, { reason: d.skip[1] });
  const action = res.action === "unchanged" ? "unchanged" : d.tablePresent ? "updated" : "created";
  return leg("mcp", action, cfg, { entry });
}

/** Remove an owned `[mcp_servers.unimatrix]` from project `.codex/config.toml`. */
function removeOwnedCodexProjectMcp(projectRoot, dryRun, userCfg) {
  const out = { actions: [], warnings: [] };
  const cfg = path.join(projectRoot, ".codex", "config.toml");
  if (userCfg && sameRealFile(cfg, userCfg)) return out;
  const compute = (raw) => {
    if (!raw) return {};
    const existing = readTomlTable(raw, OWNED_TABLE);
    if (existing.malformed) return { warn: "unimatrix: " + cfg + " is not parseable TOML; left unchanged" };
    if (!existing.present) return {};
    const cur = tomlTableToEntry(existing.body);
    if (!cur || !isUnimatrixMcpEntry("unimatrix", cur)) return { warn: foreignWarning(cfg) };
    const r = removeTomlTable(raw, OWNED_TABLE);
    return r.changed ? { text: r.text } : {};
  };
  const res = updateFileSync(cfg, (raw) => orNull(compute(raw).text), { dryRun, containWithin: projectRoot });
  const d = compute(res.before);
  if (d.warn) out.warnings.push(d.warn);
  else if (d.text !== undefined) out.actions.push(pre(dryRun, "Removed unimatrix MCP entry from " + cfg));
  return out;
}

/**
 * Top-level Codex fan-out. Not detected -> one skipped-not-detected leg and no
 * file touched. Throws only SafeWriteError; warnings go to stderr once and onto leg reasons.
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
  if (ch.warning && !o.dirs) warnings.push(ch.warning); // with o.dirs, wire.js reports dirs.warnings
  try {
    if (!detectsCodex({ root: o.projectRoot, env, home, harnessSel: o.harnessSel, platform, pathLookup: o.pathLookup })) {
      legs.push(leg("codex", "skipped-not-detected", ch.dir, {
        reason: "no Codex signal (.codex/, $CODEX_HOME dir, codex on PATH, --harness codex-cli)",
      }));
    } else {
      const base = { home, env, platform, node: o.node, dryRun, dirs: o.dirs };
      legs = legs.concat(writeCodexHooks(base));
      legs.push(writeCodexMcpToml(base));
      const rh = removeOwnedCodexHooks({ home, env, dirs: o.dirs, projectRoot: o.projectRoot, dryRun });
      legs = legs.concat(rh.legs);
      const pm = removeOwnedCodexProjectMcp(o.projectRoot, dryRun, codexPaths(base).cfg);
      for (const a of pm.actions) legs.push(leg("mcp", "removed", path.join(o.projectRoot, ".codex", "config.toml"), { reason: a }));
      for (const w of pm.warnings) warnings.push(w);
    }
  } catch (e) {
    if (e instanceof SafeWriteError) throw e; // FR-31: hardened-writer failure ends the run
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
