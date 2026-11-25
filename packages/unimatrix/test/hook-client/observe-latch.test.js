"use strict";

// Test plan: test-plan/observe-403-client.md (vnc-050 C6c, AC-18, R-17, HD 10).
// Spawns the REAL hook entry per event against the shared stub server
// (test/helpers/stub-server.js) with env-var remote config + isolated HOME.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { spawn } = require("child_process");
const { startStubServer, refusedPort } = require("../helpers/stub-server");
const configMod = require("../../lib/hook-client/config");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");
const READ_TOKEN = "SENTINEL-read-token-aaaa";
const RW_TOKEN = "SENTINEL-rw-token-bbbb";
const DENY = { status: 403, contentType: "application/json", body: '{"error":"token lacks SessionWrite"}' };
const WARN = /observe denied/;
const STDIN = JSON.stringify({ hook_event_name: "Stop", session_id: "s1" });

let root;
let stub;

function stateDir() {
  const hash = configMod.computeProjectHash(configMod.walkToProjectRoot(root));
  return path.join(root, ".unimatrix", hash, "hook-client");
}
const marker = () => path.join(stateDir(), "observe-denied.json");
const queueCount = () => {
  try {
    return fs.readdirSync(path.join(stateDir(), "queue")).length;
  } catch (_e) {
    return 0;
  }
};

function run(token, url) {
  const env = Object.assign({}, process.env, {
    HOME: root,
    USERPROFILE: root,
    UNIMATRIX_REMOTE_URL: url || stub.url + "/observe",
    UNIMATRIX_REMOTE_TOKEN: token,
  });
  return new Promise((resolve, reject) => {
    const c = spawn(process.execPath, [ENTRY, "Stop"], { cwd: root, env });
    const out = [];
    const err = [];
    c.stdout.on("data", (d) => out.push(d));
    c.stderr.on("data", (d) => err.push(d));
    c.on("error", reject);
    c.on("close", (code) => resolve({ code, stdout: Buffer.concat(out).toString(), stderr: Buffer.concat(err).toString() }));
    c.stdin.on("error", () => {});
    c.stdin.end(STDIN);
  });
}

async function runN(n, token, url) {
  const rs = [];
  for (let i = 0; i < n; i++) rs.push(await run(token, url));
  return rs;
}
const warnings = (rs) => rs.reduce((a, r) => a + (r.stderr.match(/observe denied/g) || []).length, 0);
const byToken = (map) => (e) => map[(e.headers.authorization || "").replace(/^Bearer /, "")] || { status: 204 };

