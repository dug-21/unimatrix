"use strict";
// Shared fixture for the shim tests: renders the real templates the way
// install-store does and builds a throwaway ~/.unimatrix tree.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
require("../../lib/project-state.js"); // load before config.js (circular require quirk)
const { computeProjectHash } = require("../../lib/hook-client/config.js");

const SHIM_DIR = path.resolve(__dirname, "../../lib/shim");
const REMOTE_VARS = [
  "UNIMATRIX_BUNDLE",
  "UNIMATRIX_REMOTE_MCP_URL",
  "UNIMATRIX_REMOTE_URL",
  "UNIMATRIX_REMOTE_TOKEN",
  "UNIMATRIX_REMOTE_FINGERPRINT",
];

function render(file, vals) {
  let t = fs.readFileSync(path.join(SHIM_DIR, file), "utf8");
  for (const [k, v] of Object.entries(vals)) t = t.split("@@" + k + "@@").join(v);
  return t;
}

function mkTmp(label) {
  return fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), label || "shim-")));
}

// Stub node: records each spawn (argv line + stdin) and exits with STUB_EXIT.
function writeStubNode(p, log) {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(
    p,
    "#!/bin/sh\necho \"$#:$*\" >> '" + log + "'\ncat > '" + log + ".stdin'\nexit ${STUB_EXIT:-0}\n",
    { mode: 0o755 }
  );
}

// Returns { home, bin, shim, log }. opts.prenode (default true), opts.nodePath.
function installShim(home, opts) {
  opts = opts || {};
  const uh = path.join(home, ".unimatrix");
  const bin = path.join(uh, "bin");
  fs.mkdirSync(bin, { recursive: true });
  const log = path.join(home, "spawn.log");
  const nodePath = opts.nodePath || path.join(home, "stubnode", "node");
  if (!opts.noStub) writeStubNode(nodePath, log);
  const shim = path.join(bin, "unimatrix");
  fs.writeFileSync(
    shim,
    render("unimatrix.sh", {
      NODE: nodePath,
      UHOME: uh,
      REMOTE_VARS: REMOTE_VARS.join(" "),
      PRENODE_CALL: opts.prenode === false ? "" : "unimatrix_may_be_project || exit 0",
    }),
    { mode: 0o700 }
  );
  fs.writeFileSync(path.join(bin, "shim-entry.js"), "// stub\n");
  return { home, uh, bin, shim, log, nodePath };
}

function spawns(log) {
  try {
    return fs.readFileSync(log, "utf8").split("\n").filter(Boolean).length;
  } catch (_e) {
    return 0;
  }
}

function runShim(shim, cwd, env, args, input) {
  return spawnSync("/bin/sh", [shim, ...(args || [])], {
    cwd,
    env,
    input: input === undefined ? "" : input,
    encoding: "utf8",
  });
}

function baseEnv(home, extra) {
  return Object.assign({ PATH: process.env.PATH, HOME: home }, extra || {});
}

module.exports = {
  render, mkTmp, writeStubNode, installShim, spawns, runShim, baseEnv,
  computeProjectHash, REMOTE_VARS, SHIM_DIR,
};
