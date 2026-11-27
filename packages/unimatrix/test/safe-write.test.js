"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { updateFileSync, writeFileSync, SafeWriteError, tempPath, renameWithRetry } = require("../lib/safe-write.js");

const posixOnly = process.platform === "win32" ? { skip: "posix" } : {};
const tmpd = () => fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "sw-")));
const temps = (d) => fs.readdirSync(d).filter((n) => /\.tmp-[0-9a-f]{16}$/.test(n));
const mode = (p) => fs.statSync(p).mode & 0o777;
const isCode = (c) => (e) => e instanceof SafeWriteError && e.code === c;
function spy(obj, name, impl) {
  const orig = obj[name];
  const calls = [];
  obj[name] = function (...a) {
    calls.push(a);
    return impl ? impl(orig, ...a) : orig.apply(this, a);
  };
  return { calls, restore: () => { obj[name] = orig; } };
}

test("absent file: created with 0600 default under umask 022 and 077", posixOnly, () => {
  for (const um of [0o022, 0o077, 0o000]) {
    const d = tmpd();
    const f = path.join(d, "a.json");
    const old = process.umask(um);
    try {
      const r = updateFileSync(f, (raw) => (raw === null ? "x" : raw));
      assert.strictEqual(r.action, "created");
      assert.strictEqual(r.before, null);
    } finally { process.umask(old); }
    assert.strictEqual(mode(f), 0o600);
    assert.strictEqual(fs.readFileSync(f, "utf8"), "x");
  }
});

test("mutate null / equal bytes -> unchanged, no temp, no rename, mtime kept", () => {
  const d = tmpd();
  const f = path.join(d, "a");
  fs.writeFileSync(f, "same");
  const t = new Date(2020, 1, 1);
  fs.utimesSync(f, t, t);
  const rn = spy(fs, "renameSync");
  try {
    assert.strictEqual(updateFileSync(f, () => null).action, "unchanged");
    const r = updateFileSync(f, (raw) => raw);
    assert.strictEqual(r.action, "unchanged");
    assert.strictEqual(r.before, "same");
    assert.strictEqual(writeFileSync(f, Buffer.from("same")).action, "unchanged");
  } finally { rn.restore(); }
  assert.strictEqual(rn.calls.length, 0);
  assert.strictEqual(fs.statSync(f).mtimeMs, t.getTime());
  assert.deepStrictEqual(temps(d), []);
});

test("absent file + mutate null -> unchanged, nothing created", () => {
  const d = tmpd();
  assert.strictEqual(updateFileSync(path.join(d, "n"), () => null).action, "unchanged");
  assert.deepStrictEqual(fs.readdirSync(d), []);
});

test("dryRun: planned action, no file/dir/temp", () => {
  const d = tmpd();
  const f = path.join(d, "sub", "a");
  assert.strictEqual(updateFileSync(f, () => "x", { dryRun: true }).action, "created");
  assert.ok(!fs.existsSync(path.join(d, "sub")));
  fs.writeFileSync(path.join(d, "b"), "1");
  assert.strictEqual(updateFileSync(path.join(d, "b"), () => "2", { dryRun: true }).action, "updated");
  assert.strictEqual(fs.readFileSync(path.join(d, "b"), "utf8"), "1");
});

test("existing mode kept (0644, 0664); enforceMode chmods and is idempotent", posixOnly, () => {
  const d = tmpd();
  const f = path.join(d, "a");
  fs.writeFileSync(f, "1");
  fs.chmodSync(f, 0o664);
  updateFileSync(f, () => "2");
  assert.strictEqual(mode(f), 0o664);
  fs.chmodSync(f, 0o644);
  const rn = spy(fs, "renameSync");
  try {
    assert.strictEqual(updateFileSync(f, () => null, { enforceMode: 0o600 }).action, "updated");
  } finally { rn.restore(); }
  assert.strictEqual(rn.calls.length, 0);
  assert.strictEqual(mode(f), 0o600);
  const ch = spy(fs, "chmodSync");
  try {
    assert.strictEqual(updateFileSync(f, () => null, { enforceMode: 0o600 }).action, "unchanged");
  } finally { ch.restore(); }
  assert.strictEqual(ch.calls.length, 0);
});

test("chmod EPERM on enforced file -> io naming file", posixOnly, () => {
  const d = tmpd();
  const f = path.join(d, "a");
  fs.writeFileSync(f, "1", { mode: 0o644 });
  const ch = spy(fs, "chmodSync", () => { const e = new Error("EPERM"); e.code = "EPERM"; throw e; });
  try {
    assert.throws(() => updateFileSync(f, () => null, { enforceMode: 0o600 }), (e) => isCode("io")(e) && e.file === f);
  } finally { ch.restore(); }
});

