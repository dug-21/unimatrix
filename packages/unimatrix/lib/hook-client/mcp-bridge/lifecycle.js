"use strict";

// lifecycle.js (C2 unit, R-17) — transparent JSON-RPC proxy over http-session.
// Forwards the client's method/params verbatim; the ONLY synthesized field is a
// STABLE clientInfo.name on initialize (server keys audit attribution on it +
// the transport session id — vnc-014/#4708; AC-12). Captures the negotiated
// protocolVersion at initialize for the MCP-Protocol-Version echo (G1).
//
// Self-heal (#830, design-review B3/C1-C3): a cloud server evicts idle sessions
// (rmcp keep_alive); the next post 404s "session not found". On that narrow
// signal — post-init requests only — drop the dead session id, RE-INITIALIZE
// through this same path (clientInfo.name stays byte-identical: vnc-039 audit
// contract holds; the rotating Mcp-Session-Id is fine), then retry the failed
// call EXACTLY ONCE. Single-flight: concurrent 404s share one re-init. Any other
// error, a failed re-init, or a second 404 propagates — no storms, no loops.

const { dispatchResponse, jsonRpcError, correlateById, INTERNAL_ERROR, SESSION_NOT_FOUND, TRANSPORT_TIMEOUT } = require("./dispatch.js");

const CLIENT_INFO_NAME = "unimatrix-mcp-bridge"; // STABLE, fixed (FR-03a/AC-12)
// Exhausted-heal normalization: an internal sentinel never reaches the client.
const SENTINEL_TEXT = { [SESSION_NOT_FOUND]: "MCP session lost, re-init failed", [TRANSPORT_TIMEOUT]: "MCP endpoint timed out" };

class Lifecycle {
  constructor(session, deps) {
    this.session = session;
    this.initMessage = null; // captured client initialize, replayed on re-init (B3)
    this.reinitInFlight = null; // single-flight guard for concurrent 404s (C2)
    this.errOut = (deps && deps.errOut) || ((s) => process.stderr.write(s));
  }

  // handle(msg) -> Promise<jsonRpcMessage | null>  (null for notifications)
  async handle(msg) {
    const isInit = msg && msg.method === "initialize";
    if (isInit) {
      // Inject the STABLE clientInfo.name (fixed; never per-spawn — R-17/B3).
      if (!msg.params || typeof msg.params !== "object") msg.params = {};
      if (!msg.params.clientInfo || typeof msg.params.clientInfo !== "object") msg.params.clientInfo = {};
      msg.params.clientInfo.name = CLIENT_INFO_NAME;
      this.initMessage = msg; // already-stamped; replayed verbatim on re-init (B3)
    }

    let out = await this._send(msg, isInit);

    // Self-heal post-init only (a 404 on initialize itself is fatal — C1):
    // re-init once (single-flight), then retry the call once. Heal failure or a
    // second 404 leaves `out` the original 404 — no loop, no storm (C1/C2).
    if (!isInit && msg && lostId(out, msg.id)) {
      let ok = false;
      try { ok = await this._reinit(); } catch (_e) {}
      if (ok) out = await this._send(msg, false);
    }

    if (!msg || msg.id == null) return null; // notification — no response
    const m = correlateById(out, msg.id) || out[0] || jsonRpcError(msg.id, INTERNAL_ERROR, "empty response");
    // A leaked sentinel (heal exhausted) is normalized — never reaches a client.
    const why = SENTINEL_TEXT[m && m.error && m.error.code];
    return why ? jsonRpcError(msg.id, INTERNAL_ERROR, why) : m;
  }

  // One request + dispatch -> jsonRpcMessage[]. Transport-class failures (already
  // exited loud if pin-class) become a per-request JSON-RPC error array; they are
  // never SESSION_NOT_FOUND, so they neither trigger nor satisfy self-heal.
  async _send(msg, isInit) {
    let resp;
    try {
      resp = await this.session.request(msg, { isInitialize: isInit });
    } catch (err) {
      const id = msg && msg.id != null ? msg.id : null;
      // A transport TIMEOUT gets a distinct sentinel so handle() can self-heal it
      // like a 404 (#839 F2); any other transport fault is a terminal error.
      if (err && err.code === "ETIMEDOUT") return [jsonRpcError(id, TRANSPORT_TIMEOUT, "MCP endpoint timed out")];
      return [jsonRpcError(id, INTERNAL_ERROR, "MCP endpoint unreachable")];
    }
    const out = await dispatchResponse(resp);
    if (isInit) {
      // Capture the negotiated protocolVersion for the MCP-Protocol-Version echo.
      for (const m of out) {
        if (m && m.result && typeof m.result.protocolVersion === "string") {
          this.session.protocolVersion = m.result.protocolVersion;
          break;
        }
      }
    }
    return out;
  }

  // Single-flight: concurrent 404s await ONE shared re-init (C2). Resolves true
  // on a fresh session id, false otherwise. Never throws / loops.
  _reinit() {
    if (!this.reinitInFlight) {
      this.reinitInFlight = this._doReinit().finally(() => { this.reinitInFlight = null; });
    }
    return this.reinitInFlight;
  }

  async _doReinit() {
    const init = this.initMessage;
    if (!init) return false; // never saw initialize — cannot heal
    const prevId = this.session.sessionId;
    this.errOut("mcp-bridge: session evicted (404); re-init\n");
    this.session.sessionId = null; // discard the dead id before re-init
    await this._send(init, true); // protocolVersion re-captured inside
    const newId = this.session.sessionId;
    return !(newId == null || newId === prevId); // fresh session id minted?
  }
}

// True when a heal-triggering sentinel is the response for this id: a 404
// eviction (#830) OR a transport timeout / silent eviction (#839) — ONLY the two
// SENTINEL_TEXT keys, never auth/5xx (avoids re-init storms). `out` is message[].
function lostId(out, id) {
  const m = correlateById(out, id) || out[0];
  return !!(m && m.error && m.error.code in SENTINEL_TEXT);
}

module.exports = { Lifecycle, CLIENT_INFO_NAME };
