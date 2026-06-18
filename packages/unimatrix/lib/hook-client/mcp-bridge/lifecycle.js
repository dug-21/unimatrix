"use strict";

// lifecycle.js (C2 unit, R-17) — transparent JSON-RPC proxy over http-session.
// Forwards the client's method/params verbatim; the ONLY synthesized field is a
// STABLE clientInfo.name on initialize (server keys audit attribution on it +
// the transport session id — vnc-014/#4708; AC-12). Captures the negotiated
// protocolVersion at initialize for the MCP-Protocol-Version echo (G1).

const { dispatchResponse, jsonRpcError, correlateById, INTERNAL_ERROR } = require("./dispatch.js");

const CLIENT_INFO_NAME = "unimatrix-mcp-bridge"; // STABLE, fixed (FR-03a/AC-12)

class Lifecycle {
  constructor(session) {
    this.session = session;
  }

  // handle(msg) -> Promise<jsonRpcMessage | null>  (null for notifications)
  async handle(msg) {
    const isInit = msg && msg.method === "initialize";
    if (isInit) {
      if (!msg.params || typeof msg.params !== "object") msg.params = {};
      if (!msg.params.clientInfo || typeof msg.params.clientInfo !== "object") {
        msg.params.clientInfo = {};
      }
      msg.params.clientInfo.name = CLIENT_INFO_NAME; // fixed; never per-spawn (R-17)
    }

    let resp;
    try {
      resp = await this.session.request(msg, { isInitialize: isInit });
    } catch (err) {
      // Pin-class failures already exited loud inside the session. Transport /
      // connect / timeout class -> JSON-RPC error (per-request, not fatal).
      return jsonRpcError(msg && msg.id != null ? msg.id : null, INTERNAL_ERROR, classify(err));
    }

    const out = await dispatchResponse(resp);

    if (isInit) {
      const pv = extractProtocolVersion(out);
      if (pv) {
        this.protocolVersion = pv;
        this.session.protocolVersion = pv;
      }
    }

    // Notifications (no id) expect no response.
    if (!msg || msg.id === undefined || msg.id === null) return null;
    return correlateById(out, msg.id) || out[0] || jsonRpcError(msg.id, INTERNAL_ERROR, "empty response");
  }
}

function classify(err) {
  const code = err && err.code;
  if (code === "ETIMEDOUT") return "MCP endpoint timed out";
  return "MCP endpoint unreachable";
}

function extractProtocolVersion(messages) {
  for (const m of messages) {
    if (m && m.result && typeof m.result.protocolVersion === "string") {
      return m.result.protocolVersion;
    }
  }
  return null;
}

module.exports = { Lifecycle, CLIENT_INFO_NAME, extractProtocolVersion };
