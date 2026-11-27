"use strict";

// Per-machine user-level footprint (ADR-001, D-030, D-031, D-036, D-078, D-082, D-090):
// stable package copy, shim + shim-entry.js + trust.js, install.json atomic swap,
// prune, root marker, explicit modes/ACLs, Windows trust.json.

const fs = require("fs");
const path = require("path");
const { walkToProjectRoot, computeProjectHash } = require("./hook-client/config.js");
const { PLATFORMS } = require("./resolve-runtime.js");
const { VARS } = require("./hook-client/remote-resolve.js");
const crypto = require("crypto");
const trust = require("./trust.js");
const sw = require("./safe-write.js");

// Flip to true iff the ADR-005 not-a-project benchmark breaches on any OS (D-025).
const PRENODE_ENABLED = false;
const TMP_AGE_MS = 60000;

class UnsafeShimPathError extends Error {
  constructor(p, ch) {
    super(`unimatrix: unsafe character ${JSON.stringify(ch)} in path ${p}; use a different home directory`);
    this.name = "UnsafeShimPathError";
    this.path = p;
    this.ch = ch;
  }
}

class NodeNotRecordableError extends Error {
  constructor(p) {
    super(`unimatrix: cannot record node (${p} missing or not absolute); install Node and re-run 'npx unimatrix init'`);
    this.name = "NodeNotRecordableError";
    this.path = p;
  }
}

class BinaryPackageMissingError extends Error {
  constructor(pkg) {
    super(`unimatrix: platform binary package ${pkg} not found; reinstall @dug-21/unimatrix and re-run 'npx unimatrix init'`);
    this.name = "BinaryPackageMissingError";
    this.pkg = pkg;
  }
}

function checkShimPathSafe(p, platform) {
  const bad = platform === "win32" ? ["%", '"'] : ["'"];
  for (const ch of bad) if (String(p).includes(ch)) throw new UnsafeShimPathError(p, ch);
}

const legFor = (file, action) => ({ harness: "unimatrix", surface: "store", action, path: file });
const tmpDirName = (dest) => path.join(path.dirname(dest), `.${path.basename(dest)}.tmp-${crypto.randomBytes(8).toString("hex")}`);

function isFile(p) {
  try {
    return fs.statSync(p).isFile();
  } catch (_) {
    return false;
  }
}

function rmrf(p) {
  try {
    fs.rmSync(p, { recursive: true, force: true, maxRetries: 3, retryDelay: 25 });
  } catch (_) {}
}

function mkdir0700(p) {
  fs.mkdirSync(p, { recursive: true, mode: 0o700 });
  fs.chmodSync(p, 0o700);
}

function readInstallJson(home) {
  try {
    const j = JSON.parse(fs.readFileSync(path.join(home, ".unimatrix", "install.json"), "utf8"));
    return j && typeof j === "object" && !Array.isArray(j) ? j : null;
  } catch (_) {
    return null;
  }
}

function lockdownWindows(p, spawn) {
  const run = spawn || ((exe, args) => require("child_process").spawnSync(exe, args, { windowsHide: true, timeout: 15000 }));
  const root = process.env.SystemRoot || "C:\\Windows";
  const user = process.env.USERNAME || "";
  const domain = process.env.USERDOMAIN;
  const who = domain ? `${domain}\\${user}` : user;
  const r = run(path.win32.join(root, "System32", "icacls.exe"), [p, "/inheritance:r", "/grant:r", `${who}:(OI)(CI)F`]);
  if (!r || r.status !== 0) throw new Error(`unimatrix: could not restrict ACL on ${p}`);
}

// Dirs 0700; files keep only the owner bits (exec preserved). Posix only.
function chmodTree(dir) {
  fs.chmodSync(dir, 0o700);
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) chmodTree(p);
    else if (e.isFile()) fs.chmodSync(p, fs.statSync(p).mode & 0o100 ? 0o700 : 0o600);
  }
}

function copyBinaryPackage(src, tmp, platform, arch, required) {
  const pkg = platform === "linux" ? PLATFORMS[`${platform}-${arch}`] : null;
  if (!pkg) return;
  let from = null;
  try {
    from = path.dirname(require.resolve(`${pkg}/package.json`, { paths: [src] }));
  } catch (_) {}
  if (!from) {
    if (required) throw new BinaryPackageMissingError(pkg);
    return;
  }
  const to = path.join(tmp, "node_modules", ...pkg.split("/"));
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.cpSync(from, to, { recursive: true, dereference: true });
}

function isCompleteCopy(dest) {
  return isFile(path.join(dest, ".complete"));
}

function mkTmpDir(dest) {
  for (let i = 0; ; i++) {
    const t = tmpDirName(dest);
    try {
      fs.mkdirSync(t);
      return t;
    } catch (e) {
      if (i >= 4 || !e || e.code !== "EEXIST") throw e;
    }
  }
}

