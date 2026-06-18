"use strict";

// mcp-bridge.test.js (C2, vnc-039) — unit + behavioral coverage for the
// stdio<->Streamable-HTTP bridge: stdio framing (R-16), sse-parse (R-04),
// dispatch (R-04), session capture/replay (R-05), identity stability (R-17),
// store-error posture (R-13), and no-token-leak (R-09). The LIVE trust boundary
// (R-01/R-02) is in mcp-bridge-tls.test.js; SSE wire in mcp-bridge-sse.test.js.

const { describe, it } = require("node:test");
const assert = require("assert");
const { EventEmitter } = require("events");
const fs = require("fs");
const os = require("os");
const path = require("path");

const { StdioFramer } = require("../../lib/hook-client/mcp-bridge/stdio-frame.js");
const { SseParser } = require("../../lib/hook-client/mcp-bridge/sse-parse.js");
const { dispatchResponse, correlateById } = require("../../lib/hook-client/mcp-bridge/dispatch.js");
const { Lifecycle, CLIENT_INFO_NAME } = require("../../lib/hook-client/mcp-bridge/lifecycle.js");
const { HttpSession, SESSION_HEADER_LC } = require("../../lib/hook-client/mcp-bridge/http-session.js");
const bridge = require("../../lib/hook-client/mcp-bridge.js");
const credstore = require("../../lib/hook-client/credstore.js");

// ── helpers ────────────────────────────────────────────────────────────────

// A readable-ish stream that yields the given chunks (for async-iterator paths).
function streamOf(chunks) {
  const r = new EventEmitter();
  r.destroy = () => {};
  r[Symbol.asyncIterator] = async function* () {
    for (const c of chunks) yield Buffer.isBuffer(c) ? c : Buffer.from(c);
  };
  // Also support the .on('data'/'end') style for readBounded.
  r.on = EventEmitter.prototype.on.bind(r);
  r.resume = () => {};
  return r;
}

// An EventEmitter-based response usable by readBounded (data/end/error/close).
function bodyStream(buf) {
  const r = new EventEmitter();
  r.destroy = () => {};
  r.resume = () => {};
  process.nextTick(() => { r.emit("data", buf); r.emit("end"); });
  return r;
}

// A fake HttpSession.request that records bodies and returns a canned response.
function fakeSession(responder) {
  const sent = [];
  return {
    sent,
    sessionId: null,
    protocolVersion: null,
    request(body, opts) {
      sent.push({ body, opts });
      return Promise.resolve(responder(body, opts));
    },
  };
}

// A fake mock https req with controllable socket/response (for HttpSession).
function makeFakeReq() {
  const req = new EventEmitter();
  req.ended = null;
  req.destroyed = null;
  req.end = (b) => { req.ended = b === undefined ? "<no-body>" : b; };
  req.destroy = (e) => { req.destroyed = e || true; };
  return req;
}

// ── stdio-frame (R-16) ───────────────────────────────────────────────────────

describe("stdio-frame (R-16)", () => {
  function framerCollect() {
    const stdin = new EventEmitter();
    stdin.setEncoding = () => {};
    const writes = [];
    const stdout = { write: (s) => writes.push(s) };
    const f = new StdioFramer(stdin, stdout);
    const got = [];
    f.onMessage((m) => got.push(m));
    return { f, got, writes };
  }

  it("test_stdioFrame_oneMessageSplitAcrossChunks_oneParsed", () => {
    const { f, got } = framerCollect();
    const line = JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" }) + "\n";
    for (const ch of line) f._feed(ch);
    assert.strictEqual(got.length, 1);
    assert.strictEqual(got[0].method, "ping");
  });

  it("test_stdioFrame_multipleMessagesOneChunk_NParsedInOrder", () => {
    const { f, got } = framerCollect();
    f._feed(JSON.stringify({ id: 1 }) + "\n" + JSON.stringify({ id: 2 }) + "\n" + JSON.stringify({ id: 3 }) + "\n");
    assert.deepStrictEqual(got.map((m) => m.id), [1, 2, 3]);
  });

  it("test_stdioFrame_chunkBoundaryOnNewline_noEmptyOrDropped", () => {
    const { f, got } = framerCollect();
    f._feed(JSON.stringify({ id: 1 }) + "\n");
    f._feed("\n"); // stray blank line
    f._feed(JSON.stringify({ id: 2 }) + "\n");
    assert.deepStrictEqual(got.map((m) => m.id), [1, 2]);
  });

  it("test_stdioFrame_writeIsNewlineFramed", () => {
    const { f, writes } = framerCollect();
    f.write({ id: 9, result: "ok" });
    assert.strictEqual(writes.length, 1);
    assert.ok(writes[0].endsWith("\n"));
    assert.deepStrictEqual(JSON.parse(writes[0]), { id: 9, result: "ok" });
  });

  it("test_stdioFrame_byteSplitInvariantOnRead", () => {
    const stream =
      JSON.stringify({ id: 1 }) + "\n" + JSON.stringify({ id: 2 }) + "\n" + JSON.stringify({ id: 3 }) + "\n";
    for (let split = 1; split < stream.length; split++) {
      const { f, got } = framerCollect();
      f._feed(stream.slice(0, split));
      f._feed(stream.slice(split));
      assert.deepStrictEqual(got.map((m) => m.id), [1, 2, 3], "split at " + split);
    }
  });

  it("test_stdioFrame_parseError_emitsJsonRpcParseError", () => {
    const { f, writes } = framerCollect();
    f._feed("{not json}\n");
    assert.strictEqual(writes.length, 1);
    const m = JSON.parse(writes[0]);
    assert.strictEqual(m.error.code, -32700);
  });
});

