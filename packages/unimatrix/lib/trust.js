"use strict";
// Trust check for the shim, install.json, redirect.json, trust.json and
// packageRoot (ADR-002, D-029, D-036, D-090). SELF-CONTAINED: Node builtins
// only, no package requires -- install-store copies this file verbatim to
// ~/.unimatrix/bin/trust.js and the copy runs before any packageRoot code.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

class TrustError extends Error {
  constructor(p, reason) {
    super(`unimatrix: trust check failed for ${p}: ${reason}; run 'npx unimatrix init'`);
    this.name = "TrustError";
    this.path = p;
    this.reason = reason;
  }
}

const udir = (home) => path.join(home, ".unimatrix");
const homeOf = (opts) => (opts && opts.home) || os.homedir();

function normalizeKey(p) {
  const s = String(p).replace(/\\/g, "/");
  return /^[A-Za-z]:/.test(s) ? s[0].toLowerCase() + s.slice(1) : s;
}

function lstatOrThrow(p) {
  try {
    return fs.lstatSync(p);
  } catch (e) {
    if (e && e.code === "ENOENT") throw new TrustError(p, "missing");
    throw new TrustError(p, `fs error ${(e && e.code) || "unknown"}`);
  }
}

function statIdentity(p) {
  const st = lstatOrThrow(p);
  if (st.isSymbolicLink()) throw new TrustError(p, "symlink not allowed");
  return { size: st.size, mtimeMs: st.mtimeMs, ino: st.ino, mode: st.mode };
}

function existing(p) {
  try {
    fs.lstatSync(p);
    return true;
  } catch (_) {
    return false;
  }
}

// Required members are always listed so a missing one is refused, not skipped.
function trustSet(home, ctx) {
  const c = ctx || {};
  const u = udir(home);
  const bin = path.join(u, "bin");
  const shim = path.join(bin, c.platform === "win32" ? "unimatrix.cmd" : "unimatrix");
  const set = [u, bin, shim, path.join(bin, "shim-entry.js"), path.join(bin, "trust.js"), path.join(u, "install.json")];
  for (const f of ["redirect.json", "trust.json"]) {
    const p = path.join(u, f);
    if (existing(p)) set.push(p);
  }
  const root = c.installJson && c.installJson.packageRoot;
  if (typeof root === "string" && root) {
    set.push(root, path.dirname(root));
  }
  if (typeof c.clientRoot === "string" && c.clientRoot) set.push(c.clientRoot);
  return Array.from(new Set(set));
}

function checkPosix(paths, opts) {
  const euid = opts && opts.uid !== undefined ? opts.uid : process.geteuid();
  const u = udir(homeOf(opts));
  const dirs = new Set([u, path.join(u, "bin"), path.join(u, "pkg")]);
  for (const p of paths) {
    const st = lstatOrThrow(p);
    if (st.isSymbolicLink()) throw new TrustError(p, "symlink not allowed");
    if (st.uid !== euid) throw new TrustError(p, "not owned by current user");
    if (st.mode & 0o022) throw new TrustError(p, "group/other writable");
    if (dirs.has(p) && st.mode & 0o077) throw new TrustError(p, "directory mode must be 0700");
  }
}

function readTrustJson(home) {
  try {
    const j = JSON.parse(fs.readFileSync(path.join(udir(home), "trust.json"), "utf8"));
    return j && typeof j === "object" && !Array.isArray(j) ? j : null;
  } catch (_) {
    return null;
  }
}

function sleepMs(ms) {
  try {
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
  } catch (_) {}
}

