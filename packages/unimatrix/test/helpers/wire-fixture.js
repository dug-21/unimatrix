"use strict";
// wire()/init fixtures (nan-024): temp git project + isolated HOME so no test writes
// under the real home. wireOpts() skips the real platform-binary copy (requireBinary false).
const fs = require("fs");
const os = require("os");
const path = require("path");
const { assertTmpHome } = require("./home-guard");

function makeProject(prefix) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), prefix || "unimatrix-wire-test-")));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function makeHome() {
  return assertTmpHome(fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-wire-home-"))));
}

/** Options for wire(): isolated home, no platform-binary copy, local (binaryPath) or cloud. */
function wireOpts(home, extra) {
  assertTmpHome(home);
  return Object.assign({ home, requireBinary: false, env: { PATH: process.env.PATH, HOME: home } }, extra || {});
}

/** Sorted relative file list of a tree (for dry-run == real comparisons). */
function listTree(dir) {
  const out = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else out.push(path.relative(dir, p));
    }
  };
  try { walk(dir); } catch (_) { /* absent */ }
  return out.sort();
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

module.exports = { makeProject, makeHome, wireOpts, listTree, readJson };
