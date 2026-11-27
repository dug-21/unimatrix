"use strict";
// `unimatrix status` (nan-024 ADR-004, D-038/D-050/D-051/D-068): read-only, exit 0 always.
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const path = require("path");
const { fixture, run, runAs, w } = require("./helpers/launcher-fixture.js");

const posix = process.platform === "win32" ? { skip: "posix only" } : {};
// D-032: macOS/Windows are cloud-only; the fixture's "installed" project holds a credential there.
const KIND = process.platform === "linux" ? "local" : "cloud";
const OWNED_MCP = { command: "unimatrix", args: ["serve"] };

function snapshot(dir) {
  const out = {};
  (function walk(d) {
    for (const n of fs.readdirSync(d)) {
      const p = path.join(d, n);
      const st = fs.lstatSync(p);
      if (st.isDirectory()) walk(p); else out[p] = st.mtimeMs + ":" + st.size;
    }
  })(dir);
  return out;
}

test("status: installed local project, field order exact, exit 0, no writes", posix, () => {
  const f = fixture();
  const before = snapshot(f.base);
  const r = run(f, ["status"]);
  assert.equal(r.status, 0, r.stderr);
  const keys = r.stdout.split("\n").filter(Boolean).map((l) => l.split(":")[0]);
  assert.deepEqual(keys, ["project", "installed", "transport", "cloud env", "trust", "mcp node", "hook claude-code", "hook codex-cli",
    "config claude-code mcp", "config claude-code user hooks", "config codex-cli", "config opencode mcp", "mcp claude-code"]);
  assert.match(r.stdout, new RegExp("^project: " + KIND + " [0-9a-f]{16}$", "m"));
  assert.match(r.stdout, /^installed: yes$/m);
  assert.match(r.stdout, new RegExp("^transport: " + KIND + "$", "m"));
  assert.match(r.stdout, /^trust: ok$/m);
  assert.match(r.stdout, /^mcp node: n\/a$/m);
  assert.match(r.stdout, /^hook claude-code: not wired$/m);
  assert.deepEqual(snapshot(f.base), before);
});

test("status: cloned repo (no store) -> project: none (...), installed no, exit 0", posix, () => {
  const f = fixture({ noStore: true, noInstall: true });
  const r = run(f, ["status"]);
  assert.equal(r.status, 0);
  assert.ok(r.stdout.includes(`project: none (this machine has no Unimatrix store or credential for ${f.proj}; run 'npx unimatrix init' to attach)`));
  assert.match(r.stdout, process.platform === "linux" ? /^installed: no \(no install.json\)$/m : /^installed: no \(missing credential\)$/m);
  assert.match(r.stdout, /^transport: none$/m);
});

test("status: macOS/Windows missing credential reason", posix, () => {
  const f = fixture({ noInstall: true });
  const r = runAs(f, "darwin", ["status"]);
  assert.equal(r.status, 0);
  assert.match(r.stdout, /^installed: no \(missing credential\)$/m);
});

test("status: leftover owned entries -> legacy lines in order; JSON legacyProjectMcp same order", posix, () => {
  const f = fixture();
  w(path.join(f.proj, "opencode.json"), JSON.stringify({ mcp: { unimatrix: { type: "local", command: ["unimatrix", "serve"] } } }));
  w(path.join(f.proj, ".mcp.json"), JSON.stringify({ mcpServers: { unimatrix: OWNED_MCP } }));
  w(path.join(f.proj, ".codex", "config.toml"), '[mcp_servers.unimatrix]\ncommand = "unimatrix"\nargs = ["serve"]\n');
  const r = run(f, ["status"]);
  const legacy = r.stdout.split("\n").filter((l) => l.startsWith("legacy:"));
  assert.deepEqual(legacy, [".mcp.json", ".codex/config.toml", "opencode.json"].map(
    (x) => `legacy: ${x} has a Unimatrix MCP entry; re-run 'npx unimatrix init' to migrate`));
  assert.deepEqual(JSON.parse(run(f, ["status", "--json"]).stdout).legacyProjectMcp, [".mcp.json", ".codex/config.toml", "opencode.json"]);
  // foreign entry is not listed
  w(path.join(f.proj, ".mcp.json"), JSON.stringify({ mcpServers: { unimatrix: { command: "other" } } }));
  assert.ok(!run(f, ["status"]).stdout.includes("legacy: .mcp.json"));
});

