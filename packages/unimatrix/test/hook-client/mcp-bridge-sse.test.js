"use strict";

// mcp-bridge-sse.test.js (C2, vnc-039, R-04) — SSE wire path through the live
// stub. The unit-level sse-parse fuzz lives in mcp-bridge.test.js; this asserts
// the full lifecycle works when the endpoint frames responses as
// text/event-stream (the rmcp default config — OVERVIEW finding 4). BUILT
// because source analysis says SSE is required; the LIVE probe in Stage 3c is
// the definitive gate (if a future config flips to JSON-direct this drops).

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const { startMcpStubServer, SKIP, cleanupCerts } = require("../helpers/mcp-stub-server.js");
const { HttpSession } = require("../../lib/hook-client/mcp-bridge/http-session.js");
const { Lifecycle } = require("../../lib/hook-client/mcp-bridge/lifecycle.js");

const TOKEN = "deadbeef".repeat(8);

describe("mcp-bridge SSE wire path (R-04, probe-gated → SSE required)", { skip: SKIP }, () => {
  let stub;
  before(async () => { stub = await startMcpStubServer({ sse: true }); });
  after(async () => { if (stub) await stub.close(); cleanupCerts(); });

  it("test_bridge_fullLifecycle_overSse_jsonResults", async () => {
    const s = HttpSession.create({ mcpUrl: stub.url + "/v1/slug", token: TOKEN, pinnedFp: stub.pinnedFp });
    const lc = new Lifecycle(s);
    const init = await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
    assert.ok(init.result, "initialize result decoded from an SSE data: event");
    const list = await lc.handle({ jsonrpc: "2.0", id: 2, method: "tools/list" });
    assert.ok(list.result.tools.some((t) => t.name === "context_search"));
    const call = await lc.handle({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "context_get" } });
    assert.strictEqual(call.id, 3, "id correlated through the SSE frame");
  });

  it("test_bridge_acceptHeaderIncludesEventStream (avoids rmcp 406)", async () => {
    const s = HttpSession.create({ mcpUrl: stub.url + "/v1/slug", token: TOKEN, pinnedFp: stub.pinnedFp });
    const lc = new Lifecycle(s);
    await lc.handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
    const init = stub.requests.find((r) => r.parsed && r.parsed.method === "initialize");
    assert.match(init.accept, /application\/json/);
    assert.match(init.accept, /text\/event-stream/);
  });
});
