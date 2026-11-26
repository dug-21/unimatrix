"use strict";
// `unimatrix hook` and `unimatrix mcp-stdio` routes (ADR-004, D-042/D-043/D-077/D-086).
// Never throws, never process.exit(); stdout carries nothing before the child/bridge
// starts (R-07). Errors -> one stderr line + exitCode 1 (never 2).
const os = require("os");
const path = require("path");
const fs = require("fs");
const ps = require("./project-state.js");
const { resolveRuntime } = require("./resolve-runtime.js");

const NOT_ATTACHED_HINT = "unimatrix: not attached on this machine; run 'npx unimatrix init'";
const SIGNALS = ["SIGINT", "SIGTERM", "SIGHUP"];

const err = (m) => {
  try { process.stderr.write(m + "\n"); } catch (_e) { /* ignore */ }
};
const fail = (m) => { err(m); process.exitCode = 1; };

function readInstallJson(home) {
  try {
    const j = JSON.parse(fs.readFileSync(path.join(home, ".unimatrix", "install.json"), "utf8"));
    return j && typeof j === "object" ? j : null;
  } catch (_e) { return null; }
}

// Shared preamble. Returns {project, d, rt, env} or null (outcome already set).
function prepare(ctx, mcp) {
  const env = process.env;
  const home = os.homedir();
  const platform = process.platform;
  const cwd = process.cwd();
  const pre = ctx && ctx.preflight;
  const project = pre ? pre.project : ps.isUnimatrixProject(cwd, env, home);
  if (!project) return null;
  // Installed-ness is decided read-only before trust (same order as shim-entry.js):
  // no install.json -> no trust set to verify -> exit-table outcome, not a trust
  // error. Trust runs when installed, or on Linux when install.json exists (a
  // half-removed install is refused loudly); always before packageRoot code loads.
  const d = pre && pre.decision ? pre.decision : ps.isInstalled(project, platform, { env, home });
  const notAttached = () => {
    if (mcp) fail(NOT_ATTACHED_HINT);
    else if (platform === "linux") err(NOT_ATTACHED_HINT);
    return null;
  };
  const instFile = (() => {
    try { return fs.statSync(path.join(home, ".unimatrix", "install.json")).isFile(); } catch (_e) { return false; }
  })();
  if (!d.installed && !(platform === "linux" && instFile)) return notAttached();
  if (!(pre && pre.trustChecked)) {
    try {
      const trustMod = require("./trust.js");
      trustMod.checkTrust(
        trustMod.trustSet(home, { installJson: readInstallJson(home), platform }), platform, { home });
    } catch (e) { fail(e && e.message); return null; }
  }
  if (!d.installed) return notAttached();
  const eff = ps.effectiveEnv(env, cwd, home);
  let rt;
  try {
    rt = resolveRuntime({ env: eff, home, platform, transport: d.transport });
  } catch (e) { fail(e && e.message); return null; }
  return { project, d, rt, env: eff, rawEnv: env, cwd, platform };
}

async function run(args, ctx) {
  try {
    const p = prepare(ctx, false);
    if (!p) return;
    const entry = path.join(p.rt.clientRoot, "lib", "hook-client", "index.js");
    let client;
    try { client = require(entry); } catch (e) {
      fail("unimatrix: cannot load hook client at " + p.rt.clientRoot + ": " + (e && e.message));
      return;
    }
    try { await client.main(["node", entry].concat(args.slice(1))); } catch (e) {
      fail("unimatrix: hook client error: " + (e && e.message));
    }
  } catch (e) {
    fail("unimatrix: " + (e && e.message));
  }
}

function signalNumber(sig) {
  const n = os.constants && os.constants.signals && os.constants.signals[sig];
  return typeof n === "number" ? n : 0;
}

function spawnRust(binary, env) {
  const childEnv = Object.assign({}, env, {
    LD_LIBRARY_PATH: path.dirname(binary) + (env.LD_LIBRARY_PATH ? ":" + env.LD_LIBRARY_PATH : ""),
  });
  const platform = process.platform;
  const child = require("child_process").spawn(binary, [], { stdio: "inherit", env: childEnv });
  const handlers = SIGNALS.map((sig) => {
    const h = () => { try { child.kill(sig); } catch (_e) { /* ignore */ } };
    process.on(sig, h);
    return [sig, h];
  });
  child.on("error", (e) => fail("unimatrix: cannot start " + binary + ": " + (e && e.message)));
  child.on("exit", (code, sig) => {
    if (platform === "win32") {
      // D-086: unreachable in practice (Windows mcp-stdio is cloud-only); exit code only.
      process.exitCode = code == null ? 1 : code;
    } else if (sig != null) {
      for (const [s, h] of handlers) process.removeListener(s, h);
      process.exitCode = 128 + signalNumber(sig);
      try { process.kill(process.pid, sig); } catch (_e) { /* exitCode already set */ }
    } else {
      process.exitCode = code;
    }
  });
  return new Promise((resolve) => { child.on("close", resolve); child.on("error", resolve); });
}

async function runMcpStdio(args, ctx) {
  try {
    const p = prepare(ctx, true);
    if (!p) return;
    if (p.d.transport === "cloud") {
      let bridge;
      try { bridge = require(path.join(p.rt.clientRoot, "lib", "hook-client", "mcp-bridge.js")); } catch (e) {
        fail("unimatrix: cannot load mcp bridge at " + p.rt.clientRoot + ": " + (e && e.message));
        return;
      }
      bridge.main(["node", "mcp-bridge", p.project.hash], { env: p.env });
      return;
    }
    // Local (Linux, D-077): spawn with inherited stdio; mirror exit code/signal.
    return await spawnRust(p.rt.binary, p.env);
  } catch (e) {
    fail("unimatrix: " + (e && e.message));
  }
}

module.exports = { run, runMcpStdio, NOT_ATTACHED_HINT };
