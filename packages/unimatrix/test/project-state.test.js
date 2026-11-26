"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const os = require("os");
const path = require("path");
const ps = require("../lib/project-state.js");
const { computeProjectHash } = require("../lib/hook-client/config.js");
const fx = require("./helpers/project-state-fixtures.js");

const tmp = () => fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "ps-")));

const table = fx.rows();
for (const r of table.rows) if (r.prep) r.prep();

for (const r of table.rows) {
  test("D-045/D-052 row: " + r.name, () => {
    const p = ps.isUnimatrixProject(r.cwd, r.env, table.home);
    if (r.expectKind === null) assert.equal(p, null);
    else {
      assert.equal(p.kind, r.expectKind);
      assert.match(p.hash, /^[0-9a-f]{16}$/);
    }
    const eff = ps.effectiveEnv(r.env, r.cwd, table.home);
    const stripped = Object.keys(r.env).some(
      (k) => (k === "UNIMATRIX_BUNDLE" || k.startsWith("UNIMATRIX_REMOTE_")) && !(k in eff)
    );
    assert.equal(stripped, r.effectiveStripped);
    if (r.effectiveStripped) {
      for (const k of ["UNIMATRIX_BINARY", "UNIMATRIX_PROJECT_ROOT"]) {
        if (k in r.env) assert.equal(eff[k], r.env[k]);
      }
    }
  });
}

test("effectiveEnv does not mutate input", () => {
  const env = { UNIMATRIX_BUNDLE: "x", UNIMATRIX_BINARY: "/b" };
  const eff = ps.effectiveEnv(env, table.bare, table.home);
  assert.ok("UNIMATRIX_BUNDLE" in env);
  assert.deepEqual(eff, { UNIMATRIX_BINARY: "/b" });
});

test("hash equals Rust store dir name semantics (16 hex sha256)", () => {
  const p = ps.isUnimatrixProject(table.A, {}, table.home);
  assert.equal(p.hash, computeProjectHash(p.root));
  assert.equal(ps.storeDir("/h", "abc"), path.join("/h", ".unimatrix", "abc"));
});

test("HEADLESS_VARS excludes HEADERS", () => {
  assert.ok(ps.HEADLESS_VARS.includes("UNIMATRIX_BUNDLE"));
  assert.ok(!ps.HEADLESS_VARS.includes("UNIMATRIX_REMOTE_HEADERS"));
});

test("never throws on garbage input", () => {
  assert.equal(ps.isUnimatrixProject(undefined, undefined, undefined), null);
  assert.equal(ps.headlessEnvSet(null), false);
  assert.equal(ps.headlessRootContains({}, "/"), false);
});

// R-31: five store dirs, no credential, no install -> not installed
test("R-31 five store dirs, no credential: not installed", () => {
  const home = tmp();
  for (let i = 0; i < 5; i++) fs.mkdirSync(path.join(home, ".unimatrix", "abcdef012345678" + i), { recursive: true });
  const project = { hash: "abcdef0123456780", kind: "local" };
  for (const plat of ["linux", "darwin", "win32"]) {
    const r = ps.isInstalled(project, plat, { env: {}, home });
    assert.equal(r.installed, false);
  }
});

function validInstall(home) {
  const pkg = path.join(home, "pkg");
  fs.mkdirSync(pkg, { recursive: true });
  fs.mkdirSync(path.join(home, ".unimatrix"), { recursive: true });
  fs.writeFileSync(path.join(home, ".unimatrix", "install.json"),
    JSON.stringify({ version: "1", packageRoot: pkg, node: process.execPath }));
}

test("isInstalled linux table", () => {
  const home = tmp();
  const local = { hash: "0123456789abcdef", kind: "local" };
  let r = ps.isInstalled(local, "linux", { env: {}, home });
  assert.deepEqual(r, { installed: false, transport: null, reason: "no install.json" });
  validInstall(home);
  assert.deepEqual(ps.isInstalled(local, "linux", { env: {}, home }), { installed: true, transport: "local" });
  // credential wins -> cloud
  const sd = path.join(home, ".unimatrix", local.hash);
  fs.mkdirSync(sd, { recursive: true });
  fs.writeFileSync(path.join(sd, "remote.json"), "{}");
  assert.deepEqual(ps.isInstalled(local, "linux", { env: {}, home }), { installed: true, transport: "cloud" });
  // headless, no install.json
  const h2 = tmp();
  assert.deepEqual(ps.isInstalled({ hash: "f", kind: "headless" }, "linux", { env: { UNIMATRIX_BUNDLE: "b" }, home: h2 }),
    { installed: true, transport: "cloud" });
});

