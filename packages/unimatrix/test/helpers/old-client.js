"use strict";

// old-client.js — vnc-050 R-21 compat matrix: materialize the N-1 published
// client (@dug-21/unimatrix@0.11.3) outside the package tree (lesson #5830),
// integrity-pinned. Fallback when the registry is unreachable:
// `git archive v0.11.3 packages/unimatrix`, flagged via `source`.

const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { execFileSync } = require("child_process");

const OLD_VERSION = "0.11.3";
const OLD_INTEGRITY =
  "sha512-yih9DsKFK2CEtBXrZhcoultpjPM2ODDdIs4HlUb6nnH/qMPEgQEi2GcQIB0ZK5OKTJdHX1PYST9Xb2TwXcUbiw==";
const REPO = path.resolve(__dirname, "../../../..");

function cacheRoot() {
  return process.env.UNIMATRIX_COMPAT_CACHE || path.join(os.tmpdir(), "uni-compat-cache", OLD_VERSION);
}

function integrityOf(file) {
  return "sha512-" + crypto.createHash("sha512").update(fs.readFileSync(file)).digest("base64");
}

/**
 * @returns {{ dir: string, bin: string, source: "npm"|"git-archive" }}
 * dir = extracted package root (contains bin/, lib/). Throws if neither source works.
 */
function materializeOldClient() {
  const root = cacheRoot();
  const tgz = path.join(root, "dug-21-unimatrix-" + OLD_VERSION + ".tgz");
  const npmDir = path.join(root, "npm");
  const gitDir = path.join(root, "git");
  const done = (dir, source) => ({ dir, bin: path.join(dir, "bin", "unimatrix.js"), source });
  fs.mkdirSync(root, { recursive: true });

  if (fs.existsSync(path.join(npmDir, "package", "bin", "unimatrix.js")) && fs.existsSync(tgz) &&
      integrityOf(tgz) === OLD_INTEGRITY) {
    return done(path.join(npmDir, "package"), "npm");
  }
  try {
    if (!fs.existsSync(tgz)) {
      execFileSync("npm", ["pack", "@dug-21/unimatrix@" + OLD_VERSION, "--pack-destination", root],
        { stdio: "ignore", timeout: 90000 });
    }
    if (integrityOf(tgz) !== OLD_INTEGRITY) {
      fs.rmSync(tgz, { force: true });
      throw new Error("integrity mismatch for " + tgz);
    }
    fs.rmSync(npmDir, { recursive: true, force: true });
    fs.mkdirSync(npmDir);
    execFileSync("tar", ["xzf", tgz, "-C", npmDir]);
    return done(path.join(npmDir, "package"), "npm");
  } catch (_e) {
    fs.rmSync(gitDir, { recursive: true, force: true });
    fs.mkdirSync(gitDir);
    const tar = execFileSync("git", ["-C", REPO, "archive", "v" + OLD_VERSION, "packages/unimatrix"],
      { maxBuffer: 256 * 1024 * 1024 });
    execFileSync("tar", ["xf", "-", "-C", gitDir], { input: tar });
    return done(path.join(gitDir, "packages", "unimatrix"), "git-archive");
  }
}

module.exports = { materializeOldClient, OLD_VERSION, OLD_INTEGRITY };
