"use strict";

// transport-uds.js — local Unix-domain-socket transport for the hook client.
//
// post(config, frame, opts) -> Promise<SendResult>
//   SendResult = { ok, status, contentType, body, failureClass }
//   failureClass: null | "auth" | "connect" | "timeout" | "http_4xx" | "http_5xx"
//
// Adapter onto the transport-http.js:5-7 contract (ADR-002). NEVER throws/rejects
// — always resolves a SendResult (NFR-3 fail-open). No retries (queue / offset
// re-drive are the retry mechanism). Emits NO stdout/stderr; no secrets logged.
//
// Framing is byte-identical to wire.rs:16,349,372 (4-byte BE u32 length + JSON,
// 1 MiB cap, zero-length / oversized declared length rejected before allocating).
// Socket lifecycle per ADR-003 (flush-before-FIN for FNF, half-close + accumulate
// read for sync; never destroy() unflushed; no process.exit). UDS is Unix-only —
// Windows uses HTTP-remote (config.js never selects mode "uds" there).

const net = require("net");

const MAX_PAYLOAD_SIZE = 1048576; // wire.rs:16 — byte-identical cap (1 MiB)
const FRAME_HEADER_SIZE = 4; // 4-byte BE u32 length prefix
const TIMEOUT_MS = 40; // ADR-002 §6 fixed parity (connect/sync/fnf all 40 ms)

// ADR-001 §2: only these sync frames carry the preformatted-response request.
const SYNC_ACCEPT_TYPES = Object.freeze({ ContextSearch: true, CompactPayload: true });

/** Build a failure SendResult. */
function fail(cls, status) {
  return { ok: false, status, contentType: null, body: null, failureClass: cls };
}

/** Build a success SendResult. */
function okResult(status, contentType, body) {
  return { ok: true, status, contentType, body, failureClass: null };
}

/** Classify a socket errno into a failure class (mirrors transport-http.js:43-48). */
function classifyErrno(err) {
  const code = err && err.code;
  if (code === "ETIMEDOUT") return "timeout";
  // ENOENT (dir absent), ECONNREFUSED (stale socket), EACCES (peer-cred), EPIPE…
  return "connect";
}

/**
 * Encode a HookRequest frame to wire bytes, or null on a client-side reject
 * (>1 MiB or unserializable). Injects accept:"text/plain" into the SERIALIZED
 * bytes only for sync injection-bearing frames (ADR-001 §2) — never mutates the
 * caller's frame, so the queue stays transport-agnostic (queued frames never
 * carry accept).
 *
 * @returns {Buffer|null}
 */
function encodeFrame(frame, opts) {
  let payloadObj = frame;
  if (opts && opts.sync && frame && SYNC_ACCEPT_TYPES[frame.type]) {
    payloadObj = Object.assign({}, frame, { accept: "text/plain" });
  }
  let json;
  try {
    json = Buffer.from(JSON.stringify(payloadObj), "utf8");
  } catch (_err) {
    return null; // circular / BigInt — caller maps to http_4xx
  }
  if (json.length > MAX_PAYLOAD_SIZE) {
    return null; // C-02-equivalent client-side reject — no write
  }
  const header = Buffer.alloc(FRAME_HEADER_SIZE);
  header.writeUInt32BE(json.length, 0);
  return Buffer.concat([header, json]);
}

/**
 * Map a deserialized HookResponse to a SendResult (ADR-002 §2, normative). FNF
 * success (status 0) is produced in the FNF path, not here.
 */
function mapHookResponse(obj) {
  if (!obj || typeof obj !== "object") return fail("connect", 0);
  switch (obj.type) {
    case "Text":
      return okResult(200, "text/plain", Buffer.from(obj.body || "", "utf8"));
    case "Ack":
      return okResult(204, null, null); // sync empty injection → 204-equivalent
    case "Pong":
      return okResult(200, "application/json", Buffer.from(JSON.stringify(obj), "utf8"));
    case "Error": {
      const cls = obj.code >= 500 ? "http_5xx" : "http_4xx";
      return fail(cls, obj.code);
    }
    default:
      return fail("connect", 0); // unexpected variant → protocol violation
  }
}

