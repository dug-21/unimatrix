"use strict";

// tls-guard.js — vnc-050 C10 (ADR-006 #5818, ADR-007 #5817). CA-trust branch
// guard + extra-header merge shared by the bridge and the observe transport.
// cert-pin.js stays untouched: pinned trust paths are byte-identical.

const { RESERVED_HEADERS } = require("./remote-resolve.js");

// Unpinned TLS must carry explicit rejectUnauthorized:true (per-connection,
// overrides global NODE_TLS_REJECT_UNAUTHORIZED=0; ADR-006 #5829): throw
// before any socket otherwise, or on a hostname override (SR-01, R-01.4).
function assertCaOptions(o) {
  if (o.rejectUnauthorized !== true) throw new Error("internal: unpinned request must set rejectUnauthorized:true");
  if ("checkServerIdentity" in o) throw new Error("internal: unpinned request must not override hostname verification");
  return o;
}

// Unpinned TLS: verified iff Node marked the socket authorized.
const caVerifyError = (s) => (s.authorized === true ? null : new Error("TLS certificate verification failed"));

// Caller headers first; the caller then sets reserved headers, which win.
// Reserved names are skipped as defense in depth (parser already rejects).
function applyExtraHeaders(target, extra) {
  for (const [k, v] of extra || []) if (!RESERVED_HEADERS.has(String(k).toLowerCase())) target[k] = v;
  return target;
}

const isTlsVerifyCode = (c) => typeof c === "string" && (c.startsWith("ERR_TLS_") || /CERT|SELF_SIGNED|UNABLE_TO_VERIFY/.test(c));

module.exports = { assertCaOptions, caVerifyError, applyExtraHeaders, isTlsVerifyCode };