test("symlinks: absolute, relative, 2-hop -> link kept, target written", posixOnly, () => {
  const d = tmpd();
  const real = path.join(d, "real.json");
  fs.writeFileSync(real, "0");
  fs.symlinkSync(real, path.join(d, "abs"));
  fs.symlinkSync("real.json", path.join(d, "rel"));
  fs.symlinkSync("abs", path.join(d, "hop"));
  for (const [i, l] of ["abs", "rel", "hop"].entries()) {
    const lp = path.join(d, l);
    const before = fs.readlinkSync(lp);
    const r = updateFileSync(lp, () => String(i + 1));
    assert.strictEqual(r.target, real);
    assert.ok(fs.lstatSync(lp).isSymbolicLink());
    assert.strictEqual(fs.readlinkSync(lp), before);
    assert.strictEqual(fs.readFileSync(real, "utf8"), String(i + 1));
  }
  assert.deepStrictEqual(temps(d), []);
});

test("dangling, loop, dir, link-to-dir refused", posixOnly, () => {
  const d = tmpd();
  fs.symlinkSync(path.join(d, "nope"), path.join(d, "dang"));
  assert.throws(() => updateFileSync(path.join(d, "dang"), () => "x"), (e) => isCode("dangling-symlink")(e) && e.file === path.join(d, "dang"));
  assert.ok(!fs.existsSync(path.join(d, "nope")));
  fs.symlinkSync("l2", path.join(d, "l1"));
  fs.symlinkSync("l1", path.join(d, "l2"));
  assert.throws(() => updateFileSync(path.join(d, "l1"), () => "x"), isCode("not-regular"));
  fs.mkdirSync(path.join(d, "dir"));
  assert.throws(() => updateFileSync(path.join(d, "dir"), () => "x"), isCode("not-regular"));
  fs.symlinkSync(path.join(d, "dir"), path.join(d, "ld"));
  assert.throws(() => updateFileSync(path.join(d, "ld"), () => "x"), isCode("not-regular"));
});

test("symlinked parent dir: real target written", posixOnly, () => {
  const d = tmpd();
  fs.mkdirSync(path.join(d, "realdir"));
  fs.symlinkSync(path.join(d, "realdir"), path.join(d, "ldir"));
  const r = updateFileSync(path.join(d, "ldir", "a"), () => "x");
  assert.strictEqual(r.target, path.join(d, "realdir", "a"));
  assert.strictEqual(fs.readFileSync(path.join(d, "realdir", "a"), "utf8"), "x");
});

test("unwritable real dir -> io, no temp", posixOnly, () => {
  if (process.getuid && process.getuid() === 0) return;
  const d = tmpd();
  const ro = path.join(d, "ro");
  fs.mkdirSync(ro);
  fs.writeFileSync(path.join(ro, "a"), "1");
  fs.chmodSync(ro, 0o500);
  try {
    assert.throws(() => updateFileSync(path.join(ro, "a"), () => "2"), isCode("io"));
    assert.deepStrictEqual(temps(ro), []);
  } finally { fs.chmodSync(ro, 0o700); }
});

test("containWithin: inside, outside, prefix sibling, symlinked root, parent link, escape-and-return", posixOnly, () => {
  const d = tmpd();
  const R = path.join(d, "p");
  const out = path.join(d, "out");
  fs.mkdirSync(R);
  fs.mkdirSync(out);
  fs.mkdirSync(path.join(d, "p-x"));
  fs.writeFileSync(path.join(R, "in.json"), "1");
  fs.writeFileSync(path.join(out, "o.json"), "1");
  fs.writeFileSync(path.join(d, "p-x", "s.json"), "1");
  fs.symlinkSync(path.join(R, "in.json"), path.join(R, "lin"));
  assert.strictEqual(updateFileSync(path.join(R, "lin"), () => "2", { containWithin: R }).action, "updated");
  fs.symlinkSync(path.join(out, "o.json"), path.join(R, "lout"));
  assert.throws(() => updateFileSync(path.join(R, "lout"), () => "2", { containWithin: R }), (e) => isCode("outside-project")(e) && e.message.includes(path.join(R, "lout")) && e.message.includes(path.join(out, "o.json")));
  assert.throws(() => updateFileSync(path.join(R, "lout"), () => "2", { containWithin: R, dryRun: true }), isCode("outside-project"));
  assert.strictEqual(fs.readFileSync(path.join(out, "o.json"), "utf8"), "1");
  fs.symlinkSync(path.join(d, "p-x", "s.json"), path.join(R, "lsib"));
  assert.throws(() => updateFileSync(path.join(R, "lsib"), () => "2", { containWithin: R }), isCode("outside-project"));
  fs.symlinkSync(R, path.join(d, "Rlink"));
  assert.strictEqual(updateFileSync(path.join(d, "Rlink", "in.json"), () => "3", { containWithin: path.join(d, "Rlink") }).action, "updated");
  fs.symlinkSync(out, path.join(R, ".claude"));
  assert.throws(() => updateFileSync(path.join(R, ".claude", "new.json"), () => "x", { containWithin: R }), isCode("outside-project"));
  fs.symlinkSync(path.join(R, "in.json"), path.join(out, "back"));
  fs.symlinkSync(path.join(out, "back"), path.join(R, "ret"));
  assert.strictEqual(updateFileSync(path.join(R, "ret"), () => "4", { containWithin: R }).action, "updated");
  // without containWithin an outside link is followed
  assert.strictEqual(updateFileSync(path.join(R, "lout"), () => "9").action, "updated");
  assert.strictEqual(fs.readFileSync(path.join(out, "o.json"), "utf8"), "9");
});

