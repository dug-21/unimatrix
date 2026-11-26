"use strict";

// vnc-050 R-21 backward-compat matrix (FR-34..39, AC-20..24) — test-plan
// backward-compat.md. Old client = published @dug-21/unimatrix@0.11.3
// (integrity-pinned; git-archive fallback flagged). Server = this checkout's
// cargo-built binary via the real-server harness. Every cell is 30 s bounded.
//
// N/A cells (stated, not tested): old × UNIMATRIX_BUNDLE (variable unknown to
// 0.11.3); new × scoped UNIMATRIX_BUNDLE (bridge-transports + headless-smoke).

const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const { spawn } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");

const { startRealServer, resolveServerBinary } = require("./helpers/real-server");
const { materializeOldClient } = require("./helpers/old-client");
const { makeCaFixture } = require("./helpers/ca-fixture");
const newBundle = require("../lib/hook-client/bundle");

const NEW_BIN = path.resolve(__dirname, "../bin/unimatrix.js");
const FIXTURE = path.join(__dirname, "fixtures", "pre-vnc050-bundle.txt");
const CELL_MS = 30000;

let SKIP = false;
try {
  resolveServerBinary();
} catch (e) {
  SKIP = "server binary not built (cargo build first)";
}

const sha = (f) => crypto.createHash("sha256").update(fs.readFileSync(f)).digest("hex");

/** Spawn with a hard 30 s bound; resolves { code, stdout, stderr, timedOut }. */
function run(cmd, args, opts, input) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, Object.assign({ stdio: ["pipe", "pipe", "pipe"] }, opts));
    const o = [];
    const e = [];
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, CELL_MS);
    child.stdout.on("data", (c) => o.push(c));
    child.stderr.on("data", (c) => e.push(c));
    child.on("error", reject);
    child.on("exit", (code) => {
      clearTimeout(timer);
      resolve({ code, timedOut, stdout: Buffer.concat(o).toString(), stderr: Buffer.concat(e).toString() });
    });
    if (input === undefined) child.stdin.end();
    else input(child);
  });
}

/** Sequential stdio MCP exchange (waits for each response before the next request). */
function bridge(cmd, args, opts, requests) {
  const responses = new Map();
  return run(cmd, args, opts, (child) => {
    let buf = "";
    let i = 0;
    const wanted = requests.filter((r) => r.id !== undefined).length;
    const pump = () => {
      while (i < requests.length) {
        const r = requests[i++];
        child.stdin.write(JSON.stringify(r) + "\n");
        if (r.id !== undefined) return;
      }
      child.stdin.end();
    };
    child.stdout.on("data", (c) => {
      buf += c.toString();
      let n;
      while ((n = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, n).trim();
        buf = buf.slice(n + 1);
        if (!line) continue;
        const m = JSON.parse(line);
        if (m.id !== undefined) responses.set(m.id, m);
        if (responses.size >= wanted) child.stdin.end();
        else pump();
      }
    });
    pump();
  }).then((r) => Object.assign(r, { responses }));
}

const INIT = { jsonrpc: "2.0", id: 1, method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "compat", version: "0" } } };
const INITIALIZED = { jsonrpc: "2.0", method: "notifications/initialized" };
const STATUS = { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "context_status", arguments: {} } };
const LOOKUP = { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "context_lookup", arguments: { query: "compat" } } };
const ok = (m) => !!m && !m.error && m.result && m.result.isError !== true;

/** Old-client env observe (AC-22): resolve config from env via the OLD config.js, post a Ping via the OLD transport. */
function oldObserve(oldDir, env, cwd) {
  const script =
    "const c=require(process.argv[1]+'/lib/hook-client/config.js');" +
    "const t=require(process.argv[1]+'/lib/hook-client/transport-http.js');" +
    "const r=c.resolve(process.cwd());" +
    "if(!r||r.kind!=='http'&&!r.url){console.log(JSON.stringify({resolved:r&&r.kind||null}));process.exit(0)}" +
    "t.post(r,{type:'Ping'},{}).then(x=>console.log(JSON.stringify({status:x.status,ok:x.ok})))";
  return run(process.execPath, ["-e", script, oldDir], { env, cwd });
}

