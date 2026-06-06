"use strict";

// Shared stub-server helper for hook-client suites (vnc-026 test-plan OVERVIEW).
// Local `http` server with scriptable responses (status, contentType, body,
// delayMs, destroy) and a request log (method/path/headers/body). Used by the
// transport, transform, delta, queue, index, and init-remote plans.
//
// Usage:
//   const stub = await startStubServer();
//   stub.respondWith({ status: 200, contentType: "text/plain", body: "hi" });
//   ... POST to stub.url ...
//   stub.requests[0] -> { method, path, headers, body: Buffer }
//   await stub.close();

const http = require("http");
const net = require("net");

/**
 * Start a scriptable HTTP stub server on an ephemeral port.
 *
 * @param {object} [opts]
 * @param {string} [opts.host]  Bind address (default "127.0.0.1"; use "::1" for IPv6).
 * @returns {Promise<object>} { url, port, host, requests, respondWith, close }
 */
function startStubServer(opts) {
  const host = (opts && opts.host) || "127.0.0.1";
  const requests = [];
  // Responder: (requestEntry) => responseSpec | undefined. Default 204 empty.
  let responder = () => ({ status: 204 });

  const server = http.createServer((req, res) => {
    // Clients may abort mid-response (timeout tests) — never crash the stub.
    req.on("error", () => {});
    res.on("error", () => {});
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const entry = {
        method: req.method,
        path: req.url,
        headers: req.headers,
        body: Buffer.concat(chunks),
      };
      requests.push(entry);
      const spec = responder(entry) || {};
      const send = () => {
        if (res.destroyed || res.writableEnded) return;
        try {
          if (spec.destroy) {
            res.destroy();
            return;
          }
          const headers = {};
          if (spec.contentType) headers["Content-Type"] = spec.contentType;
          res.writeHead(spec.status !== undefined ? spec.status : 204, headers);
          res.end(spec.body !== undefined ? spec.body : "");
        } catch (_err) {
          // Socket torn down between checks — irrelevant to the test.
        }
      };
      if (spec.delayMs > 0) {
        const t = setTimeout(send, spec.delayMs);
        if (t.unref) t.unref();
      } else {
        send();
      }
    });
  });
  // Sockets held open by delayed responses must not block close().
  const sockets = new Set();
  server.on("connection", (s) => {
    sockets.add(s);
    s.on("error", () => {});
    s.on("close", () => sockets.delete(s));
  });

  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, host, () => {
      const port = server.address().port;
      const urlHost = host.includes(":") ? "[" + host + "]" : host;
      resolve({
        url: "http://" + urlHost + ":" + port,
        port,
        host,
        requests,
        /** Set the response for subsequent requests. Accepts a spec object or
         *  a function (requestEntry) => spec. Spec: { status, contentType,
         *  body, delayMs, destroy }. */
        respondWith(specOrFn) {
          responder = typeof specOrFn === "function" ? specOrFn : () => specOrFn;
        },
        close() {
          for (const s of sockets) s.destroy();
          return new Promise((res2) => server.close(() => res2()));
        },
      });
    });
  });
}

/**
 * Start a raw TCP server that ACCEPTS connections but never speaks — used to
 * stall TLS handshakes / never produce an HTTP response.
 *
 * @returns {Promise<object>} { port, close }
 */
function startSilentTcpServer() {
  const sockets = new Set();
  const server = net.createServer((s) => {
    sockets.add(s);
    s.on("close", () => sockets.delete(s));
    s.on("error", () => {});
    // Accept and say nothing.
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      resolve({
        port: server.address().port,
        close() {
          for (const s of sockets) s.destroy();
          return new Promise((res2) => server.close(() => res2()));
        },
      });
    });
  });
}

/**
 * Reserve-then-release an ephemeral port so connections to it are refused
 * (ECONNREFUSED). Small race window is acceptable for tests.
 *
 * @returns {Promise<number>} a port nothing is listening on
 */
function refusedPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const port = server.address().port;
      server.close(() => resolve(port));
    });
  });
}

module.exports = { startStubServer, startSilentTcpServer, refusedPort };