test("containWithin case-insensitive on injected win32/darwin", posixOnly, () => {
  const d = tmpd();
  const R = path.join(d, "Proj");
  fs.mkdirSync(R);
  const { checkContained } = require("../lib/trust.js");
  checkContained("f", path.join(R, "a").replace("Proj", "proj"), R, "darwin");
  assert.throws(() => checkContained("f", path.join(R, "a").replace("Proj", "proj"), R, "linux"), isCode("outside-project"));
});

test("fault before-rename retargets link outside root -> outside-project, outside bytes intact", posixOnly, () => {
  const d = tmpd();
  const R = path.join(d, "p");
  fs.mkdirSync(R);
  fs.writeFileSync(path.join(R, "in"), "1");
  fs.writeFileSync(path.join(d, "o"), "1");
  const l = path.join(R, "l");
  fs.symlinkSync(path.join(R, "in"), l);
  const fault = (s) => { if (s === "before-rename") { fs.unlinkSync(l); fs.symlinkSync(path.join(d, "o"), l); } };
  assert.throws(() => updateFileSync(l, () => "2", { containWithin: R, fault }), isCode("outside-project"));
  assert.strictEqual(fs.readFileSync(path.join(d, "o"), "utf8"), "1");
  assert.deepStrictEqual(temps(R), []);
});

test("CAS: one concurrent change -> attempt 2 keeps both; every attempt -> conflict, no temps", () => {
  const d = tmpd();
  const f = path.join(d, "a");
  fs.writeFileSync(f, "base");
  let n = 0;
  const once = (s) => { if (s === "before-rename" && n++ === 0) fs.writeFileSync(f, "base+other"); };
  const r = updateFileSync(f, (raw) => (raw.endsWith("+mine") ? null : raw + "+mine"), { fault: once });
  assert.strictEqual(r.action, "updated");
  assert.strictEqual(fs.readFileSync(f, "utf8"), "base+other+mine");
  let k = 0;
  const always = (s) => { if (s === "before-rename") fs.writeFileSync(f, `x${k++}`); };
  assert.throws(() => updateFileSync(f, (raw) => raw + "!", { fault: always }), (e) => isCode("conflict")(e) && /after 3 attempts/.test(e.message));
  assert.deepStrictEqual(temps(d), []);
  let j = 0;
  const afterRead = (s) => { if (s === "after-read") fs.writeFileSync(f, `y${j++}`); };
  assert.throws(() => updateFileSync(f, (raw) => (raw.startsWith("y") ? raw + "!" : raw + "?"), { fault: afterRead }), isCode("conflict"));
});

test("verify: content stripped after rename / non-idempotent mutate -> verify-failed", () => {
  const d = tmpd();
  const f = path.join(d, "a");
  fs.writeFileSync(f, "1");
  const strip = (s) => { if (s === "after-rename") fs.writeFileSync(f, "1"); };
  assert.throws(() => updateFileSync(f, (raw) => (raw === "2" ? null : "2"), { fault: strip }), isCode("verify-failed"));
  assert.throws(() => updateFileSync(f, (raw) => raw + "a"), isCode("verify-failed"));
  assert.deepStrictEqual(temps(d), []);
});

test("mutate errors propagate unchanged, no temp", () => {
  const d = tmpd();
  const f = path.join(d, "a");
  fs.writeFileSync(f, "{");
  class ParseErr extends Error {}
  assert.throws(() => updateFileSync(f, () => { throw new ParseErr("Malformed"); }), ParseErr);
  assert.deepStrictEqual(temps(d), []);
});

