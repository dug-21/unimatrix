"use strict";
// Launcher: `hook`, `mcp-stdio`, pass-through (nan-024 ADR-004, D-077, D-086).
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const { fixture, run, runAs, spawnLauncher } = require("./helpers/launcher-fixture.js");

const posix = process.platform === "win32" ? { skip: "posix only" } : {};
const lines = (s) => s.split("\n").filter(Boolean);

test("hook outside a project: silent exit 0", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["hook", "Stop"], { UNIMATRIX_BINARY: f.stub });
  assert.equal(r.status, 0);
  assert.equal(r.stdout + r.stderr, "");
  assert.ok(!fs.existsSync(f.marker));
});

test("hook installed: Node client main runs with EVENT + --provider; Rust stub never exec'd (AC-03)", posix, () => {
  const f = fixture();
  const r = run(f, ["hook", "SessionStart", "--provider", "codex-cli"], { UNIMATRIX_BINARY: f.stub });
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(fs.readFileSync(f.marker, "utf8")), ["SessionStart", "--provider", "codex-cli"]);
  assert.ok(!fs.existsSync(f.stubLog));
});

test("hook: Linux project not installed -> exit 0, empty stdout, exactly one stderr line", posix, () => {
  const f = fixture({ noInstall: true });
  const r = run(f, ["hook", "Stop"]);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, "");
  assert.deepEqual(lines(r.stderr), ["unimatrix: not attached on this machine; run 'npx unimatrix init'"]);
});

test("hook: macOS/Windows without credential -> silent exit 0", posix, () => {
  for (const plat of ["darwin", "win32"]) {
    const f = fixture({ noInstall: true });
    const r = runAs(f, plat, ["hook", "Stop"]);
    assert.equal(r.status, 0, plat + r.stderr);
    assert.equal(r.stdout + r.stderr, "", plat);
  }
});

test("hook: trust failure -> exit 1 (never 2), stdout empty", posix, () => {
  const f = fixture();
  fs.chmodSync(f.uh, 0o755);
  const r = run(f, ["hook", "Stop"], { UNIMATRIX_BINARY: f.stub });
  assert.equal(r.status, 1);
  assert.equal(r.stdout, "");
  assert.match(r.stderr, /trust check failed/);
});

test("hook: client throws (sync/async) -> exit 1, never 2, stdout empty", posix, () => {
  for (const body of ['throw new Error("boom")', 'await Promise.reject(new Error("boom"))']) {
    const f = fixture({ clientMain: body });
    const r = run(f, ["hook", "Stop"], { UNIMATRIX_BINARY: f.stub });
    assert.equal(r.status, 1);
    assert.equal(r.stdout, "");
    assert.match(r.stderr, /hook client error: boom/);
  }
});

test("hook: client package without entry -> exit 1 naming path", posix, () => {
  const f = fixture();
  fs.rmSync(f.pkg + "/lib", { recursive: true });
  const r = run(f, ["hook", "Stop"]);
  assert.equal(r.status, 1);
  assert.equal(r.stdout, "");
  assert.ok(r.stderr.includes(f.pkg));
});

test("hook: Linux local with unresolvable binary -> exit 1 (Broken)", posix, () => {
  const f = fixture();
  const r = run(f, ["hook", "Stop"], { UNIMATRIX_BINARY: f.base + "/missing" });
  assert.equal(r.status, 1);
  assert.equal(r.stdout, "");
  assert.ok(r.stderr.includes("missing"));
});

test("hook: preflight ctx skips the redundant checks", posix, async () => {
  const f = fixture();
  const hr = require("../lib/hook-route.js");
  const saveHome = process.env.HOME;
  process.env.HOME = f.home;
  const os = require("os");
  const orig = os.homedir;
  os.homedir = () => f.home;
  try {
    process.exitCode = undefined;
    await hr.run(["hook", "Stop"], { preflight: { project: null } });
    assert.equal(process.exitCode, undefined);
  } finally {
    os.homedir = orig;
    process.env.HOME = saveHome;
  }
});

// ---- pass-through ----
test("pass-through serve: Linux argv + LD_LIBRARY_PATH preserved (R-10)", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["serve", "--flag", "x"], { UNIMATRIX_BINARY: f.stub, LD_LIBRARY_PATH: "/prev" });
  assert.equal(r.status, 0, r.stderr);
  const rec = JSON.parse(fs.readFileSync(f.stubLog, "utf8"));
  assert.deepEqual(rec.argv, ["serve", "--flag", "x"]);
  assert.equal(rec.ld, f.base + ":/prev");
});

test("pass-through: exit status propagates", posix, () => {
  const f = fixture({ noStore: true, stub: "process.exit(7)" });
  assert.equal(run(f, ["version"], { UNIMATRIX_BINARY: f.stub }).status, 7);
});

test("pass-through: unresolvable binary -> exit 1 with sources tried", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["serve"], { UNIMATRIX_BINARY: f.base + "/nope" });
  assert.equal(r.status, 1);
  assert.ok(r.stderr.includes("UNIMATRIX_BINARY"));
});

