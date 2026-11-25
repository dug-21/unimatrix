"use strict";

// ca-trust-tls.test.js — vnc-050 C10 (ADR-006/007, R-01/R-08/R-09/AC-02):
// unpinned HTTPS uses Node CA verification; untrusted / wrong-host certs get
// ZERO application bytes; extra headers ride every request; reserved headers
// are always the client's. Real sockets; child processes carry
// NODE_EXTRA_CA_CERTS (read only at process start).

const { describe, it, before, after } = require("node:test");
const assert = require("node:assert");
const path = require("path");
const { spawn } = require("child_process");
const { makeCaFixture, startByteCapture } = require("../helpers/ca-fixture.js");
const { assertCaOptions, applyExtraHeaders } = require("../../lib/hook-client/tls-guard.js");

const LIB = path.resolve(__dirname, "../../lib/hook-client");
const TOKEN = "tok_SENTINEL_9f8e7d";
const HDR = "hdr_SENTINEL_1a2b3c";
const fx = makeCaFixture();
const opt = fx ? {} : { skip: "openssl unavailable" };

// Run transport-http in a child with the local CA trusted; prints JSON result.
function childPost(port, extra, fn, extraEnv) {
  const script = `
    const t = require(${JSON.stringify(path.join(LIB, "transport-http.js"))});
    const a = JSON.parse(process.env.ARGS), T = { connectMs: 8000, syncMs: 9000, fnfMs: 9000 };
    (a.fn === "ping" ? t.pingForInit(a.url, a.tok, T, null, a.extra)
      : t.post({ url: a.url, token: a.tok, extraHeaders: a.extra, timeouts: T }, { type: "Ping" }, { sync: true }))
      .then((r) => { process.stdout.write(JSON.stringify(Object.assign({}, r, { body: undefined }))); });`;
  // Async spawn: spawnSync would block this process's capture server.
  return new Promise((resolve) => {
    const c = spawn(process.execPath, ["-e", script], {
      env: { ...process.env, NODE_EXTRA_CA_CERTS: fx.caPath, ...extraEnv,
        ARGS: JSON.stringify({ url: `https://localhost:${port}/v1/s/observe`, tok: TOKEN, extra, fn }) },
    });
    let stdout = "", stderr = "";
    c.stdout.on("data", (d) => { stdout += d; });
    c.stderr.on("data", (d) => { stderr += d; });
    c.on("exit", () => resolve({ res: stdout ? JSON.parse(stdout) : null, stderr, stdout }));
  });
}

function runBridge(port, extraEnv) {
  return new Promise((resolve) => {
    const c = spawn(process.execPath, [path.join(LIB, "mcp-bridge.js")], {
      env: { ...process.env, HOME: fx.dir, NODE_EXTRA_CA_CERTS: fx.caPath,
        UNIMATRIX_REMOTE_MCP_URL: `https://localhost:${port}/v1/s/mcp`, UNIMATRIX_REMOTE_TOKEN: TOKEN,
        UNIMATRIX_REMOTE_HEADERS: JSON.stringify({ "X-Sentinel": HDR }), ...extraEnv },
    });
    let out = "", err = "";
    c.stdout.on("data", (d) => { out += d; if (out.includes("\n")) c.stdin.end(); });
    c.stderr.on("data", (d) => { err += d; });
    c.on("exit", (code) => resolve({ code, out, err }));
    c.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize",
      params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "t", version: "0" } } }) + "\n");
    setTimeout(() => c.kill(), 15000);
  });
}

// Minimal MCP responder: initialize → JSON with session id; DELETE → 204.
function mcpRespond(buf, sock) {
  const s = buf.toString("latin1");
  const reqs = s.split(/(?=POST |DELETE )/).filter(Boolean);
  const last = reqs[reqs.length - 1];
  const he = last.indexOf("\r\n\r\n");
  if (he < 0 || sock._answered === reqs.length) return;
  const m = /content-length:\s*(\d+)/i.exec(last.slice(0, he));
  if (m && Buffer.byteLength(last.slice(he + 4), "latin1") < Number(m[1])) return;
  sock._answered = reqs.length;
  if (last.startsWith("DELETE")) { sock.end("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"); return; }
  const body = JSON.stringify({ jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-06-18",
    capabilities: {}, serverInfo: { name: "s", version: "0" } } });
  sock.end("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: sid-1\r\nConnection: close\r\n" +
    "Content-Length: " + Buffer.byteLength(body) + "\r\n\r\n" + body);
}