test("temp: wx flag, name pattern, in real target dir; collisions regenerate, 5 -> io", () => {
  const d = tmpd();
  const f = path.join(d, "a.json");
  const op = spy(fs, "openSync");
  try { writeFileSync(f, "x"); } finally { op.restore(); }
  const c = op.calls.find((a) => a[1] === "wx");
  assert.ok(c);
  assert.strictEqual(path.dirname(c[0]), d);
  assert.match(path.basename(c[0]), /^\.[^/]+\.tmp-[0-9a-f]{16}$/);
  assert.match(path.basename(tempPath(f)), /^\.a\.json\.tmp-[0-9a-f]{16}$/);
  let hits = 0;
  const eex = (n) => spy(fs, "openSync", (orig, p, fl, m) => {
    if (fl === "wx" && hits++ < n) { const e = new Error("EEXIST"); e.code = "EEXIST"; throw e; }
    return orig(p, fl, m);
  });
  let s = eex(2);
  try { writeFileSync(f, "y"); } finally { s.restore(); }
  assert.strictEqual(fs.readFileSync(f, "utf8"), "y");
  hits = 0;
  s = eex(5);
  try { assert.throws(() => writeFileSync(f, "z"), isCode("io")); } finally { s.restore(); }
  assert.deepStrictEqual(temps(d), []);
});

test("failure at write/fsync/rename leaves no temp", () => {
  for (const name of ["writeSync", "fsyncSync", "renameSync"]) {
    const d = tmpd();
    const f = path.join(d, "a");
    fs.writeFileSync(f, "1");
    const s = spy(fs, name, () => { const e = new Error("EIO"); e.code = "EIO"; throw e; });
    try { assert.throws(() => updateFileSync(f, () => "2"), isCode("io")); } finally { s.restore(); }
    assert.deepStrictEqual(temps(d), [], name);
    assert.strictEqual(fs.readFileSync(f, "utf8"), "1");
  }
});

test("posix: dir fsync open throwing still succeeds", posixOnly, () => {
  const d = tmpd();
  const f = path.join(d, "a");
  const s = spy(fs, "openSync", (orig, p, fl, m) => { if (fl === "r" && p === d) throw new Error("EISDIR"); return orig(p, fl, m); });
  try { assert.strictEqual(writeFileSync(f, "x").action, "created"); } finally { s.restore(); }
});

test("renameWithRetry: win32 retries EPERM/EBUSY/EACCES up to 10, posix none", () => {
  for (const code of ["EPERM", "EBUSY", "EACCES"]) {
    let n = 0;
    const s = spy(fs, "renameSync", () => { if (n++ < 3) { const e = new Error(code); e.code = code; throw e; } });
    try { renameWithRetry("a", "b", "win32", { count: 10, delayMs: 0 }); } finally { s.restore(); }
    assert.strictEqual(n, 4);
  }
  let n = 0;
  let s = spy(fs, "renameSync", () => { n++; const e = new Error("EPERM"); e.code = "EPERM"; throw e; });
  try { assert.throws(() => renameWithRetry("a", "b", "win32", { count: 10, delayMs: 0 })); } finally { s.restore(); }
  assert.strictEqual(n, 10);
  n = 0;
  s = spy(fs, "renameSync", () => { n++; const e = new Error("EPERM"); e.code = "EPERM"; throw e; });
  try { assert.throws(() => renameWithRetry("a", "b", "linux")); } finally { s.restore(); }
  assert.strictEqual(n, 1);
});

test("binary Buffer round-trips byte-exact", () => {
  const d = tmpd();
  const f = path.join(d, "bin");
  const b = Buffer.from([0, 255, 0xc3, 0x28, 10]);
  writeFileSync(f, b);
  assert.ok(fs.readFileSync(f).equals(b));
  assert.strictEqual(writeFileSync(f, b).action, "unchanged");
});

test("static: writer has no async/await/then/setTimeout; safe-write <= 300 code lines", () => {
  for (const p of ["safe-write.js", "trust.js"]) {
    const src = fs.readFileSync(path.join(__dirname, "..", "lib", p), "utf8").replace(/\/\*[\s\S]*?\*\/|\/\/.*$/gm, "");
    assert.doesNotMatch(src, /\basync\b|\bawait\b|\.then\(|setTimeout/, p);
  }
  const lines = fs.readFileSync(path.join(__dirname, "..", "lib", "safe-write.js"), "utf8").split("\n").filter((l) => l.trim() && !/^\s*(\/\/|\*|\/\*)/.test(l));
  assert.ok(lines.length <= 300);
});
