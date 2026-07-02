"use strict";

// http-session.js (C2 unit, R-01/R-02/R-05/R-17) — pinned HTTPS POST with
// Mcp-Session-Id capture/replay + per-socket re-pin. Trust contract (ADR-001,
// mirrors transport-http.js:150-176): flush the Bearer body ONLY after the
// secureConnect pin matches; on mismatch destroy BEFORE any body byte and FAIL
// LOUD. agent:false → fresh re-pinned socket per request (no pool, R-02).

const https = require("https");
const { applyCertPin, verifyPeerFingerprint } = require("../cert-pin.js");

// rmcp 1.7.0 wire constants (OVERVIEW "VERIFIED WIRE VALUES"; CONFIRM LIVE).
const SESSION_HEADER = "Mcp-Session-Id";
const SESSION_HEADER_LC = "mcp-session-id";
const BODY_LIMIT_BYTES = 1048576;
// Generous defaults (ms) so slow-but-healthy calls (large embeddings) are never
// aborted; the tighter connect/TLS-handshake deadline catches the half-open
// socket (agent:false → secureConnect can hang; socket `timeout` misses it).

function stripIPv6Brackets(h) {
  return h && h.startsWith("[") && h.endsWith("]") ? h.slice(1, -1) : h;
}

class HttpSession {
  static create(opts) {
    return new HttpSession(opts);
  }

  constructor({ mcpUrl, token, pinnedFp, exit, errOut, connectMs, idleMs }) {
    this.url = new URL(mcpUrl); // POSTed VERBATIM (AC-05)
    this.token = token;
    this.pinnedFp = pinnedFp;
    this.sessionId = null; // server-minted, captured on initialize (R-17)
    this.protocolVersion = null;
    this.exit = exit || ((c) => process.exit(c));
    this.errOut = errOut || ((s) => process.stderr.write(s));
    this.requester = (o) => https.request(o); // injectable for tests
    this.connectMs = connectMs || 15000;
    this.idleMs = idleMs || 120000;
    // Last successful round-trip wall-clock; mirrored by the lifecycle so a
    // transport timeout can report idle_ms (dormancy vs server-down). Seeded to
    // now so an early failure reports a small, sane idle span (#872).
    this.lastActivityMs = Date.now();
  }

  _headers(body, opts) {
    const headers = {
      "Content-Type": "application/json",
      "Content-Length": body.length,
      Accept: "application/json, text/event-stream",
      Authorization: "Bearer " + this.token,
    };
    if (this.sessionId !== null) headers[SESSION_HEADER] = this.sessionId;
    if (this.protocolVersion !== null && !(opts && opts.isInitialize)) {
      headers["MCP-Protocol-Version"] = this.protocolVersion;
    }
    return headers;
  }

  _options(method, headers) {
    return applyCertPin(
      {
        protocol: "https:",
        hostname: stripIPv6Brackets(this.url.hostname),
        port: this.url.port || undefined,
        path: this.url.pathname + this.url.search, // VERBATIM (AC-05)
        method,
        headers,
        agent: false, // no pool — fresh re-pinned socket per request (R-02)
        timeout: this.idleMs, // idle on an established socket; emits "timeout" only
      },
      true,
      this.pinnedFp
    );
  }

  // Re-pin THIS socket on secureConnect, BEFORE any body byte. onPin() runs the
  // body flush (req.end) only on a match; mismatch → destroy + fail-loud. The
  // sole place req.end is reached — an unwritten request cannot leak the token.
  _pinThenFlush(req, onPin, onMismatch, onConnect) {
    req.on("socket", (s) => {
      s.once("secureConnect", () => {
        if (onConnect) onConnect(); // clear the connect/TLS deadline (F4)
        let err;
        try {
          err = verifyPeerFingerprint(s, this.pinnedFp);
        } catch (_e) {
          err = new Error("pinned certificate verification failed");
        }
        if (err) {
          req.destroy(err);
          onMismatch(err);
          return;
        }
        onPin();
      });
    });
  }

