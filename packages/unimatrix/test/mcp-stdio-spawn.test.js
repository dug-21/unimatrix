"use strict";
// D-077 assembled: init writes the MCP entry; spawn it for real (shell:false, like the harness) against a
// stub platform binary. First stdout byte is the stub's, exit code and signal are mirrored, stderr is
// banner-free. Fixture homes only (home-guard).
const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");
const H = require("./helpers/e2e-harness.js");
const { scrubbedEnv } = require("./helpers/home-guard.js"); // R-09: no ambient config-dir overrides
const { makeCache, supported } = require("./helpers/fake-package.js");

const STUB = `#!${process.execPath}
const m = process.env.STUB_MODE || "echo";
if (m === "silent") process.exit(0);
if (m.startsWith("exit")) process.exit(Number(m.slice(4)));
if (m === "sigterm") process.kill(process.pid, "SIGTERM");
process.stdin.once("data", (d) => { process.stdout.write("{" + JSON.stringify(String(d).trim()).slice(1, -1) + "\\n"); process.exit(0); });
`;

describe("D-077 mcp-stdio spawn through the written MCP entry", { skip: !supported && "linux only (local mcp-stdio spawn)" }, () => {
  let f, env, entry;
  before(async () => {
    f = H.makeFixture();
    const pk = makeCache(f.root, STUB);
    env = scrubbedEnv({ HOME: f.home, USERPROFILE: f.home });
    for (const k of Object.keys(env)) if (/^UNIMATRIX_/.test(k)) delete env[k];
    const r = await H.run(process.execPath, [pk.cliBin, "init"], { spawn: { cwd: f.proj, env }, timeoutMs: 120000 });
    assert.equal(r.code, 0, r.out + r.err);
    const j = JSON.parse(fs.readFileSync(path.join(f.home, ".claude.json"), "utf8"));
    entry = j.projects[Object.keys(j.projects)[0]].mcpServers.unimatrix;
  });
  after(() => f && f.cleanup());

  const go = (mode, input) => new Promise((resolve) => {
    const p = spawn(entry.command, entry.args || [], { cwd: f.proj, env: Object.assign({}, env, entry.env || {}, { STUB_MODE: mode }), stdio: ["pipe", "pipe", "pipe"] });
    let out = "", err = "";
    p.stdout.on("data", (c) => { out += c; });
    p.stderr.on("data", (c) => { err += c; });
    p.stdin.on("error", () => {});
    p.on("close", (code, signal) => resolve({ code, signal, out, err }));
    p.stdin.end(input === undefined ? "" : input);
  });

  it("stdio round trip: first stdout byte is the stub's, stderr banner-free", async () => {
    const r = await go("echo", "hello\n");
    assert.equal(r.code, 0, r.err);
    assert.equal(r.out, "{hello\n");
    assert.equal(r.err, "");
  });

  it("a stub that writes nothing yields empty stdout (Node wrote none before spawn)", async () => {
    const r = await go("silent");
    assert.equal(r.code, 0, r.err);
    assert.equal(r.out, "");
  });

  it("exit codes mirrored exactly", async () => {
    for (const c of [1, 7, 42]) {
      const r = await go("exit" + c);
      assert.equal(r.code, c, "mode exit" + c + ": " + r.err);
      assert.equal(r.out, "");
    }
  });

  it("stub killed by SIGTERM -> the entry process dies by the same signal", async () => {
    const r = await go("sigterm");
    assert.equal(r.signal, "SIGTERM", JSON.stringify(r));
    assert.equal(r.code, null);
  });
});
