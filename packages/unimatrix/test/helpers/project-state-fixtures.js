"use strict";

// Shared fixture table for project-state parity (R-28): consumed by
// project-state.test.js and project-parity.test.js (and the shim-entry inline
// copy once it exists). build() materialises the tree under a fresh tmp dir.

const fs = require("fs");
const os = require("os");
const path = require("path");
const { computeProjectHash, walkToProjectRoot } = require("../../lib/hook-client/config.js");

const BUNDLE = { UNIMATRIX_BUNDLE: "b64" };

function build() {
  const base = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "ps-fx-")));
  const home = path.join(base, "home");
  const mk = (p) => { fs.mkdirSync(p, { recursive: true }); return p; };
  mk(path.join(home, ".unimatrix"));
  const A = mk(path.join(base, "fx", "repoA"));
  mk(path.join(A, ".git"));
  const sub = mk(path.join(A, "sub", "deep"));
  const B = mk(path.join(base, "fx", "repoB"));
  mk(path.join(B, ".git"));
  const other = mk(path.join(base, "fx", "repoA-other"));
  const nested = mk(path.join(A, "vendor", "inner"));
  mk(path.join(nested, ".git"));
  const nested2 = mk(path.join(A, "vendor", "inner2"));
  mk(path.join(nested2, ".git"));
  const link = path.join(base, "fx", "linkA");
  const rootLink = path.join(base, "fx", "rootLinkA");
  fs.symlinkSync(A, link);
  fs.symlinkSync(A, rootLink);
  // worktree: .git FILE pointing at the main repo's gitdir
  const wt = mk(path.join(base, "fx", "wt"));
  fs.writeFileSync(path.join(wt, ".git"), "gitdir: " + path.join(A, ".git", "worktrees", "wt") + "\n");
  const bare = mk(path.join(base, "fx", "bare")); // no .git anywhere

  const store = (root, remote) => {
    const d = path.join(home, ".unimatrix", computeProjectHash(walkToProjectRoot(root)));
    mk(d);
    if (remote) fs.writeFileSync(path.join(d, "remote.json"), "{}");
  };
  const out = { base, home, A, B, other, sub, nested, link, wt, bare, rows: [] };
  return { out, store };
}

// Row: {name, cwd, env, expectKind, effectiveStripped}. Stores are prepared
// per-row by `prep` so rows stay independent of each other's store dirs.
function rows() {
  const { out, store } = build();
  const { A, B, other, sub, nested, nested2, link, wt, bare, home } = out;
  const R = (name, cwd, env, expectKind, effectiveStripped, prep) =>
    out.rows.push({ name, cwd, env, expectKind, effectiveStripped: !!effectiveStripped, prep });
  const root = (r, extra) => Object.assign({ UNIMATRIX_PROJECT_ROOT: r }, BUNDLE, extra);

  R("none: no store, no env", bare, {}, null);
  R("local store at root", A, {}, "local", false, () => store(A, false));
  R("local store from subdir", sub, {}, "local", false);
  R("local store via symlinked cwd", link, {}, "local", false);
  R("local store trailing slash cwd", A + path.sep, {}, "local", false);
  R("nested repo is its own project (no store)", nested2, {}, null);
  R("nested repo with its own store", nested, {}, "local", false, () => store(nested, false));
  R("cloud remote.json", B, {}, "cloud", false, () => store(B, true));
  R("worktree cwd no store/remote/headless (D-079)", wt, {}, null);
  R("bare dir no .git", bare, {}, null);
  R("headless root contains cwd (root)", other, root(other), "headless", false);
  R("headless subdir", sub, root(A), "headless", false);
  R("headless symlinked root", sub, root(path.join(out.base, "fx", "rootLinkA")), "headless", false);
  R("headless wins over cloud", B, root(B), "headless", false);
  R("headless root unset -> outside", bare, BUNDLE, null, true);
  R("headless root empty", bare, root(""), null, true);
  R("headless root relative", bare, root("fx/repoA"), null, true);
  R("headless root `/`-relative never matches", bare, root("."), null, true);
  R("headless cwd outside (repoB vs A)", bare, root(A), null, true);
  R("headless sibling shared prefix (repoA-other vs A)", other, root(A), null, true);
  R("headless cwd = / vs A", path.parse(A).root, root(A), null, true);
  R("headless cwd = $HOME vs A", home, root(A), null, true);
  R("only HEADERS is not headless", bare, { UNIMATRIX_PROJECT_ROOT: bare, UNIMATRIX_REMOTE_HEADERS: "x:y" }, null, false);
  R("blank bundle is not headless", bare, { UNIMATRIX_PROJECT_ROOT: bare, UNIMATRIX_BUNDLE: "  " }, null, false);
  R("outside root, store present keeps local store verdict", B, root(A, { UNIMATRIX_BINARY: "/x" }), "cloud", true);
  return out;
}

module.exports = { rows, build, BUNDLE };
