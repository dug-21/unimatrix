"use strict";
// wiring-e2e (linux local, AC-07): init with the BUILT server (cargo build --release), then run the
// literal written hook command through the harness shells and read the stored observation from the
// server's own SQLite store. A stub binary cannot store an observation, so this needs the real build.
// Skips when no built binary exists unless E2E_REQUIRE_LOCAL=1 (set by CI) -- then a missing binary fails.
require("./helpers/home-guard"); // R-09: clears ambient config-dir overrides at load
const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const crypto = require("node:crypto");
const { spawn } = require("node:child_process");
const H = require("./helpers/e2e-harness.js");

const PKG = path.join(__dirname, "..");
const ARCH = process.arch === "arm64" ? "arm64" : "x64";
const REPO = path.join(PKG, "..", "..");
const BINARY = process.env.E2E_SERVER_BINARY || [path.join(REPO, "target", "release", "unimatrix")].find((p) => fs.existsSync(p));
const REQUIRE = process.env.E2E_REQUIRE_LOCAL === "1";
const skip = process.platform !== "linux" ? "linux only (UDS local transport)" : !BINARY && !REQUIRE ? "no built server binary" : false;

function dbRows(db) {
  return H.run("python3", ["-I", "-c",
    "import sqlite3,sys,json;c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True);print(json.dumps(c.execute('select session_id,hook,tool,source_domain from observations').fetchall()))", db],
    { spawn: { env: process.env } }).then((r) => { try { return JSON.parse(r.out); } catch (_e) { return []; } });
}
async function waitRow(db, sid, ms) {
  const end = Date.now() + ms;
  for (;;) {
    const rows = (await dbRows(db)).filter((r) => r[0] === sid);
    if (rows.length || Date.now() > end) return rows;
    await new Promise((r) => setTimeout(r, 200));
  }
}

describe("wiring-e2e linux local: built server, UDS, literal command", { skip }, () => {
  let f, env, server, dbDir, staged, marker;
  before(async () => {
    assert.ok(BINARY, "E2E_REQUIRE_LOCAL=1 but no built server binary (cargo build --release)");
    // stage the platform package where install-store resolves it (init copies it into the stable pkg dir)
    staged = path.join(PKG, "node_modules", "@dug-21", "unimatrix-linux-" + ARCH);
    assert.ok(!fs.existsSync(staged), "refusing to overwrite an existing " + staged);
    fs.mkdirSync(path.join(staged, "bin"), { recursive: true });
    fs.copyFileSync(path.join(PKG, "..", "unimatrix-linux-" + ARCH, "package.json"), path.join(staged, "package.json"));
    fs.copyFileSync(BINARY, path.join(staged, "bin", "unimatrix"));
    fs.chmodSync(path.join(staged, "bin", "unimatrix"), 0o755);
    f = H.makeFixture();
    // marker "binary": if the hook path ever execs Rust it records here (zero Rust exec on the hook path)
    marker = path.join(f.root, "rust-exec.log");
    const decoy = path.join(f.root, "decoy-unimatrix");
    fs.writeFileSync(decoy, "#!/bin/sh\necho x >> '" + marker + "'\nexit 0\n", { mode: 0o755 });
    env = { PATH: process.env.PATH, HOME: f.home, USERPROFILE: f.home };
    const r = await H.runInit(f, env, []);
    assert.equal(r.code, 0, r.out + r.err);
    const hash = crypto.createHash("sha256").update(f.proj).digest("hex").slice(0, 16);
    dbDir = path.join(f.home, ".unimatrix", hash);
    server = spawn(BINARY, ["--project-dir", f.proj, "serve", "--foreground"], { env, stdio: "ignore" });
    for (let i = 0; i < 150 && !fs.existsSync(path.join(dbDir, "unimatrix.sock")); i++) await new Promise((x) => setTimeout(x, 200));
    assert.ok(fs.existsSync(path.join(dbDir, "unimatrix.sock")), "server socket did not appear");
    env.UNIMATRIX_BINARY = decoy;
  });
  after(() => {
    if (server) try { server.kill(); } catch (_e) { /* gone */ }
    if (staged) {
      fs.rmSync(staged, { recursive: true, force: true });
      for (const d of [path.dirname(staged), path.join(PKG, "node_modules")]) try { fs.rmdirSync(d); } catch (_e) { /* not empty */ }
    }
    if (f) f.cleanup();
  });

  it("every harness shell: stored observation, empty stdout, zero Rust exec", async () => {
    const line = H.hookCommand(path.join(f.proj, ".claude", "settings.local.json"), "PostToolUse");
    let n = 0;
    for (const sh of H.shells()) {
      const sid = "e2e-local-" + n++;
      const r = await H.runLine(sh, line, f.proj, env, H.event(f, sid));
      assert.equal(r.code, 0, sh.name + ": " + r.err);
      assert.equal(r.out, "");
      const rows = await waitRow(path.join(dbDir, "unimatrix.db"), sid, 8000);
      assert.ok(rows.length >= 1, sh.name + ": no stored observation");
      assert.equal(rows[0][1], "PostToolUse");
    }
    assert.ok(!fs.existsSync(marker), "hook path executed the Rust binary");
  });

  it("provider attribution reaches the store (--provider opencode)", async () => {
    const line = H.hookCommand(path.join(f.proj, ".claude", "settings.local.json"), "PostToolUse") + " --provider opencode";
    const sh = H.shells()[0];
    const r = await H.runLine(sh, line, f.proj, env, H.event(f, "e2e-provider"));
    assert.equal(r.code, 0, r.err);
    const rows = await waitRow(path.join(dbDir, "unimatrix.db"), "e2e-provider", 8000);
    assert.equal(rows.length >= 1 && rows[0][3], "opencode");
  });

  it("non-project cwd: exit 0, silent, nothing stored", async () => {
    const line = H.hookCommand(path.join(f.proj, ".claude", "settings.local.json"), "PostToolUse");
    const r = await H.runLine(H.shells()[0], line, f.other, env, JSON.stringify({ session_id: "e2e-none", hook_event_name: "PostToolUse", cwd: f.other }));
    assert.equal(r.code, 0);
    assert.equal(r.out + r.err, "");
    assert.equal((await dbRows(path.join(dbDir, "unimatrix.db"))).filter((x) => x[0] === "e2e-none").length, 0);
  });
});
