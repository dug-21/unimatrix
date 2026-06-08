"use strict";

// transport-uds.js unit suite (vnc-027, test-plan/transport-uds.md).
// ADRs: ADR-002 (SendResult mapping), ADR-003 (socket lifecycle), ADR-001 §2
// (accept injection). ACs: AC-01, AC-03, AC-04, AC-05. Risks: R-01, R-06, R-18,
// R-15. Contract: post(config, frame, opts) -> Promise<SendResult>, never rejects,
// no stdout/stderr, no retry. Oracle: transport-http.js. Stub listener via `net`.
// Live-listener assertions (AC-03/04/05) live in parity-corpus-uds (Stage 3c).

const { describe, it, after } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const net = require("net");

const transport = require("../../lib/hook-client/transport-uds");
const { post, encodeFrame, mapHookResponse, MAX_PAYLOAD_SIZE, TIMEOUT_MS } = transport;
const {
  startUdsStubServer,
  startUdsBlackholeServer,
  absentSocketPath,
  frameResponse,
} = require("../helpers/stub-server");

const FAILURE_CLASSES = new Set(["auth", "connect", "timeout", "http_4xx", "http_5xx"]);

const IS_WINDOWS = process.platform === "win32";

function cfg(socketPath) {
  return { socketPath };
}

/** Build a frame whose JSON serialization is exactly `total` bytes. */
function frameOfExactJsonSize(total) {
  const base = Buffer.byteLength(JSON.stringify({ type: "RecordEvent", pad: "" }), "utf8");
  const padLen = total - base;
  return { type: "RecordEvent", pad: "a".repeat(padLen) };
}

/** Build a Text HookResponse whose framed JSON body is exactly `total` bytes. */
function textResponseOfExactBodySize(total) {
  const base = Buffer.byteLength(JSON.stringify({ type: "Text", body: "" }), "utf8");
  return { type: "Text", body: "a".repeat(total - base) };
}

