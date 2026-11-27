"use strict";
// D-081/D-084 detection matrix through the real init CLI: each row asserts whether Codex hooks.json and
// config.toml [mcp_servers.unimatrix] were written, and WHERE. Fixture homes only (home-guard); PATH is
// controlled per row (a stub `codex` only in row 3).
const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const H = require("./helpers/e2e-harness.js");
const { scrubbedEnv } = require("./helpers/home-guard.js"); // R-09: no ambient config-dir overrides
const { makeCache, supported } = require("./helpers/fake-package.js");

describe("D-081 Codex detection matrix (init CLI)", { skip: !supported && "linux only (local init needs the platform binary package)" }, () => {
  let f, pk, bare;
  beforeEach(() => {
    f = H.makeFixture();
    pk = makeCache(f.root);
    bare = path.join(f.root, "barebin");
    fs.mkdirSync(bare);
  });
  afterEach(() => f.cleanup());

  async function init(row) {
    const env = scrubbedEnv({ HOME: f.home, USERPROFILE: f.home, PATH: row.path || bare });
    for (const k of Object.keys(env)) if (/^UNIMATRIX_/.test(k)) delete env[k];
    if (row.codexHome) env.CODEX_HOME = row.codexHome;
    const r = await H.run(process.execPath, [pk.cliBin, "init"].concat(row.args || []), { spawn: { cwd: f.proj, env }, timeoutMs: 120000 });
    assert.equal(r.code, 0, r.out + r.err);
    return r;
  }
  const has = (dir) => ({ hooks: fs.existsSync(path.join(dir, "hooks.json")), mcp: fs.existsSync(path.join(dir, "config.toml")) && /\[mcp_servers\.unimatrix\]/.test(fs.readFileSync(path.join(dir, "config.toml"), "utf8")) });
  const dflt = () => path.join(f.home, ".codex");

  it("row 1: project .codex/ -> ~/.codex hooks + MCP", async () => {
    fs.mkdirSync(path.join(f.proj, ".codex"));
    await init({});
    assert.deepEqual(has(dflt()), { hooks: true, mcp: true });
  });

  it("row 2: existing $CODEX_HOME -> written there, ~/.codex not created", async () => {
    await init({ codexHome: f.codex });
    assert.deepEqual(has(f.codex), { hooks: true, mcp: true });
    assert.ok(!fs.existsSync(dflt()));
  });

  it("row 3: codex on PATH -> ~/.codex", async () => {
    const bin = path.join(f.root, "pathbin");
    fs.mkdirSync(bin);
    fs.writeFileSync(path.join(bin, "codex"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
    await init({ path: bin });
    assert.deepEqual(has(dflt()), { hooks: true, mcp: true });
  });

  it("row 4: wire --harness codex-cli -> written, ~/.codex created", async () => {
    await init({});
    assert.ok(!fs.existsSync(dflt()));
    const env = scrubbedEnv({ HOME: f.home, USERPROFILE: f.home, PATH: bare });
    for (const k of Object.keys(env)) if (/^UNIMATRIX_/.test(k)) delete env[k];
    const r = await H.run(process.execPath, [pk.cliBin, "wire", "--harness", "codex-cli"], { spawn: { cwd: f.proj, env }, timeoutMs: 60000 });
    assert.equal(r.code, 0, r.out + r.err);
    assert.deepEqual(has(dflt()), { hooks: true, mcp: true });
  });

  it("row 5: no trigger -> no Codex file created or touched", async () => {
    await init({});
    assert.ok(!fs.existsSync(dflt()));
    assert.ok(!fs.existsSync(path.join(f.proj, ".codex")));
  });

  it("row 6: $CODEX_HOME set but absent, no other trigger -> not written", async () => {
    const ch = path.join(f.root, "absent-ch");
    await init({ codexHome: ch });
    assert.ok(!fs.existsSync(ch));
    assert.ok(!fs.existsSync(dflt()));
  });

  it("row 7: .codex/ + $CODEX_HOME -> single write to $CODEX_HOME, hooks and MCP agree", async () => {
    fs.mkdirSync(path.join(f.proj, ".codex"));
    await init({ codexHome: f.codex });
    assert.deepEqual(has(f.codex), { hooks: true, mcp: true });
    assert.ok(!fs.existsSync(dflt()));
  });
});
