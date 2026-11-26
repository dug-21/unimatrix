"use strict";
// D-079 superset proof (R-28/R-29): for every fixture where Node's
// isUnimatrixProject is non-null, the real shim must have started Node.
// The converse is not asserted (false positives are allowed).
const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const F = require("./helpers/shim-fixture.js");
const { isUnimatrixProject } = require("../lib/project-state.js");

const skip = process.platform === "win32" ? "POSIX shim (cmd twin: shim-cmd.test.js, always starts Node)" : false;

// spec: git: none|dir|file-cwd|file-anc; store: none|marker|legacy|remote|foreign;
// headless: none|contains|outside|noroot; sub: nested depth; link: symlinked cwd; space: root dir has space
function build(spec) {
  const home = F.mkTmp("shimsup-");
  const s = F.installShim(home);
  const name = spec.space ? "repo A" : "repoA";
  const repo = path.join(home, "fx", name);
  fs.mkdirSync(repo, { recursive: true });
  let cwd = repo;
  for (let i = 0; i < (spec.sub || 0); i++) cwd = path.join(cwd, "d" + i);
  fs.mkdirSync(cwd, { recursive: true });
  let projRoot = repo;
  if (spec.git === "dir") fs.mkdirSync(path.join(repo, ".git"));
  if (spec.git === "file-cwd" || spec.git === "file-anc") {
    const main = path.join(home, "fx", "main");
    fs.mkdirSync(path.join(main, ".git", "worktrees", "wt"), { recursive: true });
    const where = spec.git === "file-cwd" ? cwd : repo;
    fs.writeFileSync(path.join(where, ".git"), "gitdir: " + path.join(main, ".git", "worktrees", "wt") + "\n");
    projRoot = main;
  }
  const { walkToProjectRoot } = require("../lib/hook-client/config.js");
  const nodeRoot = walkToProjectRoot(cwd);
  const sd = path.join(home, ".unimatrix", F.computeProjectHash(nodeRoot));
  if (spec.store && spec.store !== "none" && spec.store !== "foreign") {
    fs.mkdirSync(sd, { recursive: true });
    if (spec.store === "marker") fs.writeFileSync(path.join(sd, "root"), nodeRoot + (spec.crlf ? "\r\n" : "\n"));
    if (spec.store === "remote") {
      fs.writeFileSync(path.join(sd, "remote.json"), "{}");
      fs.writeFileSync(path.join(sd, "root"), nodeRoot + "\n");
    }
  }
  if (spec.store === "foreign" || spec.decoy) {
    // unrelated store (e.g. a prefix sibling's) so the glob is non-empty
    const sib = path.join(home, "fx", name + "-other");
    fs.mkdirSync(sib, { recursive: true });
    const d = path.join(home, ".unimatrix", F.computeProjectHash(sib));
    fs.mkdirSync(d, { recursive: true });
    fs.writeFileSync(path.join(d, "root"), sib + "\n");
  }
  fs.mkdirSync(path.join(home, ".unimatrix", "not-a-hash-dir"), { recursive: true });
  const env = F.baseEnv(home);
  if (spec.headless && spec.headless !== "none") {
    env.UNIMATRIX_REMOTE_TOKEN = "t";
    if (spec.headless !== "noroot") {
      env.UNIMATRIX_PROJECT_ROOT = spec.headless === "contains" ? repo : path.join(home, "elsewhere");
    }
  }
  let runCwd = cwd;
  if (spec.link) {
    runCwd = path.join(home, "link");
    fs.symlinkSync(cwd, runCwd);
  }
  return { home, s, env, cwd: runCwd, realCwd: fs.realpathSync(cwd) };
}

function check(spec) {
  const f = build(spec);
  // Node predicate evaluated with the same env/home; cwd as the process would see it
  const verdict = isUnimatrixProject(f.realCwd, f.env, f.home);
  const r = F.runShim(f.s.shim, f.cwd, f.env);
  const n = F.spawns(f.s.log);
  assert.equal(r.status, 0, "exit 0: " + JSON.stringify(spec));
  assert.equal(r.stdout + r.stderr, "");
  if (verdict !== null) assert.equal(n, 1, "superset violated: " + JSON.stringify(spec));
  return { n, verdict };
}

