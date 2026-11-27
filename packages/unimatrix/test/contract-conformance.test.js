"use strict";
// 1036 C16 conformance greps: S-5 (R-10), FR-20 / AC-10 single-writer, writer-core sync-only.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const PKG = path.join(__dirname, "..");
const LIB = path.join(PKG, "lib");
const SELF = __filename;

function walk(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) { if (e.name !== "node_modules" && e.name !== "fixtures") walk(p, out); }
    else if (e.name.endsWith(".js")) out.push(p);
  }
  return out;
}
const rel = (p) => path.relative(PKG, p).split(path.sep).join("/");
const src = (p) => fs.readFileSync(p, "utf8");
const libFiles = () => walk(LIB);
// Hook-time runtime-state writers (queue, offsets/health, last-hook, latches) are out of FR-20 scope (D-020).
const isHookRuntime = (r) => r.startsWith("lib/hook-client/");
const WRITER_CORES = new Set(["lib/trust.js", "lib/safe-write.js"]);

test("S-5: lib/atomic-write.js is absent and never required", () => {
  assert.ok(!fs.existsSync(path.join(LIB, "atomic-write.js")));
  for (const f of [...libFiles(), ...walk(path.join(PKG, "test"))]) {
    if (f === SELF) continue;
    assert.ok(!/require\([^)]*atomic-write/.test(src(f)), rel(f));
  }
});

test("S-5: retired contract names absent from lib and test", () => {
  const names = [
    "isUnimatrixOpencodePlugin", "DanglingSymlinkError", "opencodeMcp", "otherLocations",
    "atomicWriteText", "GITIGNORE_WARNING", "gitIgnored", "removeOwnedClaudeLegacy", "isWithinProject",
  ];
  const hits = [];
  for (const f of [...libFiles(), ...walk(path.join(PKG, "test"))]) {
    if (f === SELF) continue;
    const s = src(f);
    for (const n of names) if (new RegExp(`\\b${n}\\b`).test(s)) hits.push(`${rel(f)}: ${n}`);
  }
  assert.deepStrictEqual(hits, []);
});

test("S-5: status.js has no private codexHome", () => {
  assert.ok(!/function\s+codexHome|codexHome\s*[:=]\s*(function|\()/.test(src(path.join(LIB, "status.js"))));
});

test("AC-10: no .pid-suffixed temp names on FR-20 paths", () => {
  const hits = [];
  for (const f of libFiles()) {
    const r = rel(f);
    if (isHookRuntime(r) || WRITER_CORES.has(r)) continue;
    src(f).split("\n").forEach((l, i) => {
      if (/process\.pid/.test(l) && /tmp|\.tmp/i.test(l)) hits.push(`${r}:${i + 1}`);
    });
  }
  assert.deepStrictEqual(hits, []);
});

test("FR-20/AC-10: raw rename/write/copy only in writer cores (package-dir rename exempt)", () => {
  // file -> allowed raw-call count; reviewed in PR.
  const ALLOW = { "lib/install-store.js": { renameSync: 1, openW: 1 } }; // copyPackage: move invalid package dir aside; .complete marker
  const hits = [];
  for (const f of libFiles()) {
    const r = rel(f);
    if (isHookRuntime(r) || WRITER_CORES.has(r)) continue;
    const s = src(f);
    for (const fn of ["renameSync", "writeFileSync", "copyFileSync"]) {
      const n = (s.match(new RegExp(`\\bfs\\.${fn}\\s*\\(`, "g")) || []).length;
      const allowed = (ALLOW[r] && ALLOW[r][fn]) || 0;
      if (n > allowed) hits.push(`${r}: fs.${fn} x${n}`);
    }
    const w = (s.match(/\bfs\.openSync\([^)]*["'][wa]/g) || []).length;
    if (w > ((ALLOW[r] && ALLOW[r].openW) || 0)) hits.push(`${r}: fs.openSync write x${w}`);
  }
  assert.deepStrictEqual(hits, []);
});

test("writer cores are synchronous: no async/await/.then(/setTimeout", () => {
  for (const r of WRITER_CORES) {
    const s = src(path.join(PKG, r)).replace(/\/\*[\s\S]*?\*\/|\/\/.*$/gm, "");
    for (const re of [/\basync\b/, /\bawait\b/, /\.then\(/, /\bsetTimeout\b/]) assert.ok(!re.test(s), `${r}: ${re}`);
  }
});