// Stage in an exclusive sibling dir, then one atomic dir rename (D-020, §3.8).
function copyPackage(o, dest, platform, fault) {
  const tmp = mkTmpDir(dest);
  try {
    for (const name of ["bin", "lib", "package.json"]) {
      const from = path.join(o.sourcePackageRoot, name);
      if (!fs.existsSync(from)) continue;
      fs.cpSync(from, path.join(tmp, name), {
        recursive: true,
        dereference: true,
        filter: (s) => !/[\\/](node_modules|test|opencode-plugin)([\\/]|$)/.test(path.relative(o.sourcePackageRoot, s)),
      });
    }
    fault("copy-mid");
    copyBinaryPackage(o.sourcePackageRoot, tmp, platform, o.arch || process.arch, o.requireBinary);
    if (platform !== "win32") chmodTree(tmp);
    const fd = fs.openSync(path.join(tmp, ".complete"), "wx", 0o600);
    try {
      fs.fsyncSync(fd);
    } finally {
      fs.closeSync(fd);
    }
    try {
      const dfd = fs.openSync(tmp, "r");
      try {
        fs.fsyncSync(dfd);
      } finally {
        fs.closeSync(dfd);
      }
    } catch (_) {}
    fault("before-pkg-rename");
    // Never delete a valid dest (install.json may point at it): keep it, drop ours.
    for (let i = 0; i < 5 && !isCompleteCopy(dest); i++) {
      try {
        sw.renameWithRetry(tmp, dest, platform);
        return;
      } catch (e) {
        if (isCompleteCopy(dest)) break; // concurrent run won; its copy is identical
        if (!fs.existsSync(dest)) continue;
        // invalid dest: atomically move it aside (never rm in place), then retry
        const aside = tmpDirName(dest);
        try {
          fs.renameSync(dest, aside);
          rmrf(aside);
        } catch (_) {}
      }
    }
    if (!isCompleteCopy(dest)) throw new Error("unimatrix: could not install package copy");
    rmrf(tmp);
  } catch (e) {
    rmrf(tmp);
    throw e;
  }
}

function prune(pkgDir, keep) {
  let names = [];
  try {
    names = fs.readdirSync(pkgDir);
  } catch (_) {
    return [];
  }
  const pruned = [];
  const now = Date.now();
  for (const n of names) {
    const p = path.join(pkgDir, n);
    if (n.includes(".tmp-")) {
      let old = false;
      try {
        old = now - fs.statSync(p).mtimeMs > TMP_AGE_MS;
      } catch (_) {}
      if (old) rmrf(p);
    } else if (!keep.has(n)) {
      rmrf(p);
      pruned.push(n);
    }
  }
  return pruned;
}

function refuseSymlinkedTrustJson(tj) {
  let lst;
  try {
    lst = fs.lstatSync(tj);
  } catch (_) {
    return;
  }
  if (lst.isSymbolicLink()) {
    const link = safeReadlink(tj);
    throw new Error(`unimatrix: ${tj} is a symlink (-> ${link}); trust state is never written through a symlink. Remove it ('rm "${tj}"') and re-run 'npx unimatrix init'`);
  }
}

function safeReadlink(p) {
  try {
    return fs.readlinkSync(p);
  } catch (_) {
    return "?";
  }
}

function plannedTrustAction(set, home) {
  const tj = path.join(home, ".unimatrix", "trust.json");
  let cur = null;
  try {
    cur = fs.readFileSync(tj, "utf8");
  } catch (_) {}
  if (cur === null) return "created";
  try {
    return trust.trustRecordFor(set, home) === cur ? "unchanged" : "updated";
  } catch (_) {
    return "updated";
  }
}

function writeRootMarker(home, projectRoot, dryRun) {
  const root = walkToProjectRoot(projectRoot);
  const dir = path.join(home, ".unimatrix", computeProjectHash(root));
  const file = path.join(dir, "root");
  if (!dryRun) mkdir0700(dir);
  const r = sw.writeFileSync(file, root + "\n", { dryRun: !!dryRun, enforceMode: 0o600, newMode: 0o600 });
  return { file, action: r.action };
}

function render(template, o, uh, platform) {
  const vars = Object.values(VARS).filter((v) => v !== VARS.HEADERS);
  let text = template
    .split("@@PRENODE_CALL@@").join(PRENODE_ENABLED ? (platform === "win32" ? "call :may_be_project & if errorlevel 1 exit /b 0" : "unimatrix_may_be_project || exit 0") : "")
    .split("@@NODE@@").join(o.nodePath)
    .split("@@UHOME@@").join(uh)
    .split("@@REMOTE_VARS@@").join(vars.join(" "));
  text = text.replace(/\r?\n/g, platform === "win32" ? "\r\n" : "\n");
  return text;
}

