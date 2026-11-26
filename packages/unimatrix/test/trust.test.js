"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { TrustError, checkTrust, trustSet, recordTrustIdentity, statIdentity, rightsGrantWrite } = require("../lib/trust.js");

const posixOnly = process.platform === "win32" ? { skip: "posix" } : {};
const ME = "S-1-5-21-1-2-3-1001";

function mkHome() {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "trust-"));
  const u = path.join(home, ".unimatrix");
  const bin = path.join(u, "bin");
  fs.mkdirSync(bin, { recursive: true, mode: 0o700 });
  fs.chmodSync(u, 0o700);
  fs.chmodSync(bin, 0o700);
  for (const f of [path.join(bin, "unimatrix"), path.join(bin, "unimatrix.cmd"), path.join(bin, "shim-entry.js"), path.join(bin, "trust.js"), path.join(u, "install.json")]) {
    fs.writeFileSync(f, "x", { mode: 0o600 });
  }
  return { home, u, bin };
}
const set = (h, platform) => trustSet(h.home, { platform });

test("posix: clean set passes, umask 002 irrelevant", posixOnly, () => {
  const h = mkHome();
  const old = process.umask(0o002);
  try { fs.writeFileSync(path.join(h.u, "redirect.json"), "{}", { mode: 0o600 }); } finally { process.umask(old); }
  fs.chmodSync(path.join(h.u, "redirect.json"), 0o600);
  checkTrust(set(h), "linux", { home: h.home });
});

for (const [name, rel] of [["install.json", "install.json"], ["redirect.json", "redirect.json"], ["shim", "bin/unimatrix"], ["trust.json", "trust.json"]]) {
  test(`posix: group-writable ${name} refused naming path`, posixOnly, () => {
    const h = mkHome();
    const f = path.join(h.u, rel);
    if (!fs.existsSync(f)) fs.writeFileSync(f, "{}");
    fs.chmodSync(f, 0o660);
    assert.throws(() => checkTrust(set(h), "linux", { home: h.home }), (e) => e instanceof TrustError && e.path === f && /group\/other writable/.test(e.reason));
  });
}

test("posix: ~/.unimatrix and bin not 0700 refused", posixOnly, () => {
  for (const which of ["u", "bin"]) {
    const h = mkHome();
    fs.chmodSync(h[which], 0o750);
    assert.throws(() => checkTrust(set(h), "linux", { home: h.home }), (e) => e.path === h[which] && /0700/.test(e.reason));
  }
});

test("posix: foreign owner refused (injected uid)", posixOnly, () => {
  const h = mkHome();
  assert.throws(() => checkTrust(set(h), "linux", { home: h.home, uid: process.geteuid() + 1 }), (e) => /not owned/.test(e.reason));
});

test("posix: symlinked redirect/install/shim refused, not resolved", posixOnly, () => {
  for (const rel of ["redirect.json", "install.json", "bin/unimatrix"]) {
    const h = mkHome();
    const f = path.join(h.u, rel);
    const real = path.join(h.home, "real");
    fs.writeFileSync(real, "{}", { mode: 0o600 });
    try { fs.unlinkSync(f); } catch (_) {}
    fs.symlinkSync(real, f);
    assert.throws(() => checkTrust(set(h), "linux", { home: h.home }), (e) => e.path === f && e.reason === "symlink not allowed");
  }
});

test("posix: missing shim refused as missing; absent redirect fine", posixOnly, () => {
  const h = mkHome();
  fs.unlinkSync(path.join(h.bin, "unimatrix"));
  assert.throws(() => checkTrust(set(h), "linux", { home: h.home }), (e) => e.reason === "missing");
});

test("posix: pkg dir must be 0700, packageRoot parent included", posixOnly, () => {
  const h = mkHome();
  const pkg = path.join(h.u, "pkg");
  const root = path.join(pkg, "1.0.0");
  fs.mkdirSync(root, { recursive: true });
  fs.chmodSync(pkg, 0o755);
  fs.chmodSync(root, 0o700);
  const s = trustSet(h.home, { installJson: { packageRoot: root } });
  assert.ok(s.includes(pkg) && s.includes(root));
  assert.throws(() => checkTrust(s, "linux", { home: h.home }), (e) => e.path === pkg);
});

