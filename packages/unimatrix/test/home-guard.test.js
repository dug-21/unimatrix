"use strict";
const { describe, it } = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { assertTmpHome } = require("./helpers/home-guard");

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
});
