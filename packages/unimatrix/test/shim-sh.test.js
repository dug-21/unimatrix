"use strict";
const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const F = require("./helpers/shim-fixture.js");

const skip = process.platform === "win32" ? "POSIX shim" : false;

function project(home) {
  const root = path.join(home, "proj");
  fs.mkdirSync(path.join(root, ".git"), { recursive: true });
  const sd = path.join(home, ".unimatrix", F.computeProjectHash(root));
  fs.mkdirSync(sd, { recursive: true });
  fs.writeFileSync(path.join(sd, "root"), root + "\n");
  return root;
}

describe("shim.sh behaviour", { skip }, () => {
  it("recorded node used; argv (spaces) and stdin byte-exact", () => {
    const home = F.mkTmp("shim home-é-");
    const s = F.installShim(home, { nodePath: path.join(home, "my node dir", "node") });
    const r = F.runShim(s.shim, project(home), F.baseEnv(home), ["a b", "--provider", "codex-cli"], '{"x":"\\u00e9"}\n');
    assert.equal(r.status, 0);
    assert.match(fs.readFileSync(s.log, "utf8"), /shim-entry\.js a b --provider codex-cli/);
    assert.equal(fs.readFileSync(s.log + ".stdin", "utf8"), '{"x":"\\u00e9"}\n');
  });

  for (const code of [0, 1, 7]) {
    it("propagates exit code " + code, () => {
      const home = F.mkTmp();
      const s = F.installShim(home);
      const r = F.runShim(s.shim, project(home), F.baseEnv(home, { STUB_EXIT: String(code) }));
      assert.equal(r.status, code);
    });
  }

  it("recorded node missing -> PATH node used", () => {
    const home = F.mkTmp();
    const s = F.installShim(home, { noStub: true, nodePath: path.join(home, "gone", "node") });
    const pathDir = path.join(home, "pbin");
    F.writeStubNode(path.join(pathDir, "node"), s.log);
    const r = F.runShim(s.shim, project(home), F.baseEnv(home, { PATH: pathDir + ":/usr/bin:/bin" }));
    assert.equal(r.status, 0);
    assert.equal(F.spawns(s.log), 1);
  });

  it("no node anywhere: exit 1 + hint inside a project, exit 0 silent outside", () => {
    const home = F.mkTmp();
    const s = F.installShim(home, { noStub: true, nodePath: path.join(home, "gone", "node") });
    const env = F.baseEnv(home, { PATH: path.join(home, "empty") });
    const inside = F.runShim(s.shim, project(home), env);
    assert.equal(inside.status, 1);
    assert.match(inside.stderr, /node not found/);
    assert.equal(inside.stdout, "");
    const out = path.join(home, "elsewhere");
    fs.mkdirSync(out);
    const outside = F.runShim(s.shim, out, env);
    assert.equal(outside.status, 0);
    assert.equal(outside.stderr + outside.stdout, "");
  });

  it("pre-check disabled: Node spawned exactly once even outside a project", () => {
    const home = F.mkTmp();
    const s = F.installShim(home, { prenode: false });
    const out = path.join(home, "elsewhere");
    fs.mkdirSync(out);
    F.runShim(s.shim, out, F.baseEnv(home));
    assert.equal(F.spawns(s.log), 1);
  });

  it("pre-check enabled: outside a project spawns 0", () => {
    const home = F.mkTmp();
    const s = F.installShim(home);
    const out = path.join(home, "elsewhere");
    fs.mkdirSync(out);
    const r = F.runShim(s.shim, out, F.baseEnv(home));
    assert.equal(r.status, 0);
    assert.equal(F.spawns(s.log), 0);
  });

  it("marker content is never evaluated", () => {
    const home = F.mkTmp();
    const s = F.installShim(home);
    const pwn = path.join(home, "pwn");
    const root = path.join(home, "m");
    fs.mkdirSync(root);
    const sd = path.join(home, ".unimatrix", "0123456789abcdef");
    fs.mkdirSync(sd);
    fs.writeFileSync(path.join(sd, "root"), "$(touch " + pwn + ")\r\n");
    F.runShim(s.shim, root, F.baseEnv(home));
    assert.equal(fs.existsSync(pwn), false);
  });

  it("CRLF marker, root with spaces, prefix sibling, non-16-hex dir", () => {
    const home = F.mkTmp();
    const s = F.installShim(home);
    const root = path.join(home, "my repo");
    const sib = path.join(home, "my repo-other");
    fs.mkdirSync(path.join(root, "sub"), { recursive: true });
    fs.mkdirSync(sib);
    const bad = path.join(home, ".unimatrix", "not-a-hash");
    fs.mkdirSync(bad);
    fs.writeFileSync(path.join(bad, "root"), sib + "\n");
    const sd = path.join(home, ".unimatrix", "aaaaaaaaaaaaaaaa");
    fs.mkdirSync(sd);
    fs.writeFileSync(path.join(sd, "root"), root + "\r\n");
    F.runShim(s.shim, path.join(root, "sub"), F.baseEnv(home));
    assert.equal(F.spawns(s.log), 1);
    fs.unlinkSync(s.log);
    F.runShim(s.shim, sib, F.baseEnv(home));
    assert.equal(F.spawns(s.log), 0);
  });
});