test("status: trust failure surfaces as FAIL line, still exit 0", posix, () => {
  const f = fixture();
  fs.chmodSync(f.uh, 0o755);
  const r = run(f, ["status"]);
  assert.equal(r.status, 0);
  assert.ok(r.stdout.includes("trust: FAIL " + f.uh + ": directory mode must be 0700"));
});

test("status: codex hooks wired never fired -> approval step; with last-hook -> time; corrupt -> unknown", posix, () => {
  const f = fixture();
  const codex = path.join(f.base, "codexhome");
  w(path.join(codex, "hooks.json"), JSON.stringify({ hooks: { Stop: [{ hooks: [{ type: "command", command: "unimatrix hook Stop --provider codex-cli" }] }] } }));
  let r = run(f, ["status"], { CODEX_HOME: codex });
  assert.match(r.stdout, /^hook codex-cli: wired, never fired$/m);
  assert.match(r.stdout, /\/hooks/);
  w(path.join(f.uh, f.hash, "last-hook.json"), JSON.stringify({ "codex-cli": "2026-01-02T03:04:05.000Z" }));
  r = run(f, ["status"], { CODEX_HOME: codex });
  assert.match(r.stdout, /^hook codex-cli: wired, last fired 2026-01-02T03:04:05.000Z$/m);
  w(path.join(f.uh, f.hash, "last-hook.json"), "{corrupt");
  r = run(f, ["status"], { CODEX_HOME: codex });
  assert.equal(r.status, 0);
  assert.match(r.stdout, /^hook codex-cli: wired, last fired unknown$/m);
});

test("status: claude-code hooks wired in settings.local.json", posix, () => {
  const f = fixture();
  w(path.join(f.proj, ".claude", "settings.local.json"), JSON.stringify({ hooks: { Stop: [{ hooks: [{ command: "unimatrix hook Stop" }] }] } }));
  assert.match(run(f, ["status"]).stdout, /^hook claude-code: wired, never fired$/m);
});

test("status: Windows fallback node missing -> mcp node: MISSING", posix, () => {
  const f = fixture();
  const missing = path.join(f.base, "no-node");
  const entry = { command: missing, args: [path.join(f.home, ".unimatrix", "bin", "shim-entry.js"), "mcp-stdio"] };
  const projects = {};
  projects[f.proj] = { mcpServers: { unimatrix: entry } };
  // Pretend-win32 resolver builds path.win32.join(<posix home>, ".claude.json"): on a POSIX host
  // that is a backslash-named file relative to cwd (f.proj), so place the fixture there.
  w(path.join(f.proj, path.win32.join(f.home, ".claude.json")), JSON.stringify({ projects }));
  const r = runAs(f, "win32", ["status"]);
  assert.equal(r.status, 0, r.stderr);
  assert.ok(r.stdout.includes(`mcp node: MISSING ${missing}; run 'npx unimatrix init'`));
});

test("status: everything broken still exits 0", posix, () => {
  const f = fixture({ noInstall: true, noStore: true });
  fs.writeFileSync(path.join(f.proj, ".mcp.json"), "{garbage");
  fs.chmodSync(f.uh, 0o777);
  const r = run(f, ["status", "--json"]);
  assert.equal(r.status, 0);
  assert.ok(JSON.parse(r.stdout));
});

