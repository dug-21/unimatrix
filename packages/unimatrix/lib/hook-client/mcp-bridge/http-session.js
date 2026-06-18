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
const ACCEPT_VALUE = "application/json, text/event-stream";
const CONTENT_TYPE_REQUEST = "application/json";
const BODY_LIMIT_BYTES = 1048576;

function stripIPv6Brackets(h) {
  return h && h.startsWith("[") && h.endsWith("]") ? h.slice(1, -1) : h;
}

// Fail-loud exit on pin mismatch; token-free message (NFR-06).
function failLoud(session, message) {
  try { session.errOut("mcp-bridge: " + message + "\n"); } catch (_e) {}
  session.exit(1);
}

class HttpSession {
  static create(opts) {
    return new HttpSession(opts);
  }

  constructor({ mcpUrl, token, pinnedFp, exit, errOut }) {
    this.url = new URL(mcpUrl); // POSTed VERBATIM (AC-05)
    this.token = token;
    this.pinnedFp = pinnedFp;
    this.sessionId = null; // server-minted, captured on initialize (R-17)
    this.protocolVersion = null;
    this.exit = exit || ((c) => process.exit(c));
    this.errOut = errOut || ((s) => process.stderr.write(s));
    this.requester = (o) => https.request(o); // injectable for tests
  }

  _headers(body, opts) {
    const headers = {
      "Content-Type": CONTENT_TYPE_REQUEST,
      "Content-Length": body.length,
      Accept: ACCEPT_VALUE,
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
      },
      true,
      this.pinnedFp
    );
  }

  // Re-pin THIS socket on secureConnect, BEFORE any body byte. onPin() runs the
  // body flush (req.end) only on a match; mismatch → destroy + fail-loud. The
  // sole place req.end is reached — an unwritten request cannot leak the token.
  _pinThenFlush(req, onPin, onMismatch) {
    req.on("socket", (s) => {
      s.once("secureConnect", () => {
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
      this._pinThenFlush(
        req,
        () => { flushed = true; req.end(body); }, // pin OK — flush Bearer body
        (err) => {
          // Production: failLoud exits non-zero. Tests inject a recording exit;
          // settle the promise (token-free) so awaiting callers do not hang.
          failLoud(this, err.message); // token-free expected-vs-presented
          reject(err);
        }
      );
      req.on("response", (res) => {
        if (opts && opts.isInitialize) {
          const sid = res.headers[SESSION_HEADER_LC];
          if (sid) this.sessionId = sid; // stable for process life (R-17)
        }
        resolve({ status: res.statusCode, contentType: res.headers["content-type"] || "", res });
      });
      req.on("error", (err) => {
        if (!flushed && this.pinnedFp) return; // mismatch already exited loud
        reject(err);
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
  ACCEPT_VALUE,
  CONTENT_TYPE_REQUEST,
  BODY_LIMIT_BYTES,
  stripIPv6Brackets,
};
