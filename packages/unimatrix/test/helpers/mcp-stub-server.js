"use strict";

// mcp-stub-server.js — provenance-pinned Streamable-HTTP MCP stub for the C2
// bridge tests (vnc-039, test-plan/mcp-bridge.md "Harness"). A real
// https.createServer with a self-signed leaf (openssl, like cert-pin-tls.test.js;
// skip if unavailable). Its wire contract is pinned to the captured rmcp
// initialize response (test/fixtures/mcp/rmcp-initialize-capture.json — R-03):
//
//  - initialize  -> 200, mints + returns a server-side Mcp-Session-Id header.
//  - any follow-up -> requires the Mcp-Session-Id request header; 400 if absent.
//  - logs every request: url, method, authorization, mcp-session-id,
//    clientInfo.name, body.
//  - server.pinnedFp = computeFingerprint(leaf DER) via the production helper.
//  - application/json by default; text/event-stream only when opted in (SSE
//    branch, probe-gated). certSwap() can present a wrong leaf on later sockets
//    (R-02 mid-session swap).

const https = require("https");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { spawnSync } = require("child_process");
const { computeFingerprint } = require("../../lib/hook-client/cert-pin.js");

function genSelfSigned(cn) {
  let dir;
  try {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-mcp-tls-"));
  } catch (_e) {
    return null;
  }
  const certPath = path.join(dir, "cert.pem");
  const keyPath = path.join(dir, "key.pem");
  let r;
  try {
    r = spawnSync("openssl", [
      "req", "-x509", "-newkey", "rsa:2048",
      "-keyout", keyPath, "-out", certPath,
      "-days", "1", "-nodes", "-subj", "/CN=" + (cn || "localhost"),
    ], { stdio: "ignore" });
  } catch (_e) {
    try { fs.rmSync(dir, { recursive: true, force: true }); } catch (_e2) {}
    return null;
  }
  if (r.error || r.status !== 0 || !fs.existsSync(certPath) || !fs.existsSync(keyPath)) {
    try { fs.rmSync(dir, { recursive: true, force: true }); } catch (_e2) {}
    return null;
  }
  const certPem = fs.readFileSync(certPath);
  const keyPem = fs.readFileSync(keyPath);
  return {
    dir,
    certPem,
    keyPem,
    fp: computeFingerprint(new crypto.X509Certificate(certPem).raw),
  };
}

// One-time fixtures for the suite. GEN is the primary (good) leaf; ALT is a
// distinct leaf for the mid-session cert-swap (R-02).
const GEN = genSelfSigned("localhost");
const ALT = GEN ? genSelfSigned("localhost") : null;

function sseFrame(obj) {
  return "event: message\ndata: " + JSON.stringify(obj) + "\n\n";
}

// startMcpStubServer({ sse }) -> Promise<stub>
//   stub: { url, pinnedFp, wrongFp, requests, sessionIds, certSwapToWrong(), close() }
function startMcpStubServer(opts) {
  if (!GEN) return Promise.reject(new Error("openssl unavailable"));
  const useSse = !!(opts && opts.sse);
  const requests = [];
  const sessionIds = [];
  const sockets = new Set();

  const server = https.createServer({
    cert: GEN.certPem,
    key: GEN.keyPem,
  }, (req, res) => {
    req.on("error", () => {});
    res.on("error", () => {});
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks);
      let parsed = null;
      try { parsed = JSON.parse(body.toString("utf8")); } catch (_e) {}
      const clientName = parsed && parsed.params && parsed.params.clientInfo
        ? parsed.params.clientInfo.name : undefined;
      requests.push({
        url: req.url,
        method: req.method,
        authorization: req.headers["authorization"],
        sessionId: req.headers["mcp-session-id"],
        accept: req.headers["accept"],
        contentType: req.headers["content-type"],
        clientInfoName: clientName,
        body,
        parsed,
      });

      if (req.method === "DELETE") { res.writeHead(204); res.end(); return; }

      const isInit = parsed && parsed.method === "initialize";
      if (!isInit && !req.headers["mcp-session-id"]) {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ jsonrpc: "2.0", id: parsed ? parsed.id : null,
          error: { code: -32600, message: "session not found" } }));
        return;
      }

      const headers = {};
      let sid;
      if (isInit) {
        sid = crypto.randomUUID();
        sessionIds.push(sid);
        headers["Mcp-Session-Id"] = sid;
      }
      const result = isInit
        ? { protocolVersion: "2025-06-18", capabilities: { tools: {} },
            serverInfo: { name: "unimatrix", version: "test" } }
        : (parsed && parsed.method === "tools/list")
          ? { tools: [{ name: "context_search" }, { name: "context_get" }] }
          : { content: [{ type: "text", text: "ok" }] };
      const payload = { jsonrpc: "2.0", id: parsed ? parsed.id : null, result };

      if (useSse) {
        headers["Content-Type"] = "text/event-stream";
        res.writeHead(200, headers);
        res.end(sseFrame(payload));
      } else {
        headers["Content-Type"] = "application/json";
        res.writeHead(200, headers);
        res.end(JSON.stringify(payload));
      }
    });
  });

  server.on("connection", (s) => {
    sockets.add(s);
    s.on("close", () => sockets.delete(s));
    s.on("error", () => {});
  });

  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      resolve({
        url: "https://127.0.0.1:" + port,
        pinnedFp: GEN.fp,
        wrongFp: "sha256:" + "0".repeat(64),
        altFp: ALT ? ALT.fp : null,
        requests,
        sessionIds,
        // Swap the server's served leaf to a DIFFERENT self-signed cert for all
        // SUBSEQUENT connections (no SNI on an IP host, so setSecureContext is
        // the mechanism). The bridge's next fresh socket presents the wrong leaf.
        certSwapToWrong() {
          if (ALT) server.setSecureContext({ cert: ALT.certPem, key: ALT.keyPem });
        },
        close() {
          for (const s of sockets) s.destroy();
          return new Promise((res2) => server.close(() => res2()));
        },
      });
    });
  });
}

const SKIP = GEN ? false : { skip: "openssl unavailable — cannot generate the self-signed TLS fixture" };

function cleanupCerts() {
  for (const g of [GEN, ALT]) {
    if (g) { try { fs.rmSync(g.dir, { recursive: true, force: true }); } catch (_e) {} }
  }
}

module.exports = { startMcpStubServer, SKIP, sseFrame, cleanupCerts, GEN };