test("TrustError message", () => {
  const e = new TrustError("/p", "r");
  assert.match(e.message, /trust check failed for \/p: r; run 'npx unimatrix init'/);
});

test("rightsGrantWrite", () => {
  assert.ok(rightsGrantWrite("0x1f01ff"));
  assert.ok(!rightsGrantWrite("0x1200a9"));
  assert.ok(rightsGrantWrite("FA"));
  assert.ok(rightsGrantWrite("GW"));
  assert.ok(!rightsGrantWrite("GR"));
  assert.ok(!rightsGrantWrite("0x120089"));
});

// ---- Windows branch via injected platform + fake spawn ----
function sddl(owner, aces, flags) {
  return `O:${owner}G:${owner}D:${flags === undefined ? "P" : flags}${aces.map((a) => `(A;OICI;${a[0]};;;${a[1]})`).join("")}`;
}
function fakeSpawn(opts = {}) {
  const calls = [];
  const fn = (exe, args) => {
    calls.push([exe, args]);
    if (/whoami/i.test(exe)) return { status: 0, stdout: `"HOST\\me","${ME}"\r\n` };
    const p = args[0];
    const s = (opts.sddl && opts.sddl(p)) || sddl(ME, [["FA", ME], ["FA", "SY"], ["FA", "BA"]]);
    return { status: 0, stdout: s };
  };
  fn.calls = calls;
  fn.full = () => calls.filter((c) => /whoami/i.test(c[0])).length;
  return fn;
}
const w = (h, spawn) => ({ home: h.home, spawn, env: { SystemRoot: "C:\\Windows" } });

test("win: first event full check, trust.json written; next event spawns nothing", () => {
  const h = mkHome();
  const sp = fakeSpawn();
  const s = set(h, "win32");
  checkTrust(s, "win32", w(h, sp));
  assert.strictEqual(sp.full(), 1);
  assert.ok(sp.calls.every((c) => c[0] === "C:\\Windows\\System32\\whoami.exe" || c[0] === "C:\\Windows\\System32\\icacls.exe"));
  const tj = JSON.parse(fs.readFileSync(path.join(h.u, "trust.json"), "utf8"));
  assert.ok(tj[h.u] && tj[h.u].ino !== undefined);
  const before = sp.calls.length;
  checkTrust(trustSet(h.home, { platform: "win32" }), "win32", w(h, sp)); // set now includes trust.json
  assert.strictEqual(sp.calls.length, before);
});

test("win: extra writer ACE refused", () => {
  const h = mkHome();
  const sp = fakeSpawn({ sddl: (p) => sddl(ME, [["FA", ME], ["FA", "S-1-5-21-9-9-9-500"]]) });
  assert.throws(() => checkTrust(set(h, "win32"), "win32", w(h, sp)), (e) => e instanceof TrustError && /extra writer S-1-5-21-9-9-9-500/.test(e.reason));
});

test("win: foreign owner refused; unprotected dir DACL refused", () => {
  const h = mkHome();
  assert.throws(() => checkTrust(set(h, "win32"), "win32", w(h, fakeSpawn({ sddl: () => sddl("S-1-5-21-7", [["FA", ME]]) }))), /owner is not current user/);
  assert.throws(() => checkTrust(set(h, "win32"), "win32", w(h, fakeSpawn({ sddl: () => sddl(ME, [["FA", ME]], "") }))), /inheritance/);
});

test("win: file mismatch -> one full check, rewrite, then no spawn", () => {
  const h = mkHome();
  const sp = fakeSpawn();
  checkTrust(set(h, "win32"), "win32", w(h, sp));
  fs.writeFileSync(path.join(h.bin, "shim-entry.js"), "changed-size");
  const s = trustSet(h.home, { platform: "win32" });
  checkTrust(s, "win32", w(h, sp));
  assert.strictEqual(sp.full(), 2);
  checkTrust(s, "win32", w(h, sp));
  assert.strictEqual(sp.full(), 2);
});

test("win: same size + restored mtime + different ino -> full check (R-30)", () => {
  const h = mkHome();
  const sp = fakeSpawn();
  checkTrust(set(h, "win32"), "win32", w(h, sp));
  const f = path.join(h.u, "install.json");
  const st = fs.statSync(f);
  fs.writeFileSync(f + ".new", "x");
  fs.renameSync(f + ".new", f);
  fs.utimesSync(f, st.atime, st.mtime);
  checkTrust(trustSet(h.home, { platform: "win32" }), "win32", w(h, sp));
  assert.strictEqual(sp.full(), 2);
});

