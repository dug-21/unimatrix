"use strict";
// Launcher fixtures (nan-024): isolated HOME with a trust-clean ~/.unimatrix, a stub
// client package, a stub Rust binary, and a git project; runs bin/unimatrix.js for real.
const fs = require("fs");
const os = require("os");
const path = require("path");
const { assertTmpHome, scrubbedEnv } = require("./home-guard");
const { spawnSync, spawn } = require("child_process");
const { computeProjectHash, walkToProjectRoot } = require("../../lib/hook-client/config.js");

const BIN = path.join(__dirname, "..", "..", "bin", "unimatrix.js");

function w(p, c, m) {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, c);
  fs.chmodSync(p, m || 0o644);
}

// o: {noStore, noInstall, clientMain (source of main body), stub (stub binary source)}
function fixture(o) {
  o = o || {};
  const base = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "launch-")));
  const home = assertTmpHome(path.join(base, "home"));
  const uh = path.join(home, ".unimatrix");
  const pkg = path.join(uh, "pkg", "1.0.0");
  for (const d of [uh, path.join(uh, "bin"), path.join(uh, "pkg"), pkg]) fs.mkdirSync(d, { recursive: true });
  for (const d of [uh, path.join(uh, "bin"), path.join(uh, "pkg")]) fs.chmodSync(d, 0o700);
  for (const f of ["unimatrix", "shim-entry.js", "trust.js"]) w(path.join(uh, "bin", f), "//", f === "unimatrix" ? 0o755 : 0o644);
  const marker = path.join(base, "client-ran.json");
  w(path.join(pkg, "lib", "hook-client", "index.js"),
    `exports.main = async (argv) => { ${o.clientMain || `require("fs").writeFileSync(${JSON.stringify(marker)}, JSON.stringify(argv.slice(2)))`} };\n`);
  w(path.join(pkg, "lib", "hook-client", "mcp-bridge.js"),
    `exports.main = (argv, deps) => { process.stdout.write("BRIDGE:" + argv[2]); };\n`);
  if (!o.noInstall) {
    w(path.join(uh, "install.json"), JSON.stringify({ version: "1.0.0", packageRoot: pkg, node: process.execPath }));
  }
  const proj = path.join(base, "proj");
  fs.mkdirSync(path.join(proj, ".git"), { recursive: true });
  const hash = computeProjectHash(walkToProjectRoot(proj));
  if (!o.noStore) fs.mkdirSync(path.join(uh, hash), { recursive: true });
  // D-032: macOS/Windows are cloud-only, so an "installed" project there is one holding a
  // credential (remote.json). The same installed-path assertions then run on every POSIX OS.
  if (!o.noStore && !o.noInstall && o.cred !== false && process.platform !== "linux") {
    w(path.join(uh, hash, "remote.json"), "{}", 0o600);
  }
  const stubLog = path.join(base, "stub-ran.json");
  const stub = path.join(base, "stub-bin");
  w(stub, `#!${process.execPath}\n${o.stub || `require("fs").writeFileSync(${JSON.stringify(stubLog)}, JSON.stringify({argv: process.argv.slice(2), ld: process.env.LD_LIBRARY_PATH}));`}\n`, 0o755);
  return { base, home, uh, pkg, proj, hash, marker, stub, stubLog };
}

// R-09: ambient config-dir overrides (CI sets XDG_CONFIG_HOME) are scrubbed; tests pass them explicitly under tmp.
function envFor(f, extra) {
  const env = scrubbedEnv(Object.assign({ HOME: f.home, USERPROFILE: f.home }, extra || {}));
  for (const k of Object.keys(env)) if (k.startsWith("UNIMATRIX_") && !(extra && k in extra)) delete env[k];
  return env;
}

function run(f, args, extra, opts) {
  return spawnSync(process.execPath, [BIN].concat(args), Object.assign(
    { cwd: f.proj, env: envFor(f, extra), encoding: "utf8", timeout: 10000 }, opts || {}));
}

// Run with process.platform overridden (platform matrix on any host).
function runAs(f, platform, args, extra) {
  const src = `Object.defineProperty(process,"platform",{value:${JSON.stringify(platform)}});` +
    `Promise.resolve(require(${JSON.stringify(BIN)}).run(${JSON.stringify(args)}));`;
  return spawnSync(process.execPath, ["-e", src], { cwd: f.proj, env: envFor(f, extra), encoding: "utf8", timeout: 10000 });
}

function spawnLauncher(f, args, extra) {
  return spawn(process.execPath, [BIN].concat(args), { cwd: f.proj, env: envFor(f, extra), stdio: ["pipe", "pipe", "pipe"] });
}

// Skip option for tests of the Linux-only local transport (D-032).
const { linuxLocal } = require("./linux-local");

module.exports = { linuxLocal, fixture, run, runAs, spawnLauncher, BIN, w };
