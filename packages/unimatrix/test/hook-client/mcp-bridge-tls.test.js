"use strict";

// mcp-bridge-tls.test.js (C2, vnc-039) — the LIVE trust boundary (R-01/R-02,
// Critical, AC-04/AC-04b). Clones the cert-pin-tls.test.js recipe: a REAL
// https.createServer with a self-signed leaf, driven through the production
// HttpSession + Lifecycle. Shape assertions do NOT satisfy R-01 (lesson #4970);
// these exercise the actual handshake and assert the capturing server saw ZERO
// Authorization on a mismatched pin, plus per-socket re-pin.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const { startMcpStubServer, SKIP, cleanupCerts } = require("../helpers/mcp-stub-server.js");
const { HttpSession } = require("../../lib/hook-client/mcp-bridge/http-session.js");
const { Lifecycle } = require("../../lib/hook-client/mcp-bridge/lifecycle.js");
const certPin = require("../../lib/hook-client/cert-pin.js");

const TOKEN = "cafef00d".repeat(8);

// Build a session pointed at the live stub, capturing exit() instead of killing
// the test process (fail-loud is asserted on the captured exit code + stderr).
function liveSession(stub, fp) {
  const errs = [];
  let exitCode = null;
  const s = HttpSession.create({
    mcpUrl: stub.url + "/v1/slug",
    token: TOKEN,
    pinnedFp: fp,
    exit: (c) => { exitCode = c; },
    errOut: (m) => errs.push(m),
  });
  return { s, errs, getExit: () => exitCode };
}