/** Parse a response body Buffer; protocol violation on failure (connect class). */
function parseResponse(body) {
  let obj;
  try {
    obj = JSON.parse(body.toString("utf8"));
  } catch (_err) {
    return fail("connect", 0);
  }
  return mapHookResponse(obj);
}

/**
 * POST a HookRequest frame over the local UDS to the daemon. Always resolves a
 * SendResult, never rejects.
 *
 * @param {object} config { socketPath }
 * @param {object} frame  HookRequest object
 * @param {object} opts   { sync: boolean }
 */
function post(config, frame, opts) {
  const options = opts || {};
  const frameBuf = encodeFrame(frame, options);
  if (frameBuf === null) {
    // ADR-002 §2: client-side reject (>1 MiB / unserializable). No connection.
    return Promise.resolve(fail("http_4xx", 0));
  }

  return new Promise((resolve) => {
    let settled = false;
    let deadline = null;
    // Settle exactly once; clear the deadline on every path (transport-http.js:98-104).
    const done = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(deadline);
      resolve(result);
    };

    let socket = null;
    try {
      socket = net.connect(config.socketPath);
    } catch (_err) {
      done(fail("connect", 0)); // synchronous connect throw (wrapped)
      return;
    }

    deadline = setTimeout(() => {
      try {
        socket.destroy();
      } catch (_err) {
        // already torn down — irrelevant to the timeout classification
      }
      done(fail("timeout", 0));
    }, TIMEOUT_MS);
    if (deadline.unref) deadline.unref(); // never holds the event loop open

    // error before 'connect' → connect failure; once-guarded so a fired deadline wins.
    socket.on("error", (err) => done(fail(classifyErrno(err), 0)));

    if (options.sync) {
      armSyncPath(socket, frameBuf, done);
    } else {
      armFnfPath(socket, frameBuf, done);
    }
  });
}

/**
 * FNF path — flush before FIN (ADR-003 §2). Resolve success on 'finish' (all data
 * handed to the OS, the Rust write_all-equivalent guarantee). destroy() is NEVER
 * called before 'finish'; cleanup happens after settle. No read — the server's
 * EPIPE writing its Ack to our FIN'd socket is expected and DEBUG-classed (#3448).
 */
function armFnfPath(socket, frameBuf, done) {
  socket.on("connect", () => {
    socket.end(frameBuf); // write THEN FIN; queued data flushed to kernel before FIN
  });
  socket.on("finish", () => {
    done(okResult(0, null, null)); // FNF success → status 0 (ADR-002 §2)
    try {
      socket.destroy(); // cleanup AFTER settle only
    } catch (_err) {
      // socket already gone
    }
  });
}

/**
 * Sync path — half-close + accumulate to declared length (ADR-003 §3). client→
 * server FIN after flush; the Unix socket stays readable. Reject declared length
 * 0 or >1 MiB BEFORE allocating the body (hostile-prefix DoS guard).
 */
function armSyncPath(socket, frameBuf, done) {
  const chunks = [];
  let received = 0;
  let declaredLen = null;

  socket.on("connect", () => {
    socket.end(frameBuf); // half-close: flush + FIN, signals exactly-one-request
  });

  socket.on("data", (chunk) => {
    chunks.push(chunk);
    received += chunk.length;
    if (declaredLen === null && received >= FRAME_HEADER_SIZE) {
      declaredLen = Buffer.concat(chunks).readUInt32BE(0);
      if (declaredLen === 0 || declaredLen > MAX_PAYLOAD_SIZE) {
        // Protocol violation — reject before reading/allocating the body.
        try {
          socket.destroy();
        } catch (_err) {
          // already gone
        }
        done(fail("connect", 0));
        return;
      }
    }
    if (declaredLen !== null && received >= FRAME_HEADER_SIZE + declaredLen) {
      const body = Buffer.concat(chunks).subarray(
        FRAME_HEADER_SIZE,
        FRAME_HEADER_SIZE + declaredLen
      );
      try {
        socket.destroy(); // got the one frame
      } catch (_err) {
        // already gone
      }
      done(parseResponse(body));
    }
  });

  socket.on("end", () => {
    // Server closed before a complete frame → truncated/short response.
    done(fail("connect", 0));
  });
}

module.exports = { post, encodeFrame, mapHookResponse, MAX_PAYLOAD_SIZE, TIMEOUT_MS };
