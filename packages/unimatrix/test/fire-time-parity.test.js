"use strict";
// ADR-004 fire-time order parity: shim-entry.js and the legacy PATH `unimatrix hook`
// (bin/unimatrix.js -> hook-route) must give identical exit/stdout/stderr.
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");
const { fixture, run, BIN, w } = require("./helpers/launcher-fixture.js");

const posix = process.platform === "win32" ? { skip: "posix only" } : {};
const LIB = path.join(__dirname, "..", "lib");

function viaShim(f, args) {
  const entry = path.join(f.uh, "bin", "shim-entry.js");
  const env = Object.assign({}, process.env, { HOME: f.home, USERPROFILE: f.home });
  for (const k of Object.keys(env)) if (k.startsWith("UNIMATRIX_")) delete env[k];
  env.UNIMATRIX_BINARY = f.stub;
  return spawnSync(process.execPath, [entry].concat(args), { cwd: f.proj, env, encoding: "utf8", timeout: 10000 });
}

function build(o) {
  const f = fixture(o);
  for (const n of ["shim-entry.js", "trust.js"]) w(path.join(f.uh, "bin", n), fs.readFileSync(path.join(LIB, n)));
  // Installed launcher delegates to the real one so both paths share hook-route.
  w(path.join(f.pkg, "bin", "unimatrix.js"), `module.exports = require(${JSON.stringify(BIN)});\n`);
  return f;
}

const table = [
  ["not a project", { noStore: true, noInstall: true }, null, 0, ""],
  ["project, no install", { noInstall: true }, null, 0, process.platform === "linux" ? "not attached" : ""],
  ["installed ok", {}, null, 0, ""],
  ["installed, packageRoot deleted", {}, (f) => fs.rmSync(f.pkg, { recursive: true }), 1, "trust check failed"],
  ["installed tampered", {}, (f) => fs.chmodSync(f.uh, 0o755), 1, "trust check failed"],
];

for (const [name, o, mut, code, errRe] of table) {
  test("parity: " + name, posix, () => {
    for (const args of [["hook", "Stop"], ["mcp-stdio"]]) {
      if (args[0] === "mcp-stdio") continue; // hook is the fire-time path; mcp-stdio covered in launcher tests
      const a = build(o); if (mut) mut(a);
      const b = build(o); if (mut) mut(b);
      const s = viaShim(a, args);
      const l = run(b, args, { UNIMATRIX_BINARY: b.stub });
      assert.equal(s.status, code, s.stderr);
      assert.equal(l.status, s.status, name + " exit");
      assert.equal(l.stdout, s.stdout, name + " stdout");
      const norm = (x) => x.split(b.home).join("H").split(a.home).join("H");
      assert.equal(norm(l.stderr).replace(/H\S*/g, "P"), norm(s.stderr).replace(/H\S*/g, "P"), name + " stderr");
      assert.equal(s.stdout, "");
      if (errRe) assert.match(s.stderr, new RegExp(errRe));
      else assert.equal(s.stderr, "");
    }
  });
}