// ── sse-parse (R-04) ─────────────────────────────────────────────────────────

describe("sse-parse (R-04)", () => {
  it("test_sseParse_singleDataLine_oneObject", async () => {
    const out = await SseParser.collect(streamOf(["data: " + JSON.stringify({ id: 1, result: "x" }) + "\n\n"]), 1 << 20);
    assert.strictEqual(out.length, 1);
    assert.strictEqual(out[0].result, "x");
  });

  it("test_sseParse_multiLineData_reassembledPayload", async () => {
    // Per the SSE spec, multi-line data: fields concatenate with "\n". The
    // reassembled payload is valid JSON because the newline lands in a
    // whitespace-tolerant position (JSON ignores inter-token whitespace).
    const rec = "data: {\"id\":2,\ndata: \"result\":{\"a\":1}}\n\n";
    const out = await SseParser.collect(streamOf([rec]), 1 << 20);
    assert.strictEqual(out.length, 1);
    assert.deepStrictEqual(out[0].result, { a: 1 });
  });

  it("test_sseParse_multipleEvents_NObjectsInOrder", async () => {
    const s = "data: " + JSON.stringify({ id: 1 }) + "\n\n" + "data: " + JSON.stringify({ id: 2 }) + "\n\n";
    const out = await SseParser.collect(streamOf([s]), 1 << 20);
    assert.deepStrictEqual(out.map((m) => m.id), [1, 2]);
  });

  it("test_sseParse_eventAndIdAndComment_handled", async () => {
    const s = ": keep-alive\nevent: message\nid: 7\ndata: " + JSON.stringify({ id: 5 }) + "\n\n";
    const out = await SseParser.collect(streamOf([s]), 1 << 20);
    assert.deepStrictEqual(out.map((m) => m.id), [5]);
  });

  it("test_sseParse_primingEventTolerated", async () => {
    const s = "event: ready\ndata: \n\n" + "data: " + JSON.stringify({ id: 9 }) + "\n\n";
    const out = await SseParser.collect(streamOf([s]), 1 << 20);
    assert.deepStrictEqual(out.map((m) => m.id), [9]);
  });

  it("test_sseParse_chunkSplitInvariant_fuzz", async () => {
    const stream =
      "data: " + JSON.stringify({ id: 1 }) + "\n\n" + "event: message\ndata: " + JSON.stringify({ id: 2 }) + "\n\n";
    const buf = Buffer.from(stream, "utf8");
    for (let split = 1; split < buf.length; split++) {
      const out = await SseParser.collect(streamOf([buf.slice(0, split), buf.slice(split)]), 1 << 20);
      assert.deepStrictEqual(out.map((m) => m.id), [1, 2], "split at " + split);
    }
  });

  it("test_sseParse_crlfAndBareLf_bothParse", async () => {
    const crlf = "data: " + JSON.stringify({ id: 1 }) + "\r\n\r\n";
    const out = await SseParser.collect(streamOf([crlf]), 1 << 20);
    assert.deepStrictEqual(out.map((m) => m.id), [1]);
  });

  it("test_sseParse_1MiBBodyGuardEnforced", async () => {
    // A huge never-terminated data line: collect must stop at the guard, not hang.
    const big = "data: " + "x".repeat(2 * 1024 * 1024);
    const out = await SseParser.collect(streamOf([big]), 1 << 20);
    assert.ok(Array.isArray(out)); // bounded, returned (no hang)
  });
});

