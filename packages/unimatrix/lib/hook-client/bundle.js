"use strict";

// bundle.js — v:2 connection-bundle decoder (ADR-002 #5081, ADR-001 #5080).
// Mirrors the Rust encoder in client_bundle.rs EXACTLY; the wire form is the
// cross-stack contract and the parity corpus is the shared oracle (R-03):
//
//   unimatrix-bundle:<base64url(canonical-json)>      (single line, no padding)
//   canonical JSON = {"v":2,"mcp_url":..,"observe_url":..,"token":..,"fp":..}
//                    (fixed field order = Rust Bundle struct declaration order)
//
// The server composes BOTH URLs (mcp_url, observe_url) from {public_base} + the
// route grammar; the client posts them VERBATIM (dumb-client invariant, ADR-001).
// The client derives NO slug and composes NO path — assertSlugAllowlist/SLUG_RE
// are retired for the bundle path.
//
// This is a TRUST BOUNDARY: `raw` is untrusted operator paste. Guard ordering is
// LOCKED (ADR-002, preserved from v:1):
//   GUARD 1 — 4 KB RAW-string byte-length cap, BEFORE decode/parse (DoS pre-
//             filter; an over-cap non-base64url string rejects on LENGTH, not a
//             decode error).
//   GUARD 5 — strict schema reject is the LOAD-BEARING guard: exactly the five
//             keys {v,mcp_url,observe_url,token,fp}, v===2, https URLs, correct
//             token/fp grammar; any missing / extra / wrong-type / wrong-version
//             field is a hard reject. A v:1 bundle fails closed here (R-04).
//
// Error messages NEVER include the token (NFR-06).

const BUNDLE_SCHEME = "unimatrix-bundle:";
const MAX_RAW_LEN = 4096; // bytes, RAW string, BEFORE decode/parse
const TOKEN_RE = /^[0-9a-f]{64}$/;
const FP_RE = /^sha256:[0-9a-f]{64}$/;
const EXPECTED_KEYS = ["v", "mcp_url", "observe_url", "token", "fp"];

/**
 * BundleError — a distinct error type so `init`'s catch can present bundle
 * parse failures cleanly. The message is operator-facing and token-free.
 */
class BundleError extends Error {
  constructor(message) {
    super(message);
    this.name = "BundleError";
  }
}

/** True iff `keys` is exactly EXPECTED_KEYS (order-independent, no extras). */
function keysAreExactly(keys) {
  if (keys.length !== EXPECTED_KEYS.length) {
    return false;
  }
  for (const k of EXPECTED_KEYS) {
    if (!keys.includes(k)) {
      return false;
    }
  }
  return true;
}

/**
 * Decode a `unimatrix-bundle:` string into a validated v:2 bundle. Throws
 * BundleError on any guard failure. The token never appears in any thrown
 * message (NFR-06). The returned URLs are byte-equal to the payload and are
 * posted verbatim by the caller (ADR-001).
 *
 * @param {string} raw - The raw pasted bundle string.
 * @returns {{v:2, mcp_url:string, observe_url:string, token:string, fp:string}}
 */
function decodeBundle(raw) {
  if (typeof raw !== "string") {
    throw new BundleError("bundle must be a string");
  }

  // GUARD 1 — LENGTH CAP FIRST on the RAW pasted string, BEFORE decode/parse.
  // Must reject even when the input is not valid base64url: the cap guards
  // against decoding/parsing an unbounded paste, so it runs before that work,
  // classifying purely on byte length.
  if (Buffer.byteLength(raw, "utf8") > MAX_RAW_LEN) {
    throw new BundleError("bundle too long (> 4 KB) — refusing to decode");
  }

  // GUARD 2 — scheme prefix (self-identification; reject bare URL/token).
  if (!raw.startsWith(BUNDLE_SCHEME)) {
    throw new BundleError(
      "not a unimatrix bundle (missing 'unimatrix-bundle:' prefix)"
    );
  }
  const body = raw.slice(BUNDLE_SCHEME.length);

  // GUARD 3 — base64url decode (no padding). Node's "base64url" is lenient, so
  // a round-trip re-encode rejects smuggled non-alphabet characters; the strict
  // schema (GUARD 5) is the load-bearing guard, this is a cheap early reject.
  let jsonStr;
  try {
    const bytes = Buffer.from(body, "base64url");
    jsonStr = bytes.toString("utf8");
    if (bytes.toString("base64url") !== body.replace(/=+$/, "")) {
      throw new BundleError("bundle payload is not valid base64url");
    }
  } catch (err) {
    if (err instanceof BundleError) {
      throw err;
    }
    throw new BundleError("bundle payload is not valid base64url");
  }

  // GUARD 4 — JSON parse.
  let obj;
  try {
    obj = JSON.parse(jsonStr);
  } catch (_err) {
    throw new BundleError("bundle payload is not valid JSON");
  }
  if (obj === null || typeof obj !== "object" || Array.isArray(obj)) {
    throw new BundleError("bundle payload is not a JSON object");
  }

  // GUARD 5 — STRICT SCHEMA (LOAD-BEARING): EXACTLY {v,mcp_url,observe_url,token,fp}.
  if (!keysAreExactly(Object.keys(obj))) {
    throw new BundleError(
      "bundle has unexpected fields " +
        "(expected exactly v, mcp_url, observe_url, token, fp)"
    );
  }
  if (obj.v !== 2) {
    // R-04: a v:1 (or any unknown major) bundle lands here. Actionable message
    // telling the operator to re-issue — never a silent/opaque throw.
    throw new BundleError(
      "unsupported bundle version: " +
        String(obj.v) +
        " — re-issue a v:2 bundle with `unimatrix client-bundle <slug>`"
    );
  }
  if (typeof obj.mcp_url !== "string" || !obj.mcp_url.startsWith("https://")) {
    throw new BundleError("mcp_url must be an https URL");
  }
  if (
    typeof obj.observe_url !== "string" ||
    !obj.observe_url.startsWith("https://")
  ) {
    throw new BundleError("observe_url must be an https URL");
  }
  if (typeof obj.token !== "string" || !TOKEN_RE.test(obj.token)) {
    // never echo the token (NFR-06).
    throw new BundleError("token must be 64 lowercase hex chars");
  }
  if (typeof obj.fp !== "string" || !FP_RE.test(obj.fp)) {
    throw new BundleError("fp must be sha256:<64 hex>");
  }

  return {
    v: 2,
    mcp_url: obj.mcp_url,
    observe_url: obj.observe_url,
    token: obj.token,
    fp: obj.fp,
  };
}

module.exports = {
  decodeBundle,
  BundleError,
  BUNDLE_SCHEME,
  MAX_RAW_LEN,
};
