"use strict";
// `unimatrix status [--json]` (ADR-004, D-038/D-050/D-051/D-068/D-091). Read-only,
// no network, no writes, always exit 0. Each field is collected under its own
// try/catch so a broken input degrades to that field's "unknown" value.
const fs = require("fs");
const os = require("os");
const path = require("path");
const ps = require("./project-state.js");
const own = require("./hook-ownership.js");
const trustMod = require("./trust.js");
const { walkToProjectRoot } = require("./hook-client/config.js");

const INIT_HINT = "run 'npx unimatrix init'";
const FALLBACK_APPROVAL =
  "codex: open /hooks in Codex and approve the Unimatrix hooks (Codex asks again after hooks change)";

const nz = (v) => typeof v === "string" && v.trim() !== "";
const readText = (f) => { try { return fs.readFileSync(f, "utf8"); } catch (_e) { return null; } };
const readJson = (f) => { try { return JSON.parse(fs.readFileSync(f, "utf8")); } catch (_e) { return null; } };
const safe = (fn, dflt) => { try { return fn(); } catch (_e) { return dflt; } };

function codexHome(env, home) {
  const ci = safe(() => require("./codex-install.js"), {});
  if (typeof ci.codexHome === "function") { const h = safe(() => ci.codexHome(env, home), null); if (h && h.dir) return h.dir; }
  return nz(env.CODEX_HOME) ? env.CODEX_HOME : path.join(home, ".codex");
}

function approvalStep() {
  const ci = safe(() => require("./codex-install.js"), {});
  if (typeof ci.codexApprovalStep === "function") { const s = safe(() => ci.codexApprovalStep(), null); if (s) return s; }
  return FALLBACK_APPROVAL;
}

function cloudEnvStatus(env, cwd) {
  const vars = Object.keys(env)
    .filter((k) => (k === "UNIMATRIX_BUNDLE" || k.startsWith("UNIMATRIX_REMOTE_")) && nz(env[k]))
    .sort();
  const root = nz(env.UNIMATRIX_PROJECT_ROOT) ? env.UNIMATRIX_PROJECT_ROOT : null;
  if (vars.length === 0) return { status: "unset", vars: [], reason: null, projectRoot: root };
  if (!root) return { status: "ignored", vars, reason: "project-root-unset", projectRoot: null };
  if (!ps.headlessRootContains(env, cwd)) return { status: "ignored", vars, reason: "cwd-outside-root", projectRoot: root };
  return { status: "in-effect", vars, reason: null, projectRoot: root };
}

function legacyFiles(root) {
  const out = [];
  const jsonHas = (f) => {
    const j = readJson(path.join(root, f));
    const m = j && j.mcpServers;
    return !!m && typeof m === "object" && own.isUnimatrixMcpEntry("unimatrix", m.unimatrix);
  };
  if (safe(() => jsonHas(".mcp.json"), false)) out.push(".mcp.json");
  const ci = safe(() => require("./codex-install.js"), {});
  const toml = readText(path.join(root, ".codex", "config.toml"));
  if (toml !== null && safe(() => tomlOwned(ci, toml), false)) out.push(".codex/config.toml");
  const oc = readJson(path.join(root, "opencode.json"));
  const m = oc && oc.mcp;
  if (m && typeof m === "object" && safe(() => own.isUnimatrixMcpEntry("unimatrix", ocEntry(m.unimatrix)), false)) {
    out.push("opencode.json");
  }
  return out;
}

function ocEntry(e) { return e && typeof e === "object" ? e : null; }

function tomlEntry(ci, raw) {
  const t = ci.readTomlTable(raw, "mcp_servers.unimatrix");
  if (!t || !t.present) return null;
  const cmd = /^\s*command\s*=\s*"((?:[^"\\]|\\.)*)"/m.exec(t.body);
  const am = /^\s*args\s*=\s*\[([^\]]*)\]/m.exec(t.body);
  const args = am ? (am[1].match(/"((?:[^"\\]|\\.)*)"/g) || []).map((s) => JSON.parse(s)) : [];
  return cmd ? { command: JSON.parse('"' + cmd[1] + '"'), args } : null;
}

function tomlOwned(ci, raw) {
  return own.isUnimatrixMcpEntry("unimatrix", tomlEntry(ci, raw));
}

function trustStatus(home, platform) {
  try {
    const inst = readJson(path.join(home, ".unimatrix", "install.json"));
    trustMod.checkTrust(trustMod.trustSet(home, { installJson: inst, platform }), platform, { home });
    return { ok: true };
  } catch (e) {
    return { ok: false, path: e && e.path, reason: e && e.reason };
  }
}

function mcpNode(env, home, root, platform) {
  if (platform !== "win32") return { status: "n/a", path: null };
  let entry = null;
  const cj = readJson(path.join(home, ".claude.json"));
  const projects = cj && cj.projects;
  if (projects && typeof projects === "object") {
    const key = require("./claude-json.js").resolveProjectKey(projects, root, platform);
    const e = projects[key] && projects[key].mcpServers && projects[key].mcpServers.unimatrix;
    if (own.isUnimatrixMcpEntry("unimatrix", e)) entry = e;
  }
  if (!entry) {
    const raw = readText(path.join(codexHome(env, home), "config.toml"));
    const ci = safe(() => require("./codex-install.js"), {});
    const e = raw !== null ? safe(() => tomlEntry(ci, raw), null) : null;
    if (e && own.isUnimatrixMcpEntry("unimatrix", e)) entry = e;
  }
  if (!entry || typeof entry.command !== "string") return { status: "n/a", path: null };
  const ok = (() => { try { return fs.statSync(entry.command).isFile(); } catch (_e) { return false; } })();
  return { status: ok ? "ok" : "missing", path: entry.command };
}

