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

const http = require("http");
const { startSilentTcpServer } = require("../helpers/stub-server.js");
const { StdioFramer } = require("../../lib/hook-client/mcp-bridge/stdio-frame.js");
const { SseParser } = require("../../lib/hook-client/mcp-bridge/sse-parse.js");
const { dispatchResponse, correlateById, isSessionNotFound, SESSION_NOT_FOUND, readBounded } = require("../../lib/hook-client/mcp-bridge/dispatch.js");
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

// ── self-heal on idle eviction (#830, design-review B3/C1-C3) ────────────────

describe("session self-heal on 404 (#830)", () => {
  // dispatch.js: the 404 "session not found" body class is distinguishable.
  it("test_dispatch_404SessionNotFound_classifiedDistinctly", async () => {
    const body = Buffer.from(JSON.stringify({ error: "Session not found" }));
    assert.strictEqual(isSessionNotFound(404, body), true);
    assert.strictEqual(isSessionNotFound(404, Buffer.from("other 404")), false);
    assert.strictEqual(isSessionNotFound(401, body), false); // auth, not session
    assert.strictEqual(isSessionNotFound(500, body), false); // server fault
    const out = await dispatchResponse({ status: 404, contentType: "application/json",
      res: bodyStream(Buffer.from(JSON.stringify({ id: 7, error: "Session not found" }))) });
    assert.strictEqual(out[0].error.code, SESSION_NOT_FOUND);
    assert.strictEqual(out[0].id, 7);
  });

  // Build a fake session whose responder is wired to the eviction scenario.
  // The responder returns a {status, contentType, res} triplet; the session
  // tracks sessionId the way HttpSession would (init mints a new id).
  function evictingSession(plan) {
    const sess = {
      sent: [],
      sessionId: null,
      protocolVersion: null,
      initCount: 0,
      idCounter: 0,
      request(body, opts) {
        sess.sent.push({ body, opts, sessionAtSend: sess.sessionId });
        if (opts && opts.isInitialize) {
          sess.initCount += 1;
          sess.sessionId = "SID-" + ++sess.idCounter; // server mints fresh id
          return Promise.resolve(jsonRes({ jsonrpc: "2.0", id: body.id, result: { protocolVersion: "2025-06-18" } }));
        }
        return Promise.resolve(plan(body, sess));
      },
    };
    return sess;
  }
  function jsonRes(obj) {
    return { status: 200, contentType: "application/json", res: bodyStream(Buffer.from(JSON.stringify(obj))) };
  }
  function sessionNotFoundRes(id) {
    return { status: 404, contentType: "application/json",
      res: bodyStream(Buffer.from(JSON.stringify({ id, error: "Session not found" }))) };
  }
  async function initialized(lc) {
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize", params: { clientInfo: { name: "claude-code" } } });
  }

  it("test_selfHeal_404OnToolsCall_reinitsOnceAndRetrySucceeds", async () => {
    let firstCall = true;
    const sess = evictingSession((body) => {
      if (firstCall) { firstCall = false; return sessionNotFoundRes(body.id); } // evicted
      return jsonRes({ jsonrpc: "2.0", id: body.id, result: { ok: true } }); // retry on new id
    });
    const errs = [];
    const lc = new Lifecycle(sess, { errOut: (s) => errs.push(s) });
    await initialized(lc);
    assert.strictEqual(sess.initCount, 1);

    const out = await lc.handle({ jsonrpc: "2.0", id: 9, method: "tools/call", params: {} });
    assert.strictEqual(out.id, 9);
    assert.deepStrictEqual(out.result, { ok: true }, "retry result surfaced");
    assert.strictEqual(sess.initCount, 2, "exactly one re-initialize");
    // vnc-039: the re-init preserved the STABLE clientInfo.name (B3).
    const reinit = sess.sent.filter((s) => s.opts && s.opts.isInitialize)[1];
    assert.strictEqual(reinit.body.params.clientInfo.name, CLIENT_INFO_NAME);
    // The retry went out under the NEW session id, never the dead one.
    const retry = sess.sent[sess.sent.length - 1];
    assert.strictEqual(retry.sessionAtSend, "SID-2");
    assert.match(errs.join(""), /session evicted \(404\); re-init/);
  });

  it("test_selfHeal_secondConsecutive404_doesNotLoop", async () => {
    // Server always 404s tools/call (eviction persists) -> heal once, the retry
    // 404s again, and we surface an error WITHOUT a second re-init or recursion.
    const sess = evictingSession((body) => sessionNotFoundRes(body.id));
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await initialized(lc);

    const out = await lc.handle({ jsonrpc: "2.0", id: 11, method: "tools/call", params: {} });
    assert.strictEqual(out.id, 11);
    assert.ok(out.error, "error surfaced, not retried forever");
    assert.notStrictEqual(out.error.code, SESSION_NOT_FOUND, "sentinel never reaches client");
    assert.strictEqual(sess.initCount, 2, "exactly one re-init (init + one heal), no loop");
  });

  it("test_selfHeal_reinitFailure_aborts_noLoop", async () => {
    // tools/call 404s; the re-init transport-fails -> abort, surface original.
    let reinitTried = false;
    let initCount = 0;
    const sess = {
      sent: [], sessionId: "OLD", protocolVersion: null,
      request(body, opts) {
        sess.sent.push({ body, opts });
        if (opts && opts.isInitialize) {
          initCount += 1;
          if (initCount > 1) { // first init = handshake; second = self-heal re-init
            reinitTried = true;
            return Promise.reject(Object.assign(new Error("down"), { code: "ECONNREFUSED" }));
          }
          sess.sessionId = "SID-NEW";
          return Promise.resolve(jsonRes({ jsonrpc: "2.0", id: body.id, result: {} }));
        }
        return Promise.resolve(sessionNotFoundRes(body.id));
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize" });
    const out = await lc.handle({ jsonrpc: "2.0", id: 13, method: "tools/call", params: {} });
    assert.ok(reinitTried, "re-init was attempted");
    assert.ok(out.error, "original error surfaced after failed re-init");
    assert.strictEqual(out.id, 13);
  });

  it("test_selfHeal_concurrent404s_singleReinit", async () => {
    // Two tools/call in flight both 404; only ONE re-init must occur (C2).
    const evictedIds = new Set();
    const sess = evictingSession((body) => {
      if (!evictedIds.has(body.id)) { evictedIds.add(body.id); return sessionNotFoundRes(body.id); }
      return jsonRes({ jsonrpc: "2.0", id: body.id, result: { id: body.id } });
    });
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await initialized(lc);

    const [a, b] = await Promise.all([
      lc.handle({ jsonrpc: "2.0", id: 21, method: "tools/call", params: {} }),
      lc.handle({ jsonrpc: "2.0", id: 22, method: "tools/call", params: {} }),
    ]);
    assert.deepStrictEqual(a.result, { id: 21 });
    assert.deepStrictEqual(b.result, { id: 22 });
    assert.strictEqual(sess.initCount, 2, "one initial + exactly one shared re-init (single-flight)");
  });

  it("test_selfHeal_404OnInitialize_isFatal_noReinit", async () => {
    // An initialize that itself 404s must NOT trigger self-heal recursion.
    const sess = {
      sent: [], sessionId: null, protocolVersion: null, initCount: 0,
      request(body, opts) {
        sess.sent.push({ body, opts });
        if (opts && opts.isInitialize) sess.initCount += 1;
        return Promise.resolve(sessionNotFoundRes(body.id));
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    const out = await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize" });
    assert.strictEqual(sess.initCount, 1, "no re-init triggered for a 404 on initialize");
    assert.ok(out.error);
    assert.notStrictEqual(out.error.code, SESSION_NOT_FOUND);
  });
});

// ── transport timeout + silent-eviction self-heal (#839) ─────────────────────

describe("transport timeout self-heal (#839)", () => {
  // (a) Connect/TLS-handshake stall fails fast within the configured deadline,
  // not forever. TLS-stall trick (#4768): an https-shaped request to a silent
  // plain-TCP listener connects but secureConnect never fires, so the connect
  // timer is the only thing that can settle the Promise.
  it("test_timeout_connectStall_failsFastWithinDeadline", async () => {
    const silent = await startSilentTcpServer();
    try {
      const s = HttpSession.create({
        mcpUrl: "https://127.0.0.1:" + silent.port + "/v1/slug",
        token: "TKN", pinnedFp: "sha256:" + "a".repeat(64),
        connectMs: 80, idleMs: 5000,
      });
      // Real socket to the silent listener; secureConnect never fires.
      s.requester = (opts) => http.request({ host: "127.0.0.1", port: silent.port, method: opts.method });
      const t0 = Date.now();
      await assert.rejects(
        s.request({ method: "tools/call", id: 1 }, {}),
        (e) => e && e.code === "ETIMEDOUT"
      );
      assert.ok(Date.now() - t0 < 4000, "settled near the 80ms deadline, not at test-runner timeout");
    } finally {
      await silent.close();
    }
  });

  // _send maps an ETIMEDOUT throw to the distinct TRANSPORT_TIMEOUT sentinel so
  // handle() can heal it (F2) — and normalizes it on the surface (never leaks).
  it("test_timeout_mapsToTransportTimeoutSentinel_normalizedOnExhaustion", async () => {
    // No initialize captured -> heal cannot fire -> sentinel must be normalized.
    const sess = { sent: [], sessionId: null, protocolVersion: null,
      request: () => Promise.reject(Object.assign(new Error("x"), { code: "ETIMEDOUT" })) };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    const out = await lc.handle({ jsonrpc: "2.0", id: 7, method: "tools/call", params: {} });
    assert.strictEqual(out.id, 7);
    assert.ok(out.error);
    assert.notStrictEqual(out.error.code, -32098, "internal TRANSPORT_TIMEOUT sentinel never reaches client");
    assert.match(out.error.message, /timed out/);
  });

  // (b) A transport timeout on the first call triggers EXACTLY ONE transparent
  // re-init through the existing single-flight; the retry under the new session
  // succeeds. The re-init POST shares request(), so it cannot re-hang.
  it("test_timeout_singleTransparentReinitThenSuccess", async () => {
    let initCount = 0, firstCall = true;
    const sess = {
      sent: [], sessionId: null, protocolVersion: null,
      request(body, opts) {
        sess.sent.push({ opts });
        if (opts && opts.isInitialize) {
          initCount += 1; sess.sessionId = "SID-" + initCount;
          return Promise.resolve({ status: 200, contentType: "application/json",
            res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: { protocolVersion: "2025-06-18" } }))) });
        }
        if (firstCall) { firstCall = false; return Promise.reject(Object.assign(new Error("stall"), { code: "ETIMEDOUT" })); }
        return Promise.resolve({ status: 200, contentType: "application/json",
          res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: { ok: true } }))) });
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize" });
    assert.strictEqual(initCount, 1);
    const out = await lc.handle({ jsonrpc: "2.0", id: 9, method: "tools/call", params: {} });
    assert.deepStrictEqual(out.result, { ok: true }, "retry succeeded after one re-init");
    assert.strictEqual(initCount, 2, "exactly one transparent re-init on a transport timeout");
  });

  // (c) When the re-init ALSO times out, surface a bounded error with no loop:
  // request() bounds the re-init POST (same timeout), newId stays null -> false.
  it("test_timeout_reinitAlsoStalls_boundedError_noLoop", async () => {
    let initCount = 0;
    const sess = {
      sent: [], sessionId: "OLD", protocolVersion: null,
      request(body, opts) {
        sess.sent.push({ opts });
        if (opts && opts.isInitialize) {
          initCount += 1;
          if (initCount === 1) { // first handshake ok
            sess.sessionId = "SID-1";
            return Promise.resolve({ status: 200, contentType: "application/json",
              res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: {} }))) });
          }
          return Promise.reject(Object.assign(new Error("stall"), { code: "ETIMEDOUT" })); // re-init stalls
        }
        return Promise.reject(Object.assign(new Error("stall"), { code: "ETIMEDOUT" })); // call stalls
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize" });
    const out = await lc.handle({ jsonrpc: "2.0", id: 13, method: "tools/call", params: {} });
    assert.strictEqual(out.id, 13);
    assert.ok(out.error, "bounded error surfaced");
    assert.notStrictEqual(out.error.code, -32098, "sentinel normalized, never leaked");
    assert.strictEqual(initCount, 2, "exactly one re-init attempt; no loop/storm");
  });

  // The widened heal trigger is NARROW: it must NOT fire on auth/5xx errors.
  it("test_timeout_doesNotHealOnGenericTransportError", async () => {
    let initCount = 0, calls = 0;
    const sess = {
      sent: [], sessionId: null, protocolVersion: null,
      request(body, opts) {
        if (opts && opts.isInitialize) {
          initCount += 1; sess.sessionId = "SID-1";
          return Promise.resolve({ status: 200, contentType: "application/json",
            res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: {} }))) });
        }
        calls += 1;
        return Promise.reject(Object.assign(new Error("refused"), { code: "ECONNREFUSED" }));
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize" });
    const out = await lc.handle({ jsonrpc: "2.0", id: 5, method: "tools/call", params: {} });
    assert.ok(out.error);
    assert.strictEqual(initCount, 1, "no re-init on a non-timeout transport fault (no storm)");
    assert.strictEqual(calls, 1, "the call was not retried");
  });

  // ── mid-stream stall (F6 + N1): connect ok, stream starts, then stalls ──────
  // An SSE response that yields a priming chunk then HANGS mid-frame (no end),
  // exactly like a server that accepts + starts streaming then goes silent. The
  // for-await rethrows whatever res.destroy(err) is called with (mirrors Node), so
  // the idle-read deadline in SseParser.collect routes ETIMEDOUT into _send.
  function stallingSse(primer) {
    const r = new EventEmitter();
    let onKill;
    const killed = new Promise((res) => { onKill = res; });
    r.destroy = (err) => onKill(err || new Error("destroyed"));
    r.resume = () => {};
    r[Symbol.asyncIterator] = async function* () {
      yield Buffer.from(primer); // priming event arrives, then the stream stalls
      throw await killed; // resolves only when collect's idle timer destroys us
    };
    r.on = EventEmitter.prototype.on.bind(r);
    return r;
  }
  function sseStall() {
    return { status: 200, contentType: "text/event-stream", res: stallingSse(": keep-alive\n\n") };
  }

  // (d) Mid-stream stall is BOUNDED by the idle-read deadline, not forever.
  it("test_timeout_midStreamStall_boundedByIdleReadDeadline", async () => {
    // No initialize captured -> heal cannot fire -> sentinel normalized; the call
    // must still SETTLE (within the 60ms idle budget), not hang to runner-timeout.
    const sess = { sent: [], sessionId: null, protocolVersion: null, idleMs: 60,
      request: () => Promise.resolve(sseStall()) };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    const t0 = Date.now();
    const out = await lc.handle({ jsonrpc: "2.0", id: 7, method: "tools/call", params: {} });
    assert.ok(Date.now() - t0 < 4000, "settled near the 60ms idle deadline, not forever");
    assert.strictEqual(out.id, 7);
    assert.ok(out.error);
    assert.notStrictEqual(out.error.code, -32098, "TRANSPORT_TIMEOUT sentinel normalized, never leaked");
    assert.match(out.error.message, /timed out/);
  });

  // (e) Mid-stream timeout -> EXACTLY ONE transparent re-init then success
  // (parity with the connect-stall case; the re-init reply is a clean JSON 200).
  it("test_timeout_midStreamStall_singleReinitThenSuccess", async () => {
    let initCount = 0, firstCall = true;
    const sess = {
      sent: [], sessionId: null, protocolVersion: null, idleMs: 60,
      request(body, opts) {
        if (opts && opts.isInitialize) {
          initCount += 1; sess.sessionId = "SID-" + initCount;
          return Promise.resolve({ status: 200, contentType: "application/json",
            res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: { protocolVersion: "2025-06-18" } }))) });
        }
        if (firstCall) { firstCall = false; return Promise.resolve(sseStall()); } // mid-stream stall
        return Promise.resolve({ status: 200, contentType: "application/json",
          res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: { ok: true } }))) });
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize" });
    assert.strictEqual(initCount, 1);
    const out = await lc.handle({ jsonrpc: "2.0", id: 9, method: "tools/call", params: {} });
    assert.deepStrictEqual(out.result, { ok: true }, "retry succeeded after one re-init");
    assert.strictEqual(initCount, 2, "exactly one transparent re-init on a mid-stream timeout");
  });

  // (f) Mid-stream re-init that ALSO stalls -> bounded error, no loop/storm.
  it("test_timeout_midStreamReinitAlsoStalls_boundedError_noLoop", async () => {
    let initCount = 0;
    const sess = {
      sent: [], sessionId: "OLD", protocolVersion: null, idleMs: 60,
      request(body, opts) {
        if (opts && opts.isInitialize) {
          initCount += 1;
          if (initCount === 1) { // first handshake ok (clean JSON)
            sess.sessionId = "SID-1";
            return Promise.resolve({ status: 200, contentType: "application/json",
              res: bodyStream(Buffer.from(JSON.stringify({ jsonrpc: "2.0", id: body.id, result: {} }))) });
          }
          return Promise.resolve(sseStall()); // re-init's read stalls mid-stream
        }
        return Promise.resolve(sseStall()); // call's read stalls mid-stream
      },
    };
    const lc = new Lifecycle(sess, { errOut: () => {} });
    await lc.handle({ jsonrpc: "2.0", id: 0, method: "initialize" });
    const out = await lc.handle({ jsonrpc: "2.0", id: 13, method: "tools/call", params: {} });
    assert.strictEqual(out.id, 13);
    assert.ok(out.error, "bounded error surfaced");
    assert.notStrictEqual(out.error.code, -32098, "sentinel normalized, never leaked");
    assert.strictEqual(initCount, 2, "exactly one re-init attempt; no loop/storm");
  });

  // ── F6 unref regression (#847): the idle-read deadline must be REF'd ─────────
  // Exercise SseParser.collect AND readBounded DIRECTLY against a handle-free
  // stall (stallingSse: a bare EventEmitter, no socket/timer/libuv handle) with
  // NO other ref'd handle armed in the test — the F6 deadline is the sole liveness
  // source. We must NOT add a competing keepalive timer here: a ref'd race timer
  // would itself hold the loop open and mask an unref'd deadline. We bound the
  // wall-clock with Date.now() instead. If either setTimeout is .unref()'d, the
  // loop drains before the deadline on Node 18/20/22 and the awaited promise never
  // settles -> the runner reports it unresolved (deterministic failure). REF'd, it
  // fires and rejects within the tiny idle budget.
  it("test_f6_sseCollect_idleDeadline_refd_settlesWithNoOtherHandle", async () => {
    const t0 = Date.now();
    await assert.rejects(
      SseParser.collect(stallingSse(": keep-alive\n\n"), 1 << 20, 30),
      /timed out/,
      "collect's ref'd idle deadline must reject, not hang the idle loop"
    );
    assert.ok(Date.now() - t0 < 4000, "settled at the ~30ms idle deadline, not forever");
  });

  it("test_f6_readBounded_idleDeadline_refd_settlesWithNoOtherHandle", async () => {
    // stallingSse never emits data/end/error/close, so readBounded's only settle
    // path is its arm() deadline -> res.destroy() + reject(ETIMEDOUT).
    const t0 = Date.now();
    await assert.rejects(
      readBounded(stallingSse(""), 1 << 20, 30),
      (e) => e && e.code === "ETIMEDOUT",
      "readBounded's ref'd idle deadline must reject, not hang the idle loop"
    );
    assert.ok(Date.now() - t0 < 4000, "settled at the ~30ms idle deadline, not forever");
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