// Identity of every checked path (trust.json itself excluded) -> trust.json,
// tmp + rename. A failed write is not fatal: the next event re-checks.
function recordTrustIdentity(paths, home) {
  const tjKey = normalizeKey(path.join(udir(home), "trust.json"));
  const rec = {};
  try {
    for (const p of paths) {
      const k = normalizeKey(p);
      if (k !== tjKey) rec[k] = statIdentity(p);
    }
    const dest = path.join(udir(home), "trust.json");
    const tmp = `${dest}.tmp.${process.pid}.${Date.now()}.${Math.random().toString(36).slice(2)}`;
    fs.writeFileSync(tmp, JSON.stringify(rec), { mode: 0o600 });
    for (let i = 0; i < 4; i++) {
      try {
        fs.renameSync(tmp, dest);
        return;
      } catch (e) {
        if (i < 3 && e && (e.code === "EPERM" || e.code === "EBUSY")) sleepMs(10);
        else break;
      }
    }
    try {
      fs.unlinkSync(tmp);
    } catch (_) {}
  } catch (_) {}
}

const SID_ALLOW_BASE = ["S-1-5-18", "S-1-5-32-544", "S-1-3-0", "SY", "BA", "CO"];
const WRITE_BITS = 0x2 | 0x4 | 0x40 | 0x10000 | 0x40000 | 0x80000 | 0x10000000 | 0x40000000;
const WRITE_ALIAS = new Set(["GA", "GW", "FA", "FW", "WD", "AD", "WO", "DE", "DC", "SD", "WP", "DT"]);

function rightsGrantWrite(r) {
  if (/^0x/i.test(r)) return (Number.parseInt(r, 16) & WRITE_BITS) !== 0;
  for (let i = 0; i + 1 < r.length; i += 2) if (WRITE_ALIAS.has(r.slice(i, i + 2))) return true;
  return false;
}

function defaultSpawn(exe, args, extraEnv) {
  const env = extraEnv ? Object.assign({}, process.env, extraEnv) : process.env;
  const r = require("node:child_process").spawnSync(exe, args, { encoding: "utf8", windowsHide: true, timeout: 20000, env });
  return { status: r.status, stdout: r.stdout || "" };
}

function currentUserSid(run, sys32) {
  const r = run(path.win32.join(sys32, "whoami.exe"), ["/user", "/fo", "csv", "/nh"]);
  const m = r && r.status === 0 && /S-1-[\d-]+/.exec(String(r.stdout));
  if (!m) throw new TrustError("whoami", "cannot determine current user");
  return m[0];
}

function readSddl(run, sys32, p) {
  const tmp = path.join(os.tmpdir(), `unx-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}.sddl`);
  try {
    const r = run(path.win32.join(sys32, "icacls.exe"), [p, "/save", tmp]);
    if (!r || r.status !== 0) throw new TrustError(p, "icacls failed");
    let text = "";
    try {
      const b = fs.readFileSync(tmp);
      // icacls writes UTF-16LE with no BOM (captured: 61 00 63 00 ...)
      text = b.length > 1 && ((b[0] === 0xff && b[1] === 0xfe) || b[1] === 0) ? b.toString("utf16le") : b.toString("utf8");
    } catch (_) {}
    text = String(text || r.stdout || "").replace(/^\uFEFF/, "");
    // icacls /save emits the DACL only (no O:); the owner comes from ownersOf.
    const m = /^[ \t]*(?:O:|D:)[^\r\n]*/m.exec(text);
    if (!m) throw new TrustError(p, "cannot read ACL");
    return m[0].trim();
  } finally {
    try {
      fs.unlinkSync(tmp);
    } catch (_) {}
  }
}

// One PowerShell spawn resolves every owner SID (init / identity mismatch only).
// Paths travel in an env var, one SID per output line, in input order.
function ownersOf(run, sys32, paths) {
  const script =
    "$ErrorActionPreference='Stop';foreach($p in ($env:UNX_PATHS -split '\\|')){try{(Get-Acl -LiteralPath $p).GetOwner([System.Security.Principal.SecurityIdentifier]).Value}catch{''}}";
  const exe = path.win32.join(sys32, "WindowsPowerShell", "v1.0", "powershell.exe");
  const r = run(exe, ["-NoProfile", "-NonInteractive", "-Command", script], { UNX_PATHS: paths.join("|") });
  if (!r || r.status !== 0) throw new TrustError("powershell", "cannot read owner");
  const lines = String(r.stdout).split(/\r?\n/).map((l) => l.trim());
  const out = new Map();
  paths.forEach((p, i) => {
    if (/^S-1-[\d-]+$/.test(lines[i] || "")) out.set(p, lines[i]);
  });
  return out;
}

