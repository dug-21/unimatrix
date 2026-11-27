"use strict";
// Trust check for the shim, install.json, redirect.json, trust.json and
// packageRoot (ADR-002, D-029, D-036, D-090). SELF-CONTAINED: Node builtins
// only, no package requires -- install-store copies this file verbatim to
// ~/.unimatrix/bin/trust.js and the copy runs before any packageRoot code.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const crypto = require("node:crypto");

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

// ---- Hardened writer core (ADR-003, ADR-005). lib/safe-write.js is the API. ----
class SafeWriteError extends Error {
  constructor(code, file, msg) {
    super(msg);
    this.name = "SafeWriteError";
    this.code = code;
    this.file = file;
  }
}

const sha256 = (b) => crypto.createHash("sha256").update(b).digest("hex");
const tempPath = (dest) => path.join(path.dirname(dest), `.${path.basename(dest)}.tmp-${crypto.randomBytes(8).toString("hex")}`);
const RETRY_CODES = new Set(["EPERM", "EBUSY", "EACCES"]);

function renameWithRetry(from, to, platform, retry) {
  const r = retry || { count: 10, delayMs: 25 };
  const max = platform === "win32" ? r.count : 1;
  for (let i = 1; ; i++) {
    try {
      return fs.renameSync(from, to);
    } catch (e) {
      if (i < max && e && RETRY_CODES.has(e.code)) sleepMs(r.delayMs);
      else throw e;
    }
  }
}

// realpath of the deepest existing ancestor + the remaining components.
function realPathForm(p) {
  let cur = path.resolve(p);
  const rest = [];
  for (;;) {
    try {
      return path.join(fs.realpathSync.native(cur), ...rest.reverse());
    } catch (_) {
      const up = path.dirname(cur);
      if (up === cur) return path.join(cur, ...rest.reverse());
      rest.push(path.basename(cur));
      cur = up;
    }
  }
}

function resolveTarget(file) {
  const loop = () => new SafeWriteError("not-regular", file, `${file}: symlink loop`);
  let lst;
  try {
    lst = fs.lstatSync(file);
  } catch (e) {
    if (e && e.code === "ENOENT") return { target: realPathForm(file), isLink: false };
    if (e && e.code === "ELOOP") throw loop();
    throw new SafeWriteError("io", file, `${file}: ${e && e.message}`);
  }
  if (lst.isSymbolicLink()) {
    const linkText = fs.readlinkSync(file);
    let target;
    try {
      target = fs.realpathSync.native(file);
    } catch (e) {
      if (e && e.code === "ENOENT") throw new SafeWriteError("dangling-symlink", file, `${file} is a dangling symlink (-> ${linkText}); not written`);
      if (e && e.code === "ELOOP") throw loop();
      throw new SafeWriteError("io", file, `${file}: ${e && e.message}`);
    }
    if (!fs.statSync(target).isFile()) throw new SafeWriteError("not-regular", file, `${file} -> ${target} is not a regular file`);
    return { target, isLink: true, linkText };
  }
  if (!lst.isFile()) throw new SafeWriteError("not-regular", file, `${file} is not a regular file`);
  return { target: fs.realpathSync.native(file), isLink: false };
}

function checkContained(file, target, containWithin, platform) {
  if (!containWithin) return;
  const root = realPathForm(containWithin);
  const ci = platform === "win32" || platform === "darwin";
  const n = (s) => (ci ? s.replace(/\\/g, "/").toLowerCase() : s);
  const t = n(target);
  const r = n(root).replace(/[\\/]+$/, "");
  if (t === r || t.startsWith(r + (ci ? "/" : path.sep))) return;
  throw new SafeWriteError("outside-project", file, `${file} resolves to ${target}, outside project root ${containWithin}; not written`);
}

function readState(target) {
  try {
    const bytes = fs.readFileSync(target);
    return { bytes, hash: sha256(bytes), mode: fs.statSync(target).mode & 0o777 };
  } catch (e) {
    if (e && e.code === "ENOENT") return { bytes: null, hash: "absent", mode: null };
    throw new SafeWriteError("io", target, `${target}: ${e && e.message}`);
  }
}