describe("transport-uds", { skip: IS_WINDOWS }, function () { // UDS is Unix-only (vnc-027)
  // ── Framing — AC-01 / R-18 (wire.rs byte authority) ───────────────────────

  describe("framing", function () {
    it("test_frame_write_4byte_be_u32_prefix_plus_json", function () {
      const frame = { type: "RecordEvent", event_type: "x", session_id: "s" };
      const buf = encodeFrame(frame, {});
      const json = Buffer.from(JSON.stringify(frame), "utf8");
      assert.strictEqual(buf.length, 4 + json.length);
      assert.strictEqual(buf.readUInt32BE(0), json.length); // big-endian length prefix
      assert.deepStrictEqual(buf.subarray(4), json); // payload verbatim
    });

    it("test_write_rejects_payload_over_1mib", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const frame = frameOfExactJsonSize(MAX_PAYLOAD_SIZE + 1);
      assert.strictEqual(
        Buffer.byteLength(JSON.stringify(frame), "utf8"),
        MAX_PAYLOAD_SIZE + 1
      );
      assert.strictEqual(encodeFrame(frame, {}), null); // no buffer produced
      const res = await post(cfg(stub.socketPath), frame, { sync: false });
      assert.deepStrictEqual(res, {
        ok: false,
        status: 0,
        contentType: null,
        body: null,
        failureClass: "http_4xx",
      });
      assert.strictEqual(stub.requests.length, 0); // never sent
    });

    it("test_write_accepts_exactly_1mib", async function () {
      // Sync round-trip so server full-frame receipt is confirmed by the reply
      // (FNF would resolve on flush, before the server parses the frame).
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const frame = frameOfExactJsonSize(MAX_PAYLOAD_SIZE);
      const buf = encodeFrame(frame, { sync: true });
      assert.strictEqual(buf.readUInt32BE(0), MAX_PAYLOAD_SIZE); // accept not injected (RecordEvent)
      stub.respondWith({ frame: { type: "Ack" } });
      const res = await post(cfg(stub.socketPath), frame, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(stub.requests.length, 1);
      assert.strictEqual(stub.requests[0].length, MAX_PAYLOAD_SIZE);
    });

    it("test_read_rejects_zero_declared_length", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const header = Buffer.alloc(4); // declares length 0
      stub.respondWith({ raw: header, noEnd: true });
      const res = await post(cfg(stub.socketPath), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
    });

    it("test_read_rejects_over_1mib_declared_length", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const header = Buffer.alloc(4);
      header.writeUInt32BE(0xffffffff, 0); // hostile prefix — reject before allocating
      stub.respondWith({ raw: header, noEnd: true });
      const res = await post(cfg(stub.socketPath), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
    });

    it("test_read_accepts_exactly_1mib_response", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const obj = textResponseOfExactBodySize(MAX_PAYLOAD_SIZE);
      assert.strictEqual(Buffer.byteLength(JSON.stringify(obj), "utf8"), MAX_PAYLOAD_SIZE);
      stub.respondWith({ frame: obj });
      const res = await post(cfg(stub.socketPath), { type: "ContextSearch" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.status, 200);
      assert.strictEqual(res.body.length, obj.body.length);
    });

    it("test_sync_accumulates_chunked_response", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      stub.respondWith({ frame: { type: "Text", body: "hello world" }, chunkSize: 1 });
      const res = await post(cfg(stub.socketPath), { type: "ContextSearch" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.body.toString("utf8"), "hello world");
    });
  });

  // ── accept injection (ADR-001 §2) — serialization-time only ───────────────

  describe("accept injection", function () {
    it("test_encode_injects_accept_for_sync_injection_frame", function () {
      const buf = encodeFrame({ type: "ContextSearch", query: "q" }, { sync: true });
      const obj = JSON.parse(buf.subarray(4).toString("utf8"));
      assert.strictEqual(obj.accept, "text/plain");
    });

    it("test_encode_no_accept_on_fnf", function () {
      const buf = encodeFrame({ type: "ContextSearch", query: "q" }, { sync: false });
      const obj = JSON.parse(buf.subarray(4).toString("utf8"));
      assert.strictEqual(obj.accept, undefined);
    });

    it("test_encode_no_accept_for_non_injection_type", function () {
      const buf = encodeFrame({ type: "RecordEvent", x: 1 }, { sync: true });
      const obj = JSON.parse(buf.subarray(4).toString("utf8"));
      assert.strictEqual(obj.accept, undefined);
    });

    it("test_encode_does_not_mutate_caller_frame", function () {
      const frame = { type: "CompactPayload", body: "b" };
      encodeFrame(frame, { sync: true });
      assert.strictEqual("accept" in frame, false); // queue stays transport-agnostic
    });
  });

  // ── SendResult mapping — every ADR-002 §2 row ─────────────────────────────

  describe("SendResult mapping", function () {
    it("test_map_text_to_200_text_plain_buffer", function () {
      const r = mapHookResponse({ type: "Text", body: "ctx" });
      assert.deepStrictEqual(r, {
        ok: true,
        status: 200,
        contentType: "text/plain",
        body: Buffer.from("ctx", "utf8"),
        failureClass: null,
      });
    });

    it("test_map_ack_to_204_null", function () {
      assert.deepStrictEqual(mapHookResponse({ type: "Ack" }), {
        ok: true,
        status: 204,
        contentType: null,
        body: null,
        failureClass: null,
      });
    });

    it("test_map_fnf_flush_to_status_0", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const res = await post(cfg(stub.socketPath), { type: "RecordEvent" }, { sync: false });
      assert.deepStrictEqual(res, {
        ok: true,
        status: 0,
        contentType: null,
        body: null,
        failureClass: null,
      });
    });

    it("test_map_pong_to_200_application_json", function () {
      const r = mapHookResponse({ type: "Pong", server_version: "1" });
      assert.strictEqual(r.ok, true);
      assert.strictEqual(r.status, 200);
      assert.strictEqual(r.contentType, "application/json");
      assert.deepStrictEqual(JSON.parse(r.body.toString("utf8")), {
        type: "Pong",
        server_version: "1",
      });
    });

    it("test_map_error_5xx_and_4xx", function () {
      assert.strictEqual(mapHookResponse({ type: "Error", code: 500 }).failureClass, "http_5xx");
      assert.strictEqual(mapHookResponse({ type: "Error", code: 503 }).failureClass, "http_5xx");
      assert.strictEqual(mapHookResponse({ type: "Error", code: 400 }).failureClass, "http_4xx");
      assert.strictEqual(mapHookResponse({ type: "Error", code: 404 }).failureClass, "http_4xx");
      const r = mapHookResponse({ type: "Error", code: 404 });
      assert.strictEqual(r.ok, false);
      assert.strictEqual(r.status, 404);
    });

    it("test_map_unexpected_variant_is_connect", function () {
      assert.strictEqual(mapHookResponse({ type: "Whatever" }).failureClass, "connect");
      assert.strictEqual(mapHookResponse(null).failureClass, "connect");
      assert.strictEqual(mapHookResponse(42).failureClass, "connect");
    });

    it("test_map_connect_failure", async function () {
      // ENOENT: socket dir / file absent.
      const r1 = await post(cfg(absentSocketPath()), { type: "RecordEvent" }, { sync: false });
      assert.strictEqual(r1.ok, false);
      assert.strictEqual(r1.failureClass, "connect");
      // Not-a-socket path (ENOTSOCK / ECONNREFUSED) → default branch → connect.
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "uds-file-"));
      const filePath = path.join(dir, "regular.file");
      fs.writeFileSync(filePath, "not a socket");
      after(() => fs.rmSync(dir, { recursive: true, force: true }));
      const r2 = await post(cfg(filePath), { type: "RecordEvent" }, { sync: false });
      assert.strictEqual(r2.ok, false);
      assert.strictEqual(r2.failureClass, "connect");
    });

    it("test_map_deadline_timeout", async function () {
      const bh = await startUdsBlackholeServer();
      after(() => bh.close());
      const res = await post(cfg(bh.socketPath), { type: "ContextSearch" }, { sync: true });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "timeout");
    });

    it("test_no_new_failureclass_values", async function () {
      // Exercise a spread of paths; assert only the F3 set ever appears.
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const results = [];
      results.push(mapHookResponse({ type: "Error", code: 500 }));
      results.push(mapHookResponse({ type: "Whatever" }));
      results.push(await post(cfg(absentSocketPath()), { type: "RecordEvent" }, {}));
      const big = frameOfExactJsonSize(MAX_PAYLOAD_SIZE + 1);
      results.push(await post(cfg(stub.socketPath), big, {}));
      for (const r of results) {
        if (r.failureClass !== null) assert.ok(FAILURE_CLASSES.has(r.failureClass), r.failureClass);
      }
    });
  });

  // ── Socket lifecycle — ADR-003 / R-01 (FNF) & R-06 (sync) ─────────────────

  describe("socket lifecycle", function () {
    it("test_fnf_uses_socket_end_not_destroy_before_finish", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const calls = [];
      const origConnect = net.connect;
      net.connect = function instrumented(...args) {
        const s = origConnect.apply(net, args);
        const origEnd = s.end.bind(s);
        const origDestroy = s.destroy.bind(s);
        s.end = (...a) => {
          calls.push("end");
          return origEnd(...a);
        };
        s.destroy = (...a) => {
          calls.push("destroy");
          return origDestroy(...a);
        };
        s.on("finish", () => calls.push("finish"));
        return s;
      };
      after(() => {
        net.connect = origConnect;
      });
      const res = await post(cfg(stub.socketPath), { type: "RecordEvent" }, { sync: false });
      assert.strictEqual(res.ok, true);
      assert.ok(calls.includes("end"), "socket.end was called");
      assert.ok(calls.includes("finish"), "resolved on 'finish'");
      assert.ok(calls.indexOf("end") < calls.indexOf("finish"), "end before finish");
      const destroyIdx = calls.indexOf("destroy");
      if (destroyIdx !== -1) {
        assert.ok(destroyIdx > calls.indexOf("finish"), "destroy never before finish (R-01 s3)");
      }
    });

    it("test_fnf_never_reads_response", async function () {
      // Server sends a full frame back; FNF must ignore it and resolve status 0.
      const stub = await startUdsStubServer();
      after(() => stub.close());
      stub.respondWith({ frame: { type: "Ack" } });
      const res = await post(cfg(stub.socketPath), { type: "RecordEvent" }, { sync: false });
      assert.strictEqual(res.status, 0); // not 204 — proves no read happened
      assert.strictEqual(res.ok, true);
    });

    it("test_fnf_flush_timeout_resolves_ok_false", async function () {
      // Blackhole never reads → 1 MiB write can't flush → 'finish' never fires →
      // deadline → ok:false (enqueued, never silently dropped).
      const bh = await startUdsBlackholeServer();
      after(() => bh.close());
      const frame = frameOfExactJsonSize(MAX_PAYLOAD_SIZE);
      const res = await post(cfg(bh.socketPath), frame, { sync: false });
      assert.strictEqual(res.ok, false);
      assert.ok(FAILURE_CLASSES.has(res.failureClass));
    });

    it("test_sync_end_before_complete_frame_fails_connect", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const header = Buffer.alloc(4);
      header.writeUInt32BE(100, 0); // declare 100, send only a few body bytes then end
      stub.respondWith({ raw: Buffer.concat([header, Buffer.from("short")]) });
      const res = await post(cfg(stub.socketPath), { type: "ContextSearch" }, { sync: true });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
    });

    it("test_sync_deadline_mid_read_destroys_and_timeout", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const header = Buffer.alloc(4);
      header.writeUInt32BE(100, 0); // declare 100, send partial, then stay open
      stub.respondWith({ raw: Buffer.concat([header, Buffer.from("partial")]), noEnd: true });
      const res = await post(cfg(stub.socketPath), { type: "ContextSearch" }, { sync: true });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "timeout");
    });

    it("test_settle_once_data_wins_over_end", async function () {
      // A full frame followed by socket end: the data-complete settle must win;
      // the subsequent 'end' must not re-settle to connect (settle-once).
      const stub = await startUdsStubServer();
      after(() => stub.close());
      stub.respondWith({ frame: { type: "Text", body: "ok" } });
      const res = await post(cfg(stub.socketPath), { type: "ContextSearch" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.body.toString("utf8"), "ok");
    });

    it("test_all_timers_unref", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const origSetTimeout = global.setTimeout;
      const timers = [];
      global.setTimeout = function spy(fn, ms) {
        const t = origSetTimeout(fn, ms);
        const rec = { unreffed: false };
        const ou = t.unref ? t.unref.bind(t) : null;
        if (ou) {
          t.unref = () => {
            rec.unreffed = true;
            return ou();
          };
        }
        timers.push(rec);
        return t;
      };
      try {
        await post(cfg(stub.socketPath), { type: "RecordEvent" }, { sync: false });
      } finally {
        global.setTimeout = origSetTimeout;
      }
      assert.ok(timers.length >= 1, "deadline timer created");
      for (const rec of timers) {
        assert.strictEqual(rec.unreffed, true, "timer was unref()'d");
      }
    });
  });

  // ── Fail-open, grep-gates & timeouts (FR-9, FR-10, R-06 s5) ───────────────

  describe("fail-open and discipline", function () {
    it("test_never_rejects", async function () {
      const stub = await startUdsStubServer();
      after(() => stub.close());
      const scenarios = [
        post(cfg(absentSocketPath()), { type: "RecordEvent" }, {}),
        post(cfg(absentSocketPath()), { type: "ContextSearch" }, { sync: true }),
        post(cfg(stub.socketPath), { type: "RecordEvent" }, {}),
        post(cfg(stub.socketPath), frameOfExactJsonSize(MAX_PAYLOAD_SIZE + 1), {}),
      ];
      const settled = await Promise.allSettled(scenarios);
      for (const s of settled) {
        assert.strictEqual(s.status, "fulfilled"); // never rejects
        assert.strictEqual(typeof s.value.ok, "boolean");
      }
    });

    it("test_unserializable_frame_is_http_4xx", async function () {
      const circular = { type: "RecordEvent" };
      circular.self = circular;
      const res = await post(cfg(absentSocketPath()), circular, {});
      assert.strictEqual(res.failureClass, "http_4xx");
      assert.strictEqual(encodeFrame(circular, {}), null);
    });

    it("test_no_process_exit_in_module", function () {
      const src = fs.readFileSync(
        path.join(__dirname, "..", "..", "lib", "hook-client", "transport-uds.js"),
        "utf8"
      );
      assert.ok(!/process\.exit\s*\(/.test(src), "no process.exit() in module (#4768)");
    });

    it("test_no_stdout_no_stderr_from_transport", function () {
      const src = fs.readFileSync(
        path.join(__dirname, "..", "..", "lib", "hook-client", "transport-uds.js"),
        "utf8"
      );
      assert.ok(!/process\.stdout/.test(src), "no process.stdout writes");
      assert.ok(!/process\.stderr/.test(src), "no process.stderr writes");
      assert.ok(!/console\./.test(src), "no console.* logging");
    });

    it("test_timeout_constants_are_40ms", function () {
      assert.strictEqual(TIMEOUT_MS, 40); // sourced from Rust HOOK_TIMEOUT
    });

    it("test_max_payload_size_matches_wire", function () {
      assert.strictEqual(MAX_PAYLOAD_SIZE, 1048576); // wire.rs:16
    });
  });
});