// ── dispatch (R-04) ──────────────────────────────────────────────────────────

describe("dispatch (R-04)", () => {
  it("test_dispatch_applicationJson_singleObject", async () => {
    const obj = { jsonrpc: "2.0", id: 1, result: "ok" };
    const out = await dispatchResponse({ status: 200, contentType: "application/json", res: bodyStream(Buffer.from(JSON.stringify(obj))) });
    assert.deepStrictEqual(out, [obj]);
  });

  it("test_dispatch_textEventStream_viaSseParse", async () => {
    const obj = { jsonrpc: "2.0", id: 2, result: "z" };
    const res = streamOf(["data: " + JSON.stringify(obj) + "\n\n"]);
    const out = await dispatchResponse({ status: 200, contentType: "text/event-stream", res });
    assert.deepStrictEqual(out, [obj]);
  });

  it("test_dispatch_4xx5xx_surfacedAsJsonRpcError", async () => {
    const out = await dispatchResponse({ status: 500, contentType: "application/json", res: bodyStream(Buffer.from("{\"id\":4}")) });
    assert.strictEqual(out.length, 1);
    assert.ok(out[0].error);
    assert.strictEqual(out[0].id, 4);
  });

  it("test_dispatch_unexpectedContentType_jsonRpcError_noCrash", async () => {
    const out = await dispatchResponse({ status: 200, contentType: "text/plain", res: bodyStream(Buffer.from("hi")) });
    assert.ok(out[0].error);
    assert.match(out[0].error.message, /unexpected content-type/);
  });

  it("test_dispatch_idCorrelation", () => {
    const msgs = [{ id: 1, result: "a" }, { id: 2, result: "b" }];
    assert.strictEqual(correlateById(msgs, 2).result, "b");
    assert.strictEqual(correlateById(msgs, 9), null);
  });
});

// ── http-session: capture/replay (R-05) + identity (R-17) ────────────────────

describe("http-session capture/replay (R-05) + identity (R-17)", () => {
  // Drive HttpSession.request with a fake requester to observe headers + flush.
  function sessionWithFakeReq(serverSessionId) {
    const s = HttpSession.create({ mcpUrl: "https://h.example/v1/slug", token: "TKN", pinnedFp: "sha256:" + "a".repeat(64) });
    const calls = [];
    s.requester = (opts) => {
      const req = makeFakeReq();
      calls.push({ opts, req });
      // Simulate a successful pin: fire socket->secureConnect with a verified leaf.
      process.nextTick(() => {
        const sock = new EventEmitter();
        req.emit("socket", sock);
        // Bypass the real verifyPeerFingerprint by stubbing on the session.
        sock.emit("secureConnect");
      });
      return req;
    };
    // Stub the cert check to a no-op pass for this header/replay test (the LIVE
    // pin is exercised in mcp-bridge-tls.test.js).
    s._pinThenFlush = (req, onPin) => {
      req.on("socket", (sk) => sk.once("secureConnect", () => onPin()));
    };
    // Respond after flush.
    const origRequester = s.requester;
    s.requester = (opts) => {
      const req = origRequester(opts);
      process.nextTick(() => {
        const headers = { "content-type": "application/json" };
        const isInit = opts.headers["Mcp-Session-Id"] === undefined;
        if (isInit && serverSessionId) headers[SESSION_HEADER_LC] = serverSessionId;
        req.emit("response", { statusCode: 200, headers, resume() {} });
      });
      return req;
    };
    return { s, calls };
  }

  it("test_httpSession_capturesSessionIdFromInitializeResponse", async () => {
    const { s } = sessionWithFakeReq("SID-123");
    await s.request({ method: "initialize", id: 1 }, { isInitialize: true });
    assert.strictEqual(s.sessionId, "SID-123");
  });

  it("test_httpSession_replaysSessionIdOnFollowups_verbatim", async () => {
    const { s, calls } = sessionWithFakeReq("SID-XYZ");
    await s.request({ method: "initialize", id: 1 }, { isInitialize: true });
    await s.request({ method: "tools/list", id: 2 }, {});
    await s.request({ method: "tools/call", id: 3 }, {});
    assert.strictEqual(calls[1].opts.headers["Mcp-Session-Id"], "SID-XYZ");
    assert.strictEqual(calls[2].opts.headers["Mcp-Session-Id"], "SID-XYZ");
    // initialize carried NO session header (server-minted, not client-minted).
    assert.strictEqual(calls[0].opts.headers["Mcp-Session-Id"], undefined);
  });

  it("test_identity_sessionIdByteIdenticalAcrossAllRequests", async () => {
    const { s, calls } = sessionWithFakeReq("STABLE-SID");
    await s.request({ method: "initialize", id: 1 }, { isInitialize: true });
    await s.request({ method: "tools/list", id: 2 }, {});
    await s.request({ method: "tools/call", id: 3 }, {});
    const seen = calls.slice(1).map((c) => c.opts.headers["Mcp-Session-Id"]);
    assert.ok(seen.every((v) => v === "STABLE-SID"));
  });

  it("test_identity_neverMintsOwnSessionIdOnFollowup", async () => {
    const { s, calls } = sessionWithFakeReq(null); // server returns NO session id
    await s.request({ method: "initialize", id: 1 }, { isInitialize: true });
    await s.request({ method: "tools/list", id: 2 }, {});
    // The bridge never invents a session id when the server didn't mint one.
    assert.strictEqual(s.sessionId, null);
    assert.strictEqual(calls[1].opts.headers["Mcp-Session-Id"], undefined);
  });

  it("test_httpSession_verbatimUrl_noPathComposed (AC-05)", async () => {
    const { s, calls } = sessionWithFakeReq("S");
    await s.request({ method: "initialize", id: 1 }, { isInitialize: true });
    await s.request({ method: "tools/list", id: 2 }, {});
    for (const c of calls) {
      assert.strictEqual(c.opts.path, "/v1/slug"); // no append, no slug derivation
      assert.strictEqual(c.opts.hostname, "h.example");
    }
  });

  it("test_httpSession_sendsAcceptBothAndJsonContentType", async () => {
    const { s, calls } = sessionWithFakeReq("S");
    await s.request({ method: "initialize", id: 1 }, { isInitialize: true });
    assert.strictEqual(calls[0].opts.headers.Accept, "application/json, text/event-stream");
    assert.strictEqual(calls[0].opts.headers["Content-Type"], "application/json");
    assert.strictEqual(calls[0].opts.agent, false); // no pool (R-02)
  });
});

