"use strict";

// Single definition of "Unimatrix project here" (ADR-004, D-045/D-052/D-091).
// Never throws; filesystem errors map to "absent". shim-entry.js carries an
// inlined copy of isUnimatrixProject/effectiveEnv (parity-tested, R-28).

const fs = require("fs");
const path = require("path");
const { walkToProjectRoot, computeProjectHash } = require("./hook-client/config.js");

const REMOTE_PREFIX = "UNIMATRIX_REMOTE_";
const HEADERS_VAR = REMOTE_PREFIX + "HEADERS";
// Variables whose presence (non-empty) makes the env tier "headless".
// HEADERS alone never does.
const HEADLESS_VARS = [
  "UNIMATRIX_BUNDLE",
  REMOTE_PREFIX + "MCP_URL",
  REMOTE_PREFIX + "URL",
  REMOTE_PREFIX + "TOKEN",
  REMOTE_PREFIX + "FINGERPRINT",
];

function nz(v) {
  return typeof v === "string" && v.trim() !== "";
}

function isFile(p) {
  try {
    return fs.statSync(p).isFile();
  } catch (_e) {
    return false;
  }
}

function isDir(p) {
  try {
    return fs.statSync(p).isDirectory();
  } catch (_e) {
    return false;
  }
}

function realpathOrNull(p) {
  try {
    return fs.realpathSync(p);
  } catch (_e) {
    return null;
  }
}

function isRemoteKey(k) {
  return k === "UNIMATRIX_BUNDLE" || k.startsWith(REMOTE_PREFIX);
}

function headlessEnvSet(env) {
  if (!env || typeof env !== "object") return false;
  if (nz(env.UNIMATRIX_BUNDLE)) return true;
  for (const k of Object.keys(env)) {
    if (k.startsWith(REMOTE_PREFIX) && k !== HEADERS_VAR && nz(env[k])) return true;
  }
  return false;
}

function headlessRootContains(env, cwd) {
  try {
    const r = env && env.UNIMATRIX_PROJECT_ROOT;
    if (!nz(r) || !path.isAbsolute(r)) return false;
    const rootReal = realpathOrNull(r);
    const cwdReal = realpathOrNull(cwd);
    if (rootReal === null || cwdReal === null) return false;
    const rel = path.relative(rootReal, cwdReal);
    return (
      rel === "" ||
      (rel !== ".." && !rel.startsWith(".." + path.sep) && !path.isAbsolute(rel))
    );
  } catch (_e) {
    return false;
  }
}

function storeDir(home, hash) {
  return path.join(home, ".unimatrix", hash);
}

function isUnimatrixProject(cwd, env, home) {
  try {
    const root = walkToProjectRoot(cwd);
    const hash = computeProjectHash(root);
    const sd = storeDir(home, hash);
    if (headlessEnvSet(env) && headlessRootContains(env, cwd)) {
      return { hash, kind: "headless", root };
    }
    if (isFile(path.join(sd, "remote.json"))) return { hash, kind: "cloud", root };
    if (isDir(sd)) return { hash, kind: "local", root };
    return null;
  } catch (_e) {
    return null;
  }
}

function readValidInstallJson(home) {
  try {
    const j = JSON.parse(
      fs.readFileSync(path.join(home, ".unimatrix", "install.json"), "utf8")
    );
    if (!j || typeof j !== "object") return null;
    if (![j.version, j.packageRoot, j.node].every((s) => typeof s === "string" && s)) {
      return null;
    }
    return isDir(j.packageRoot) ? j : null;
  } catch (_e) {
    return null;
  }
}

function isInstalled(project, platform, opts) {
  // {env, home} is REQUIRED (D-080): no fallback to process.env/os.homedir().
  if (!opts || typeof opts !== "object" || typeof opts.home !== "string" || !opts.env) {
    throw new TypeError("isInstalled requires {env, home}");
  }
  const { env, home } = opts;
  const headless = !!project && project.kind === "headless";
  const credFile = !!project && isFile(path.join(storeDir(home, project.hash), "remote.json"));
  const cred = headless || credFile;
  if (platform !== "linux") {
    const ok = (headless && nz(env.UNIMATRIX_BUNDLE)) || credFile;
    return ok
      ? { installed: true, transport: "cloud" }
      : { installed: false, transport: null, reason: "missing credential" };
  }
  if (cred) return { installed: true, transport: "cloud" };
  if (readValidInstallJson(home)) return { installed: true, transport: "local" };
  return { installed: false, transport: null, reason: "no install.json" };
}

function effectiveEnv(env, cwd, home) {
  try {
    if (!headlessEnvSet(env) || headlessRootContains(env, cwd)) return env;
    const copy = Object.assign({}, env);
    for (const k of Object.keys(copy)) if (isRemoteKey(k)) delete copy[k];
    return copy;
  } catch (_e) {
    return env;
  }
}

module.exports = {
  isUnimatrixProject,
  isInstalled,
  headlessEnvSet,
  headlessRootContains,
  effectiveEnv,
  storeDir,
  HEADLESS_VARS,
};