test("status --json: key order and shape", posix, () => {
  const f = fixture();
  const j = JSON.parse(run(f, ["status", "--json"]).stdout);
  assert.deepEqual(Object.keys(j), ["project", "legacyProjectMcp", "installed", "reason", "transport", "cloudEnv", "trust", "mcpNode", "hooks",
    "configDirs", "claudeMcp", "opencodeConfig", "staleLegacy", "alternateProfiles"]);
  assert.deepEqual(Object.keys(j.hooks), ["claude-code", "codex-cli"]);
  assert.equal(j.transport, KIND);
});

// ---- cloud env (D-091) ----
const CANARY = "secret-token-xyz";
const cloud = (f, env) => ({ text: run(f, ["status"], env), json: run(f, ["status", "--json"], env) });

test("status cloud env: not set", posix, () => {
  const f = fixture();
  const c = cloud(f, {});
  assert.match(c.text.stdout, /^cloud env: not set$/m);
  assert.equal(JSON.parse(c.json.stdout).cloudEnv.status, "unset");
  assert.deepEqual(JSON.parse(c.json.stdout).cloudEnv.vars, []);
});

test("status cloud env: in effect (cwd inside root), names sorted, values never printed", posix, () => {
  const f = fixture();
  const env = { UNIMATRIX_REMOTE_TOKEN: CANARY, UNIMATRIX_BUNDLE: CANARY, UNIMATRIX_PROJECT_ROOT: f.proj };
  const c = cloud(f, env);
  assert.match(c.text.stdout, /^cloud env: in effect$/m);
  const j = JSON.parse(c.json.stdout);
  assert.deepEqual(j.cloudEnv, { status: "in-effect", vars: ["UNIMATRIX_BUNDLE", "UNIMATRIX_REMOTE_TOKEN"], reason: null, projectRoot: f.proj });
  for (const o of [c.text, c.json]) { assert.equal(o.status, 0); assert.ok(!(o.stdout + o.stderr).includes(CANARY)); }
});

test("status cloud env: IGNORED, root unset", posix, () => {
  const f = fixture();
  const c = cloud(f, { UNIMATRIX_REMOTE_TOKEN: CANARY, UNIMATRIX_BUNDLE: CANARY });
  assert.ok(c.text.stdout.includes("cloud env: IGNORED UNIMATRIX_BUNDLE, UNIMATRIX_REMOTE_TOKEN: UNIMATRIX_PROJECT_ROOT is unset; set it to the project root to use them"));
  const j = JSON.parse(c.json.stdout);
  assert.equal(j.cloudEnv.status, "ignored");
  assert.equal(j.cloudEnv.reason, "project-root-unset");
  assert.equal(j.cloudEnv.projectRoot, null);
  for (const o of [c.text, c.json]) assert.ok(!(o.stdout + o.stderr).includes(CANARY));
});

test("status cloud env: IGNORED, cwd outside root; key order transport < cloudEnv < trust", posix, () => {
  const f = fixture();
  const root = path.join(f.base, "elsewhere");
  fs.mkdirSync(root);
  const c = cloud(f, { UNIMATRIX_REMOTE_MCP_URL: CANARY, UNIMATRIX_PROJECT_ROOT: root });
  assert.ok(c.text.stdout.includes(`cloud env: IGNORED UNIMATRIX_REMOTE_MCP_URL: cwd is not inside UNIMATRIX_PROJECT_ROOT ${root}; set it to the project root to use them`));
  const lines = c.text.stdout.split("\n");
  assert.ok(lines[lines.findIndex((l) => l.startsWith("transport:")) + 1].startsWith("cloud env:"));
  const j = JSON.parse(c.json.stdout);
  assert.deepEqual(j.cloudEnv, { status: "ignored", vars: ["UNIMATRIX_REMOTE_MCP_URL"], reason: "cwd-outside-root", projectRoot: root });
  const k = Object.keys(j);
  assert.ok(k.indexOf("transport") < k.indexOf("cloudEnv") && k.indexOf("cloudEnv") < k.indexOf("trust"));
  assert.ok(!(c.text.stdout + c.json.stdout).includes(CANARY));
});
