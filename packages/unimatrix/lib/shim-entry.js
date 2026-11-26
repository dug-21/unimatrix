"use strict";
// Version-independent entry run by the shim (~/.unimatrix/bin/shim-entry.js).
// Order (ADR-002/004): project predicate -> sibling trust.js -> installed
// decision -> require(<packageRoot>/bin/unimatrix.js).run in-process.
// Node builtins only before trust passes. The predicate here is an inlined
// copy of project-state.js (parity-tested, R-28); no ancestor-.git clause (D-079).
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");

const RP = "UNIMATRIX_REMOTE_";
const nz = (v) => typeof v === "string" && v.trim() !== "";
const isFile = (p) => { try { return fs.statSync(p).isFile(); } catch (_e) { return false; } };
const isDir = (p) => { try { return fs.statSync(p).isDirectory(); } catch (_e) { return false; } };
const realpathOrSelf = (p) => { try { return fs.realpathSync(p); } catch (_e) { return p; } };
const realpathOrNull = (p) => { try { return fs.realpathSync(p); } catch (_e) { return null; } };
const err = (m) => { try { process.stderr.write(m + "\n"); } catch (_e) { /* ignore */ } };

function resolveGitFile(gitFile, dir) {
  try {
    const line = fs.readFileSync(gitFile, "utf8").split("\n").find((l) => l.startsWith("gitdir:"));
    if (line) {
      const raw = line.slice(7).trim();
      let a = fs.realpathSync(path.isAbsolute(raw) ? raw : path.join(dir, raw));
      for (;;) {
        if (path.basename(a) === ".git" && fs.statSync(a).isDirectory()) return realpathOrSelf(path.dirname(a));
        const parent = path.dirname(a);
        if (parent === a) break;
        a = parent;
      }
    }
  } catch (_e) { /* fall through */ }
  return realpathOrSelf(dir);
}

function walkToProjectRoot(startDir) {
  let cur;
  try { cur = path.resolve(startDir); } catch (_e) { return startDir; }
  for (;;) {
    let st = null;
    try { st = fs.statSync(path.join(cur, ".git")); } catch (_e) { /* keep walking */ }
    if (st) return st.isFile() ? resolveGitFile(path.join(cur, ".git"), cur) : realpathOrSelf(cur);
    const parent = path.dirname(cur);
    if (parent === cur) return realpathOrSelf(path.resolve(startDir));
    cur = parent;
  }
}

const computeProjectHash = (root) =>
  crypto.createHash("sha256").update(String(root), "utf8").digest("hex").slice(0, 16);

function headlessEnvSet(env) {
  if (!env || typeof env !== "object") return false;
  if (nz(env.UNIMATRIX_BUNDLE)) return true;
  for (const k of Object.keys(env)) if (k.startsWith(RP) && k !== RP + "HEADERS" && nz(env[k])) return true;
  return false;
}

function headlessRootContains(env, cwd) {
  try {
    const r = env && env.UNIMATRIX_PROJECT_ROOT;
    if (!nz(r) || !path.isAbsolute(r)) return false;
    const a = realpathOrNull(r);
    const b = realpathOrNull(cwd);
    if (a === null || b === null) return false;
    const rel = path.relative(a, b);
    return rel === "" || (rel !== ".." && !rel.startsWith(".." + path.sep) && !path.isAbsolute(rel));
  } catch (_e) { return false; }
}

function isUnimatrixProject(cwd, env, home) {
  try {
    const root = walkToProjectRoot(cwd);
    const hash = computeProjectHash(root);
    const sd = path.join(home, ".unimatrix", hash);
    if (headlessEnvSet(env) && headlessRootContains(env, cwd)) return { hash, kind: "headless", root };
    if (isFile(path.join(sd, "remote.json"))) return { hash, kind: "cloud", root };
    if (isDir(sd)) return { hash, kind: "local", root };
    return null;
  } catch (_e) { return null; }
}

function effectiveEnv(env, cwd, _home) {
  try {
    if (!headlessEnvSet(env) || headlessRootContains(env, cwd)) return env;
    const copy = Object.assign({}, env);
    for (const k of Object.keys(copy)) if (k === "UNIMATRIX_BUNDLE" || k.startsWith(RP)) delete copy[k];
    return copy;
  } catch (_e) { return env; }
}

