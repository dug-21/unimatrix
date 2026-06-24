"use strict";

// bridge-witness.js (nan-021 C2) — WIRE WITNESS preload for the SHIPPED bridge.
//
// Loaded into the real `node mcp-bridge.js <projectHash>` process via
// NODE_OPTIONS=--require so the bridge runs UNMODIFIED (NFR-1/NFR-2: zero
// production diff, no re-authored transport). This file lives in the test tree
// and only OBSERVES the pinned-HTTPS wire — it never alters request behavior,
// headers, bodies, or the pin/flush order. It exists solely to make AC-02/FR-9
// positively assertable: the bridge CARRIED the traffic (Mcp-Session-Id
// captured + replayed byte-stable + a text/event-stream response parsed), not
// merely a 200.
//
// Mechanism: wrap https.request. On each call we emit one structured
// `BRIDGE_WITNESS:<json>` line to STDERR (never stdout — stdout is the bridge's
// JSON-RPC channel) recording the Accept header the bridge sent and whether an
// Mcp-Session-Id was replayed on the request; on the response we record the
// status + content-type. The shell gate parses these lines to PROVE:
//   * the Accept header was "application/json, text/event-stream" (SSE offered),
//   * a text/event-stream response was actually received (SSE parsed downstream),
//   * the server-minted Mcp-Session-Id captured on initialize was REPLAYED
//     byte-stable on >=1 later request.
// NOTHING here is on the trust path: the wrapper forwards args verbatim and
// returns the real ClientRequest unchanged. It NEVER logs Authorization/token
// (NFR-06) — only the session-id (non-secret, server-minted) and content-type.

const https = require("https");

const SESSION_HEADER_LC = "mcp-session-id";

function headerLookup(headers, lcName) {
  if (!headers || typeof headers !== "object") return undefined;
  for (const k of Object.keys(headers)) {
    if (k.toLowerCase() === lcName) return headers[k];
  }
  return undefined;
}

function emit(obj) {
  try {
    process.stderr.write("BRIDGE_WITNESS:" + JSON.stringify(obj) + "\n");
  } catch (_e) {
    // witness is best-effort; never perturb the bridge on a write failure.
  }
}

const realRequest = https.request.bind(https);

// Signature parity with https.request: (options[, callback]) or (url, options[, cb]).
https.request = function witnessedRequest(...args) {
  let options;
  for (const a of args) {
    if (a && typeof a === "object" && !(a instanceof URL)) {
      options = a;
      break;
    }
  }
  const method = (options && options.method) || "GET";
  const accept = headerLookup(options && options.headers, "accept");
  const sentSid = headerLookup(options && options.headers, SESSION_HEADER_LC);
  // Token MUST NOT be witnessed (NFR-06): we never read Authorization here.
  emit({
    ev: "request",
    method,
    accept: accept || null,
    sent_session_id: sentSid || null,
  });

  const req = realRequest(...args);
  req.on("response", (res) => {
    const recvSid = headerLookup(res.headers, SESSION_HEADER_LC);
    emit({
      ev: "response",
      method,
      status: res.statusCode,
      content_type: (res.headers && res.headers["content-type"]) || null,
      recv_session_id: recvSid || null,
    });
  });
  return req;
};
