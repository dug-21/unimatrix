"use strict";
const { describe, it } = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { assertTmpHome, scrubbedEnv, OVERRIDE_VARS } = require("./helpers/home-guard");

describe("NFR-05 home guard", () => {
  it("accepts a dir under os.tmpdir()", () => {
    const d = fs.mkdtempSync(path.join(os.tmpdir(), "home-guard-"));
    try { assert.strictEqual(assertTmpHome(d), d); } finally { fs.rmSync(d, { recursive: true, force: true }); }
  });
  it("throws for the real home", () => {
    const real = os.homedir();
    const tmp = fs.realpathSync.native(os.tmpdir());
    const rh = fs.realpathSync.native(real);
    if (rh === tmp || rh.startsWith(tmp + path.sep)) return; // real home lives under tmp: not distinguishable here
    assert.throws(() => assertTmpHome(real), /NFR-05/);
  });
  it("throws for a path outside tmpdir and for empty", () => {
    assert.throws(() => assertTmpHome(path.parse(process.cwd()).root), /NFR-05/);
    assert.throws(() => assertTmpHome(""), /NFR-05/);
    assert.throws(() => assertTmpHome(undefined), /NFR-05/);
  });
  it("scrubbedEnv strips all override vars even when process.env sets them", () => {
    const saved = {};
    for (const v of OVERRIDE_VARS) { saved[v] = process.env[v]; process.env[v] = "/tmp-probe"; }
    try {
      const env = scrubbedEnv();
      for (const v of OVERRIDE_VARS) assert.strictEqual(v in env, false, v);
    } finally {
      for (const v of OVERRIDE_VARS) {
        if (saved[v] === undefined) delete process.env[v]; else process.env[v] = saved[v];
      }
    }
  });
  it("scrubbedEnv re-adds an override only when it lies under tmp, else throws", () => {
    const d = fs.mkdtempSync(path.join(os.tmpdir(), "home-guard-env-"));
    try {
      assert.strictEqual(scrubbedEnv({ CLAUDE_CONFIG_DIR: d }).CLAUDE_CONFIG_DIR, d);
      assert.throws(() => scrubbedEnv({ CLAUDE_CONFIG_DIR: path.parse(process.cwd()).root }), /NFR-05/);
      assert.throws(() => scrubbedEnv({ XDG_CONFIG_HOME: os.homedir() }), /NFR-05/);
    } finally { fs.rmSync(d, { recursive: true, force: true }); }
  });
});