function readInstallJsonRaw(uh) {
  try {
    const j = JSON.parse(fs.readFileSync(path.join(uh, "install.json"), "utf8"));
    return j && typeof j === "object" ? j : null;
  } catch (_e) { return null; }
}

function installedDecision(project, platform, env, home, inst) {
  const headless = !!project && project.kind === "headless";
  const credFile = !!project && isFile(path.join(home, ".unimatrix", project.hash, "remote.json"));
  if (platform !== "linux") {
    const ok = (headless && nz(env.UNIMATRIX_BUNDLE)) || credFile;
    return ok ? { installed: true, transport: "cloud" } : { installed: false, transport: null, reason: "missing credential" };
  }
  if (headless || credFile) return { installed: true, transport: "cloud" };
  const valid = !!inst && [inst.version, inst.packageRoot, inst.node].every((s) => typeof s === "string" && s) && isDir(inst.packageRoot);
  return valid ? { installed: true, transport: "local" } : { installed: false, transport: null, reason: "no install.json" };
}

// The sibling is loaded before it can vouch for itself: stat it first (POSIX).
function loadTrust(platform) {
  const p = path.join(__dirname, "trust.js");
  if (platform !== "win32") {
    let st;
    try { st = fs.lstatSync(p); } catch (_e) { throw new Error(`unimatrix: trust check failed for ${p}: missing; run 'npx unimatrix init'`); }
    if (st.isSymbolicLink() || st.uid !== process.geteuid() || (st.mode & 0o022)) {
      throw new Error(`unimatrix: trust check failed for ${p}: unsafe permissions or owner; run 'npx unimatrix init'`);
    }
  }
  return require(p);
}

function run(args) {
  try {
    const home = os.homedir();
    const uh = path.join(home, ".unimatrix");
    const platform = process.platform;
    const env = process.env;
    const verb = args[0];
    const project = isUnimatrixProject(process.cwd(), env, home);
    if (project === null) return;
    const inst = readInstallJsonRaw(uh);
    // Installed-ness is decided read-only BEFORE trust (ADR-004): a project machine
    // with no install.json has no trust set to verify and must reach the exit-table
    // outcome, not a trust error. Trust runs whenever install.json exists on Linux
    // (so a half-removed install is refused loudly) or the machine is installed, and
    // always before any packageRoot load.
    const d = installedDecision(project, platform, env, home, inst);
    const notAttached = () => {
      const hint = "unimatrix: not attached on this machine; run 'npx unimatrix init'";
      if (verb === "mcp-stdio") { err(hint); process.exitCode = 1; return; }
      if (platform === "linux") err(hint);
    };
    if (!d.installed && !(platform === "linux" && isFile(path.join(uh, "install.json")))) return notAttached();
    try {
      const trust = loadTrust(platform);
      trust.checkTrust(trust.trustSet(home, { installJson: inst, platform }), platform, { home });
    } catch (e) {
      err(e && e.message);
      process.exitCode = 1;
      return;
    }
    if (!d.installed) return notAttached();
    if (!inst || typeof inst.packageRoot !== "string") {
      err("unimatrix: install pointer missing or invalid; run 'npx unimatrix init'");
      process.exitCode = 1;
      return;
    }
    let launcher;
    try {
      launcher = require(path.join(inst.packageRoot, "bin", "unimatrix.js"));
    } catch (e) {
      err("unimatrix: cannot load " + inst.packageRoot + ": " + (e && e.message));
      process.exitCode = 1;
      return;
    }
    return Promise.resolve()
      .then(() => launcher.run(args, { preflight: { project, trustChecked: true, install: inst, decision: d } }))
      .catch((e) => { err("unimatrix: " + (e && e.message)); process.exitCode = 1; });
  } catch (e) {
    err("unimatrix: " + (e && e.message));
    process.exitCode = 1;
  }
}

module.exports = { isUnimatrixProject, effectiveEnv, installedDecision, run };

if (require.main === module) run(process.argv.slice(2));