test("isInstalled linux: malformed / dangling install.json is not installed", () => {
  const home = tmp();
  fs.mkdirSync(path.join(home, ".unimatrix"), { recursive: true });
  const f = path.join(home, ".unimatrix", "install.json");
  const local = { hash: "0123456789abcdef", kind: "local" };
  fs.writeFileSync(f, "{not json");
  assert.equal(ps.isInstalled(local, "linux", { env: {}, home }).installed, false);
  fs.writeFileSync(f, JSON.stringify({ version: "1", packageRoot: path.join(home, "nope"), node: "n" }));
  assert.equal(ps.isInstalled(local, "linux", { env: {}, home }).installed, false);
  fs.writeFileSync(f, JSON.stringify({ version: 1, packageRoot: home, node: "n" }));
  assert.equal(ps.isInstalled(local, "linux", { env: {}, home }).installed, false);
});

test("isInstalled darwin/win32 table (D-032)", () => {
  const home = tmp();
  const hash = "0123456789abcdef";
  validInstall(home); // install.json irrelevant off Linux
  for (const plat of ["darwin", "win32"]) {
    assert.deepEqual(ps.isInstalled({ hash, kind: "local" }, plat, { env: {}, home }),
      { installed: false, transport: null, reason: "missing credential" });
    assert.deepEqual(ps.isInstalled({ hash, kind: "headless" }, plat, { env: { UNIMATRIX_BUNDLE: "b" }, home }),
      { installed: true, transport: "cloud" });
    // headless via REMOTE_* but no bundle: not installed off Linux
    assert.equal(ps.isInstalled({ hash, kind: "headless" }, plat, { env: { UNIMATRIX_REMOTE_URL: "u" }, home }).installed, false);
    // bundle present but project not headless (D-052): ignored
    assert.equal(ps.isInstalled({ hash, kind: "local" }, plat, { env: { UNIMATRIX_BUNDLE: "b" }, home }).installed, false);
  }
  const sd = path.join(home, ".unimatrix", hash);
  fs.mkdirSync(sd, { recursive: true });
  fs.writeFileSync(path.join(sd, "remote.json"), "{}");
  assert.deepEqual(ps.isInstalled({ hash, kind: "cloud" }, "darwin", { env: {}, home }), { installed: true, transport: "cloud" });
});

test("D-080 injected home wins over process.env.HOME decoy", () => {
  const decoy = tmp(); validInstall(decoy);
  const home = tmp();
  const saved = process.env.HOME;
  process.env.HOME = decoy;
  try {
    const r = ps.isInstalled({ hash: "0123456789abcdef", kind: "local" }, "linux", { env: {}, home });
    assert.equal(r.installed, false);
  } finally {
    if (saved === undefined) delete process.env.HOME; else process.env.HOME = saved;
  }
});

test("D-080 third argument is required (two-arg call throws)", () => {
  const p = { hash: "0123456789abcdef", kind: "local" };
  assert.throws(() => ps.isInstalled(p, "linux"));
  assert.throws(() => ps.isInstalled(p, "linux", {}));
  assert.throws(() => ps.isInstalled(p, "linux", { env: {} }));
});

test("Codex-only project (store dir, no project config) is a project (D-044/R-20)", () => {
  const home = tmp(); const cwd = tmp();
  fs.mkdirSync(path.join(cwd, ".git"));
  const hash = computeProjectHash(fs.realpathSync(cwd));
  fs.mkdirSync(path.join(home, ".unimatrix", hash), { recursive: true });
  assert.equal(ps.isUnimatrixProject(cwd, {}, home).kind, "local");
});