/**
 * Writes the per-machine footprint. o: {home, platform, arch?, nodePath,
 * sourcePackageRoot, projectRoot, dryRun, requireBinary, libDir?, spawn?, _fault?}
 */
function installStore(o) {
  const platform = o.platform || process.platform;
  const fault = o._fault || (() => {});
  const uh = path.join(o.home, ".unimatrix");
  const bin = path.join(uh, "bin");
  const pkgDir = path.join(uh, "pkg");
  const libDir = o.libDir || __dirname;
  const shim = path.join(bin, platform === "win32" ? "unimatrix.cmd" : "unimatrix");
  checkShimPathSafe(shim, platform);
  checkShimPathSafe(o.nodePath, platform);
  checkShimPathSafe(uh, platform);
  const isAbs = platform === "win32" ? path.win32.isAbsolute : path.isAbsolute;
  if (typeof o.nodePath !== "string" || !isAbs(o.nodePath) || !isFile(o.nodePath)) throw new NodeNotRecordableError(o.nodePath);

  const version = JSON.parse(fs.readFileSync(path.join(o.sourcePackageRoot, "package.json"), "utf8")).version;
  const dest = path.join(pkgDir, version);
  const prev = readInstallJson(o.home);
  const rerecordedNode = prev === null || prev.node !== o.nodePath;
  const installJson = path.join(uh, "install.json");
  const dryRun = !!o.dryRun;
  const mode = platform === "win32" ? 0o600 : 0o700;
  const legs = [];
  const put = (file, data, m) => legs.push(legFor(file, sw.writeFileSync(file, data, { dryRun, enforceMode: m, newMode: m, platform }).action));

  // 1. directories (none in dry-run, FR-62)
  if (!dryRun) {
    for (const d of [uh, bin, pkgDir]) mkdir0700(d);
    if (platform === "win32") {
      lockdownWindows(uh, o.spawn);
      lockdownWindows(bin, o.spawn);
    }
  }
  // 2. stable package copy
  const pkgAction = isCompleteCopy(dest) ? "unchanged" : "created";
  if (!dryRun && pkgAction === "created") copyPackage(o, dest, platform, fault);
  legs.push(legFor(dest, pkgAction));
  // 3. shim
  const tplName = platform === "win32" ? "unimatrix.cmd" : "unimatrix.sh";
  const template = fs.readFileSync(path.join(libDir, "shim", tplName), "utf8");
  put(shim, render(template, o, uh, platform), mode);
  // 4. shim-entry.js + trust.js siblings
  for (const f of ["shim-entry.js", "trust.js"]) {
    fault("sibling-" + f);
    put(path.join(bin, f), fs.readFileSync(path.join(libDir, f)), mode);
  }
  // 5. install.json swap
  fault("install-json");
  put(installJson, JSON.stringify({ version, packageRoot: dest, node: o.nodePath }), 0o600);
  // 6. prune
  let prunedVersions = [];
  if (!dryRun) {
    const keep = new Set([version]);
    if (prev && typeof prev.version === "string") keep.add(prev.version);
    prunedVersions = prune(pkgDir, keep);
  }
  // 7. root marker
  const rm = writeRootMarker(o.home, o.projectRoot, dryRun);
  legs.push(legFor(rm.file, rm.action));
  // 8. trust
  const tj = path.join(uh, "trust.json");
  refuseSymlinkedTrustJson(tj);
  const set = trust.trustSet(o.home, { platform, installJson: { packageRoot: dest } });
  try {
    if (platform === "win32") {
      let a;
      if (dryRun) a = plannedTrustAction(set, o.home);
      else {
        trust.fullCheckWindows(set, { home: o.home, spawn: o.spawn });
        a = trust.recordTrustIdentity(set, o.home) || "updated";
      }
      legs.push(legFor(tj, a));
    } else if (!dryRun) trust.checkTrust(set, platform, { home: o.home });
  } catch (e) {
    if (e instanceof trust.TrustError && /symlink not allowed/.test(e.reason || e.message) && e.path) {
      throw new Error(`unimatrix: ${e.path} is a symlink (-> ${safeReadlink(e.path)}); trust files must be regular files. Remove it ('rm "${e.path}"') and re-run 'npx unimatrix init'`);
    }
    throw e;
  }
  const pre = dryRun ? "[dry-run] would write " : "wrote ";
  return { legs, actions: legs.map((l) => pre + l.path), shimPath: shim, version, prunedVersions, rerecordedNode };
}

module.exports = {
  installStore,
  checkShimPathSafe,
  readInstallJson,
  writeRootMarker,
  refuseSymlinkedTrustJson,
  UnsafeShimPathError,
  NodeNotRecordableError,
  BinaryPackageMissingError,
  PRENODE_ENABLED,
};