describe("observe-denied marker latch (vnc-050 HD 10)", () => {
  beforeEach(async () => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-latch-test-"));
    fs.mkdirSync(path.join(root, ".git"));
    stub = await startStubServer();
  });
  afterEach(async () => {
    await stub.close();
    try {
      fs.chmodSync(stateDir(), 0o700);
    } catch (_e) {}
    fs.rmSync(root, { recursive: true, force: true });
  });

  it("read_bundle_n_events_one_warning + latch_stops_further_posts", async () => {
    stub.respondWith(() => DENY);
    const rs = await runN(4, READ_TOKEN);
    for (const r of rs) {
      assert.strictEqual(r.code, 0);
      assert.strictEqual(r.stdout, "");
    }
    assert.strictEqual(warnings(rs), 1);
    assert.strictEqual(stub.requests.length, 1);
    assert.strictEqual(queueCount(), 0); // never queued
  });

  it("marker_keyed_on_credential_hash: sha256, no secret, 0600, in stateDir", async () => {
    stub.respondWith(() => DENY);
    await run(READ_TOKEN);
    const bytes = fs.readFileSync(marker(), "utf8");
    assert.strictEqual(JSON.parse(bytes).cred, crypto.createHash("sha256").update(READ_TOKEN).digest("hex"));
    assert.ok(!bytes.includes("SENTINEL") && !bytes.includes("http"));
    if (process.platform !== "win32") assert.strictEqual(fs.statSync(marker()).mode & 0o777, 0o600);
  });

  it("marker_no_expiry", async () => {
    stub.respondWith(() => DENY);
    await run(READ_TOKEN);
    const old = new Date(Date.now() - 366 * 86400e3);
    fs.utimesSync(marker(), old, old);
    const r = await run(READ_TOKEN);
    assert.strictEqual(stub.requests.length, 1);
    assert.doesNotMatch(r.stderr, WARN);
  });

  it("credential_change_resets_latch", async () => {
    stub.respondWith(() => DENY);
    await run(READ_TOKEN);
    const r = await run(READ_TOKEN + "-2");
    assert.strictEqual(stub.requests.length, 2);
    assert.match(r.stderr, WARN);
  });

  it("corrupt_marker_retries_once_and_rewrites", async () => {
    stub.respondWith(() => DENY);
    fs.mkdirSync(stateDir(), { recursive: true });
    fs.writeFileSync(marker(), '{"cred":"abc');
    const rs = await runN(3, READ_TOKEN);
    assert.strictEqual(stub.requests.length, 1);
    assert.strictEqual(warnings(rs), 1);
    assert.ok(JSON.parse(fs.readFileSync(marker(), "utf8")).cred);
  });

  it("rw_credential_never_creates_marker", async () => {
    stub.respondWith(byToken({ [READ_TOKEN]: DENY }));
    const rs = await runN(3, RW_TOKEN);
    assert.strictEqual(stub.requests.length, 3);
    assert.strictEqual(warnings(rs), 0);
    assert.ok(!fs.existsSync(marker()));
  });

  for (const [name, spec] of [
    ["other_403", { status: 403, contentType: "application/json", body: '{"error":"token not permitted for project"}' }],
    ["401", { status: 401 }],
    ["500", { status: 500 }],
  ]) {
    it("latch_only_on_sessionwrite_403: " + name, async () => {
      stub.respondWith(() => spec);
      const rs = await runN(2, READ_TOKEN);
      assert.ok(stub.requests.length >= 2); // posts (plus queue replay) as today
      assert.strictEqual(warnings(rs), 0);
      assert.ok(!fs.existsSync(marker()));
      assert.ok(queueCount() >= 1); // existing enqueue behaviour
      for (const r of rs) assert.strictEqual(r.code, 0);
    });
  }

  it("latch_only_on_sessionwrite_403: connection refused", async () => {
    const port = await refusedPort();
    const rs = await runN(2, READ_TOKEN, "http://127.0.0.1:" + port + "/observe");
    assert.strictEqual(warnings(rs), 0);
    assert.ok(!fs.existsSync(marker()));
  });

  it("missing_or_unreadable_marker_fails_open", async () => {
    stub.respondWith(() => ({ status: 204 }));
    fs.mkdirSync(stateDir(), { recursive: true });
    fs.writeFileSync(marker(), JSON.stringify({ cred: crypto.createHash("sha256").update(READ_TOKEN).digest("hex") }));
    if (process.platform !== "win32" && process.getuid && process.getuid() !== 0) {
      fs.chmodSync(marker(), 0o000);
      const r = await run(READ_TOKEN);
      assert.strictEqual(r.code, 0);
      assert.strictEqual(stub.requests.length, 1);
    }
    fs.rmSync(marker(), { force: true });
    const n = stub.requests.length;
    await run(READ_TOKEN);
    assert.strictEqual(stub.requests.length, n + 1);
  });

  it("marker_write_failure_no_behavior_change", async () => {
    if (process.platform === "win32" || (process.getuid && process.getuid() === 0)) return;
    stub.respondWith(() => DENY);
    fs.mkdirSync(stateDir(), { recursive: true });
    fs.chmodSync(stateDir(), 0o500);
    const rs = await runN(2, READ_TOKEN);
    for (const r of rs) {
      assert.strictEqual(r.code, 0);
      assert.strictEqual(r.stdout, "");
      assert.strictEqual((r.stderr.match(/observe denied/g) || []).length, 1);
    }
    assert.ok(!fs.existsSync(marker()));
  });

  it("warning_sentinel_free", async () => {
    stub.respondWith(() => DENY);
    const r = await run(READ_TOKEN);
    assert.ok(!r.stderr.includes("SENTINEL") && !r.stderr.includes("/observe"));
  });
});