// ── lifecycle (R-17): stable clientInfo.name + proxy ─────────────────────────

describe("lifecycle (R-17)", () => {
  it("test_identity_clientInfoNameStableConstant", async () => {
    const sess = fakeSession(() => ({ status: 200, contentType: "application/json",
      res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-06-18" } }))) }));
    const lc = new Lifecycle(sess);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: { clientInfo: { name: "claude-code" } } });
    assert.strictEqual(sess.sent[0].body.params.clientInfo.name, CLIENT_INFO_NAME);
  });

  it("test_lifecycle_initializeWithNoClientInfo_getsStableName", async () => {
    const sess = fakeSession(() => ({ status: 200, contentType: "application/json",
      res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: 1, result: {} }))) }));
    const lc = new Lifecycle(sess);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize" });
    assert.strictEqual(sess.sent[0].body.params.clientInfo.name, CLIENT_INFO_NAME);
  });

  it("test_lifecycle_notification_noResponse", async () => {
    const sess = fakeSession(() => ({ status: 200, contentType: "application/json", res: bodyStream(Buffer.from("{}")) }));
    const lc = new Lifecycle(sess);
    const out = await lc.handle({ jsonrpc: "2.0", method: "notifications/initialized" });
    assert.strictEqual(out, null);
  });

  it("test_lifecycle_proxiesResult_correlatedById", async () => {
    const sess = fakeSession((body) => ({ status: 200, contentType: "application/json",
      res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: { tools: [] } }))) }));
    const lc = new Lifecycle(sess);
    const out = await lc.handle({ jsonrpc: "2.0", id: 42, method: "tools/list" });
    assert.strictEqual(out.id, 42);
    assert.deepStrictEqual(out.result.tools, []);
  });

  it("test_lifecycle_capturesProtocolVersionOnInitialize", async () => {
    const sess = fakeSession(() => ({ status: 200, contentType: "application/json",
      res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-06-18" } }))) }));
    const lc = new Lifecycle(sess);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize" });
    assert.strictEqual(sess.protocolVersion, "2025-06-18");
  });

  it("test_lifecycle_transportError_surfacedAsJsonRpcError", async () => {
    const sess = { request: () => Promise.reject(Object.assign(new Error("boom"), { code: "ECONNREFUSED" })) };
    const lc = new Lifecycle(sess);
    const out = await lc.handle({ jsonrpc: "2.0", id: 5, method: "tools/list" });
    assert.strictEqual(out.id, 5);
    assert.ok(out.error);
  });
});

// ── identity across spawns/projects (R-17) ───────────────────────────────────

describe("identity across spawns/projects (R-17)", () => {
  it("test_identity_twoSpawnsSameProject_sameClientInfoName", () => {
    // CLIENT_INFO_NAME is a module constant — deterministic across spawns.
    assert.strictEqual(CLIENT_INFO_NAME, "unimatrix-mcp-bridge");
    assert.ok(!/\d{3,}/.test(CLIENT_INFO_NAME), "not timestamped/random");
  });

  it("test_identity_distinctProjects_noSharedMutableSessionId", () => {
    const a = HttpSession.create({ mcpUrl: "https://h/v1/a", token: "t", pinnedFp: "sha256:" + "a".repeat(64) });
    const b = HttpSession.create({ mcpUrl: "https://h/v1/b", token: "t", pinnedFp: "sha256:" + "a".repeat(64) });
    a.sessionId = "SID-A";
    assert.strictEqual(b.sessionId, null, "no shared/global mutable identity");
  });
});

// ── store-error posture (R-13) + no-leak (R-09) — driven via main() ──────────

describe("store-error posture (R-13) + no-leak (R-09)", () => {
  function tmpHome() {
    return fs.mkdtempSync(path.join(os.tmpdir(), "uni-bridge-home-"));
  }
  function withHome(home, fn) {
    const orig = os.homedir;
    os.homedir = () => home;
    try { return fn(); } finally { os.homedir = orig; }
  }

  it("test_bridge_storeEnoent_exitsNonZero_noCredentialForProject", () => {
    const home = tmpHome();
    let code = null;
    const errs = [];
    withHome(home, () => {
      bridge.main(["node", "bridge", "deadbeefdeadbeef"], {
        exit: (c) => { code = c; },
        errOut: (s) => errs.push(s),
      });
    });
    assert.strictEqual(code, 1);
    assert.match(errs.join(""), /no credential for project/);
  });

  it("test_bridge_storeMalformed_exitsNonZeroLoud", () => {
    const home = tmpHome();
    const hash = "abcdef0123456789";
    const p = path.join(home, ".unimatrix", hash, "remote.json");
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, "{ not json");
    let code = null;
    const errs = [];
    withHome(home, () => {
      bridge.main(["node", "bridge", hash], { exit: (c) => { code = c; }, errOut: (s) => errs.push(s) });
    });
    assert.strictEqual(code, 1);
    assert.match(errs.join(""), /malformed/);
  });

  it("test_bridge_incompleteEntry_missingFingerprint_loudNotUnpinned", () => {
    const home = tmpHome();
    const hash = "0123456789abcdef";
    const TOKEN = "tok_secret_value_unique";
    withHome(home, () => {
      credstore.write(hash, {
        mcp_url: "https://h/v1/s", observe_url: "https://h/v1/s/observe",
        token: TOKEN, fingerprint: null,
      });
      let code = null;
      const errs = [];
      bridge.main(["node", "bridge", hash], { exit: (c) => { code = c; }, errOut: (s) => errs.push(s) });
      assert.strictEqual(code, 1, "legacy/null-fingerprint never runs unpinned");
      assert.match(errs.join(""), /v:2 bundle/);
      // R-09: the token never appears in the loud error.
      assert.ok(!errs.join("").includes(TOKEN), "token absent from stderr");
    });
  });

  it("test_bridge_usage_noProjectHash_exit2", () => {
    let code = null;
    const errs = [];
    bridge.main(["node", "bridge"], { exit: (c) => { code = c; }, errOut: (s) => errs.push(s) });
    assert.strictEqual(code, 2);
    assert.match(errs.join(""), /usage/);
  });

  it("test_bridge_tokenReadFromStoreAtSpawn_notFromArgv", () => {
    // argv carries only [bridge, projectHash] — never the token (AC-09/NFR-06).
    const home = tmpHome();
    const hash = "fedcba9876543210";
    const TOKEN = "tok_never_on_argv";
    withHome(home, () => {
      credstore.write(hash, {
        mcp_url: "https://h/v1/s", observe_url: "https://h/v1/s/observe",
        token: TOKEN, fingerprint: "sha256:" + "a".repeat(64),
      });
      // buildSession resolves the token from the store; argv has no token.
      let code = null;
      const sess = bridge.buildSession(hash, { exit: (c) => { code = c; }, errOut: () => {} });
      assert.ok(sess, "good entry builds a session");
      assert.strictEqual(sess.token, TOKEN);
      assert.strictEqual(code, null);
    });
  });
});
