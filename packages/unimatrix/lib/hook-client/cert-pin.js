"use strict";

// cert-pin.js — C2 cert-fingerprint pinning (ADR-002, R-02). The OSS trust model
// is fingerprint pinning, NOT CA-trust / SAN hostname validation. We hold only
// the pinned `fp` from the bundle (NOT the cert), so CA-trust cannot be used and
// `checkServerIdentity` is the wrong hook: Node runs CA-chain verification FIRST
// and rejects a self-signed leaf (DEPTH_ZERO_SELF_SIGNED_CERT) before any
// identity callback fires. Instead we let the self-signed handshake COMPLETE
// (`rejectUnauthorized: false`) and verify the leaf fingerprint MANUALLY on the
// TLS `secureConnect` event via verifyPeerFingerprint — destroying the socket on
// mismatch BEFORE the request body (and its Bearer token) is written.
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
 * Build the CLEAN, DIAGNOSABLE mismatch Error (FR-A11 / AC-CT-ROT) naming the
 * expected and presented `sha256:` fingerprints and pointing at re-bundle —
 * never a bare opaque TLS handshake error.
 *
 * @param {string} pinnedFp   - The bundle `fp` (expected).
 * @param {string} presented  - The fingerprint the server actually served.
 * @returns {Error}
 */
function mismatchError(pinnedFp, presented) {
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

/**
 * Verify a live TLS peer's leaf certificate against the pinned fingerprint.
 * Called from the `secureConnect` handler once the (self-signed) handshake has
 * COMPLETED with `rejectUnauthorized: false`. Reads the leaf DER via
 * `socket.getPeerCertificate(true).raw` — the SAME bytes rustls served
 * (ADR-002) — computes sha256, and compares to `pinnedFp`.
 *
 * @param {import("tls").TLSSocket} socket - The connected TLS socket.
 * @param {string} pinnedFp - The pinned fingerprint (sha256:<64 hex>).
 * @returns {Error|null} `null` on match; a diagnosable Error on mismatch or when
 *   no certificate was presented.
 */
function verifyPeerFingerprint(socket, pinnedFp) {
  let cert;
  try {
    cert = socket && typeof socket.getPeerCertificate === "function"
      ? socket.getPeerCertificate(true)
      : null;
  } catch (_err) {
    return new Error("no certificate presented by server");
  }
  if (!cert || !cert.raw || cert.raw.length === 0) {
    return new Error("no certificate presented by server");
  }
  const presented = computeFingerprint(cert.raw);
  if (presented !== pinnedFp) {
    return mismatchError(pinnedFp, presented);
  }
  return null; // match
}

/**
 * Build a custom `checkServerIdentity(host, cert)` that pins the served leaf
 * cert against `pinnedFp`. Returns the Node convention: an Error to REJECT,
 * `undefined` to ACCEPT.
 *
 * NOTE: this is retained for direct/unit verification of the compare logic. It
 * is NOT the live-handshake mechanism — with a self-signed leaf Node never
 * reaches `checkServerIdentity` (chain verification fails first under
 * rejectUnauthorized:true, and the callback is skipped entirely under
 * rejectUnauthorized:false). The live path uses verifyPeerFingerprint on
 * `secureConnect`. See ADR-002 and the F1 security finding.
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
      return mismatchError(pinnedFp, presented);
    }
    return undefined; // accept (Node convention)
  };
}

/**
 * Thread the pin into HTTPS request options for a TLS request. No-op for
 * non-TLS or when no pin is configured.
 *
 * The pin IS the trust model and we hold only the fingerprint, not the cert, so
 * CA-trust cannot be used. We set `rejectUnauthorized = false` so the
 * self-signed handshake COMPLETES; the fingerprint is then verified MANUALLY in
 * the transport's `secureConnect` handler via verifyPeerFingerprint, which
 * destroys the socket on mismatch before any token is written. (Setting
 * `rejectUnauthorized = true` with `ca: undefined` rejects the legitimate
 * self-signed leaf before the pin ever runs — the F1 defect.)
 *
 * @param {object} options - The https.request options being assembled (mutated).
 * @param {boolean} isTls - True iff the request is over https:.
 * @param {string|null|undefined} pinnedFp - The pinned fingerprint, if any.
 * @returns {object} The same options object, for chaining.
 */
function applyCertPin(options, isTls, pinnedFp) {
  if (isTls && pinnedFp) {
    // Complete the self-signed handshake; trust is established by the manual
    // secureConnect fingerprint check, not by the CA chain.
    options.rejectUnauthorized = false;
    options.ca = undefined; // no CA trust path; the pin is the trust model
  }
  return options;
}

module.exports = {
  computeFingerprint,
  makeCheckServerIdentity,
  verifyPeerFingerprint,
  applyCertPin,
};