function mkProject(root, name) {
  const dir = path.join(root, name);
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function clientEnv(home, extra) {
  const env = { PATH: process.env.PATH, HOME: home, USERPROFILE: home };
  return Object.assign(env, extra || {});
}

describe("vnc-050 R-21 compat matrix", { skip: SKIP }, () => {
  let old;
  let tmp;
  let ca; // 0.11.3 env observe is UNPINNED: it needs a CA-trusted leaf (NODE_EXTRA_CA_CERTS)
  before(() => {
    ca = makeCaFixture();
    assert.ok(ca, "openssl required for the local CA fixture");
    old = materializeOldClient();
    // Flag the fallback source loudly in the test output (OQ-7).
    if (old.source !== "npm") console.log("# compat: old client from " + old.source + " (registry unreachable)");
    tmp = fs.mkdtempSync(path.join(os.tmpdir(), "uni-compat-"));
  });
  after(() => {
    fs.rmSync(tmp, { recursive: true, force: true });
    if (ca) fs.rmSync(ca.dir, { recursive: true, force: true });
  });

  it("pinned_fixture_bundle_frozen", () => {
    const raw = fs.readFileSync(FIXTURE, "utf8").trim();
    const oldBundle = require(path.join(old.dir, "lib", "hook-client", "bundle.js"));
    const a = oldBundle.decodeBundle(raw);
    const b = newBundle.decodeBundle(raw);
    assert.deepEqual(b, a);
    assert.match(b.fp, /^sha256:[0-9a-f]{64}$/);
  });

  describe("legacy enabled", () => {
    let srv;
    let legacy;
    before(async () => {
      srv = await startRealServer({
        publicUrl: true, tlsPair: ca.good,
        // Reuse the developer/CI model cache so the AC-20 old-client write can embed.
        env: { UNIMATRIX_MODEL_CACHE: process.env.UNIMATRIX_MODEL_CACHE || path.join(os.homedir(), ".cache", "unimatrix", "models") },
      });
      const r = await srv.cli(["client-bundle", srv.slug]);
      assert.equal(r.code, 0, r.stderr);
      legacy = r.stdout.trim();
    });
    after(async () => srv && (await srv.close()));

    it("compat_matrix_legacy_enabled: old init --bundle + old/new bridge, remote.json byte-identical (AC-20/21/24)", async () => {
      const home = path.join(tmp, "en-home");
      fs.mkdirSync(home);
      const proj = mkProject(tmp, "en-proj");
      const init = await run(process.execPath, [old.bin, "init", "--bundle", legacy], { cwd: proj, env: clientEnv(home) });
      assert.equal(init.code, 0, init.stderr + init.stdout);
      const hash = require(path.join(old.dir, "lib", "hook-client", "config.js")).computeProjectHash(proj);
      const remote = path.join(home, ".unimatrix", hash, "remote.json");
      assert.ok(fs.existsSync(remote), "old init wrote remote.json");
      const before = sha(remote);

      const reqs = [INIT, INITIALIZED, STATUS, LOOKUP];
      const ob = await bridge(process.execPath, [path.join(old.dir, "lib/hook-client/mcp-bridge.js"), hash],
        { cwd: proj, env: clientEnv(home) }, reqs);
      assert.ok(ok(ob.responses.get(2)) && ok(ob.responses.get(3)), "old bridge: " + ob.stderr);

      const nb = await bridge(process.execPath, [NEW_BIN, "mcp-bridge", hash], { cwd: proj, env: clientEnv(home) }, reqs);
      assert.ok(ok(nb.responses.get(2)) && ok(nb.responses.get(3)), "new bridge on old remote.json: " + nb.stderr);
      assert.equal(sha(remote), before, "remote.json byte-identical after old + new runs");

      // AC-24: new client, pre-vnc-050 pinned bundle via env — pinned branch, no CA store involved
      // (the self-signed cert would fail any CA verification).
      const eb = await bridge(process.execPath, [NEW_BIN, "mcp-bridge"],
        { cwd: tmp, env: clientEnv(path.join(tmp, "en-home2"), { UNIMATRIX_BUNDLE: legacy, UNIMATRIX_PROJECT_ROOT: tmp }) }, reqs);
      assert.ok(ok(eb.responses.get(2)), "new bridge pinned UNIMATRIX_BUNDLE: " + eb.stderr);
      assert.equal(typeof newBundle.decodeBundle(legacy).fp, "string");
    });

    it("compat_matrix_legacy_enabled: old bridge writes via legacy token (AC-20 write)", async () => {
      const home = path.join(tmp, "wr-home");
      fs.mkdirSync(home);
      const proj = mkProject(tmp, "wr-proj");
      const init = await run(process.execPath, [old.bin, "init", "--bundle", legacy], { cwd: proj, env: clientEnv(home) });
      assert.equal(init.code, 0, init.stderr + init.stdout);
      const hash = require(path.join(old.dir, "lib", "hook-client", "config.js")).computeProjectHash(proj);
      const content = "old client legacy write compat marker";
      const STORE = { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "context_store",
        arguments: { title: "compat old write", content, topic: "compat-write", category: "pattern", agent_id: "human" } } };
      const deadline = Date.now() + 120000; // embedding model loads asynchronously after boot
      let last;
      for (;;) {
        last = await bridge(process.execPath, [path.join(old.dir, "lib/hook-client/mcp-bridge.js"), hash],
          { cwd: proj, env: clientEnv(home) }, [INIT, INITIALIZED, STORE]);
        if (ok(last.responses.get(2)) || Date.now() > deadline) break;
        await new Promise((res) => setTimeout(res, 2000));
      }
      assert.ok(ok(last.responses.get(2)), "old bridge store: " + JSON.stringify(last.responses.get(2)) + last.stderr);
      const LOOK = { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "context_lookup", arguments: { topic: "compat-write" } } };
      const lk = await bridge(process.execPath, [path.join(old.dir, "lib/hook-client/mcp-bridge.js"), hash],
        { cwd: proj, env: clientEnv(home) }, [INIT, INITIALIZED, LOOK]);
      assert.ok(ok(lk.responses.get(3)) && JSON.stringify(lk.responses.get(3)).includes("compat old write"),
        "written entry visible: " + JSON.stringify(lk.responses.get(3)));
    });

    it("compat_matrix_legacy_enabled: old env observe lands (AC-22; 0.11.3 ignores the fingerprint var, env path is CA-verified)", async () => {
      const home = path.join(tmp, "obs-home");
      fs.mkdirSync(home);
      const proj = mkProject(tmp, "obs-proj");
      const r = await oldObserve(old.dir, clientEnv(home, {
        UNIMATRIX_REMOTE_URL: srv.url, UNIMATRIX_REMOTE_TOKEN: srv.token, UNIMATRIX_REMOTE_FINGERPRINT: srv.pinnedFp, NODE_EXTRA_CA_CERTS: ca.caPath,
      }), proj);
      assert.equal(r.code, 0, r.stderr);
      const res = JSON.parse(r.stdout.trim());
      assert.ok(res.status >= 200 && res.status < 300, "old observe status " + r.stdout);
    });

    it("old_client_rejects_fp_null_loudly", async () => {
      const b = newBundle.decodeBundle(legacy);
      const nullFp = "unimatrix-bundle:" + Buffer.from(JSON.stringify(
        { v: 2, mcp_url: b.mcp_url, observe_url: b.observe_url, token: b.token, fp: null })).toString("base64url");
      const home = path.join(tmp, "fp-home");
      fs.mkdirSync(home);
      const proj = mkProject(tmp, "fp-proj");
      const r = await run(process.execPath, [old.bin, "init", "--bundle", nullFp], { cwd: proj, env: clientEnv(home) });
      assert.notEqual(r.code, 0);
      assert.ok(!r.timedOut);
      assert.ok(!fs.existsSync(path.join(home, ".unimatrix")), "no credential written");
      assert.ok(!(r.stdout + r.stderr).includes(b.token), "token never echoed");
    });
  });

  describe("legacy disabled", () => {
    let srv;
    let legacy;
    before(async () => {
      srv = await startRealServer({ publicUrl: true, tlsPair: ca.good, env: { UNIMATRIX_LEGACY_TOKEN_ENABLED: "false" } });
      const r = await srv.cli(["client-bundle", srv.slug]);
      assert.equal(r.code, 0, r.stderr);
      legacy = r.stdout.trim();
    });
    after(async () => srv && (await srv.close()));

    it("compat_matrix_legacy_disabled: 401 disabled, clients exit without hang (AC-23)", async () => {
      const raw = await srv.post({ type: "Ping" });
      assert.equal(raw.status, 401);
      assert.match(raw.body.toString(), /legacy token disabled/);

      const home = path.join(tmp, "dis-home");
      fs.mkdirSync(home);
      const proj = mkProject(tmp, "dis-proj");
      const init = await run(process.execPath, [old.bin, "init", "--bundle", legacy], { cwd: proj, env: clientEnv(home) });
      assert.ok(!init.timedOut, "old init must not hang");
      assert.notEqual(init.code, 0, "old init fails on 401");
      assert.ok(!(init.stdout + init.stderr).includes(srv.token));

      const obs = await oldObserve(old.dir, clientEnv(home, {
        UNIMATRIX_REMOTE_URL: srv.url, UNIMATRIX_REMOTE_TOKEN: srv.token, UNIMATRIX_REMOTE_FINGERPRINT: srv.pinnedFp, NODE_EXTRA_CA_CERTS: ca.caPath,
      }), proj);
      assert.equal(obs.code, 0, "old observe non-fatal: " + obs.stderr);
      assert.equal(JSON.parse(obs.stdout.trim()).status, 401);

      const nb = await bridge(process.execPath, [NEW_BIN, "mcp-bridge"],
        { cwd: tmp, env: clientEnv(path.join(tmp, "dis-home2"), { UNIMATRIX_BUNDLE: legacy }) }, [INIT, INITIALIZED, STATUS]);
      assert.ok(!nb.timedOut, "new bridge must not hang");
      assert.ok(!ok(nb.responses.get(1)) || !ok(nb.responses.get(2)), "legacy-disabled bridge call must fail");
      assert.ok(!(nb.stdout + nb.stderr).includes(srv.token));
    });
  });

  it("fr39_legacy_response_diff", { skip: process.env.UNIMATRIX_COMPAT_BASELINE_BIN ? false :
    "needs a main-built server binary: set UNIMATRIX_COMPAT_BASELINE_BIN (pre-PR gate)" }, async () => {
    const shape = (v) => (Array.isArray(v) ? v.map(shape) : v && typeof v === "object"
      ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, shape(v[k])])) : typeof v);
    const probe = async (bin) => {
      const s = await startRealServer({ publicUrl: true, bin });
      try {
        const out = [];
        for (const f of [{ type: "Ping" }, { type: "SessionRegister", session_id: "fr39", cwd: "/x", agent_role: null, feature: null }]) {
          const r = await s.post(f);
          let body = null;
          try { body = shape(JSON.parse(r.body.toString())); } catch (_e) { body = r.body.length ? "text" : "empty"; }
          out.push({ status: r.status, ct: r.contentType, body });
        }
        return out;
      } finally {
        await s.close();
      }
    };
    assert.deepEqual(await probe(undefined), await probe(process.env.UNIMATRIX_COMPAT_BASELINE_BIN));
  });
});