// One attempt: write `data` to the resolved target of `file` iff it still
// matches opts.expect {target, hash}. Throws SafeWriteError("conflict") on CAS
// mismatch (the caller retries). The temp file never survives a failure.
function hardenedWriteSync(file, data, opts) {
  const o = opts;
  const dest = o.expect.target;
  const buf = Buffer.isBuffer(data) ? data : Buffer.from(String(data), "utf8");
  const mode = o.mode === undefined || o.mode === null ? 0o600 : o.mode;
  const fault = o.fault || (() => {});
  let tmp = null;
  let fd = null;
  try {
    const dir = path.dirname(dest);
    fs.mkdirSync(dir, { recursive: true });
    for (let k = 0; fd === null; k++) {
      tmp = tempPath(dest);
      try {
        fd = fs.openSync(tmp, "wx", mode);
      } catch (e) {
        tmp = null;
        if (e && e.code === "EEXIST" && k < 4) continue;
        if (e && e.code === "EEXIST") throw new SafeWriteError("io", file, `${file}: could not create temp file`);
        throw new SafeWriteError("io", dest, `${dest}: ${e && e.message}`);
      }
    }
    for (let off = 0; off < buf.length; ) off += fs.writeSync(fd, buf, off, buf.length - off);
    if (o.platform !== "win32") fs.fchmodSync(fd, mode);
    fs.fsyncSync(fd);
    fs.closeSync(fd);
    fd = null;
    fault("before-rename");
    const now = resolveTarget(file);
    checkContained(file, now.target, o.containWithin, o.platform);
    if (now.target !== dest || readState(now.target).hash !== o.expect.hash) throw new SafeWriteError("conflict", file, `${file} changed while writing`);
    renameWithRetry(tmp, dest, o.platform, o.retry);
    tmp = null;
    if (o.platform !== "win32") {
      try {
        const dfd = fs.openSync(dir, "r");
        try {
          fs.fsyncSync(dfd);
        } finally {
          fs.closeSync(dfd);
        }
      } catch (_) {}
    }
    return "written";
  } catch (e) {
    if (fd !== null) {
      try {
        fs.closeSync(fd);
      } catch (_) {}
    }
    if (tmp !== null) {
      try {
        fs.unlinkSync(tmp);
      } catch (_) {}
    }
    throw e instanceof SafeWriteError ? e : new SafeWriteError("io", file, `${file}: ${e && e.message}`);
  }
}

// Identity of every checked path (trust.json itself excluded), serialised once.
function trustRecordFor(paths, home) {
  const tjKey = normalizeKey(path.join(udir(home), "trust.json"));
  const rec = {};
  for (const p of paths) {
    const k = normalizeKey(p);
    if (k !== tjKey) rec[k] = statIdentity(p);
  }
  return JSON.stringify(rec);
}

// Hook-time trust.json rewrite through the hardened core. A trust.json symlink
// is skipped (never followed); any failure is non-fatal: the next event re-checks.
function recordTrustIdentity(paths, home) {
  try {
    const dest = path.join(udir(home), "trust.json");
    let lst = null;
    try {
      lst = fs.lstatSync(dest);
    } catch (_) {}
    if (lst && lst.isSymbolicLink()) return "unchanged";
    const data = Buffer.from(trustRecordFor(paths, home), "utf8");
    const r = resolveTarget(dest);
    const cur = readState(r.target);
    if (cur.bytes !== null && cur.bytes.equals(data)) {
      if (cur.mode === 0o600 || process.platform === "win32") return "unchanged";
      fs.chmodSync(r.target, 0o600);
      return "updated";
    }
    hardenedWriteSync(dest, data, { expect: { target: r.target, hash: cur.hash }, mode: 0o600, platform: process.platform, retry: { count: 4, delayMs: 10 } });
    return cur.bytes === null ? "created" : "updated";
  } catch (_) {
    return undefined;
  }
}

const SID_ALLOW_BASE = ["S-1-5-18", "S-1-5-32-544", "S-1-3-0", "SY", "BA", "CO", "OW"];
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
  return { status: r.status, stdout: r.stdout || "", stderr: r.stderr || "" };
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
  // .NET only: Get-Acl needs a module load that fails under a foreign PSModulePath.
  const script =
    "$ErrorActionPreference='Stop';$o=[System.Security.AccessControl.AccessControlSections]::Owner;$t=[System.Security.Principal.SecurityIdentifier];" +
    "foreach($p in ($env:UNX_PATHS -split '\\|')){try{if([System.IO.Directory]::Exists($p)){$a=(New-Object System.IO.DirectoryInfo $p).GetAccessControl($o)}else{$a=(New-Object System.IO.FileInfo $p).GetAccessControl($o)};$a.GetOwner($t).Value}catch{'ERR '+$_.Exception.Message}}";
  const exe = path.win32.join(sys32, "WindowsPowerShell", "v1.0", "powershell.exe");
  const r = run(exe, ["-NoProfile", "-NonInteractive", "-Command", script], { UNX_PATHS: paths.join("|") });
  if (!r || r.status !== 0) throw new TrustError("powershell", `cannot read owner (status ${r && r.status}; ${String((r && r.stderr) || "").slice(0, 160).replace(/\s+/g, " ")})`);
  const lines = String(r.stdout).split(/\r?\n/).map((l) => l.trim());
  const out = new Map();
  out.raw = r.stdout;
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
  // SDDL alias LA = the machine's RID-500 account; it is "me" when me ends in -500.
  const allowed = new Set(SID_ALLOW_BASE.concat(/-500$/.test(me) ? [me, "LA"] : [me]));
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
      if (!sid) throw new TrustError(p, `cannot read owner (${String(owners.raw).slice(0, 200).replace(/\s+/g, " ")})`);
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

module.exports = {
  TrustError,
  checkTrust,
  trustSet,
  fullCheckWindows,
  recordTrustIdentity,
  statIdentity,
  normalizeKey,
  rightsGrantWrite,
  SafeWriteError,
  hardenedWriteSync,
  renameWithRetry,
  tempPath,
  resolveTarget,
  checkContained,
  readState,
  sha256,
  trustRecordFor,
  realPathForm,
};
