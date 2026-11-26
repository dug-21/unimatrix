"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");
const { computeProjectHash, walkToProjectRoot } = require("../lib/hook-client/config.js");

const SRC = path.join(__dirname, "..", "lib", "shim-entry.js");
const TRUST = path.join(__dirname, "..", "lib", "trust.js");
const posix = process.platform === "win32" ? { skip: "posix" } : {};

// Builds ~/.unimatrix/{bin/{shim,shim-entry.js,trust.js},install.json,pkg/1.0.0} + a project.
function fixture(o) {
  o = o || {};
  const base = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "shim-e-")));
  const home = path.join(base, "home");
  const uh = path.join(home, ".unimatrix");
  const bin = path.join(uh, "bin");
  const pkg = path.join(uh, "pkg", "1.0.0");
  for (const d of [uh, bin, path.join(uh, "pkg"), pkg, path.join(pkg, "bin")]) fs.mkdirSync(d, { recursive: true });
  for (const d of [uh, bin, path.join(uh, "pkg")]) fs.chmodSync(d, 0o700);
  const w = (p, c, m) => { fs.writeFileSync(p, c); fs.chmodSync(p, m || 0o644); };
  w(path.join(bin, "unimatrix"), "#!/bin/sh\n", 0o755);
  w(path.join(bin, "shim-entry.js"), fs.readFileSync(SRC));
  if (!o.noTrust) w(path.join(bin, "trust.js"), fs.readFileSync(TRUST));
  const out = path.join(base, "launcher-out.json");
  w(path.join(pkg, "bin", "unimatrix.js"), o.launcher || `require("fs").writeFileSync(${JSON.stringify(out)}, JSON.stringify({args:process.argv.slice(2)}));\nexports.run=(a,c)=>{require("fs").writeFileSync(${JSON.stringify(out)}, JSON.stringify({args:a,pf:Object.keys(c.preflight)}));};\n`);
  const inst = o.install === undefined ? { version: "1.0.0", packageRoot: pkg, node: process.execPath } : o.install;
  if (inst !== null) w(path.join(uh, "install.json"), typeof inst === "string" ? inst : JSON.stringify(inst));
  const proj = path.join(base, "proj");
  fs.mkdirSync(path.join(proj, ".git"), { recursive: true });
  if (!o.noStore) fs.mkdirSync(path.join(uh, computeProjectHash(walkToProjectRoot(proj))), { recursive: true });
  return { base, home, uh, bin, pkg, proj, out, entry: path.join(bin, "shim-entry.js") };
}

function exec(f, args, extra) {
  const env = Object.assign({}, process.env, { HOME: f.home, USERPROFILE: f.home }, extra || {});
  for (const k of Object.keys(env)) if (k.startsWith("UNIMATRIX_")) delete env[k];
  return spawnSync(process.execPath, [f.entry].concat(args), { cwd: f.proj, env, encoding: "utf8", timeout: 5000 });
}

test("not a project: silent exit 0 even with broken trust, install and package", posix, () => {
  const f = fixture({ noStore: true, noTrust: true, install: "{garbage" });
  fs.rmSync(f.pkg, { recursive: true });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, "");
  assert.equal(r.stderr, "");
});

test("in project: launcher receives argv, preflight passed", posix, () => {
  const f = fixture();
  const r = exec(f, ["hook", "Stop", "--provider", "codex-cli"]);
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, "");
  const got = JSON.parse(fs.readFileSync(f.out, "utf8"));
  assert.deepEqual(got.args, ["hook", "Stop", "--provider", "codex-cli"]);
  assert.deepEqual(got.pf.sort(), ["decision", "install", "project", "trustChecked"]);
});

test("mcp-stdio argv forwarded", posix, () => {
  const f = fixture();
  assert.equal(exec(f, ["mcp-stdio"]).status, 0);
  assert.deepEqual(JSON.parse(fs.readFileSync(f.out, "utf8")).args, ["mcp-stdio"]);
});

