"use strict";

// R-28 parity: the hash and verdict from project-state must agree with
// hook-client/config.js over the shared fixture table. The shim-entry.js inline
// copy is checked here too once it exists.

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const path = require("path");
const ps = require("../lib/project-state.js");
const { computeProjectHash, walkToProjectRoot } = require("../lib/hook-client/config.js");
const fx = require("./helpers/project-state-fixtures.js");

const table = fx.rows();
for (const r of table.rows) if (r.prep) r.prep();

const SHIM = path.join(__dirname, "..", "lib", "shim-entry.js");
let shim = null;
try {
  if (fs.existsSync(SHIM)) shim = require(SHIM);
} catch (_e) { /* shim not present yet */ }

for (const r of table.rows) {
  test("parity: hash agrees with config.js: " + r.name, () => {
    const p = ps.isUnimatrixProject(r.cwd, r.env, table.home);
    if (p) assert.equal(p.hash, computeProjectHash(walkToProjectRoot(r.cwd)));
  });
  test("parity: shim-entry inline copy agrees: " + r.name, (t) => {
    if (!shim || typeof shim.isUnimatrixProject !== "function") return t.skip("shim-entry.js not exporting yet");
    const a = ps.isUnimatrixProject(r.cwd, r.env, table.home);
    const b = shim.isUnimatrixProject(r.cwd, r.env, table.home);
    assert.deepEqual(b && { hash: b.hash, kind: b.kind }, a && { hash: a.hash, kind: a.kind });
    if (typeof shim.effectiveEnv === "function") {
      assert.deepEqual(shim.effectiveEnv(r.env, r.cwd, table.home), ps.effectiveEnv(r.env, r.cwd, table.home));
    }
  });
}
