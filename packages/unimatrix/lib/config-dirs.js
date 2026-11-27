"use strict";
/**
 * C1 config-dir resolver (1036, ADR-002 #5960): the only builder of harness
 * config paths. Pure over injected {env, home, platform}; never reads
 * process.env, never mutates env, never throws.
 */
const fs = require("node:fs");
const path = require("node:path");

const VARS = ["CLAUDE_CONFIG_DIR", "OPENCODE_CONFIG_DIR", "XDG_CONFIG_HOME", "CODEX_HOME"];
const CODEX_LEGACY_WARNING =
  "unimatrix: CODEX_HOME is not an absolute path; ignoring it and using ~/.codex";

function pathMod(platform) {
  return platform === "win32" ? path.win32 : path.posix;
}

function stripTrailingSep(v, platform) {
  const P = pathMod(platform);
  const root = P.parse(v).root;
  let s = v;
  while (s.length > root.length && /[\\/]$/.test(s) && (platform === "win32" || s.endsWith("/"))) {
    s = s.slice(0, -1);
  }
  return s;
}

function getVar(env, name) {
  try {
    const v = env ? env[name] : undefined;
    return typeof v === "string" && v !== "" ? v : null;
  } catch {
    return null;
  }
}

function validVar(env, name, platform) {
  const v = getVar(env, name);
  if (v === null) return { value: null, bad: false };
  if (v.includes("\0") || v.trim() === "" || !pathMod(platform).isAbsolute(v)) {
    return { value: null, bad: true, raw: v };
  }
  return { value: stripTrailingSep(v, platform), bad: false };
}

function readVar(env, name, platform, fallbackLabel, warnings) {
  const r = validVar(env, name, platform);
  if (r.bad) {
    warnings.push(
      `unimatrix: ${name} is not an absolute path (${JSON.stringify(r.raw)}); ignoring it and using ${fallbackLabel}`,
    );
  }
  return r.value;
}

/** D-081/D-085: Codex home; bad $CODEX_HOME ignored with the legacy warning text (FR-05). */
function codexHome(env, home, platform) {
  const plat = platform || process.platform;
  const P = pathMod(plat);
  const r = validVar(env, "CODEX_HOME", plat);
  if (r.value) return { dir: r.value };
  if (r.bad) return { dir: P.join(home, ".codex"), warning: CODEX_LEGACY_WARNING };
  return { dir: P.join(home, ".codex") };
}

function resolveConfigDirs(opts) {
  const o = opts || {};
  const env = o.env || {};
  const home = typeof o.home === "string" ? o.home : "";
  const platform = o.platform || process.platform;
  const P = pathMod(platform);
  const warnings = [];

  const cd = readVar(env, "CLAUDE_CONFIG_DIR", platform,
    "~/.claude.json and ~/.claude/settings.json", warnings);
  const claude = cd
    ? { dir: cd, claudeJson: P.join(cd, ".claude.json"), userSettings: P.join(cd, "settings.json"),
        source: "CLAUDE_CONFIG_DIR", value: cd }
    : { dir: P.join(home, ".claude"), claudeJson: P.join(home, ".claude.json"),
        userSettings: P.join(home, ".claude", "settings.json"), source: "default", value: null };

  const xdgPeek = validVar(env, "XDG_CONFIG_HOME", platform).value;
  const nextLabel = xdgPeek ? "$XDG_CONFIG_HOME/opencode" : "~/.config/opencode";
  const od = readVar(env, "OPENCODE_CONFIG_DIR", platform, nextLabel, warnings);
  const xd = readVar(env, "XDG_CONFIG_HOME", platform, "~/.config/opencode", warnings);
  let opencode;
  if (od) opencode = { dir: od, source: "OPENCODE_CONFIG_DIR", value: od };
  else if (xd) opencode = { dir: P.join(xd, "opencode"), source: "XDG_CONFIG_HOME", value: xd };
  else opencode = { dir: P.join(home, ".config", "opencode"), source: "default", value: null };
  opencode.configJson = P.join(opencode.dir, "opencode.json");

  const ch = codexHome(env, home, platform);
  if (ch.warning) warnings.push(ch.warning);
  const cv = !ch.warning && getVar(env, "CODEX_HOME") !== null ? ch.dir : null;
  const codex = {
    dir: ch.dir, hooksJson: P.join(ch.dir, "hooks.json"), configToml: P.join(ch.dir, "config.toml"),
    source: cv ? "CODEX_HOME" : "default", value: cv,
  };

  const legacy = {
    userSettings: P.join(home, ".claude", "settings.json"),
    opencodeJson: P.join(home, ".config", "opencode", "opencode.json"),
  };

  const alternates = [];
  if (claude.source !== "default") {
    const f = P.join(home, ".claude.json");
    alternates.push({ harness: "claude-code", file: f, key: "projects[<root>].mcpServers.unimatrix", var: "CLAUDE_CONFIG_DIR" });
    alternates.push({ harness: "claude-code", file: f, key: "mcpServers.unimatrix", var: "CLAUDE_CONFIG_DIR" });
  }
  if (od && xd) {
    alternates.push({ harness: "opencode", file: P.join(xd, "opencode", "opencode.json"), key: "mcp.unimatrix", var: "OPENCODE_CONFIG_DIR" });
  }
  if (codex.source === "CODEX_HOME") {
    alternates.push({ harness: "codex-cli", file: P.join(home, ".codex", "hooks.json"), key: "hooks", var: "CODEX_HOME" });
    alternates.push({ harness: "codex-cli", file: P.join(home, ".codex", "config.toml"), key: "mcp_servers.unimatrix", var: "CODEX_HOME" });
  }
  return { claude, opencode, codex, legacy, alternates, warnings };
}

/** Realpath of the deepest existing ancestor plus the remaining components (FR-12). */
function realPathForm(p) {
  let start;
  try {
    start = path.resolve(String(p));
  } catch {
    return String(p);
  }
  const rest = [];
  let cur = start;
  for (;;) {
    try {
      return path.join(fs.realpathSync.native(cur), ...rest.reverse());
    } catch (e) {
      if (!e || (e.code !== "ENOENT" && e.code !== "ENOTDIR")) return start;
      rest.push(path.basename(cur));
      const parent = path.dirname(cur);
      if (parent === cur) return start;
      cur = parent;
    }
  }
}

function statOrNull(p) {
  try {
    return fs.statSync(p);
  } catch {
    return null;
  }
}

function norm(p, platform) {
  if (platform !== "win32") return p.length > 1 ? p.replace(/\/+$/, "") : p;
  return p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

/** D-007: true when a and b are the same real file (existing or not). */
function sameRealFile(a, b, platform) {
  try {
    const plat = platform || process.platform;
    const sa = statOrNull(a);
    const sb = statOrNull(b);
    if (sa && sb) return sa.dev === sb.dev && sa.ino === sb.ino;
    if (plat === "win32" && plat !== process.platform) {
      return norm(path.win32.resolve(a), plat) === norm(path.win32.resolve(b), plat);
    }
    return norm(realPathForm(a), plat) === norm(realPathForm(b), plat);
  } catch {
    return false;
  }
}

module.exports = { resolveConfigDirs, sameRealFile, codexHome, realPathForm, VARS };
