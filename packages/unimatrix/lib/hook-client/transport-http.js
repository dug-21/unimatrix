"use strict";

// transport-http.js — the only network module of the hook client.
//
// post(config, frame, opts) -> Promise<SendResult>  (posts config.url VERBATIM)
//   SendResult = { ok, status, contentType, body, failureClass }
//   failureClass: null | "auth" | "connect" | "timeout" | "http_4xx" | "http_5xx"
//
// NEVER throws/rejects — always resolves a SendResult (ADR-005 fail-open). No
// retries (queue / offset re-drive are the retry mechanisms). Emits NO
// stdout/stderr and never logs the token, full URL, or body (R-16).

const http = require("http");
const https = require("https");
const { applyCertPin, verifyPeerFingerprint } = require("./cert-pin.js");

// ADR-005 defaults (config-overridable via unimatrix.remote.timeouts). config.js
// supplies resolved values; these back pingForInit and override-less callers.
const DEFAULT_TIMEOUTS = Object.freeze({ connectMs: 750, syncMs: 2000, fnfMs: 3000 });

// C-02: 1 MiB post-serialization body guard (backstop; delta.js pre-checks its
// own frames). Also caps response-body buffering.
const BODY_LIMIT_BYTES = 1048576;

/** Build a failure SendResult. */
function fail(cls, status) {
  return { ok: false, status, contentType: null, body: null, failureClass: cls };
}

/** Classify an HTTP status into a SendResult (R-10 breadcrumb input). */
function classifyResponse(status, contentType, bodyBuf) {
  if (status >= 200 && status < 300) {
    return { ok: true, status, contentType, body: bodyBuf, failureClass: null };
  }
  const cls =
    status === 401 || status === 403 ? "auth"
    : status >= 400 && status < 500 ? "http_4xx"
    : status >= 500 ? "http_5xx"
    : "connect";
  return { ok: false, status, contentType, body: null, failureClass: cls };
}

/** Classify a socket/request error into a failure class. */
function classifyErrno(err) {
  const code = err && err.code;
  if (code === "ETIMEDOUT") return "timeout";
  // ECONNREFUSED, ENOTFOUND, ECONNRESET, EAI_AGAIN, EPIPE, TLS errors…
  return "connect";
}

/**
 * POST a HookRequest frame to config.url VERBATIM. Always resolves a SendResult,
 * never rejects.
 *
 * ADR-001 (dumb-client invariant): config.url IS the server-composed observe URL
 * (`observe_url`), stored verbatim by init.js. The transport composes NO path —
 * it posts to config.url's pathname (+ search) byte-for-byte. The `/observe`
 * append (the last client-side route-composition site, C-3) is GONE.
 *
 * @param {object} config  { url, token, timeouts: {connectMs, syncMs, fnfMs}, pinnedFp? }
 * @param {object} frame   HookRequest object (ignored when opts.bodyBuf set)
 * @param {object} opts    { sync: boolean, bodyBuf?: Buffer }
 */
