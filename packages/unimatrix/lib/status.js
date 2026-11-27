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
const { resolveConfigDirs, sameRealFile } = require("./config-dirs.js");
const cj = require("./claude-json.js");

const INIT_HINT = "run 'npx unimatrix init'";
const FALLBACK_APPROVAL =
  "codex: open /hooks in Codex and approve the Unimatrix hooks (Codex asks again after hooks change)";

const nz = (v) => typeof v === "string" && v.trim() !== "";
const readText = (f) => { try { return fs.readFileSync(f, "utf8"); } catch (_e) { return null; } };
const readJson = (f) => { try { return JSON.parse(fs.readFileSync(f, "utf8")); } catch (_e) { return null; } };
const safe = (fn, dflt) => { try { return fn(); } catch (_e) { return dflt; } };

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

function mcpNode(dirs, root, platform) {
  if (platform !== "win32") return { status: "n/a", path: null };
  let entry = null;
  const j = readJson(dirs.claude.claudeJson);
  const projects = j && j.projects;
  if (projects && typeof projects === "object") {
    const key = cj.resolveProjectKey(projects, root, platform);
    const e = projects[key] && projects[key].mcpServers && projects[key].mcpServers.unimatrix;
    if (own.isUnimatrixMcpEntry("unimatrix", e)) entry = e;
  }
  if (!entry) {
    const raw = readText(dirs.codex.configToml);
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

const owned = (e) => safe(() => own.isUnimatrixMcpEntry("unimatrix", ocEntry(e)), false);
const get = (o, ks) => ks.reduce((a, k) => (a && typeof a === "object" ? a[k] : undefined), o);

function readJsonOr(f) {
  const raw = readText(f);
  if (raw === null) return { absent: true };
  try { return { json: JSON.parse(raw), raw }; } catch (_e) { return { unreadable: true, raw }; }
}

function ocState(file) {
  const r = readJsonOr(file);
  return { status: r.absent ? "absent" : r.unreadable ? "unreadable" : "ok", file };
}

function findStale(dirs, platform) {
  const out = [];
  const legs = [
    ["claude-code", dirs.claude, dirs.legacy.userSettings, dirs.claude.userSettings, "hooks", (j) => hasOwnedCommand(j && j.hooks)],
    ["opencode", dirs.opencode, dirs.legacy.opencodeJson, dirs.opencode.configJson, "mcp.unimatrix", (j) => owned(get(j, ["mcp", "unimatrix"]))],
  ];
  for (const [harness, d, file, live, key, test] of legs) {
    if (d.source === "default" || sameRealFile(file, live, platform)) continue;
    const r = readJsonOr(file);
    if (r.unreadable) out.push({ harness, file, key: null, unreadable: true });
    else if (r.json && test(r.json)) out.push({ harness, file, key });
  }
  return out;
}

function findAlternates(dirs, root, platform) {
  const out = [];
  const P = platform === "win32" ? path.win32 : path.posix;
  for (const a of dirs.alternates) {
    const dest = a.harness === "claude-code" ? dirs.claude.claudeJson
      : a.harness === "opencode" ? dirs.opencode.configJson : P.join(dirs.codex.dir, P.basename(a.file));
    if (sameRealFile(a.file, dest, platform)) continue;
    const hit = { harness: a.harness, file: a.file, key: a.key, var: a.var };
    if (a.harness === "codex-cli" && a.key !== "hooks") {
      const raw = readText(a.file);
      const ci = safe(() => require("./codex-install.js"), {});
      if (raw !== null && safe(() => tomlOwned(ci, raw), false)) out.push(hit);
      continue;
    }
    const r = readJsonOr(a.file);
    if (r.absent) continue;
    if (r.unreadable) {
      if (!out.some((x) => x.file === a.file && x.unreadable)) out.push(Object.assign(hit, { key: null, unreadable: true }));
      continue;
    }
    const j = r.json;
    if (a.harness === "claude-code" && a.key.startsWith("projects")) {
      const k = cj.resolveProjectKey(j && j.projects, root, platform);
      if (owned(get(j, ["projects", k, "mcpServers", "unimatrix"]))) out.push(Object.assign(hit, { key: `projects["${k}"].mcpServers.unimatrix` }));
    } else if (a.harness === "claude-code") {
      if (owned(get(j, ["mcpServers", "unimatrix"]))) out.push(hit);
    } else if (a.harness === "opencode") {
      if (owned(get(j, ["mcp", "unimatrix"]))) out.push(hit);
    } else if (hasOwnedCommand(j)) out.push(hit);
  }
  return out;
}

function configDirsOf(d) {
  return {
    claude: { claudeJson: d.claude.claudeJson, userSettings: d.claude.userSettings, source: d.claude.source },
    opencode: { configJson: d.opencode.configJson, source: d.opencode.source },
    codex: { hooksJson: d.codex.hooksJson, configToml: d.codex.configToml, source: d.codex.source },
    warnings: d.warnings.slice(),
  };
}

function collect(ctx) {
  const c = ctx || {};
  const env = c.env || process.env;
  const home = c.home || os.homedir();
  const platform = c.platform || process.platform;
  const cwd = c.cwd || process.cwd();
  const dirs = resolveConfigDirs({ env, home, platform });
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
    "codex-cli": safe(() => hookState(dirs.codex.hooksJson, lastHook, "codex-cli"),
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
    mcpNode: safe(() => mcpNode(dirs, root, platform), { status: "n/a", path: null }),
    hooks,
    configDirs: configDirsOf(dirs),
    claudeMcp: project
      ? safe(() => cj.readClaudeMcpState(dirs.claude.claudeJson, root, platform),
        { status: "unreadable", file: dirs.claude.claudeJson, key: null })
      : { status: "n/a", file: null, key: null },
    opencodeConfig: safe(() => ocState(dirs.opencode.configJson), { status: "unreadable", file: dirs.opencode.configJson }),
    staleLegacy: safe(() => findStale(dirs, platform), []),
    alternateProfiles: safe(() => findAlternates(dirs, root, platform), []),
    _dirs: dirs,
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

function claudeMcpLine(m) {
  const at = `projects["${m.key}"].mcpServers.unimatrix`;
  if (m.status === "wired") return `mcp claude-code: wired (${m.file})`;
  if (m.status === "missing") return `mcp claude-code: MISSING ${at} in ${m.file}; ${INIT_HINT}`;
  if (m.status === "foreign") return `mcp claude-code: foreign entry at ${at} in ${m.file}; not managed by Unimatrix`;
  if (m.status === "unreadable") return `mcp claude-code: unreadable (${m.file} is not valid JSON)`;
  return "mcp claude-code: n/a";
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
  const cd = i.configDirs;
  const src = (h) => (i._dirs[h].source === "default" ? "default" : `${i._dirs[h].source}=${i._dirs[h].value}`);
  L.push(`config claude-code mcp: ${cd.claude.claudeJson} (${src("claude")})`);
  L.push(`config claude-code user hooks: ${cd.claude.userSettings} (${src("claude")})`);
  L.push(`config codex-cli: ${cd.codex.hooksJson}, ${cd.codex.configToml} (${src("codex")})`);
  L.push(`config opencode mcp: ${cd.opencode.configJson} (${src("opencode")})`);
  for (const w of cd.warnings) L.push("warning: " + w);
  L.push(claudeMcpLine(i.claudeMcp));
  if (i.opencodeConfig.status === "unreadable") {
    L.push(`opencode mcp: ${i.opencodeConfig.file} is not valid JSON; OpenCode is left without Unimatrix wiring until it is fixed`);
  }
  for (const s of i.staleLegacy) {
    L.push(s.unreadable ? `stale: ${s.file} unreadable (not valid JSON)`
      : `stale: ${s.file} ${s.key} holds an owned Unimatrix entry the harness no longer reads; re-run 'npx unimatrix init' to remove it`);
  }
  for (const a of i.alternateProfiles) {
    L.push(a.unreadable ? `other profile: ${a.file} unreadable (not valid JSON)`
      : `other profile: ${a.file} ${a.key} (${a.var} points elsewhere; separate ${a.harness} config profile, init leaves it in place)`);
  }
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
