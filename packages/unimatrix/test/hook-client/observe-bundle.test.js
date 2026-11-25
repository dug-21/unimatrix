"use strict";

// vnc-050 R-07 scenarios 2 + 3 (SR-15, AC-16 observe half) through the REAL
// hook entry (lib/hook-client/index.js -> config.js -> remote-resolve.js):
//   - UNIMATRIX_BUNDLE alone (clean HOME, no remote.json, no URL/TOKEN env)
//     -> the event lands at the bundle's observe_url with the bundle token,
//     for a pinned fp bundle and an fp:null (CA-trust) bundle.
//   - precedence bundle > env pair > credential store, and bundle + env var
//     -> conflict (nothing sent), matching resolveRemote / the bridge.
// Extends test/helpers/stub-server.js (tls option) + test/helpers/ca-fixture.js.

const { describe, it, before, after, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { spawn } = require("child_process");
const { startStubServer } = require("../helpers/stub-server");
const { makeCaFixture } = require("../helpers/ca-fixture");
const configMod = require("../../lib/hook-client/config");
const credstore = require("../../lib/hook-client/credstore");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");
const STDIN = JSON.stringify({ hook_event_name: "Stop", session_id: "s-bundle" });
const BUNDLE_TOKEN = "ab".repeat(32);
const ENV_TOKEN = "cd".repeat(32);
const STORE_TOKEN = "ef".repeat(32);
// Every remote var the resolver reads; stripped from the inherited env.
const REMOTE_VARS = [
  "UNIMATRIX_BUNDLE", "UNIMATRIX_REMOTE_URL", "UNIMATRIX_REMOTE_MCP_URL", "UNIMATRIX_REMOTE_TOKEN",
  "UNIMATRIX_REMOTE_FINGERPRINT", "UNIMATRIX_REMOTE_HEADERS", "NODE_EXTRA_CA_CERTS",
];

const ca = makeCaFixture();
const SKIP = ca ? false : "openssl unavailable";

function fpOf(pem) {
  return "sha256:" + crypto.createHash("sha256").update(new crypto.X509Certificate(pem).raw).digest("hex");
}

function bundle(observeUrl, token, fp) {
  const obj = { v: 2, mcp_url: observeUrl.replace(/\/observe$/, "/mcp"), observe_url: observeUrl, token, fp };
  return "unimatrix-bundle:" + Buffer.from(JSON.stringify(obj)).toString("base64url");
}

let root;

function run(extraEnv) {
  const env = Object.assign({}, process.env);
  for (const v of REMOTE_VARS) delete env[v];
  Object.assign(env, { HOME: root, USERPROFILE: root }, extraEnv);
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

function writeStore(observeUrl) {
  const hash = configMod.computeProjectHash(configMod.walkToProjectRoot(root));
  const prev = process.env.HOME;
  process.env.HOME = root;
  try {
    credstore.write(hash, {
      mcp_url: observeUrl.replace(/\/observe$/, "/mcp"), observe_url: observeUrl,
      token: STORE_TOKEN, fingerprint: null,
    });
  } finally {
    process.env.HOME = prev;
  }
  return path.join(root, ".unimatrix", hash, "remote.json");
}

const bearer = (e) => (e.headers.authorization || "").replace(/^Bearer /, "");
const noSecrets = (r) => {
  for (const t of [BUNDLE_TOKEN, ENV_TOKEN, STORE_TOKEN]) {
    assert.ok(!r.stdout.includes(t) && !r.stderr.includes(t), "token leaked to hook output");
  }
};

describe("hook observe via UNIMATRIX_BUNDLE (vnc-050 R-07.3)", { skip: SKIP }, () => {
  let pinned;
  let caSrv;
  before(async () => {
    pinned = await startStubServer({ tls: { cert: ca.self.cert, key: ca.self.key } });
    caSrv = await startStubServer({ tls: { cert: ca.good.cert, key: ca.good.key } });
  });
  after(async () => {
    await pinned.close();
    await caSrv.close();
  });
  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-obsbundle-"));
    fs.mkdirSync(path.join(root, ".git"));
    pinned.requests.length = 0;
    caSrv.requests.length = 0;
  });
  afterEach(() => fs.rmSync(root, { recursive: true, force: true }));

  it("bundle_only_pinned_fp_event_lands_at_bundle_observe_url", async () => {
    const url = pinned.url + "/v1/alpha/observe";
    const r = await run({ UNIMATRIX_BUNDLE: bundle(url, BUNDLE_TOKEN, fpOf(ca.self.cert)) });
    assert.strictEqual(r.code, 0, r.stderr);
    assert.strictEqual(pinned.requests.length, 1, "exactly one observe POST: " + r.stderr);
    const e = pinned.requests[0];
    assert.strictEqual(e.method, "POST");
    assert.strictEqual(e.path, "/v1/alpha/observe");
    assert.strictEqual(bearer(e), BUNDLE_TOKEN);
    assert.ok(!fs.existsSync(path.join(root, ".unimatrix", configMod.computeProjectHash(root), "remote.json")),
      "no remote.json created");
    noSecrets(r);
  });

  it("bundle_only_pinned_wrong_fp_sends_nothing (pin is enforced)", async () => {
    const url = pinned.url + "/v1/alpha/observe";
    const r = await run({ UNIMATRIX_BUNDLE: bundle(url, BUNDLE_TOKEN, "sha256:" + "00".repeat(32)) });
    assert.strictEqual(r.code, 0);
    assert.strictEqual(pinned.requests.length, 0);
    noSecrets(r);
  });

  it("bundle_only_fp_null_ca_trust_event_lands", async () => {
    const url = caSrv.url.replace("127.0.0.1", "localhost") + "/v1/alpha/observe";
    const r = await run({ UNIMATRIX_BUNDLE: bundle(url, BUNDLE_TOKEN, null), NODE_EXTRA_CA_CERTS: ca.caPath });
    assert.strictEqual(r.code, 0, r.stderr);
    assert.strictEqual(caSrv.requests.length, 1, "exactly one observe POST: " + r.stderr);
    assert.strictEqual(caSrv.requests[0].path, "/v1/alpha/observe");
    assert.strictEqual(bearer(caSrv.requests[0]), BUNDLE_TOKEN);
    noSecrets(r);
  });

  it("bundle_fp_null_without_trusted_ca_sends_nothing (CA verified)", async () => {
    const url = caSrv.url.replace("127.0.0.1", "localhost") + "/v1/alpha/observe";
    const r = await run({ UNIMATRIX_BUNDLE: bundle(url, BUNDLE_TOKEN, null) });
    assert.strictEqual(r.code, 0);
    assert.strictEqual(caSrv.requests.length, 0);
    noSecrets(r);
  });
});