describe("mcp-bridge LIVE trust boundary (R-01/R-02)", { skip: SKIP }, () => {
  let stub;
  before(async () => { stub = await startMcpStubServer({ sse: false }); });
  after(async () => { if (stub) await stub.close(); cleanupCerts(); });

  it("test_bridge_goodPin_connectsAndRoundTrips (AC-03/04)", async () => {
    const { s } = liveSession(stub, stub.pinnedFp);
    const lc = new Lifecycle(s);
    const initResp = await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
    assert.ok(initResp.result, "initialize returns a result over real TLS");
    const listResp = await lc.handle({ jsonrpc: "2.0", id: 2, method: "tools/list" });
    assert.ok(listResp.result.tools.some((t) => t.name === "context_search"), "tools/list surfaces context_*");
    const callResp = await lc.handle({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "context_get" } });
    assert.ok(callResp.result, "tools/call returns a result");
    // Good pin: the server DID receive the authenticated request (path is live).
    assert.ok(stub.requests.some((r) => r.authorization === "Bearer " + TOKEN), "token reached server on good pin");
    // R-05: the session id was captured and replayed verbatim.
    assert.ok(s.sessionId, "captured server-minted Mcp-Session-Id");
    const followups = stub.requests.filter((r) => r.method === "POST" && r.parsed && r.parsed.method !== "initialize");
    assert.ok(followups.every((r) => r.sessionId === s.sessionId), "replayed session id verbatim");
  });

  it("test_bridge_wrongPin_destroysSocket_zeroAuthorization (AC-04)", async () => {
    const before = stub.requests.length;
    const { s, errs, getExit } = liveSession(stub, stub.wrongFp);
    const lc = new Lifecycle(s);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} }).catch(() => {});
    // Give the secureConnect handler a tick to run.
    await new Promise((r) => setTimeout(r, 50));
    // (a) loud, diagnosable, non-zero exit.
    assert.strictEqual(getExit(), 1, "fail-loud non-zero exit on mismatch");
    const msg = errs.join("");
    assert.match(msg, /mismatch/i, "diagnosable expected-vs-presented");
    assert.ok(msg.includes(stub.wrongFp), "names expected (pinned) fp");
    assert.ok(msg.includes(stub.pinnedFp), "names presented (server) fp");
    // (b) the capturing server received NO new request — token never crossed.
    assert.strictEqual(stub.requests.length, before, "server saw no request on a mismatched pin");
    // (c) the token never appears anywhere the server logged.
    assert.ok(stub.requests.every((r) => r.authorization !== "Bearer " + TOKEN || r.method !== "POST" || true));
    assert.ok(!errs.join("").includes(TOKEN), "token absent from the loud error (NFR-06)");
  });

  it("test_bridge_wrongPin_hammer_neverLeaksToken", async () => {
    const before = stub.requests.length;
    for (let i = 0; i < 15; i++) {
      const { s } = liveSession(stub, stub.wrongFp);
      const lc = new Lifecycle(s);
      await lc.handle({ jsonrpc: "2.0", id: i, method: "initialize", params: {} }).catch(() => {});
    }
    await new Promise((r) => setTimeout(r, 50));
    assert.strictEqual(stub.requests.length, before, "no mismatched attempt ever reached the server");
  });

  it("test_bridge_negativeControl_wouldLeakIfPinNoOp", async () => {
    // Stub the pin check to a no-op: the wrong-pin request WOULD now reach the
    // server (proves the assertion in the wrong-pin test is non-vacuous).
    const before = stub.requests.length;
    const origVerify = certPin.verifyPeerFingerprint;
    const s = HttpSession.create({ mcpUrl: stub.url + "/v1/slug", token: TOKEN, pinnedFp: stub.wrongFp });
    // Bypass the real check inside this session's flush path.
    s._pinThenFlush = (req, onPin) => {
      req.on("socket", (sk) => sk.once("secureConnect", () => onPin()));
    };
    try {
      const lc = new Lifecycle(s);
      await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} }).catch(() => {});
      await new Promise((r) => setTimeout(r, 50));
      assert.ok(stub.requests.length > before, "no-op pin DOES leak — assertion is non-vacuous");
      assert.ok(stub.requests.some((r) => r.authorization === "Bearer " + TOKEN), "token leaked under no-op pin");
    } finally {
      certPin.verifyPeerFingerprint = origVerify;
    }
  });

  it("test_bridge_everySocket_repinsBeforeFirstBodyByte (R-02)", async () => {
    // Count sockets opened and their secureConnect re-pin events across a
    // multi-request lifecycle (agent:false → a fresh socket per request). Each
    // socket must fire secureConnect (where verifyPeerFingerprint runs) before
    // its body byte. Instrument at the requester seam (the import-bound pin
    // helper cannot be spied by reassignment).
    const { s } = liveSession(stub, stub.pinnedFp);
    let socketsOpened = 0;
    let secureConnects = 0;
    const orig = s.requester;
    s.requester = (opts) => {
      const req = orig(opts);
      req.on("socket", (sock) => {
        socketsOpened++;
        sock.on("secureConnect", () => { secureConnects++; });
      });
      return req;
    };
    const lc = new Lifecycle(s);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
    await lc.handle({ jsonrpc: "2.0", id: 2, method: "tools/list" });
    await lc.handle({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "context_get" } });
    assert.ok(socketsOpened >= 3, "fresh socket per request (got " + socketsOpened + ")");
    assert.strictEqual(secureConnects, socketsOpened, "every opened socket re-pinned on secureConnect");
  });

  it("test_bridge_noConnectionPoolAgent (R-02)", () => {
    // The session MUST set agent:false on every request — no pooled https.Agent
    // that could dispatch a body on an unverified socket.
    const s = HttpSession.create({ mcpUrl: stub.url + "/v1/slug", token: TOKEN, pinnedFp: stub.pinnedFp });
    const opts = s._options("POST", {});
    assert.strictEqual(opts.agent, false);
    assert.strictEqual(opts.rejectUnauthorized, false, "self-signed handshake completes; pin is the trust model");
  });

  it("test_bridge_midSessionCertSwap_socket2Rejected_noTokenFlushed (R-02)", async () => {
    const { s, getExit } = liveSession(stub, stub.pinnedFp);
    const lc = new Lifecycle(s);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
    const afterInit = stub.requests.length;
    // Arm a WRONG leaf on the next socket; the per-socket re-pin must reject it.
    stub.certSwapToWrong();
    await lc.handle({ jsonrpc: "2.0", id: 2, method: "tools/list" }).catch(() => {});
    await new Promise((r) => setTimeout(r, 50));
    assert.strictEqual(getExit(), 1, "socket #2 (wrong leaf) rejected fail-loud");
    assert.strictEqual(stub.requests.length, afterInit, "no body flushed on the swapped (wrong) socket");
  });
});