function checkSddl(p, sddl, me, protect) {
  const ow = /O:(S-[\d-]+|[A-Z]{2})/.exec(sddl);
  if (!ow || (ow[1] !== me && ow[1] !== "S-1-5-32-544" && ow[1] !== "BA")) throw new TrustError(p, "owner is not current user");
  const di = sddl.indexOf("D:");
  if (di < 0) throw new TrustError(p, "no DACL");
  const dacl = sddl.slice(di).split("S:")[0];
  const flags = dacl.slice(2).split("(")[0];
  if (protect && !flags.includes("P")) throw new TrustError(p, "inheritance not disabled");
  const allowed = new Set(SID_ALLOW_BASE.concat([me]));
  for (const m of dacl.matchAll(/\(([^()]*)\)/g)) {
    const f = m[1].split(";");
    if (f[0] !== "A" && f[0] !== "OA") continue;
    if (rightsGrantWrite(f[2] || "") && !allowed.has(f[5])) throw new TrustError(p, `extra writer ${f[5]}`);
  }
}

function fullCheckWindows(paths, opts) {
  const o = opts || {};
  const run = o.spawn || defaultSpawn;
  const root = (o.env && o.env.SystemRoot) || process.env.SystemRoot || "C:\\Windows";
  const sys32 = path.win32.join(root, "System32");
  const u = normalizeKey(udir(homeOf(o)));
  const prot = new Set([u, normalizeKey(path.join(udir(homeOf(o)), "bin"))]);
  const me = currentUserSid(run, sys32);
  let owners = null;
  for (const p of paths) {
    lstatOrThrow(p);
    let sd = readSddl(run, sys32, p);
    if (!/^O:/.test(sd)) {
      owners = owners || ownersOf(run, sys32, paths);
      const sid = owners.get(p);
      if (!sid) throw new TrustError(p, "cannot read owner");
      sd = `O:${sid}${sd}`;
    }
    checkSddl(p, sd, me, prot.has(normalizeKey(p)));
  }
}

function fullAndRewrite(paths, opts) {
  fullCheckWindows(paths, opts);
  recordTrustIdentity(paths, homeOf(opts));
}

function checkWindowsPerEvent(paths, opts) {
  const home = homeOf(opts);
  const tj = readTrustJson(home);
  if (tj === null) return fullAndRewrite(paths, opts);
  const tjKey = normalizeKey(path.join(udir(home), "trust.json"));
  const uKey = normalizeKey(udir(home));
  for (const p of paths) {
    const k = normalizeKey(p);
    if (k === tjKey) continue; // covered by the parent dir identity
    const cur = statIdentity(p);
    const rec = tj[k];
    if (!rec || typeof rec !== "object") return fullAndRewrite(paths, opts);
    if (k === uKey) {
      // size/mtime excluded: every legitimate trust.json rewrite changes dir mtime
      if (rec.ino !== cur.ino || rec.mode !== cur.mode) return fullAndRewrite(paths, opts);
    } else if (rec.size !== cur.size || rec.mtimeMs !== cur.mtimeMs || rec.ino !== cur.ino) {
      return fullAndRewrite(paths, opts);
    }
  }
}

// Per-event entry point. Throws TrustError; never spawns on POSIX.
function checkTrust(paths, platform, opts) {
  if (platform === "win32") return checkWindowsPerEvent(paths, opts);
  return checkPosix(paths, opts);
}

module.exports = { TrustError, checkTrust, trustSet, fullCheckWindows, recordTrustIdentity, statIdentity, normalizeKey, rightsGrantWrite };