describe("vnc-050 C10 CA trust branch", opt, () => {
  const servers = [];
  const cap = async (pair, respond) => { const c = await startByteCapture(pair, respond); servers.push(c); return c; };
  after(() => servers.forEach((c) => c.server.close()));

  it("guard_throws_on_reject_unauthorized_false", () => {
    assert.throws(() => assertCaOptions({ rejectUnauthorized: false }), /rejectUnauthorized:true/);
    assert.throws(() => assertCaOptions({ hostname: "h" }), /rejectUnauthorized:true/, "missing key rejected");
    assert.throws(() => assertCaOptions({ rejectUnauthorized: true, checkServerIdentity: () => undefined }), /hostname/);
    const o = { hostname: "h", rejectUnauthorized: true };
    assert.strictEqual(assertCaOptions(o), o);
  });

  it("reserved_headers_own_even_if_parser_bypassed", () => {
    const h = applyExtraHeaders({}, [["authorization", "x"], ["Mcp-Session-Id", "y"], ["X-A", "1"]]);
    assert.deepStrictEqual(h, { "X-A": "1" });
  });

  for (const [name, which] of [["untrusted_self_signed", "self"], ["ca_trusted_wrong_host", "wrong"]]) {
    it(name + "_zero_bytes_observe_post", async () => {
      const c = await cap(fx[which]);
      const { res, stderr, stdout } = await childPost(c.port, [["X-Sentinel", HDR]]);
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
      assert.strictEqual(c.bytes.length, 0, "zero application bytes");
      assert.ok(!(stderr + stdout).includes(TOKEN) && !(stderr + stdout).includes(HDR));
    });

    it(name + "_zero_bytes_ping_for_init", async () => {
      const c = await cap(fx[which]);
      const { res } = await childPost(c.port, [["X-Sentinel", HDR]], "ping");
      assert.strictEqual(res.ok, false);
      assert.ok(!res.message.includes(TOKEN));
      assert.strictEqual(c.bytes.length, 0);
    });

    it(name + "_zero_bytes_bridge", async () => {
      const c = await cap(fx[which], mcpRespond);
      const r = await runBridge(c.port);
      assert.strictEqual(r.code, 1);
      assert.strictEqual(c.bytes.length, 0);
      assert.match(r.err, /TLS certificate verification failed/);
      assert.ok(!(r.err + r.out).includes(TOKEN) && !(r.err + r.out).includes(HDR));
    });
  }

  it("global_tls_override_ignored_ca_path", async () => {
    const c = await cap(fx.self);
    const { res } = await childPost(c.port, [], undefined, { NODE_TLS_REJECT_UNAUTHORIZED: "0" });
    assert.strictEqual(res.ok, false);
    assert.strictEqual(c.bytes.length, 0, "zero application bytes");
    const b = await cap(fx.self, mcpRespond);
    const r = await runBridge(b.port, { NODE_TLS_REJECT_UNAUTHORIZED: "0" });
    assert.strictEqual(r.code, 1);
    assert.strictEqual(b.bytes.length, 0);
    assert.ok(!(r.err + r.out).includes(TOKEN), "no token in output");
  });

  it("ca_branch_connects_with_extra_headers_observe_and_ping", async () => {
    const c = await cap(fx.good);
    const { res } = await childPost(c.port, [["X-Sentinel", HDR], ["Authorization", "evil"]]);
    assert.strictEqual(res.status, 204, JSON.stringify(res));
    const s = c.bytes.toString("latin1");
    assert.match(s, new RegExp("X-Sentinel: " + HDR));
    assert.match(s, new RegExp("Authorization: Bearer " + TOKEN));
    assert.ok(!s.includes("evil"));
    const c2 = await cap(fx.good, (b, sock) => {
      if (b.includes("\r\n\r\n") && /"Ping"/.test(b)) {
        const body = '{"type":"Pong","server_version":"t"}';
        sock.end("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: " + body.length + "\r\n\r\n" + body);
      }
    });
    const p = await childPost(c2.port, [["X-Sentinel", HDR]], "ping");
    assert.strictEqual(p.res.ok, true, p.res.message);
    assert.match(c2.bytes.toString("latin1"), new RegExp("X-Sentinel: " + HDR));
  });

  it("env_pair_attach_ca_headers_on_initialize_and_delete", async () => {
    const c = await cap(fx.good, mcpRespond);
    const r = await runBridge(c.port);
    assert.strictEqual(r.code, 0, r.err);
    assert.match(r.out, /"serverInfo"/);
    const s = c.bytes.toString("latin1");
    assert.match(s, /^POST \/v1\/s\/mcp/);
    assert.match(s, /DELETE \/v1\/s\/mcp/);
    assert.strictEqual(s.split("X-Sentinel: " + HDR).length - 1, 2, "header on POST and DELETE");
    assert.ok(!r.err.includes(TOKEN) && !r.err.includes(HDR));
  });

  it("bridge_bundle_conflict_exits_1_naming_var", async () => {
    const r = await runBridge(1, { UNIMATRIX_BUNDLE: "x" });
    assert.strictEqual(r.code, 1);
    assert.match(r.err, /UNIMATRIX_REMOTE_MCP_URL/);
    assert.ok(!r.err.includes(TOKEN));
  });
});
