"use strict";

// dispatch.js (C2 unit, R-04) — route by Content-Type (application/json vs
// text/event-stream); 1 MiB bound; 4xx/5xx -> JSON-RPC error (no crash).

const { SseParser } = require("./sse-parse.js");
const { BODY_LIMIT_BYTES } = require("./http-session.js");

const INTERNAL_ERROR = -32603;
// Bridge-internal sentinel (out of the JSON-RPC reserved range) marking a server
// 404 "session not found" so lifecycle.js can self-heal (#830, design-review C1).
// Never reaches the client: lifecycle re-inits + retries before surfacing.
const SESSION_NOT_FOUND = -32099;
// Bridge-internal sentinel for a silent (hung-socket) eviction: the transport
// timed out (#839). Heals through the same single-flight re-init as 404, then
// normalizes to a generic error if exhausted — never reaches the client.
const TRANSPORT_TIMEOUT = -32098;
// Idle-read deadline (#839 F6): a connection that establishes, streams, then
// stalls mid-frame bounds BYTES but not stall TIME. On expiry destroy + throw an
// ETIMEDOUT-coded error so _send maps it to the SAME TRANSPORT_TIMEOUT sentinel
// as the connect/request path (N1: routes the SSE branch into self-heal).
const DEFAULT_READ_MS = 120000;

function jsonRpcError(id, code, message) {
  return { jsonrpc: "2.0", id: id === undefined ? null : id, error: { code, message } };
}

// ETIMEDOUT-coded (mid-stream read stall) — same code as the connect/request
// path, so lifecycle._send routes it into self-heal via TRANSPORT_TIMEOUT.
function readTimedOut() {
  return Object.assign(new Error("MCP endpoint timed out"), { code: "ETIMEDOUT" });
}

// Narrow 404 idle-eviction class (#830): 404 with a "session not found" body.
// Gated narrowly — NOT all 4xx (401/403 = auth; 5xx = server fault) — to avoid
// re-init storms (design-review C1).
function isSessionNotFound(status, body) {
  return status === 404 && /session not found/i.test(body ? body.toString("utf8") : "");
}

function httpStatusToJsonRpc(status, body) {
  if (status === 401 || status === 403) return -32001;
  return isSessionNotFound(status, body) ? SESSION_NOT_FOUND : INTERNAL_ERROR;
}

// Read a non-SSE body bounded at `limit` bytes AND `readMs` idle-stall (#839 F6).
// Byte overflow / end / error / close resolve with what we have; an idle-read
// stall rejects ETIMEDOUT (the throw routes into self-heal via _send). The idle
// timer resets per chunk.
function readBounded(res, limit, readMs) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let received = 0;
    let t;
    const stop = () => { if (t) { clearTimeout(t); t = null; } };
    const arm = () => { stop(); t = setTimeout(() => { try { res.destroy(); } catch (_e) {} reject(readTimedOut()); }, readMs || DEFAULT_READ_MS); if (t.unref) t.unref(); };
    const ok = () => { stop(); resolve(Buffer.concat(chunks)); };
    arm();
    res.on("data", (c) => {
      if (received < limit) {
        chunks.push(c);
        received += c.length;
        arm();
      } else {
        try { res.destroy(); } catch (_e) {}
        ok();
      }
    });
    res.on("end", ok);
    res.on("error", ok);
    res.on("close", ok);
  });
}

function idFromBody(buf) {
  try {
    const o = JSON.parse(buf.toString("utf8"));
    return o && o.id !== undefined ? o.id : null;
  } catch (_e) {
    return null;
  }
}

// dispatchResponse({ status, contentType, res }, readMs?) -> Promise<jsonRpcMessage[]>
// An idle-read stall in either branch rejects ETIMEDOUT (caught by _send -> heal).
async function dispatchResponse({ status, contentType, res }, readMs) {
  const ct = contentType || "";
  if (ct.startsWith("application/json")) {
    const body = await readBounded(res, BODY_LIMIT_BYTES, readMs);
    if (status >= 400) {
      return [jsonRpcError(idFromBody(body), httpStatusToJsonRpc(status, body), "MCP endpoint returned HTTP " + status)];
    }
    try {
      return [JSON.parse(body.toString("utf8"))];
    } catch (_e) {
      return [jsonRpcError(null, INTERNAL_ERROR, "malformed JSON response")];
    }
  }
  if (ct.startsWith("text/event-stream")) {
    const messages = await SseParser.collect(res, BODY_LIMIT_BYTES, readMs);
    if (status >= 400 && messages.length === 0) {
      return [jsonRpcError(null, httpStatusToJsonRpc(status), "MCP endpoint returned HTTP " + status)];
    }
    return messages;
  }
  try { res.resume(); } catch (_e) {}
  return [jsonRpcError(null, INTERNAL_ERROR, "unexpected content-type: " + ct)];
}

function correlateById(messages, id) {
  for (const m of messages) {
    if (m && m.id === id) return m;
  }
  return null;
}

module.exports = {
  dispatchResponse,
  jsonRpcError,
  correlateById,
  readBounded,
  isSessionNotFound,
  INTERNAL_ERROR,
  SESSION_NOT_FOUND,
  TRANSPORT_TIMEOUT,
  DEFAULT_READ_MS,
};
