"use strict";

// dispatch.js (C2 unit, R-04) — route by Content-Type (application/json vs
// text/event-stream); 1 MiB bound; 4xx/5xx -> JSON-RPC error (no crash).

const { SseParser } = require("./sse-parse.js");
const { BODY_LIMIT_BYTES } = require("./http-session.js");

const INTERNAL_ERROR = -32603;

function jsonRpcError(id, code, message) {
  return { jsonrpc: "2.0", id: id === undefined ? null : id, error: { code, message } };
}

function httpStatusToJsonRpc(status) {
  return status === 401 || status === 403 ? -32001 : INTERNAL_ERROR;
}

// Read a non-SSE body bounded at `limit`; destroy + return-what-we-have on excess.
function readBounded(res, limit) {
  return new Promise((resolve) => {
    const chunks = [];
    let received = 0;
    res.on("data", (c) => {
      if (received < limit) {
        chunks.push(c);
        received += c.length;
      } else {
        try { res.destroy(); } catch (_e) {}
        resolve(Buffer.concat(chunks));
      }
    });
    res.on("end", () => resolve(Buffer.concat(chunks)));
    res.on("error", () => resolve(Buffer.concat(chunks)));
    res.on("close", () => resolve(Buffer.concat(chunks)));
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

// dispatchResponse({ status, contentType, res }) -> Promise<jsonRpcMessage[]>
async function dispatchResponse({ status, contentType, res }) {
  const ct = contentType || "";
  if (ct.startsWith("application/json")) {
    const body = await readBounded(res, BODY_LIMIT_BYTES);
    if (status >= 400) {
      return [jsonRpcError(idFromBody(body), httpStatusToJsonRpc(status), "MCP endpoint returned HTTP " + status)];
    }
    try {
      return [JSON.parse(body.toString("utf8"))];
    } catch (_e) {
      return [jsonRpcError(null, INTERNAL_ERROR, "malformed JSON response")];
    }
  }
  if (ct.startsWith("text/event-stream")) {
    const messages = await SseParser.collect(res, BODY_LIMIT_BYTES);
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

module.exports = { dispatchResponse, jsonRpcError, correlateById, readBounded, INTERNAL_ERROR };
