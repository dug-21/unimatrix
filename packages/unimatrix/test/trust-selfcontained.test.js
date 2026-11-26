"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { builtinModules } = require("node:module");
const { spawnSync } = require("node:child_process");

const SRC = path.join(__dirname, "..", "lib", "trust.js");
const posixOnly = process.platform === "win32" ? { skip: "posix" } : {};

function offenders(text) {
  const bad = [];
  const re = /\brequire\s*\(\s*([^)]*)\)|\bimport\s*(?:[^'";]*from\s*)?["']([^"']+)["']|\brequire\.resolve\b/g;
  let m;
  while ((m = re.exec(text))) {
    if (m[0].startsWith("require.resolve")) { bad.push(m[0]); continue; }
    const arg = (m[1] !== undefined ? m[1].trim() : `"${m[2]}"`);
    const lit = /^["']([^"']+)["']$/.exec(arg);
    if (!lit) { bad.push(m[0]); continue; }
    const n = lit[1].replace(/^node:/, "");
    if (!lit[1].startsWith("node:") && !builtinModules.includes(n)) bad.push(m[0]);
    else if (!builtinModules.includes(n)) bad.push(m[0]);
  }
  return bad;
}

test("static: lib/trust.js requires only Node builtins", () => {
  assert.deepStrictEqual(offenders(fs.readFileSync(SRC, "utf8")), []);
});

test("static negative control: relative require is detected", () => {
  const t = fs.readFileSync(SRC, "utf8");
  assert.ok(offenders(t + "\nrequire('./other');").length > 0);
  assert.ok(offenders(t + "\nrequire(x);").length > 0);
  assert.ok(offenders(t + "\nrequire('left-pad');").length > 0);
  assert.ok(offenders(t + "\nrequire.resolve('./a');").length > 0);
});

test("dynamic: lone copy in empty dir loads and gives identical verdicts", posixOnly, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "trust-sc-"));
  const copy = path.join(dir, "trust.js");
  fs.copyFileSync(SRC, copy);
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "trust-h-"));
  const u = path.join(home, ".unimatrix");
  fs.mkdirSync(path.join(u, "bin"), { recursive: true });
  fs.chmodSync(u, 0o700);
  fs.chmodSync(path.join(u, "bin"), 0o700);
  const files = ["bin/unimatrix", "bin/shim-entry.js", "bin/trust.js", "install.json"].map((f) => path.join(u, f));
  for (const f of files) fs.writeFileSync(f, "x", { mode: 0o600 });
  const prog = `const t=require(${JSON.stringify(copy)});const s=t.trustSet(${JSON.stringify(home)},{platform:"linux"});
    try{t.checkTrust(s,"linux",{home:${JSON.stringify(home)}});console.log("ok")}catch(e){console.log(e.name+"|"+e.path+"|"+e.reason)}`;
  const run = () => spawnSync(process.execPath, ["-e", prog], { encoding: "utf8", cwd: dir, env: { PATH: process.env.PATH } });
  let r = run();
  assert.strictEqual(r.status, 0, r.stderr);
  assert.strictEqual(r.stdout.trim(), "ok");
  fs.chmodSync(files[0], 0o666);
  r = run();
  assert.strictEqual(r.stdout.trim(), `TrustError|${files[0]}|group/other writable`);
  // parity with in-process lib verdict
  const lib = require(SRC);
  assert.throws(() => lib.checkTrust(lib.trustSet(home, { platform: "linux" }), "linux", { home }), (e) => e.path === files[0] && e.reason === "group/other writable");
});
