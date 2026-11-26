"use strict";
// nan-024: main(argv) export, require() does not auto-run, script still runs.
const { describe, it } = require("node:test");
const assert = require("assert");
const path = require("path");
const { spawn } = require("child_process");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");

describe("main export", () => {
  it("exports main taking argv; require() does not run it", () => {
    const m = require(ENTRY);
    assert.strictEqual(typeof m.main, "function");
    assert.ok(m.main.length <= 1);
  });
  it("main(argv) resolves with unknown event, never rejects", async () => {
    const code = `require(${JSON.stringify(ENTRY)}).main(["node","x","NoSuchEvent","--provider","codex-cli"]).then(()=>process.stderr.write("resolved"))`;
    const r = await new Promise((res) => {
      const c = spawn(process.execPath, ["-e", code], { stdio: ["pipe", "pipe", "pipe"] });
      let out = "", err = "";
      c.stdout.on("data", (d) => (out += d));
      c.stderr.on("data", (d) => (err += d));
      c.stdin.end("{}");
      c.on("close", (code) => res({ code, out, err }));
    });
    assert.strictEqual(r.code, 0);
    assert.strictEqual(r.out, "");
    assert.ok(r.err.includes("resolved"));
  });
  it("script invocation exits 0 with no stdout", async () => {
    const r = await new Promise((res) => {
      const c = spawn(process.execPath, [ENTRY, "NoSuchEvent"], { stdio: ["pipe", "pipe", "ignore"] });
      let out = "";
      c.stdout.on("data", (d) => (out += d));
      c.stdin.end("{}");
      c.on("close", (code) => res({ code, out }));
    });
    assert.strictEqual(r.code, 0);
    assert.strictEqual(r.out, "");
  });
});