test("win: ~/.unimatrix dir ino/mode mismatch triggers exactly one full check", () => {
  const h = mkHome();
  const sp = fakeSpawn();
  checkTrust(set(h, "win32"), "win32", w(h, sp));
  const s = trustSet(h.home, { platform: "win32" });
  // mode mismatch: tamper recorded dir mode
  let tj = JSON.parse(fs.readFileSync(path.join(h.u, "trust.json"), "utf8"));
  tj[h.u].mode += 1;
  fs.writeFileSync(path.join(h.u, "trust.json"), JSON.stringify(tj));
  checkTrust(s, "win32", w(h, sp));
  assert.strictEqual(sp.full(), 2);
  // ino mismatch
  tj = JSON.parse(fs.readFileSync(path.join(h.u, "trust.json"), "utf8"));
  tj[h.u].ino += 1;
  fs.writeFileSync(path.join(h.u, "trust.json"), JSON.stringify(tj));
  checkTrust(s, "win32", w(h, sp));
  assert.strictEqual(sp.full(), 3);
  // dir mtime/size change alone does NOT trigger
  fs.writeFileSync(path.join(h.u, "other"), "z");
  checkTrust(s, "win32", w(h, sp));
  assert.strictEqual(sp.full(), 3);
});

test("win: trust.json malformed/non-object/missing -> full check, never trusted", () => {
  for (const body of ["{not json", "[]", "null", null]) {
    const h = mkHome();
    const sp = fakeSpawn();
    if (body !== null) fs.writeFileSync(path.join(h.u, "trust.json"), body);
    checkTrust(trustSet(h.home, { platform: "win32" }), "win32", w(h, sp));
    assert.strictEqual(sp.full(), 1, String(body));
    assert.doesNotThrow(() => JSON.parse(fs.readFileSync(path.join(h.u, "trust.json"), "utf8")));
  }
});

test("win: failing full check leaves no trust.json rewrite", () => {
  const h = mkHome();
  const sp = fakeSpawn({ sddl: () => sddl(ME, [["FA", "WD"]]) });
  assert.throws(() => checkTrust(set(h, "win32"), "win32", w(h, sp)));
  assert.ok(!fs.existsSync(path.join(h.u, "trust.json")));
});

test("win: missing path -> TrustError missing", () => {
  const h = mkHome();
  const sp = fakeSpawn();
  checkTrust(set(h, "win32"), "win32", w(h, sp));
  fs.unlinkSync(path.join(h.bin, "trust.js"));
  assert.throws(() => checkTrust(trustSet(h.home, { platform: "win32" }), "win32", w(h, sp)), (e) => e.reason === "missing");
});

test("win: concurrent rewrites leave valid JSON", () => {
  const h = mkHome();
  const s = set(h, "win32");
  for (let i = 0; i < 20; i++) { recordTrustIdentity(s, h.home); recordTrustIdentity(s, h.home); }
  JSON.parse(fs.readFileSync(path.join(h.u, "trust.json"), "utf8"));
  assert.deepStrictEqual(fs.readdirSync(h.u).filter((n) => n.includes(".tmp.")), []);
});

test("statIdentity carries size, mtimeMs, ino, mode", () => {
  const h = mkHome();
  const i = statIdentity(path.join(h.u, "install.json"));
  for (const k of ["size", "mtimeMs", "ino", "mode"]) assert.ok(k in i);
});

test("residual: changes nested inside packageRoot are not detected by root-dir identity (documented)", () => {
  const h = mkHome();
  const root = path.join(h.u, "pkg", "1");
  fs.mkdirSync(path.join(root, "sub"), { recursive: true });
  const sp = fakeSpawn();
  const ctx = { platform: "win32", installJson: { packageRoot: root } };
  checkTrust(trustSet(h.home, ctx), "win32", w(h, sp));
  fs.writeFileSync(path.join(root, "sub", "evil.js"), "x");
  checkTrust(trustSet(h.home, ctx), "win32", w(h, sp));
  assert.strictEqual(sp.full(), 1);
});