describe("hook credential precedence = resolveRemote/bridge (vnc-050 R-07.2)", { skip: SKIP }, () => {
  let bSrv;
  let eSrv;
  let sSrv;
  const host = (s) => s.url.replace("127.0.0.1", "localhost") + "/v1/alpha/observe";
  before(async () => {
    const t = { tls: { cert: ca.good.cert, key: ca.good.key } };
    [bSrv, eSrv, sSrv] = await Promise.all([startStubServer(t), startStubServer(t), startStubServer(t)]);
  });
  after(async () => Promise.all([bSrv.close(), eSrv.close(), sSrv.close()]));
  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-obsprec-"));
    fs.mkdirSync(path.join(root, ".git"));
    for (const s of [bSrv, eSrv, sSrv]) s.requests.length = 0;
  });
  afterEach(() => fs.rmSync(root, { recursive: true, force: true }));
  const counts = () => [bSrv.requests.length, eSrv.requests.length, sSrv.requests.length];
  const CA = () => ({ NODE_EXTRA_CA_CERTS: ca.caPath });

  it("store_only -> store wins", async () => {
    const before = fs.readFileSync(writeStore(host(sSrv)));
    const r = await run(CA());
    assert.deepStrictEqual(counts(), [0, 0, 1], r.stderr);
    assert.strictEqual(bearer(sSrv.requests[0]), STORE_TOKEN);
    assert.deepStrictEqual(fs.readFileSync(writeStorePath()), before, "remote.json unchanged");
  });

  it("env pair + store -> env wins (store not used)", async () => {
    writeStore(host(sSrv));
    const r = await run(Object.assign(CA(), { UNIMATRIX_REMOTE_URL: host(eSrv), UNIMATRIX_REMOTE_TOKEN: ENV_TOKEN }));
    assert.deepStrictEqual(counts(), [0, 1, 0], r.stderr);
    assert.strictEqual(bearer(eSrv.requests[0]), ENV_TOKEN);
  });

  it("bundle + store -> bundle wins", async () => {
    writeStore(host(sSrv));
    const r = await run(Object.assign(CA(), { UNIMATRIX_BUNDLE: bundle(host(bSrv), BUNDLE_TOKEN, null) }));
    assert.deepStrictEqual(counts(), [1, 0, 0], r.stderr);
    assert.strictEqual(bearer(bSrv.requests[0]), BUNDLE_TOKEN);
  });

  for (const v of ["UNIMATRIX_REMOTE_URL", "UNIMATRIX_REMOTE_TOKEN"]) {
    it(`bundle + ${v} -> conflict: nothing sent, exit 0, stderr names ${v}`, async () => {
      writeStore(host(sSrv));
      const val = v === "UNIMATRIX_REMOTE_TOKEN" ? ENV_TOKEN : host(eSrv);
      const r = await run(Object.assign(CA(), { UNIMATRIX_BUNDLE: bundle(host(bSrv), BUNDLE_TOKEN, null), [v]: val }));
      assert.deepStrictEqual(counts(), [0, 0, 0], r.stderr);
      assert.strictEqual(r.code, 0, "hook is non-fatal");
      assert.match(r.stderr, new RegExp(v));
      noSecrets(r);
    });
  }

  it("bundle + env var set to \"\" -> no conflict, bundle wins (AC-28)", async () => {
    const r = await run(Object.assign(CA(), {
      UNIMATRIX_BUNDLE: bundle(host(bSrv), BUNDLE_TOKEN, null), UNIMATRIX_REMOTE_URL: "", UNIMATRIX_REMOTE_TOKEN: "",
    }));
    assert.deepStrictEqual(counts(), [1, 0, 0], r.stderr);
  });
});

function writeStorePath() {
  return path.join(root, ".unimatrix", configMod.computeProjectHash(configMod.walkToProjectRoot(root)), "remote.json");
}