function hasOwnedCommand(node) {
  if (Array.isArray(node)) return node.some(hasOwnedCommand);
  if (!node || typeof node !== "object") return false;
  for (const k of Object.keys(node)) {
    if (k === "command" && typeof node[k] === "string" && own.isUnimatrixCommand(node[k])) return true;
    if (hasOwnedCommand(node[k])) return true;
  }
  return false;
}

function hookState(file, lastHook, provider) {
  const wired = hasOwnedCommand(readJson(file));
  const t = lastHook && typeof lastHook[provider] === "string" ? lastHook[provider] : null;
  return { wired, lastFired: t };
}

function collect(ctx) {
  const env = process.env;
  const home = os.homedir();
  const platform = process.platform;
  const cwd = process.cwd();
  const root = safe(() => walkToProjectRoot(cwd), cwd);
  const project = safe(() => ps.isUnimatrixProject(cwd, env, home), null);
  const d = safe(() => ps.isInstalled(project, platform, { env, home }),
    { installed: false, transport: null, reason: "unknown" });
  const hash = project ? project.hash : null;
  const lhFile = hash ? path.join(home, ".unimatrix", hash, "last-hook.json") : null;
  let lastHook = null;
  let lastHookCorrupt = false;
  if (lhFile) {
    const raw = readText(lhFile);
    if (raw !== null) {
      try { lastHook = JSON.parse(raw); } catch (_e) { lastHookCorrupt = true; }
    }
  }
  const hooks = {
    "claude-code": safe(() => hookState(path.join(root, ".claude", "settings.local.json"), lastHook, "claude-code"),
      { wired: false, lastFired: null }),
    "codex-cli": safe(() => hookState(path.join(codexHome(env, home), "hooks.json"), lastHook, "codex-cli"),
      { wired: false, lastFired: null }),
  };
  return {
    project: project ? { kind: project.kind, hash: project.hash, root: project.root } : null,
    legacyProjectMcp: safe(() => legacyFiles(root), []),
    installed: !!d.installed,
    reason: d.installed ? null : d.reason || null,
    transport: d.installed ? d.transport : null,
    cloudEnv: safe(() => cloudEnvStatus(env, cwd),
      { status: "unset", vars: [], reason: null, projectRoot: null }),
    trust: safe(() => trustStatus(home, platform), { ok: false, reason: "unknown" }),
    mcpNode: safe(() => mcpNode(env, home, root, platform), { status: "n/a", path: null }),
    hooks,
    _root: root,
    _lastHookCorrupt: lastHookCorrupt,
  };
}

function cloudLine(c) {
  if (c.status === "unset") return "cloud env: not set";
  if (c.status === "in-effect") return "cloud env: in effect";
  const names = c.vars.join(", ");
  return c.reason === "project-root-unset"
    ? `cloud env: IGNORED ${names}: UNIMATRIX_PROJECT_ROOT is unset; set it to the project root to use them`
    : `cloud env: IGNORED ${names}: cwd is not inside UNIMATRIX_PROJECT_ROOT ${c.projectRoot}; set it to the project root to use them`;
}

function hookLine(name, h, corrupt) {
  if (!h.wired) return `hook ${name}: not wired`;
  if (corrupt) return `hook ${name}: wired, last fired unknown`;
  return h.lastFired ? `hook ${name}: wired, last fired ${h.lastFired}` : `hook ${name}: wired, never fired`;
}

function renderText(i) {
  const L = [];
  L.push("project: " + (i.project
    ? (i.project.kind === "headless" ? "headless " + i.project.root : i.project.kind + " " + i.project.hash)
    : `none (this machine has no Unimatrix store or credential for ${i._root}; run 'npx unimatrix init' to attach)`));
  for (const f of i.legacyProjectMcp) {
    L.push(`legacy: ${f} has a Unimatrix MCP entry; re-run 'npx unimatrix init' to migrate`);
  }
  L.push("installed: " + (i.installed ? "yes" : "no" + (i.reason ? ` (${i.reason})` : "")));
  L.push("transport: " + (i.transport || "none"));
  L.push(cloudLine(i.cloudEnv));
  L.push("trust: " + (i.trust.ok ? "ok" : `FAIL ${i.trust.path}: ${i.trust.reason}`));
  const n = i.mcpNode;
  L.push("mcp node: " + (n.status === "ok" ? "ok" : n.status === "missing" ? `MISSING ${n.path}; ${INIT_HINT}` : "n/a"));
  L.push(hookLine("claude-code", i.hooks["claude-code"], i._lastHookCorrupt));
  const cx = i.hooks["codex-cli"];
  L.push(hookLine("codex-cli", cx, i._lastHookCorrupt));
  if (cx.wired && !cx.lastFired && !i._lastHookCorrupt) L.push(approvalStep());
  return L.join("\n") + "\n";
}

function run(args) {
  try {
    const info = collect();
    if ((args || []).includes("--json")) {
      const out = {};
      for (const k of Object.keys(info)) if (k[0] !== "_") out[k] = info[k];
      process.stdout.write(JSON.stringify(out) + "\n");
    } else {
      process.stdout.write(renderText(info));
    }
  } catch (_e) { /* status never fails */ }
  process.exitCode = 0;
}

module.exports = { run, collect, renderText, cloudEnvStatus };
