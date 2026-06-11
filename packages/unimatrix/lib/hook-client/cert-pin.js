"use strict";

// cert-pin.js — C2 cert-fingerprint pinning (ADR-002, R-02). The OSS trust model
// is fingerprint pinning, NOT CA-trust / SAN hostname validation. A custom
// checkServerIdentity computes sha256(leaf DER) and compares it to the pinned
// `fp` from the bundle. The leaf DER is `cert.raw` — the SAME bytes rustls
// served (ADR-002).
//
// computeFingerprint MIRRORS the Rust fingerprint_leaf_der: "sha256:" +
// lowercase_hex(sha256(der)). Parity is proven against the committed Rust-oracle
// corpus (never hand-written — SR-02); see the parity test.

const crypto = require("crypto");

/**
 * Compute the C2 fingerprint of a leaf DER buffer. Mirrors the Rust
 * fingerprint_leaf_der oracle byte-for-byte.
 *
 * @param {Buffer} derBuffer - The leaf certificate DER bytes.
 * @returns {string} "sha256:" + 64 lowercase hex chars.
 */
function computeFingerprint(derBuffer) {
  const hex = crypto.createHash("sha256").update(derBuffer).digest("hex");
  return "sha256:" + hex;
}

/**
 * Build a custom `checkServerIdentity(host, cert)` that pins the served leaf
 * cert against `pinnedFp`. Returns the Node convention: an Error to REJECT,
 * `undefined` to ACCEPT. CA-chain validation is bypassed by design (self-signed,
 * no CA path — ADR-002); this function consults no CA trust store.
 *
 * A mismatch yields a CLEAN, DIAGNOSABLE error (FR-A11 / AC-CT-ROT) naming the
 * expected and presented `sha256:` fingerprints and pointing at re-bundle —
 * never a bare opaque TLS handshake error.
 *
 * @param {string} pinnedFp - The bundle `fp` (sha256:<64 hex>).
 * @returns {function(string, object): (Error|undefined)}
 */
function makeCheckServerIdentity(pinnedFp) {
  return function checkServerIdentity(_host, cert) {
    if (!cert || !cert.raw) {
      return new Error("no certificate presented by server");
    }
    const presented = computeFingerprint(cert.raw);
    if (presented !== pinnedFp) {
      return new Error(
        "pinned certificate fingerprint mismatch — the server cert was " +
          "likely rotated.\n" +
          "  expected (pinned):  " +
          pinnedFp +
          "\n" +
          "  presented (server): " +
          presented +
          "\n" +
          "  Fix: re-run `unimatrix client-bundle` on the server and re-run " +
          "`init --remote <new-bundle>`."
      );
    }
    return undefined; // accept (Node convention)
  };
}

/**
 * Thread the pin into HTTPS request options for a TLS request. No-op for
 * non-TLS or when no pin is configured. The pin IS the trust model, so CA trust
 * is cleared (`ca: undefined`) while `rejectUnauthorized` stays true to keep
 * TLS-level errors meaningful.
 *
 * @param {object} options - The https.request options being assembled (mutated).
 * @param {boolean} isTls - True iff the request is over https:.
 * @param {string|null|undefined} pinnedFp - The pinned fingerprint, if any.
 * @returns {object} The same options object, for chaining.
 */
function applyCertPin(options, isTls, pinnedFp) {
  if (isTls && pinnedFp) {
    options.rejectUnauthorized = true;
    options.checkServerIdentity = makeCheckServerIdentity(pinnedFp);
    options.ca = undefined; // no CA trust path; the pin is the trust model
  }
  return options;
}

module.exports = {
  computeFingerprint,
  makeCheckServerIdentity,
  applyCertPin,
};
