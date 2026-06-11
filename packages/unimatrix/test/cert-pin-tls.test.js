"use strict";

// cert-pin-tls.test.js — vnc-034 F1 regression (#725). The ONE test class the
// original suite lacked: a REAL TLS handshake against an actual
// https.createServer with a self-signed cert. The pre-fix suite only asserted
// the options-object shape and called the pin with a synthetic cert.raw, so it
// never exercised Node's chain-verification-before-checkServerIdentity ordering
// — and missed that the pin was dead code that rejected the legitimate leaf.
//
// Asserts:
//  (a) a client pinned to the server's REAL fingerprint connects and gets a 200
//      (the pin no longer breaks the happy path);
//  (b) a client pinned to a WRONG fingerprint is rejected with the diagnosable
//      expected-vs-presented error AND the server NEVER receives the
//      Authorization/token (no token leak on mismatch);
//  (c) pingForInit surfaces a pinned-TLS mismatch diagnosably (FR-A11 /
//      AC-CT-ROT), classified as a connect failure.
//
// Zero added deps: Node built-in https/tls/crypto only. The self-signed cert is
// GENERATED at test setup into a per-run temp dir via the openssl CLI (present
// in CI and the devcontainer) — NOT committed, so the secret scanner never sees
// a private key. The pinned fingerprint is COMPUTED from the generated leaf's
// real DER via the production computeFingerprint (self-consistent, never
// hand-written — this is not a parity golden). If openssl is unavailable the
// suite skips with a clear reason rather than failing.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const https = require("https");
const crypto = require("crypto");
const { spawnSync } = require("child_process");

const {
  computeFingerprint,
} = require("../lib/hook-client/cert-pin.js");
const transport = require("../lib/hook-client/transport-http.js");

// Generate a throwaway self-signed CN=localhost cert+key into a per-run temp
// dir. Returns { tmpDir, certPem, keyPem } on success, or null if openssl is
// unavailable / the generation failed (→ the suite skips, never fails).
function generateSelfSignedCert() {
  let tmpDir;
  try {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-vnc034-tls-"));
  } catch (_err) {
    return null;
  }
  const certPath = path.join(tmpDir, "cert.pem");
  const keyPath = path.join(tmpDir, "key.pem");
  let r;
  try {
    r = spawnSync(
      "openssl",
      [
        "req", "-x509", "-newkey", "rsa:2048",
        "-keyout", keyPath, "-out", certPath,
        "-days", "1", "-nodes", "-subj", "/CN=localhost",
      ],
      { stdio: "ignore" }
    );
  } catch (_err) {
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch (_e) {}
    return null;
  }
  if (r.error || r.status !== 0 || !fs.existsSync(certPath) || !fs.existsSync(keyPath)) {
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch (_e) {}
    return null;
  }
  return {
    tmpDir,
    certPem: fs.readFileSync(certPath),
    keyPem: fs.readFileSync(keyPath),
  };
}

const GEN = generateSelfSignedCert();
const SKIP = GEN
  ? false
  : { skip: "openssl unavailable — cannot generate the self-signed TLS fixture at runtime" };

const CERT_PEM = GEN ? GEN.certPem : null;
const KEY_PEM = GEN ? GEN.keyPem : null;

// The REAL pinned fingerprint = sha256 over the generated leaf DER, computed
// with the production helper (the exact bytes rustls/Node serve — ADR-002).
const REAL_FP = GEN ? computeFingerprint(new crypto.X509Certificate(CERT_PEM).raw) : null;
// A syntactically valid but WRONG pin (64 hex chars), so the mismatch is the
// fingerprint compare and not a malformed-fp early-out.
const WRONG_FP = "sha256:" + "0".repeat(64);

const SHORT_TIMEOUTS = Object.freeze({ connectMs: 2000, syncMs: 4000, fnfMs: 4000 });

