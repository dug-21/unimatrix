"use strict";
const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const F = require("./helpers/shim-fixture.js");

const tpl = fs.readFileSync(path.join(F.SHIM_DIR, "unimatrix.cmd"), "utf8");

describe("unimatrix.cmd static (D-088: pre-Node check disabled)", () => {
  it("has no marker or .git scanning clauses", () => {
    assert.doesNotMatch(tpl, /may_be_project|\\\.git|set \/p|PRENODE|UNIMATRIX_PROJECT_ROOT/i);
  });
  it("propagates exit code and args, no pause, no paren block around node call", () => {
    assert.match(tpl, /"%NODE%" "%~dp0shim-entry\.js" %\*\r?\nexit \/b %ERRORLEVEL%/);
    assert.doesNotMatch(tpl, /\bpause\b/i);
  });
});

describe("unimatrix.cmd behaviour", { skip: process.platform !== "win32" && "win32 only" }, () => {
  it("starts Node exactly once even outside a project; exit code visible", () => {
    const home = F.mkTmp("shimcmd ");
    const bin = path.join(home, ".unimatrix", "bin");
    fs.mkdirSync(bin, { recursive: true });
    const log = path.join(home, "spawn.log");
    // recorded "node" is a .cmd stub recording its spawn
    const stub = path.join(home, "stub.cmd");
    fs.writeFileSync(stub, '@echo off\r\necho x>>"' + log + '"\r\nexit /b 7\r\n');
    fs.writeFileSync(path.join(bin, "unimatrix.cmd"), tpl.split("@@NODE@@").join(stub));
    const r = spawnSync("cmd.exe", ["/C", path.join(bin, "unimatrix.cmd")], { cwd: home, input: "" });
    assert.equal(r.status, 7);
    assert.equal(F.spawns(log), 1);
  });
});