function post(config, frame, opts) {
  const options = opts || {};
  let u;
  try {
    u = new URL(config.url);
  } catch (_err) {
    return Promise.resolve(fail("connect", 0));
  }
  if (u.protocol !== "http:" && u.protocol !== "https:") {
    return Promise.resolve(fail("connect", 0));
  }
  const isTls = u.protocol === "https:";
  const mod = isTls ? https : http;

  let body;
  try {
    body = options.bodyBuf || Buffer.from(JSON.stringify(frame), "utf8");
  } catch (_err) {
    return Promise.resolve(fail("http_4xx", 0)); // unserializable frame — client-side reject
  }
  if (body.length > BODY_LIMIT_BYTES) {
    return Promise.resolve(fail("http_4xx", 0)); // C-02 guard: no network write
  }

  // ADR-001: post to the verbatim observe URL. No append, no trailing-slash
  // mutation, no route grammar — the server is the sole route-shape authority.
  const requestPath = u.pathname + u.search;
  const headers = {
    "Content-Type": "application/json",
    "Content-Length": body.length,
    "Authorization": "Bearer " + config.token,
    "Accept": options.sync ? "text/plain" : "application/json",
  };
  const timeouts = config.timeouts || DEFAULT_TIMEOUTS;
  const totalMs = options.sync ? timeouts.syncMs : timeouts.fnfMs;

  return new Promise((resolve) => {
    let settled = false;
    let connectTimer = null;
    let totalTimer = null;
    // Resolve exactly once; clear both timers on every settle path.
    const done = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(connectTimer);
      clearTimeout(totalTimer);
      resolve(result);
    };

    // C2 cert pin (ADR-002 / F1): a TLS request with a configured fingerprint
    // does NOT flush its body (which carries the Bearer token) until the leaf
    // fingerprint is verified on secureConnect. A mismatched/MITM server is
    // destroyed before it ever receives the token.
    const pinned = isTls && !!config.pinnedFp;

    let req;
    try {
      // For an unpinned/plain request applyCertPin is a no-op; for a pinned TLS
      // request it sets rejectUnauthorized:false so the self-signed handshake
      // completes — the fingerprint is then verified manually below.
      const reqOptions = applyCertPin(
        {
          protocol: u.protocol,
          hostname: u.hostname.replace(/^\[|\]$/g, ""), // IPv6 literal: strip brackets
          port: u.port || undefined,
          path: requestPath,
          method: "POST",
          headers,
          agent: false, // fresh socket per request (per-event process semantics)
        },
        isTls,
        config.pinnedFp
      );
      req = mod.request(reqOptions);
    } catch (_err) {
      done(fail("connect", 0));
      return;
    }

    // Connect deadline: armed now, cleared on (secure)connect.
    connectTimer = setTimeout(() => {
      req.destroy();
      done(fail("connect", 0));
    }, timeouts.connectMs);
    if (connectTimer.unref) connectTimer.unref();

    req.on("socket", (s) => {
      if (pinned) {
        // Pinned TLS: verify the leaf fingerprint the instant the handshake
        // completes. On mismatch, destroy the socket and settle as a connect
        // failure BEFORE the body (and its Authorization: Bearer token) is
        // written — the token never reaches a mismatched/MITM server. The body
        // is flushed (req.end) ONLY after a successful pin, so flush ordering is
        // not relied upon: an unwritten request cannot leak the token.
        s.once("secureConnect", () => {
          clearTimeout(connectTimer);
          let err;
          try {
            err = verifyPeerFingerprint(s, config.pinnedFp);
          } catch (_e) {
            err = new Error("pinned certificate verification failed");
          }
          if (err) {
            req.destroy(err);
            done({ ok: false, status: 0, contentType: null, body: null, failureClass: "connect", message: err.message });
            return;
          }
          if (!settled) req.end(body); // pin OK — now flush the token-bearing body
        });
      } else {
        s.once(isTls ? "secureConnect" : "connect", () => clearTimeout(connectTimer));
      }
    });

    // Total deadline.
    totalTimer = setTimeout(() => {
      req.destroy();
      done(fail("timeout", 0));
    }, totalMs);
    if (totalTimer.unref) totalTimer.unref();

    // Also fires after destroy() — once-guarded so the timer classification wins.
    req.on("error", (err) => done(fail(classifyErrno(err), 0)));

    req.on("response", (res) => {
      const chunks = [];
      let received = 0;
      const contentType = res.headers["content-type"] || null;
      res.on("data", (c) => {
        if (received < BODY_LIMIT_BYTES) {
          chunks.push(c);
          received += c.length;
        } else {
          // Oversized response: cap the buffer, classify with what we have,
          // and drop the rest — bounded read, no hang.
          res.destroy();
          done(classifyResponse(res.statusCode, contentType, Buffer.concat(chunks)));
        }
      });
      res.on("end", () =>
        done(classifyResponse(res.statusCode, contentType, Buffer.concat(chunks)))
      );
      res.on("error", () => done(fail("connect", 0)));
      res.on("close", () => {
        // Last-resort settle if the connection drops without end/error.
        done(classifyResponse(res.statusCode, contentType, Buffer.concat(chunks)));
      });
    });

    // Unpinned (plain http or https without a pin): flush immediately. The
    // pinned path defers req.end(body) into the secureConnect handler so the
    // token-bearing body is only ever written after the fingerprint matches.
    if (!pinned) req.end(body);
  });
}

/** Host-only extraction for messages (never the full URL — R-16). */
function safeHost(url) {
  try {
    return new URL(url).host;
  } catch (_err) {
    return "(invalid URL)";
  }
}

/** Compose an actionable failure message for pingForInit. */
function actionable(failureClass, status, host) {
  if (failureClass === "auth") {
    return "token rejected (HTTP " + status + ") — check --token";
  }
  if (failureClass === "connect" || failureClass === "timeout") {
    return "cannot reach " + host + " — check --remote URL";
  }
  return "server returned HTTP " + status;
}

/**
 * Strict Ping/Pong validation for `init --remote` (FR-19 / R-18) — the ONE loud
 * path (ADR-005). Returns { ok, message }; never throws. When `pinnedFp` is set,
 * the Ping runs over the PINNED TLS connection so a cert-fingerprint mismatch
 * surfaces HERE, diagnosably (FR-A11 / AC-CT-ROT), classified as a connect error.
 *
 * ADR-001: `url` is the server-composed `observe_url` (the SAME value every
 * runtime hook posts to). It is passed straight through to `post`, which posts
 * it verbatim — the Ping reaches /v1/{slug}/observe (AC-07) with no re-derivation.
 * @param {string} url server-composed observe_url, posted verbatim.
 * @param {object} [timeouts] { connectMs, syncMs, fnfMs }
 * @param {string} [pinnedFp] sha256:<64 hex> pin for the TLS request.
 */
async function pingForInit(url, token, timeouts, pinnedFp) {
  const host = safeHost(url);
  const res = await post(
    { url, token, timeouts: timeouts || DEFAULT_TIMEOUTS, pinnedFp: pinnedFp || null },
    { type: "Ping" },
    { sync: true }
  );
  if (!res.ok) {
    // A pinned-TLS cert-fingerprint mismatch carries its own diagnosable message
    // (expected-vs-presented + re-bundle guidance, FR-A11 / AC-CT-ROT); surface
    // it verbatim instead of the generic "cannot reach host" line.
    const message = res.message || actionable(res.failureClass, res.status, host);
    return { ok: false, message };
  }
  let obj;
  try {
    obj = JSON.parse(res.body.toString("utf8"));
  } catch (_err) {
    return { ok: false, message: "server returned a non-JSON Ping response" };
  }
  if (!obj || obj.type !== "Pong") {
    return {
      ok: false,
      message: "unexpected response type: " + (obj && obj.type !== undefined ? obj.type : "(none)"),
    };
  }
  const version = obj.server_version !== undefined && obj.server_version !== null
    ? obj.server_version
    : "?";
  return { ok: true, message: "Pong from " + host + " (server " + version + ")" };
}

module.exports = { post, pingForInit, DEFAULT_TIMEOUTS, BODY_LIMIT_BYTES };