describe("cert pin — REAL TLS handshake (F1 regression, R-02 / AC-W1-C2)", { skip: SKIP }, () => {
  let server;
  let baseUrl;
  // Records every Authorization header value the server actually received. If a
  // mismatched client ever leaks the token, it shows up here.
  let observedAuth;

  before(async () => {
    observedAuth = [];
    server = https.createServer({ cert: CERT_PEM, key: KEY_PEM }, (req, res) => {
      if (req.headers["authorization"]) observedAuth.push(req.headers["authorization"]);
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        // Echo a Pong for the sync/Ping path; 204 otherwise.
        if ((req.headers["accept"] || "").includes("text/plain")) {
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ type: "Pong", server_version: "test" }));
        } else {
          res.writeHead(204);
          res.end();
        }
      });
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const { port } = server.address();
    baseUrl = "https://127.0.0.1:" + port;
  });

  after(async () => {
    await new Promise((resolve) => server.close(resolve));
    if (GEN) {
      try { fs.rmSync(GEN.tmpDir, { recursive: true, force: true }); } catch (_e) {}
    }
  });

  it("(a) good pin — connects over real TLS and receives a response", async () => {
    const res = await transport.post(
      { url: baseUrl, token: "deadbeef".repeat(8), timeouts: SHORT_TIMEOUTS, pinnedFp: REAL_FP },
      { type: "Ping" },
      { sync: true }
    );
    assert.ok(res.ok, "good pin must connect and succeed, got: " + JSON.stringify(res));
    assert.strictEqual(res.status, 200);
    const obj = JSON.parse(res.body.toString("utf8"));
    assert.strictEqual(obj.type, "Pong");
    // The server DID receive the token on the happy path (proves it reached it).
    assert.ok(observedAuth.length > 0, "server received the authenticated request on a good pin");
  });

  it("(b) wrong pin — rejected diagnosably AND the token NEVER reaches the server", async () => {
    const before = observedAuth.length;
    const TOKEN = "cafef00d".repeat(8);
    const res = await transport.post(
      { url: baseUrl, token: TOKEN, timeouts: SHORT_TIMEOUTS, pinnedFp: WRONG_FP },
      { type: "Ping" },
      { sync: true }
    );
    // Rejected as a connect failure (fail-open: resolves, never throws).
    assert.strictEqual(res.ok, false, "wrong pin must fail");
    assert.strictEqual(res.failureClass, "connect");
    // Diagnosable expected-vs-presented message rides the SendResult.
    assert.ok(res.message, "mismatch carries a diagnosable message");
    assert.ok(res.message.includes(WRONG_FP), "names the expected (pinned) fp");
    assert.ok(res.message.includes(REAL_FP), "names the presented (server) fp");
    assert.ok(res.message.includes("client-bundle"), "points at re-bundle remediation");
    // CRUCIAL: no new authenticated request hit the server, and the token never
    // appears in anything the server saw.
    assert.strictEqual(observedAuth.length, before, "server received NO request on a mismatched pin");
    assert.ok(
      observedAuth.every((a) => !a.includes(TOKEN)),
      "the Bearer token never reached the server on a mismatched pin"
    );
  });

  it("(b') token-write ordering — many mismatch attempts never leak the token", async () => {
    // Hammer the mismatch path to make any flush-ordering race observable: if the
    // body were ever flushed before secureConnect validation, the token would
    // land in observedAuth.
    const before = observedAuth.length;
    const TOKEN = "abad1dea".repeat(8);
    for (let i = 0; i < 20; i++) {
      const res = await transport.post(
        { url: baseUrl, token: TOKEN, timeouts: SHORT_TIMEOUTS, pinnedFp: WRONG_FP },
        { type: "Ping" },
        { sync: true }
      );
      assert.strictEqual(res.ok, false);
    }
    assert.strictEqual(observedAuth.length, before, "no mismatched attempt ever reached the server");
    assert.ok(observedAuth.every((a) => !a.includes(TOKEN)), "token never leaked across attempts");
  });

  it("(c) pingForInit — pinned-TLS mismatch surfaces diagnosably as a connect failure (AC-CT-ROT)", async () => {
    const result = await transport.pingForInit(baseUrl, "feedface".repeat(8), SHORT_TIMEOUTS, WRONG_FP);
    assert.strictEqual(result.ok, false);
    // The ONE loud path: the diagnosable mismatch message, not a generic
    // "cannot reach host" line.
    assert.ok(/mismatch/i.test(result.message), "pingForInit surfaces the mismatch verbatim");
    assert.ok(result.message.includes(REAL_FP), "names presented fp");
    assert.ok(result.message.includes(WRONG_FP), "names expected fp");
  });

  it("(c') pingForInit — good pin yields a clean Pong over real TLS", async () => {
    const result = await transport.pingForInit(baseUrl, "0".repeat(64), SHORT_TIMEOUTS, REAL_FP);
    assert.strictEqual(result.ok, true, "good pin pings cleanly: " + result.message);
    assert.ok(/Pong/.test(result.message));
  });
});
