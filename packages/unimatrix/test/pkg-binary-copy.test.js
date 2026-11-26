"use strict";
// D-078 assembled (linux local): init from a fake "npx cache" package that carries a platform binary
// package, delete the cache, then run the LITERAL written hook command through the shell. The launcher
// preflight resolves the binary from ~/.unimatrix/pkg/<version>/node_modules (binarySource "install");
// a stub UDS server on the project socket observes the event. Fixture homes only (home-guard).
const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");
const H = require("./helpers/e2e-harness.js");
const { resolveRuntime } = require("./../lib/resolve-runtime.js");
const { makeCache, plat, supported } = require("./helpers/fake-package.js");
const { computeProjectHash } = require("./../lib/hook-client/config.js");

const skip = supported ? false : "linux only (UDS local transport)";

describe("D-078 pkg-binary-copy: binary resolves from pkg/<version> after the cache is gone", { skip }, () => {
  let f, env, cache, seen, server, sock;
  before(async () => {
    f = H.makeFixture();
    const pk = makeCache(f.root);
    cache = pk.cache;
    env = Object.assign({}, process.env, { HOME: f.home, USERPROFILE: f.home });
    for (const k of Object.keys(env)) if (/^UNIMATRIX_/.test(k) || k === "CODEX_HOME") delete env[k];
    const r = await H.run(process.execPath, [pk.cliBin, "init"], { spawn: { cwd: f.proj, env }, timeoutMs: 120000 });
    assert.equal(r.code, 0, r.out + r.err);
    fs.rmSync(pk.cacheRoot, { recursive: true, force: true });
    seen = [];
    sock = path.join(f.home, ".unimatrix", computeProjectHash(f.proj), "unimatrix.sock");
    fs.mkdirSync(path.dirname(sock), { recursive: true });
    server = net.createServer((c) => { c.on("data", (b) => seen.push(b)); c.on("error", () => {}); });
    await new Promise((res) => server.listen(sock, res));
  });
  after(() => {
    if (server) server.close();
    if (f) f.cleanup();
  });

  it("platform binary copied under pkg/<version>/node_modules; original location gone", () => {
    const ij = JSON.parse(fs.readFileSync(path.join(f.home, ".unimatrix", "install.json"), "utf8"));
    assert.ok(!fs.existsSync(cache));
    assert.ok(ij.packageRoot.startsWith(path.join(f.home, ".unimatrix", "pkg")));
    const bin = path.join(ij.packageRoot, "node_modules", ...plat.split("/"), "bin", "unimatrix");
    assert.ok(fs.statSync(bin).mode & 0o100);
    const r = resolveRuntime({ transport: "local", home: f.home, platform: "linux", arch: process.arch, env: {} });
    assert.equal(r.binarySource, "install");
  });

  it("literal hook command still exits 0, silent, and delivers the event", async () => {
    const line = H.hookCommand(path.join(f.proj, ".claude", "settings.local.json"), "PostToolUse");
    const r = await H.runLine(H.shells()[0], line, f.proj, env, H.event(f, "d078-sid"));
    assert.equal(r.code, 0, r.err);
    assert.equal(r.out, "");
    for (let i = 0; i < 50 && !Buffer.concat(seen).toString("latin1").includes("d078-sid"); i++) await new Promise((x) => setTimeout(x, 100));
    assert.ok(Buffer.concat(seen).toString("latin1").includes("d078-sid"), "stub socket saw no event");
  });
});