  // request(bodyObj, { isInitialize }) -> Promise<{ status, contentType, res }>
  request(bodyObj, opts) {
    const body = Buffer.from(JSON.stringify(bodyObj), "utf8");
    return new Promise((resolve, reject) => {
      if (body.length > BODY_LIMIT_BYTES) {
        reject(new Error("request body exceeds 1 MiB guard"));
        return;
      }
      const req = this.requester(this._options("POST", this._headers(body, opts)));
      let flushed = false;
      let settled = false; // single settle-guard (F3, vnc-039 C2 double-settle)
      let killed = false; // one-shot: emit the timeout event + destroy at most once
      const startMs = Date.now();
      // Errors-only observability (#872, ALWAYS-ON, stderr seam only). connect vs
      // idle is ONLY distinguishable here. idle_ms distinguishes dormancy timeout
      // from server-down. NO token, NO session id (redaction). B1: try/catch no-op.
      const emitTimeout = (phase) => {
        try {
          this.errOut("mcp-bridge: transport_timeout phase=" + phase +
            " elapsed_ms=" + (Date.now() - startMs) +
            " idle_ms=" + (Date.now() - this.lastActivityMs) + "\n");
        } catch (_e) {}
      };
      // ETIMEDOUT-coded so lifecycle.js maps it to "MCP endpoint timed out".
      const kill = (phase) => {
        if (killed) return;
        killed = true;
        emitTimeout(phase);
        try { req.destroy(Object.assign(new Error("timed out"), { code: "ETIMEDOUT" })); } catch (_e) {}
      };
      // Connect/TLS-handshake deadline (F4): armed before connect, cleared in
      // secureConnect (clearCt) — covers the half-open-socket stall the socket
      // `timeout` option does NOT. Idle (post-connect): Node emits "timeout".
      // REF'd on purpose (not unref'd): #872 recovery runs on an idle/dormant
      // loop where an unref'd deadline never fires (#847/#848). Every terminal
      // path routes through settle() -> clearCt(), so it never leaks a handle.
      const ct = setTimeout(() => kill("connect"), this.connectMs);
      const clearCt = () => clearTimeout(ct);
      const settle = (fn, v) => { if (settled) return; settled = true; clearCt(); fn(v); };
      req.on("timeout", () => kill("idle"));
      this._pinThenFlush(
        req,
        () => { flushed = true; req.end(body); }, // pin OK — flush Bearer body
        (err) => { // fail-loud exit on pin mismatch, token-free (NFR-06); settle under injected exit
          try { this.errOut("mcp-bridge: " + err.message + "\n"); } catch (_e) {}
          this.exit(1);
          settle(reject, err);
        },
        clearCt
      );
      req.on("response", (res) => {
        if (opts && opts.isInitialize) {
          const sid = res.headers[SESSION_HEADER_LC];
          if (sid) this.sessionId = sid; // stable for process life (R-17)
        }
        settle(resolve, { status: res.statusCode, contentType: res.headers["content-type"] || "", res });
      });
      req.on("error", (err) => {
        // Swallow the post-pin-mismatch destroy (no body byte written), BUT never
        // swallow a timeout reject — gate on the error code, not `flushed` (F3).
        if (!flushed && this.pinnedFp && err.code !== "ETIMEDOUT") return;
        settle(reject, err);
      });
    });
  }

  // Best-effort re-pinned DELETE on stdin EOF; swallow errors (non-fatal).
  teardown() {
    if (this.sessionId === null) return Promise.resolve();
    const headers = { [SESSION_HEADER]: this.sessionId, Authorization: "Bearer " + this.token };
    return new Promise((resolve) => {
      let done = false;
      const finish = () => { if (!done) { done = true; resolve(); } };
      let req;
      try { req = this.requester(this._options("DELETE", headers)); }
      catch (_e) { finish(); return; }
      this._pinThenFlush(req, () => { try { req.end(); } catch (_e) { finish(); } }, () => finish());
      req.on("response", (res) => { try { res.resume(); } catch (_e) {} finish(); });
      req.on("error", () => finish());
      const t = setTimeout(finish, 1000);
      if (t.unref) t.unref();
    });
  }
}

module.exports = {
  HttpSession,
  SESSION_HEADER,
  SESSION_HEADER_LC,
  BODY_LIMIT_BYTES,
};