test("pass-through: untrusted redirect -> exit 1", posix, () => {
  const f = fixture({ noStore: true });
  fs.writeFileSync(f.uh + "/redirect.json", "{}");
  fs.chmodSync(f.uh + "/redirect.json", 0o666);
  const r = run(f, ["serve"], { UNIMATRIX_BINARY: f.stub });
  assert.equal(r.status, 1);
  assert.match(r.stderr, /trust check failed/);
  assert.ok(!fs.existsSync(f.stubLog));
});

test("pass-through: darwin/win32 -> Linux-only message, exit 1, stub untouched (D-071)", posix, () => {
  for (const plat of ["darwin", "win32"]) {
    const f = fixture({ noStore: true });
    const r = runAs(f, plat, ["serve"], { UNIMATRIX_BINARY: f.stub });
    assert.equal(r.status, 1);
    assert.ok(r.stderr.includes("serve requires the local server (Linux only)"));
    assert.ok(!fs.existsSync(f.stubLog));
  }
});

// ---- mcp-stdio ----
test("mcp-stdio outside project: silent exit 0", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.stub });
  assert.equal(r.status, 0);
  assert.equal(r.stdout + r.stderr, "");
  assert.ok(!fs.existsSync(f.stubLog));
});

test("mcp-stdio headless env with cwd outside root: exit 0 silent", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BUNDLE: "x", UNIMATRIX_PROJECT_ROOT: f.home });
  assert.equal(r.status, 0);
  assert.equal(r.stdout + r.stderr, "");
});

test("mcp-stdio inside project, not installed: exit 1, one stderr line, empty stdout, every OS", posix, () => {
  const f = fixture({ noInstall: true });
  for (const r of [run(f, ["mcp-stdio"]), runAs(f, "darwin", ["mcp-stdio"]), runAs(f, "win32", ["mcp-stdio"])]) {
    assert.equal(r.status, 1);
    assert.equal(r.stdout, "");
    assert.deepEqual(lines(r.stderr), ["unimatrix: not attached on this machine; run 'npx unimatrix init'"]);
  }
});

test("mcp-stdio local: stub exit codes mirrored exactly", posix, () => {
  for (const n of [0, 1, 7, 42]) {
    const f = fixture({ stub: `process.exit(${n})` });
    const r = run(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.stub });
    assert.equal(r.status, n);
    assert.equal(r.stdout, "");
  }
});

test("mcp-stdio local: no stdout before the stub's first byte; started with no args + LD_LIBRARY_PATH", posix, () => {
  const f = fixture({ stub: `process.stdout.write("{" + JSON.stringify([process.argv.length, process.env.LD_LIBRARY_PATH]).slice(1))` });
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.stub });
  assert.equal(r.status, 0);
  assert.equal(r.stdout, "{" + JSON.stringify([2, f.base]).slice(1));
  assert.equal(r.stderr, "");
});

test("mcp-stdio local: inherited stdio round-trips 1 MiB unchanged", posix, () => {
  const f = fixture({ stub: `let b=[];process.stdin.on("data",d=>b.push(d));process.stdin.on("end",()=>process.stdout.write(Buffer.concat(b)))` });
  const input = Buffer.alloc(1024 * 1024, "a");
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.stub }, { input, encoding: "buffer", maxBuffer: 8 << 20 });
  assert.equal(r.status, 0);
  assert.ok(Buffer.compare(r.stdout, input) === 0);
});

test("mcp-stdio local POSIX: child killed by SIGTERM/SIGKILL -> launcher dies by same signal (D-086)", posix, () => {
  for (const sig of ["SIGTERM", "SIGKILL"]) {
    const f = fixture({ stub: `process.kill(process.pid, ${JSON.stringify(sig)})` });
    const r = run(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.stub });
    assert.equal(r.signal, sig);
    assert.equal(r.status, null);
  }
});

test("mcp-stdio local POSIX: SIGTERM to launcher reaches the stub; launcher exits after it", posix, async () => {
  const f = fixture({ stub: `process.on("SIGTERM",()=>{require("fs").writeFileSync(${JSON.stringify("/dev/null")},"");process.stdout.write("got-term");process.exit(0)});process.stdout.write("ready");setInterval(()=>{},1000)` });
  const p = spawnLauncher(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.stub });
  let out = "";
  p.stdout.on("data", (d) => { out += d; if (out === "ready") p.kill("SIGTERM"); });
  const code = await new Promise((res) => p.on("exit", (c, s) => res({ c, s })));
  assert.equal(out, "readygot-term");
  assert.equal(code.c, 0);
});

test("mcp-stdio local: spawn failure (non-executable stub) -> exit 1 naming path, stdout empty", posix, () => {
  const f = fixture();
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BINARY: f.base + "/nope" });
  assert.equal(r.status, 1);
  assert.equal(r.stdout, "");
  assert.ok(r.stderr.includes("/nope"));
});

test("mcp-stdio cloud: routes to bridge main with project hash (headless env inside root)", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BUNDLE: "x", UNIMATRIX_PROJECT_ROOT: f.proj });
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, "BRIDGE:" + f.hash);
});

test("mcp-stdio cloud env set but cwd outside root: effectiveEnv strips it -> silent", posix, () => {
  const f = fixture({ noStore: true });
  const r = run(f, ["mcp-stdio"], { UNIMATRIX_BUNDLE: "x", UNIMATRIX_PROJECT_ROOT: f.base + "/elsewhere" });
  assert.equal(r.status, 0);
  assert.equal(r.stdout + r.stderr, "");
});
