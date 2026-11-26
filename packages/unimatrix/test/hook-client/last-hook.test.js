"use strict";
// nan-024 R-32: last-hook.json write (ADR-004).
const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/last-hook.js");
const HASH = "abcd1234abcd1234";
let home, dir, origHome, origProfile;

function load() {
  delete require.cache[ENTRY];
  return require(ENTRY).recordLastHook;
}

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "um-lasthook-"));
  dir = path.join(home, ".unimatrix", HASH);
  fs.mkdirSync(dir, { recursive: true });
  origHome = process.env.HOME;
  origProfile = process.env.USERPROFILE;
  process.env.HOME = home;
  process.env.USERPROFILE = home;
});
afterEach(() => {
  if (origHome === undefined) delete process.env.HOME; else process.env.HOME = origHome;
  if (origProfile === undefined) delete process.env.USERPROFILE; else process.env.USERPROFILE = origProfile;
  fs.rmSync(home, { recursive: true, force: true });
});

const read = () => JSON.parse(fs.readFileSync(path.join(dir, "last-hook.json"), "utf8"));

describe("recordLastHook", () => {
  it("creates {provider: ISO}", () => {
    load()(HASH, "claude-code");
    const j = read();
    assert.ok(!isNaN(Date.parse(j["claude-code"])));
  });
  it("merges a second provider", () => {
    const r = load();
    r(HASH, "claude-code");
    r(HASH, "codex-cli");
    assert.deepStrictEqual(Object.keys(read()).sort(), ["claude-code", "codex-cli"]);
  });
  it("replaces corrupt file", () => {
    fs.writeFileSync(path.join(dir, "last-hook.json"), "{not json");
    load()(HASH, "codex-cli");
    assert.deepStrictEqual(Object.keys(read()), ["codex-cli"]);
  });
  it("replaces non-object JSON", () => {
    fs.writeFileSync(path.join(dir, "last-hook.json"), "[1]");
    load()(HASH, "codex-cli");
    assert.deepStrictEqual(Object.keys(read()), ["codex-cli"]);
  });
  it("mode 0600 (posix)", { skip: process.platform === "win32" }, () => {
    load()(HASH, "claude-code");
    assert.strictEqual(fs.statSync(path.join(dir, "last-hook.json")).mode & 0o777, 0o600);
  });
  it("missing hash dir: silent no-op", () => {
    assert.doesNotThrow(() => load()("nonexistent", "claude-code"));
    assert.ok(!fs.existsSync(path.join(home, ".unimatrix", "nonexistent")));
  });
  it("read-only dir: no throw", { skip: process.platform === "win32" || (process.getuid && process.getuid() === 0) }, () => {
    fs.chmodSync(dir, 0o500);
    try {
      assert.doesNotThrow(() => load()(HASH, "claude-code"));
    } finally {
      fs.chmodSync(dir, 0o700);
    }
  });
  it("bad args: no throw", () => {
    assert.doesNotThrow(() => load()(null, undefined));
  });
  it("20 concurrent writers from two providers leave valid JSON", async () => {
    const code = `require(${JSON.stringify(ENTRY)}).recordLastHook(${JSON.stringify(HASH)}, process.argv[1])`;
    const run = (p) => new Promise((res) => {
      const c = spawn(process.execPath, ["-e", code, p], { env: process.env, stdio: "ignore" });
      c.on("close", res);
    });
    await Promise.all(Array.from({ length: 20 }, (_, i) => run(i % 2 ? "codex-cli" : "claude-code")));
    const j = read();
    assert.strictEqual(typeof j, "object");
    assert.ok(Object.keys(j).length >= 1);
  });
});
