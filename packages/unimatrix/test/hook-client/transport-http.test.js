"use strict";

// transport-http.js unit suite (vnc-026, test-plan/transport-http.md).
// Risks: R-10, R-15, R-16; ACs: AC-02, AC-03, AC-09.
// ADR-005 timeouts ACCEPTED (750/2,000/3,000 ms) — do not flag vs NFR-02.
//
// vnc-038 (ADR-001, AC-08, R-01/R-12): config.url is now the server-composed
// OBSERVE URL, posted VERBATIM. The `/observe` append (C-3, the last client
// route-composition site) is DELETED. The request path is the URL's pathname
// (+ search) byte-for-byte — no suffix, no trailing-slash mutation.

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
      // ADR-001: config.url posted VERBATIM. stub.url has pathname "/" — no
      // "/observe" append. (URL with no path → "/" per the URL spec.)
      assert.strictEqual(stub.requests[0].path, "/");
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

    it("test_url_verbatim_trailing_slash", async function () {
      // ADR-001 / R-01: a trailing-slash URL is posted VERBATIM — the slash is
      // NOT stripped and NOTHING is appended (proves the trailing-slash-mutation
      // logic is gone alongside the /observe append).
      const stub = await startStubServer();
      after(() => stub.close());
      const res = await post(cfg(stub.url + "/", FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(res.ok, true);
      assert.strictEqual(stub.requests[0].path, "/", "verbatim trailing slash, no /observe");
    });

    it("test_url_verbatim_path_prefix", async function () {
      // The full server-composed observe path (e.g. /v1/<slug>/observe) is posted
      // byte-for-byte. No suffix, no normalization.
      const stub = await startStubServer();
      after(() => stub.close());
      await post(cfg(stub.url + "/v1/proj-a/observe", FAST), { type: "Ping" }, { sync: true });
      await post(cfg(stub.url + "/v1/proj-b/observe", FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(stub.requests[0].path, "/v1/proj-a/observe");
      assert.strictEqual(stub.requests[1].path, "/v1/proj-b/observe");
    });

    it("test_url_already_ending_in_observe_not_double_suffixed", async function () {
      // Edge case (#5095 double-append guard): an observe_url that already ends
      // in "/observe" is posted unchanged — never "/observe/observe". Proves the
      // append is truly gone, closing the cross-wave double-append hazard.
      const stub = await startStubServer();
      after(() => stub.close());
      await post(cfg(stub.url + "/v1/proj-a/observe", FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(stub.requests[0].path, "/v1/proj-a/observe");
      assert.ok(
        !stub.requests[0].path.includes("/observe/observe"),
        "no double-append"
      );
    });

    it("test_url_verbatim_with_query_string", async function () {
      // u.search is preserved verbatim (carried through the dumb-client post).
      const stub = await startStubServer();
      after(() => stub.close());
      await post(cfg(stub.url + "/v1/proj-a/observe?x=1", FAST), { type: "Ping" }, { sync: true });
      assert.strictEqual(stub.requests[0].path, "/v1/proj-a/observe?x=1");
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
      assert.strictEqual(stub.requests[0].path, "/");
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

  // ── Dumb-client invariant: verbatim observe URL (vnc-038 ADR-001) ─
  // R-01 (closed-set deletion + byte-for-byte post), R-12 (init Ping and
  // runtime hook use the SAME observe_url; neither re-derives). AC-08.

  describe("verbatim observe URL (ADR-001 / R-01 / R-12)", function () {
    it("test_no_observe_append_compose_site", function () {
      // R-01 sc.1 (load-bearing for SR-01 / NFR-01): the client-side route-
      // composition set in transport-http.js is EMPTY. Source-level invariant:
      // no "/observe" append, no pathname-suffixing route grammar.
      const fs = require("fs");
      const path = require("path");
      const src = fs.readFileSync(
        path.join(__dirname, "..", "..", "lib", "hook-client", "transport-http.js"),
        "utf8"
      );
      // Strip comments so prose mentioning the deleted append (e.g. /v1/{slug}/observe)
      // never trips the invariant — only executable code is asserted.
      const code = src
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .split("\n")
        .map((line) => line.replace(/\/\/.*$/, ""))
        .join("\n");
      assert.ok(!/\+\s*["']\/observe["']/.test(code), 'no  + "/observe"  append');
      assert.ok(!/["']\/observe["']\s*\+/.test(code), 'no  "/observe" +  prefix');
      assert.ok(!/\.replace\([^)]*\)\s*\+\s*["']/.test(code),
        "no pathname.replace(...) + suffix route grammar");
      assert.ok(!code.includes('"/observe"') && !code.includes("'/observe'"),
        "no /observe literal in executable code");
    });

    it("test_posts_observe_url_byte_for_byte", async function () {
      // R-01 sc.2: capture the outgoing request and reconstruct the URL it was
      // posted to; assert string equality with config.url (the bundle's
      // observe_url) — no normalization, no trailing-slash mutation, no suffix.
      const stub = await startStubServer();
      after(() => stub.close());
      const observeUrl = stub.url + "/v1/proj-a/observe";
      await post(cfg(observeUrl, FAST), { type: "RecordEvent" }, { sync: false });
      const reconstructed = stub.url + stub.requests[0].path;
      assert.strictEqual(reconstructed, observeUrl, "posted target == observe_url verbatim");
    });

    it("test_multiple_hook_events_no_recomposition", async function () {
      // Edge case: every hook event in a session posts to the IDENTICAL
      // observe_url — no per-event re-composition.
      const stub = await startStubServer();
      after(() => stub.close());
      const observeUrl = stub.url + "/v1/proj-a/observe";
      for (let i = 0; i < 3; i++) {
        await post(cfg(observeUrl, FAST), { type: "RecordEvent", n: i }, { sync: false });
      }
      assert.strictEqual(stub.requests.length, 3);
      for (const r of stub.requests) {
        assert.strictEqual(r.path, "/v1/proj-a/observe");
      }
    });

    it("test_init_ping_and_runtime_hook_same_observe_url", async function () {
      // R-12: the init Ping AND a runtime hook event hit the SAME observe_url;
      // neither entry point re-derives the route (closes the R-12 asymmetry).
      const stub = await startStubServer();
      after(() => stub.close());
      stub.respondWith((entry) =>
        entry.headers["accept"] === "text/plain"
          ? { status: 200, contentType: "application/json", body: JSON.stringify({ type: "Pong", server_version: "0.7.2" }) }
          : { status: 204 }
      );
      const observeUrl = stub.url + "/v1/proj-a/observe";
      // init Ping (sync) via pingForInit — first arg is the observe URL.
      const ping = await pingForInit(observeUrl, TOKEN, FAST);
      assert.strictEqual(ping.ok, true);
      // runtime hook (FNF) via post to the SAME url.
      await post(cfg(observeUrl, FAST), { type: "RecordEvent" }, { sync: false });
      assert.strictEqual(stub.requests.length, 2);
      assert.strictEqual(stub.requests[0].path, "/v1/proj-a/observe", "Ping verbatim");
      assert.strictEqual(stub.requests[1].path, "/v1/proj-a/observe", "hook verbatim");
      assert.strictEqual(stub.requests[0].path, stub.requests[1].path,
        "init Ping and runtime hook target the SAME observe path");
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
