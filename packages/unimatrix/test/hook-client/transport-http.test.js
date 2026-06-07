"use strict";

// transport-http.js unit suite (vnc-026, test-plan/transport-http.md).
// Risks: R-10, R-15, R-16; ACs: AC-02, AC-03, AC-09.
// ADR-005 timeouts ACCEPTED (750/2,000/3,000 ms) — do not flag vs NFR-02.

const { describe, it, after } = require("node:test");
const assert = require("assert");
const {
  post,
  pingForInit,
  DEFAULT_TIMEOUTS,
  BODY_LIMIT_BYTES,
} = require("../../lib/hook-client/transport-http");
const {
  startStubServer,
  startSilentTcpServer,
  refusedPort,
} = require("../helpers/stub-server");

const TOKEN = "unit-test-placeholder-token-3";

function cfg(url, timeouts) {
  return { url, token: TOKEN, timeouts: timeouts || DEFAULT_TIMEOUTS };
}

/** Fast timeouts for tests that exercise structure, not ADR-005 values. */
const FAST = { connectMs: 750, syncMs: 1500, fnfMs: 1500 };

describe("transport-http", function () {
  // ── Request Shape (AC-02 / AC-03) ─────────────────────────────────

  describe("request shape", function () {
    it("test_post_method_and_path", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      const frame = { type: "RecordEvent", event_type: "x", session_id: "s" };
      const res = await post(cfg(stub.url, FAST), frame, { sync: false });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(stub.requests.length, 1);
      assert.strictEqual(stub.requests[0].method, "POST");
      assert.strictEqual(stub.requests[0].path, "/observe");
      // Body = exact HookRequest JSON.
      assert.strictEqual(stub.requests[0].body.toString("utf8"), JSON.stringify(frame));
    });

    it("test_headers_fnf", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      await post(cfg(stub.url, FAST), { type: "SessionRegister" }, { sync: false });
      const h = stub.requests[0].headers;
      assert.strictEqual(h["authorization"], "Bearer " + TOKEN);
      assert.strictEqual(h["content-type"], "application/json");
      assert.strictEqual(h["accept"], "application/json");
      assert.ok(!String(h["accept"]).includes("text/plain"), "NO text/plain on FNF");
    });

    it("test_headers_sync_every_arm", async function () {
      // The #4703 canary: one missed sync arm prints raw JSON. Per-arm assertion.
      const stub = await startStubServer();
      after(() => stub.close());
      const syncArms = [
        { type: "ContextSearch", query: "q", session_id: null, role: null, task: null, feature: null, k: null, max_tokens: null },
        { type: "CompactPayload", session_id: "s", injected_entry_ids: [], role: null, feature: null, token_limit: null },
        { type: "Ping" },
      ];
      for (const frame of syncArms) {
        await post(cfg(stub.url, FAST), frame, { sync: true });
      }
      assert.strictEqual(stub.requests.length, syncArms.length);
      for (let i = 0; i < syncArms.length; i++) {
        assert.strictEqual(
          stub.requests[i].headers["accept"],
          "text/plain",
          "Accept: text/plain missing on sync arm " + syncArms[i].type
        );
        assert.strictEqual(stub.requests[i].headers["authorization"], "Bearer " + TOKEN);
        assert.strictEqual(stub.requests[i].headers["content-type"], "application/json");
      }
    });

    it("test_url_trailing_slash", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      const res = await post(cfg(stub.url + "/", FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(stub.requests[0].path, "/observe", "no //observe");
    });

    it("test_url_path_prefix", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      await post(cfg(stub.url + "/base", FAST), { type: "Ping" }, { sync: true });
      await post(cfg(stub.url + "/base/", FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(stub.requests[0].path, "/base/observe");
      assert.strictEqual(stub.requests[1].path, "/base/observe");
    });

    it("test_url_explicit_port", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      // stub.url already carries an explicit ephemeral port.
      assert.ok(/:\d+$/.test(stub.url));
      const res = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, true);
    });

    it("test_url_ipv6_literal", async function (t) {
      let stub;
      try {
        stub = await startStubServer({ host: "::1" });
      } catch (_err) {
        t.skip("IPv6 loopback unavailable in this environment");
        return;
      }
      after(() => stub.close());
      const res = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(stub.requests[0].path, "/observe");
    });

    it("test_invalid_url_connect_class_no_throw", async function () {
      const res = await post(cfg("not a url"), { type: "Ping" }, { sync: true });
      assert.deepStrictEqual(res, {
        ok: false, status: 0, contentType: null, body: null, failureClass: "connect",
      });
    });

    it("test_non_http_protocol_rejected", async function () {
      const res = await post(cfg("ftp://example.com"), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
    });

    it("test_body_buf_passthrough", async function () {
      // delta.js passes its already size-checked serialization verbatim.
      const stub = await startStubServer();
      after(() => stub.close());
      const bodyBuf = Buffer.from('{"type":"RecordEvent","pre":"serialized"}', "utf8");
      await post(cfg(stub.url, FAST), { ignored: true }, { sync: false, bodyBuf });
      assert.ok(stub.requests[0].body.equals(bodyBuf));
    });

    it("test_oversized_request_body_rejected_client_side", async function () {
      // C-02: >1 MiB post-serialization body → no network write.
      const stub = await startStubServer();
      after(() => stub.close());
      const bodyBuf = Buffer.alloc(BODY_LIMIT_BYTES + 1, 0x61);
      const res = await post(cfg(stub.url, FAST), {}, { sync: false, bodyBuf });
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "http_4xx");
      assert.strictEqual(stub.requests.length, 0, "zero requests = no network write");
    });
  });

  // ── Timeouts (ADR-005 — values accepted, structure tested) ────────

  describe("timeouts", function () {
    it("test_connect_timeout_750ms", async function () {
      // TCP accepts but TLS handshake never completes → connect deadline fires.
      const silent = await startSilentTcpServer();
      after(() => silent.close());
      const start = Date.now();
      const res = await post(
        cfg("https://127.0.0.1:" + silent.port, DEFAULT_TIMEOUTS),
        { type: "Ping" },
        { sync: true }
      );
      const elapsed = Date.now() - start;
      assert.strictEqual(res.failureClass, "connect");
      assert.ok(elapsed < 1500, "no hang: connect fail at ~750 ms, got " + elapsed + " ms");
      assert.ok(elapsed >= 600, "connect deadline armed, got " + elapsed + " ms");
    });

    it("test_sync_total_timeout_2000ms", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 200, contentType: "text/plain", body: "late", delayMs: 3500 });
      const start = Date.now();
      const res = await post(cfg(stub.url, DEFAULT_TIMEOUTS), { type: "Ping" }, { sync: true });
      const elapsed = Date.now() - start;
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "timeout");
      assert.ok(elapsed >= 1800 && elapsed < 3000,
        "aborted ~2,000 ms (sync deadline), got " + elapsed + " ms");
    });

    it("test_fnf_total_timeout_3000ms", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 204, delayMs: 4500 });
      const start = Date.now();
      const res = await post(
        cfg(stub.url, DEFAULT_TIMEOUTS),
        { type: "RecordEvent" },
        { sync: false }
      );
      const elapsed = Date.now() - start;
      assert.strictEqual(res.failureClass, "timeout");
      assert.ok(elapsed >= 2800 && elapsed < 4200,
        "aborted ~3,000 ms (FNF deadline), got " + elapsed + " ms");
    });

    it("test_timeout_overrides_from_config", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 200, delayMs: 1000 });
      const start = Date.now();
      const res = await post(
        cfg(stub.url, { connectMs: 750, syncMs: 150, fnfMs: 3000 }),
        { type: "Ping" },
        { sync: true }
      );
      const elapsed = Date.now() - start;
      assert.strictEqual(res.failureClass, "timeout");
      assert.ok(elapsed < 800, "override honored (~150 ms), got " + elapsed + " ms");
    });
  });

  // ── Response Classification (R-10 breadcrumb input) ───────────────

  describe("classification", function () {
    it("test_classification_matrix", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      const rows = [
        { status: 401, expect: "auth" },
        { status: 403, expect: "auth" },
        { status: 404, expect: "http_4xx" },
        { status: 413, expect: "http_4xx" },
        { status: 500, expect: "http_5xx" },
        { status: 503, expect: "http_5xx" },
      ];
      for (const row of rows) {
        stub.respondWith({ status: row.status, body: "err" });
        const res = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: false });
        assert.strictEqual(res.ok, false, "status " + row.status);
        assert.strictEqual(res.failureClass, row.expect, "status " + row.status);
        assert.strictEqual(res.status, row.status);
        assert.strictEqual(res.body, null, "no body on failure rows");
      }
      // 2xx success rows incl. 204 (AC-02).
      for (const status of [200, 204]) {
        stub.respondWith({ status, body: "" });
        const res = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: false });
        assert.strictEqual(res.ok, true, "status " + status);
        assert.strictEqual(res.failureClass, null);
        assert.strictEqual(res.status, status);
      }
    });

    it("test_econnrefused_connect_class", async function () {
      const port = await refusedPort();
      const res = await post(
        cfg("http://127.0.0.1:" + port, FAST),
        { type: "Ping" },
        { sync: false }
      );
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
    });
  });

  // ── Sync Response Defense (R-15) ──────────────────────────────────

  describe("sync response defense", function () {
    it("test_sync_200_nontext_content_type_dropped", async function () {
      // Transport reports contentType faithfully so the caller (transform)
      // drops non-text/plain bodies; the module itself writes NO stdout.
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 200, contentType: "application/json", body: '{"type":"Entries"}' });
      const res1 = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res1.ok, true);
      assert.strictEqual(res1.contentType, "application/json");

      // Content-Type header absent → contentType null.
      stub.respondWith({ status: 200, body: "raw" });
      const res2 = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res2.ok, true);
      assert.strictEqual(res2.contentType, null);
    });

    it("test_transport_never_writes_stdout_or_stderr", function () {
      // Grep-gate: the module emits NO stdout/stderr itself — classification
      // strings only, callers own observability (pseudocode Error Handling).
      const fs = require("fs");
      const path = require("path");
      const src = fs.readFileSync(
        path.join(__dirname, "..", "..", "lib", "hook-client", "transport-http.js"),
        "utf8"
      );
      for (const forbidden of ["process.stdout", "process.stderr", "console."]) {
        assert.ok(!src.includes(forbidden), "transport-http.js must not use " + forbidden);
      }
    });

    it("test_sync_200_empty_body_no_output", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 200, contentType: "text/plain", body: "" });
      const res = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.body.length, 0, "empty body surfaced as empty Buffer");
    });

    it("test_sync_oversized_200_body", async function () {
      // 2 MiB body: bounded read, resolves without hang, never throws.
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({
        status: 200,
        contentType: "text/plain",
        body: Buffer.alloc(2 * 1024 * 1024, 0x62),
      });
      const start = Date.now();
      const res = await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.ok(res.body.length <= BODY_LIMIT_BYTES + 256 * 1024,
        "body capped near 1 MiB, got " + res.body.length);
      assert.ok(Date.now() - start < 1500, "no hang");
    });
  });

  // ── Security (R-16) ───────────────────────────────────────────────

  describe("security", function () {
    it("test_no_token_in_errors", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      const results = [];
      for (const status of [401, 403, 404, 413, 500, 503]) {
        stub.respondWith({ status, body: "err" });
        results.push(await post(cfg(stub.url, FAST), { type: "Ping" }, { sync: false }));
      }
      const port = await refusedPort();
      results.push(await post(cfg("http://127.0.0.1:" + port, FAST), { type: "Ping" }, { sync: false }));
      results.push(await post(cfg("not a url"), { type: "Ping" }, { sync: false }));
      for (const res of results) {
        assert.ok(!JSON.stringify(res).includes(TOKEN), "token leaked into SendResult");
      }
    });

    it("test_https_used_for_https_urls", async function () {
      // https URL pointed at a plain-HTTP listener: TLS handshake fails and
      // the stub receives ZERO parsed requests — no silent http downgrade.
      const stub = await startStubServer();
      after(() => stub.close());
      const res = await post(
        cfg("https://127.0.0.1:" + stub.port, FAST),
        { type: "Ping" },
        { sync: false }
      );
      assert.strictEqual(res.ok, false);
      assert.strictEqual(res.failureClass, "connect");
      assert.strictEqual(stub.requests.length, 0, "no plaintext request sent");
    });
  });

  // ── pingForInit (FR-19 / R-18 — the ONE loud path) ────────────────

  describe("pingForInit", function () {
    it("test_ping_pong_success", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ type: "Pong", server_version: "0.7.2" }),
      });
      const r = await pingForInit(stub.url, TOKEN, FAST);
      assert.strictEqual(r.ok, true);
      assert.ok(r.message.includes("Pong"));
      assert.ok(r.message.includes("0.7.2"));
      assert.ok(r.message.includes("127.0.0.1"), "message names the host");
      // The Ping itself is a sync arm.
      assert.strictEqual(stub.requests[0].headers["accept"], "text/plain");
      assert.strictEqual(stub.requests[0].body.toString("utf8"), '{"type":"Ping"}');
    });

    it("test_ping_200_non_pong_rejected", async function () {
      // Strict Pong: 200 JSON non-Pong → init failure.
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ type: "Entries", items: [] }),
      });
      const r = await pingForInit(stub.url, TOKEN, FAST);
      assert.strictEqual(r.ok, false);
      assert.ok(r.message.includes("unexpected response type"));
      assert.ok(r.message.includes("Entries"));
    });

    it("test_ping_non_json_rejected", async function () {
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 200, contentType: "text/plain", body: "not json" });
      const r = await pingForInit(stub.url, TOKEN, FAST);
      assert.strictEqual(r.ok, false);
      assert.ok(r.message.includes("non-JSON"));
    });

    it("test_ping_wrong_token_auth_message", async function () {
      // Proves Bearer is exercised (R-18); message actionable, token-free.
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith({ status: 401, body: "unauthorized" });
      const r = await pingForInit(stub.url, "wrong-token", FAST);
      assert.strictEqual(r.ok, false);
      assert.ok(r.message.includes("--token"));
      assert.ok(r.message.includes("401"));
      assert.ok(!r.message.includes("wrong-token"), "token never in messages");
    });

    it("test_ping_unreachable_connect_message", async function () {
      const port = await refusedPort();
      const r = await pingForInit("http://127.0.0.1:" + port, TOKEN, FAST);
      assert.strictEqual(r.ok, false);
      assert.ok(r.message.includes("cannot reach"));
      assert.ok(r.message.includes("127.0.0.1:" + port), "host named in message");
    });
  });
});
