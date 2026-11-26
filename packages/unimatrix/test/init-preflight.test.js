"use strict";
// D-082 through the real CLI process: init exits 1 when node cannot be recorded; `wire` with a
// missing recorded node exits 1 (no PATH node) or warns + exits 0 (PATH node present).
// Fixture homes only (home-guard); child env carries its own HOME/PATH.
const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const H = require("./helpers/e2e-harness.js");
const { makeCache, supported } = require("./helpers/fake-package.js");

function tree(dir) {
  const out = [];
  (function walk(d) {
    for (const n of fs.readdirSync(d).sort()) {
      const p = path.join(d, n);
      const st = fs.lstatSync(p);
      out.push(path.relative(dir, p) + ":" + (st.isDirectory() ? "d" : st.size));
      if (st.isDirectory()) walk(p);
    }
  })(dir);
  return out;
}

describe("D-082 init/wire preflight via CLI", { skip: !supported && "linux only (local init needs the platform binary package)" }, () => {
  let f, env, pk;
  beforeEach(() => {
    f = H.makeFixture();
    pk = makeCache(f.root);
    env = Object.assign({}, process.env, { HOME: f.home, USERPROFILE: f.home });
    for (const k of Object.keys(env)) if (/^UNIMATRIX_/.test(k) || k === "CODEX_HOME") delete env[k];
  });
  afterEach(() => f.cleanup());

  const cli = (args, e, pre) => spawnSync(process.execPath, ["-e", (pre || "") + `require(${JSON.stringify(pk.cliBin)}).run(${JSON.stringify(args)});`],
    { cwd: f.proj, env: e, encoding: "utf8", timeout: 60000 });
  const record = (node) => {
    const r = cli(["init"], env);
    assert.equal(r.status, 0, r.stdout + r.stderr);
    const p = path.join(f.home, ".unimatrix", "install.json");
    const j = JSON.parse(fs.readFileSync(p, "utf8"));
    j.node = node;
    fs.writeFileSync(p, JSON.stringify(j));
  };

  it("init: node cannot be recorded -> exit 1, no install pointer/shim/hook/MCP written", () => {
    const before = tree(f.home);
    const r = cli(["init"], env, 'Object.defineProperty(process,"execPath",{value:"/nonexistent/node"});');
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stderr, /node/i);
    assert.ok(!fs.existsSync(path.join(f.home, ".unimatrix", "install.json")));
    assert.deepEqual(tree(f.home), before);
    assert.ok(!fs.existsSync(path.join(f.proj, ".claude", "settings.local.json")));
  });

  it("wire: recorded node missing and no node on PATH -> exit 1, tree unchanged", () => {
    record(path.join(f.root, "gone", "node"));
    const empty = path.join(f.root, "emptybin");
    fs.mkdirSync(empty);
    const before = tree(f.root);
    const r = cli(["wire", "--harness", "claude-code"], Object.assign({}, env, { PATH: empty }));
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stderr, /npx unimatrix init/);
    assert.deepEqual(tree(f.root), before);
  });

  it("wire: recorded node missing but a PATH node exists -> warning on stderr, exit 0", () => {
    const gone = path.join(f.root, "gone", "node");
    record(gone);
    const bin = path.join(f.root, "pathbin");
    fs.mkdirSync(bin);
    fs.copyFileSync(process.execPath, path.join(bin, "node"));
    fs.chmodSync(path.join(bin, "node"), 0o755);
    const r = cli(["wire", "--harness", "claude-code"], Object.assign({}, env, { PATH: bin }));
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.ok(r.stderr.includes(gone), r.stderr);
  });
});