describe("shim pre-check is a superset of isUnimatrixProject (D-079)", { skip }, () => {
  const table = {
    "local store with root": { git: "dir", store: "marker" },
    "remote.json store": { git: "dir", store: "remote" },
    "legacy store without root": { git: "dir", store: "legacy" },
    "headless valid": { git: "dir", headless: "contains" },
    "headless without root var": { git: "dir", headless: "noroot" },
    "headless cwd outside root": { git: "dir", headless: "outside" },
    "worktree .git file in cwd": { git: "file-cwd", store: "marker" },
    "worktree .git file in ancestor": { git: "file-anc", sub: 2, store: "marker" },
    "nested subdir": { git: "dir", store: "marker", sub: 3 },
    "symlinked cwd": { git: "dir", store: "marker", link: true },
    "root with spaces": { git: "dir", store: "marker", space: true, sub: 1 },
    "CRLF marker": { git: "dir", store: "marker", crlf: true },
    "no git, store at cwd": { git: "none", store: "marker" },
  };
  for (const [name, spec] of Object.entries(table)) {
    it(name, () => {
      const { n, verdict } = check(spec);
      if (name !== "headless without root var" && name !== "headless cwd outside root") {
        assert.notEqual(verdict, null, "fixture should be a project");
        assert.equal(n, 1);
      }
    });
  }

  it("negatives: none / prefix sibling / .git dir alone -> 0 spawns", () => {
    for (const spec of [
      { git: "none", store: "none" },
      { git: "none", store: "foreign" },
      { git: "dir", store: "foreign", sub: 1 },
    ]) {
      const { n, verdict } = check(spec);
      assert.equal(verdict, null);
      assert.equal(n, 0, JSON.stringify(spec));
    }
  });

  it("strict-superset cases: Node starts, Node says null, still exit 0 silent", () => {
    for (const spec of [
      { git: "dir", headless: "outside" },
      { git: "file-cwd", store: "none" },
    ]) {
      const { n, verdict } = check(spec);
      assert.equal(verdict, null, JSON.stringify(spec));
      assert.equal(n, 1, JSON.stringify(spec));
    }
  });

  it("clause (d): .git file garbage/CRLF content is not evaluated and still triggers", () => {
    const f = build({ git: "none", store: "none" });
    const d = path.join(f.home, "gw");
    fs.mkdirSync(d);
    const pwn = path.join(f.home, "pwn");
    fs.writeFileSync(path.join(d, ".git"), "$(touch " + pwn + ")\r\n\x00garbage");
    F.runShim(f.s.shim, d, f.env);
    assert.equal(F.spawns(f.s.log), 1);
    assert.equal(fs.existsSync(pwn), false);
  });

  it("property: 200 seeded random fixtures never violate the implication", () => {
    const seed0 = Number(process.env.SHIM_SEED) || 20260101;
    let seed = seed0;
    const rnd = (n) => {
      seed = (seed * 1664525 + 1013904223) >>> 0;
      return seed % n;
    };
    const pick = (a) => a[rnd(a.length)];
    for (let i = 0; i < 200; i++) {
      const spec = {
        git: pick(["none", "dir", "file-cwd", "file-anc"]),
        store: pick(["none", "marker", "legacy", "remote", "foreign"]),
        headless: pick(["none", "contains", "outside", "noroot"]),
        sub: rnd(3),
        link: rnd(4) === 0,
        space: rnd(3) === 0,
        crlf: rnd(3) === 0,
        decoy: rnd(2) === 0,
      };
      try {
        check(spec);
      } catch (e) {
        e.message = "seed " + seed0 + " iter " + i + " " + JSON.stringify(spec) + ": " + e.message;
        throw e;
      }
    }
  });
});