test("group-writable install.json -> exit 1 naming path", posix, () => {
  const f = fixture();
  fs.chmodSync(path.join(f.uh, "install.json"), 0o664);
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.equal(r.stdout, "");
  assert.match(r.stderr, /install\.json/);
});

test("tampered sibling trust.js -> exit 1 naming path", posix, () => {
  const f = fixture();
  fs.chmodSync(path.join(f.bin, "trust.js"), 0o666);
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /trust\.js/);
});

test("sibling trust.js absent -> exit 1 naming path in project", posix, () => {
  const f = fixture({ noTrust: true });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /trust\.js/);
});

test("self-contained: shim-entry + trust only, package dir absent -> verdict, no MODULE_NOT_FOUND", posix, () => {
  const f = fixture();
  fs.rmSync(path.join(f.uh, "pkg"), { recursive: true });
  const r = exec(f, ["hook", "Stop"]);
  assert.doesNotMatch(r.stderr, /MODULE_NOT_FOUND|Cannot find module/);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /unimatrix/);
});

test("packageRoot deleted -> exit 1 naming path", posix, () => {
  const f = fixture();
  fs.rmSync(f.pkg, { recursive: true });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /1\.0\.0|pkg/);
});

test("install.json missing (Linux project, not installed) -> hint exit 0, no trust failure; mcp-stdio exit 1", posix, () => {
  const f = fixture({ install: null });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, "");
  if (process.platform === "linux") assert.equal(r.stderr, "unimatrix: not attached on this machine; run 'npx unimatrix init'\n");
  assert.doesNotMatch(r.stderr, /trust check failed/);
  const m = exec(f, ["mcp-stdio"]);
  assert.equal(m.status, 1);
  assert.match(m.stderr, /not attached/);
});

test("installed (credential) but install.json missing -> trust refuses required member, exit 1", posix, () => {
  const f = fixture({ install: null });
  const hash = fs.readdirSync(f.uh).find((n) => /^[0-9a-f]{16}$/.test(n));
  fs.writeFileSync(path.join(f.uh, hash, "remote.json"), "{}");
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.equal(r.stdout, "");
  assert.match(r.stderr, /install\.json/);
});

test("install.json empty object -> not attached (linux hint, exit 0; mcp-stdio exit 1)", posix, () => {
  const f = fixture({ install: {} });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.stdout, "");
  assert.equal(r.status, 0);
  if (process.platform === "linux") assert.match(r.stderr, /not attached/);
  const m = exec(f, ["mcp-stdio"]);
  assert.equal(m.status, 1);
  assert.equal(m.stdout, "");
  assert.match(m.stderr, /not attached/);
});

test("launcher rejection -> exit 1, stderr, never 2, no hang", posix, () => {
  const f = fixture({ launcher: "exports.run=()=>Promise.reject(new Error('boom'));" });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /boom/);
});

test("launcher sync throw -> exit 1", posix, () => {
  const f = fixture({ launcher: "exports.run=()=>{throw new Error('sync')};" });
  const r = exec(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /sync/);
});

test("require() does not auto-run; exports present", () => {
  const m = require(SRC);
  for (const k of ["isUnimatrixProject", "effectiveEnv", "installedDecision", "run"]) assert.equal(typeof m[k], "function");
});

test("installedDecision: macOS/Windows need credential; linux needs valid install", () => {
  const { installedDecision } = require(SRC);
  const f = fixture();
  const p = { hash: "h", kind: "local" };
  assert.equal(installedDecision(p, "darwin", {}, f.home, null).installed, false);
  assert.deepEqual(installedDecision(p, "linux", {}, f.home, { version: "1", packageRoot: f.pkg, node: "n" }), { installed: true, transport: "local" });
  assert.equal(installedDecision(p, "linux", {}, f.home, { version: "1" }).installed, false);
  assert.equal(installedDecision({ hash: "h", kind: "headless" }, "win32", { UNIMATRIX_BUNDLE: "x" }, f.home, null).installed, true);
});
