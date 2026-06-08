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
const fs = require("fs");
const os = require("os");
const path = require("path");

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

/** Frame a JSON-serializable object as wire.rs does: 4-byte BE u32 len + JSON. */
function frameResponse(obj) {
  const json = Buffer.from(JSON.stringify(obj), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32BE(json.length, 0);
  return Buffer.concat([header, json]);
}

/**
 * Start a scriptable Unix-domain-socket stub listener (mirrors the daemon's
 * one-frame-per-connection contract). Reads the client's framed request, then
 * the scripted responder decides how to reply.
 *
 * Responder is (requestBody:Buffer) => spec, where spec is one of:
 *   { frame: object }          — reply with a framed HookResponse, then end
 *   { raw: Buffer, chunkSize } — write raw bytes (optionally in chunkSize pieces)
 *   { silent: true }           — accept, read, never reply (deadline tests)
 *   { endEarly: true }         — close before sending a complete frame
 *   undefined / {}             — accept and end (FNF: no reply at all)
 *
 * @returns {Promise<object>} { socketPath, requests, respondWith, close }
 */
function startUdsStubServer() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "uds-stub-"));
  const socketPath = path.join(dir, "stub.sock");
  const requests = [];
  let responder = () => ({});
  const sockets = new Set();

  // allowHalfOpen mirrors the daemon: the client half-closes (write FIN) after
  // sending its frame; the stub must keep its write side open to reply.
  const server = net.createServer({ allowHalfOpen: true }, (s) => {
    sockets.add(s);
    s.on("error", () => {});
    s.on("close", () => sockets.delete(s));
    const chunks = [];
    let received = 0;
    let declaredLen = null;
    const onData = (chunk) => {
      chunks.push(chunk);
      received += chunk.length;
      if (declaredLen === null && received >= 4) {
        declaredLen = Buffer.concat(chunks).readUInt32BE(0);
      }
      if (declaredLen !== null && received >= 4 + declaredLen) {
        s.removeListener("data", onData);
        const body = Buffer.concat(chunks).subarray(4, 4 + declaredLen);
        requests.push(body);
        reply(s, responder(body) || {});
      }
    };
    s.on("data", onData);
  });

  function reply(s, spec) {
    if (spec.silent) return; // hold the connection open, send nothing
    if (spec.endEarly) {
      try {
        s.end();
      } catch (_err) {
        // already closed
      }
      return;
    }
    let out = null;
    if (spec.frame !== undefined) out = frameResponse(spec.frame);
    else if (spec.raw !== undefined) out = spec.raw;
    if (out === null) {
      try {
        s.end(); // FNF: nothing to send
      } catch (_err) {
        // already closed
      }
      return;
    }
    if (spec.noEnd) {
      // Write partial bytes and hold the connection open (deadline / partial-read tests).
      try {
        s.write(out);
      } catch (_err) {
        // closed
      }
      return;
    }
    const chunkSize = spec.chunkSize;
    if (chunkSize && chunkSize > 0) {
      let i = 0;
      const writeNext = () => {
        if (i >= out.length) {
          try {
            s.end();
          } catch (_err) {
            // closed
          }
          return;
        }
        const slice = out.subarray(i, Math.min(i + chunkSize, out.length));
        i += chunkSize;
        try {
          s.write(slice);
        } catch (_err) {
          return;
        }
        setImmediate(writeNext); // separate 'data' events, well under the 40 ms budget
      };
      writeNext();
    } else {
      try {
        s.end(out);
      } catch (_err) {
        // closed
      }
    }
  }

  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(socketPath, () => {
      resolve({
        socketPath,
        requests,
        respondWith(specOrFn) {
          responder = typeof specOrFn === "function" ? specOrFn : () => specOrFn;
        },
        close() {
          for (const s of sockets) s.destroy();
          return new Promise((res2) =>
            server.close(() => {
              try {
                fs.rmSync(dir, { recursive: true, force: true });
              } catch (_err) {
                // best-effort cleanup
              }
              res2();
            })
          );
        },
      });
    });
  });
}

/** A socket path under a temp dir that has NO listener (ENOENT on connect). */
function absentSocketPath() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "uds-absent-"));
  return path.join(dir, "nope.sock");
}

/**
 * Start a UDS listener that ACCEPTS connections but never reads — the kernel
 * receive buffer fills, so a large client write never flushes ('finish' never
 * fires) and any sync read never returns. Used for flush-timeout / deadline tests.
 *
 * @returns {Promise<object>} { socketPath, close }
 */
function startUdsBlackholeServer() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "uds-blackhole-"));
  const socketPath = path.join(dir, "blackhole.sock");
  const sockets = new Set();
  const server = net.createServer({ allowHalfOpen: true }, (s) => {
    sockets.add(s);
    s.on("error", () => {});
    s.on("close", () => sockets.delete(s));
    s.pause(); // stop draining the kernel receive buffer
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(socketPath, () => {
      resolve({
        socketPath,
        close() {
          for (const s of sockets) s.destroy();
          return new Promise((res2) =>
            server.close(() => {
              try {
                fs.rmSync(dir, { recursive: true, force: true });
              } catch (_err) {
                // best-effort
              }
              res2();
            })
          );
        },
      });
    });
  });
}

module.exports = {
  startStubServer,
  startSilentTcpServer,
  refusedPort,
  startUdsStubServer,
  startUdsBlackholeServer,
  absentSocketPath,
  frameResponse,
};
