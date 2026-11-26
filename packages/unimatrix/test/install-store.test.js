"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const S = require("../lib/install-store.js");
const { PLATFORMS } = require("../lib/resolve-runtime.js");

const posixOnly = process.platform === "win32" ? { skip: "posix" } : {};
const linuxOnly = process.platform === "linux" && PLATFORMS[`linux-${process.arch}`] ? {} : { skip: "linux" };
const tmp = (p) => fs.mkdtempSync(path.join(os.tmpdir(), p));

// Fixture source package (optionally with a platform binary package in node_modules).
function mkSource(version, withBinary) {
  const src = tmp("is-src-");
  fs.writeFileSync(path.join(src, "package.json"), JSON.stringify({ name: "x", version }));
  fs.mkdirSync(path.join(src, "bin"));
  fs.writeFileSync(path.join(src, "bin", "unimatrix.js"), "// v" + version);
  fs.mkdirSync(path.join(src, "lib"));
  fs.writeFileSync(path.join(src, "lib", "a.js"), "x");
  fs.mkdirSync(path.join(src, "lib", "hook-client"));
  fs.writeFileSync(path.join(src, "lib", "hook-client", "index.js"), "");
  fs.mkdirSync(path.join(src, "test"));
  fs.writeFileSync(path.join(src, "test", "t.js"), "x");
  if (withBinary) {
    const pkg = PLATFORMS[`linux-${process.arch}`];
    const d = path.join(src, "node_modules", ...pkg.split("/"));
    fs.mkdirSync(path.join(d, "bin"), { recursive: true });
    fs.writeFileSync(path.join(d, "package.json"), JSON.stringify({ name: pkg, version }));
    fs.writeFileSync(path.join(d, "bin", "unimatrix"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
    fs.writeFileSync(path.join(d, "libonnxruntime.so"), "x");
  }
  return src;
}
function mkProject() {
  const p = fs.realpathSync(tmp("is-proj-"));
  fs.mkdirSync(path.join(p, ".git"));
  return p;
}
const opts = (home, src, proj, extra) => Object.assign(
  { home, platform: process.platform, nodePath: process.execPath, sourcePackageRoot: src, projectRoot: proj }, extra);
const ij = (home) => JSON.parse(fs.readFileSync(path.join(home, ".unimatrix", "install.json"), "utf8"));

test("copy, install.json, node re-record, siblings", () => {
  const home = tmp("is-home-"), src = mkSource("1.0.0"), proj = mkProject();
  const r = S.installStore(opts(home, src, proj));
  assert.equal(r.version, "1.0.0");
  assert.equal(r.rerecordedNode, true);
  const j = ij(home);
  assert.deepEqual(Object.keys(j).sort(), ["node", "packageRoot", "version"]);
  assert.equal(j.packageRoot, path.join(home, ".unimatrix", "pkg", "1.0.0"));
  assert.ok(fs.existsSync(path.join(j.packageRoot, ".complete")));
  assert.ok(fs.existsSync(path.join(j.packageRoot, "bin", "unimatrix.js")));
  assert.ok(!fs.existsSync(path.join(j.packageRoot, "test")));
  const bin = path.join(home, ".unimatrix", "bin");
  assert.ok(fs.existsSync(r.shimPath));
  assert.deepEqual(fs.readFileSync(path.join(bin, "trust.js")), fs.readFileSync(path.join(__dirname, "..", "lib", "trust.js")));
  assert.ok(fs.existsSync(path.join(bin, "shim-entry.js")));
  assert.ok(!fs.readFileSync(r.shimPath, "utf8").includes("@@"));
  assert.equal(S.installStore(opts(home, src, proj)).rerecordedNode, false);
});

test("modes under umask 002", posixOnly, () => {
  const home = tmp("is-home-"), src = mkSource("1.0.0"), proj = mkProject();
  const old = process.umask(0o002);
  try { S.installStore(opts(home, src, proj)); } finally { process.umask(old); }
  const u = path.join(home, ".unimatrix");
  const m = (p) => fs.statSync(p).mode & 0o777;
  assert.equal(m(u), 0o700);
  assert.equal(m(path.join(u, "bin")), 0o700);
  assert.equal(m(path.join(u, "bin", "unimatrix")), 0o700);
  assert.equal(m(path.join(u, "bin", "trust.js")), 0o700);
  assert.equal(m(path.join(u, "install.json")), 0o600);
});

test("upgrade flips pointer, prunes to current+previous", () => {
  const home = tmp("is-home-"), proj = mkProject();
  for (const v of ["1.0.0", "1.1.0", "1.2.0"]) S.installStore(opts(home, mkSource(v), proj));
  assert.equal(ij(home).version, "1.2.0");
  assert.deepEqual(fs.readdirSync(path.join(home, ".unimatrix", "pkg")).sort(), ["1.1.0", "1.2.0"]);
});

test("interrupted copy leaves pointer on previous, no partial dest", () => {
  const home = tmp("is-home-"), proj = mkProject();
  S.installStore(opts(home, mkSource("1.0.0"), proj));
  const before = fs.readFileSync(path.join(home, ".unimatrix", "install.json"), "utf8");
  assert.throws(() => S.installStore(opts(home, mkSource("2.0.0"), proj, { _fault: (s) => { if (s === "copy-mid") throw new Error("boom"); } })), /boom/);
  assert.equal(fs.readFileSync(path.join(home, ".unimatrix", "install.json"), "utf8"), before);
  assert.deepEqual(fs.readdirSync(path.join(home, ".unimatrix", "pkg")), ["1.0.0"]);
});

test("interrupted trust.js / install.json write keeps previous files intact", () => {
  const home = tmp("is-home-"), proj = mkProject();
  S.installStore(opts(home, mkSource("1.0.0"), proj));
  const bin = path.join(home, ".unimatrix", "bin");
  fs.writeFileSync(path.join(bin, "trust.js"), "OLD", { mode: 0o700 });
  assert.throws(() => S.installStore(opts(home, mkSource("2.0.0"), proj, { _fault: (s) => { if (s === "sibling-trust.js") throw new Error("boom"); } })), /boom/);
  assert.equal(fs.readFileSync(path.join(bin, "trust.js"), "utf8"), "OLD");
  assert.equal(ij(home).version, "1.0.0");
  assert.deepEqual(fs.readdirSync(bin).filter((n) => n.includes(".tmp-")), []);
});

test("D-082: relative or missing node refused, nothing written, also dry-run", () => {
  for (const dryRun of [false, true]) {
    for (const nodePath of ["node", path.join(os.tmpdir(), "no-such-node-xyz")]) {
      const home = tmp("is-home-");
      assert.throws(() => S.installStore(opts(home, mkSource("1.0.0"), mkProject(), { nodePath, dryRun })), S.NodeNotRecordableError);
      assert.equal(fs.existsSync(path.join(home, ".unimatrix")), false);
    }
  }
});

test("unsafe shim path refused, nothing written", () => {
  assert.throws(() => S.checkShimPathSafe("/a'b/x", "linux"), S.UnsafeShimPathError);
  assert.throws(() => S.checkShimPathSafe("C:\\a%b\\x", "win32"), S.UnsafeShimPathError);
  assert.throws(() => S.checkShimPathSafe('C:\\a"b\\x', "win32"), S.UnsafeShimPathError);
  S.checkShimPathSafe("/a%b/x", "linux");
  const base = tmp("is-q'-");
  assert.throws(() => S.installStore(opts(base, mkSource("1.0.0"), mkProject())), S.UnsafeShimPathError);
  assert.equal(fs.existsSync(path.join(base, ".unimatrix")), false);
});

test("root marker: one line, hash dir 0700", () => {
  const home = tmp("is-home-"), proj = mkProject();
  S.installStore(opts(home, mkSource("1.0.0"), proj));
  const { computeProjectHash } = require("../lib/hook-client/config.js");
  const dir = path.join(home, ".unimatrix", computeProjectHash(proj));
  assert.equal(fs.readFileSync(path.join(dir, "root"), "utf8"), proj + "\n");
  if (process.platform !== "win32") assert.equal(fs.statSync(dir).mode & 0o777, 0o700);
});

test("dry-run action list equals real-run files and writes nothing", () => {
  const home = tmp("is-home-"), src = mkSource("1.0.0"), proj = mkProject();
  const d = S.installStore(opts(home, src, proj, { dryRun: true }));
  assert.equal(fs.existsSync(path.join(home, ".unimatrix")), false);
  const r = S.installStore(opts(home, src, proj));
  const strip = (a) => a.map((x) => x.replace(/^(\[dry-run\] would write|wrote) /, ""));
  assert.deepEqual(strip(d.actions), strip(r.actions));
});

test("shim path and hook string unchanged across upgrade", () => {
  const home = tmp("is-home-"), proj = mkProject();
  const a = S.installStore(opts(home, mkSource("1.0.0"), proj));
  const b = S.installStore(opts(home, mkSource("2.0.0"), proj));
  assert.equal(a.shimPath, b.shimPath);
});

test("D-078: binary package copied; survives source removal", linuxOnly, () => {
  const home = tmp("is-home-"), src = mkSource("1.0.0", true), proj = mkProject();
  S.installStore(opts(home, src, proj, { requireBinary: true }));
  const root = ij(home).packageRoot;
  const pkg = PLATFORMS[`linux-${process.arch}`];
  const bin = path.join(root, "node_modules", ...pkg.split("/"), "bin", "unimatrix");
  assert.ok(fs.statSync(bin).mode & 0o100);
  fs.rmSync(src, { recursive: true, force: true });
  const { resolveRuntime } = require("../lib/resolve-runtime.js");
  const r = resolveRuntime({ transport: "local", home, platform: "linux", arch: process.arch, env: {} });
  assert.equal(r.binarySource, "install");
  assert.equal(r.binary, fs.realpathSync(bin));
});

test("binary package absent: optional skip, required throws naming it", linuxOnly, () => {
  const home = tmp("is-home-"), proj = mkProject();
  S.installStore(opts(home, mkSource("1.0.0"), proj));
  assert.ok(!fs.existsSync(path.join(ij(home).packageRoot, "node_modules")));
  assert.throws(() => S.installStore(opts(tmp("is-home-"), mkSource("1.0.0"), proj, { requireBinary: true })), /unimatrix-linux/);
});

test("non-linux: no binary copy, no error", () => {
  const home = tmp("is-home-");
  S.installStore(opts(home, mkSource("1.0.0", process.platform === "linux"), mkProject(), { platform: process.platform === "win32" ? "win32" : process.platform, arch: "x64" }));
});

test("two concurrent inits give one consistent install.json", posixOnly, () => {
  const home = tmp("is-home-"), src = mkSource("1.0.0"), proj = mkProject();
  const code = `require(${JSON.stringify(path.join(__dirname, "..", "lib", "install-store.js"))}).installStore(${JSON.stringify(opts(home, src, proj))})`;
  const { spawn } = require("node:child_process");
  const run = () => new Promise((res) => spawn(process.execPath, ["-e", code], { stdio: "inherit" }).on("exit", res));
  return Promise.all([run(), run()]).then((codes) => {
    assert.deepEqual(codes, [0, 0]);
    assert.equal(ij(home).version, "1.0.0");
  });
});

test("stress: 12 concurrent inits + reader never observe a missing pkg dir", posixOnly, () => {
  const home = tmp("is-home-"), src = mkSource("1.0.0"), proj = mkProject();
  const code = `require(${JSON.stringify(path.join(__dirname, "..", "lib", "install-store.js"))}).installStore(${JSON.stringify(opts(home, src, proj))})`;
  const { spawn } = require("node:child_process");
  const run = () => new Promise((res) => spawn(process.execPath, ["-e", code], { stdio: "inherit" }).on("exit", res));
  let done = false, bad = 0, seen = 0;
  const reader = new Promise((res) => {
    const tick = () => {
      const j = S.readInstallJson(home);
      if (j && j.packageRoot) {
        seen++;
        if (!fs.existsSync(path.join(j.packageRoot, ".complete"))) bad++;
      }
      done ? res() : setImmediate(tick);
    };
    tick();
  });
  return Promise.all(Array.from({ length: 12 }, run)).then((codes) => {
    done = true;
    return reader.then(() => {
      assert.deepEqual(codes, new Array(12).fill(0));
      assert.equal(bad, 0);
      assert.ok(seen > 0);
      assert.equal(ij(home).version, "1.0.0");
    });
  });
});
