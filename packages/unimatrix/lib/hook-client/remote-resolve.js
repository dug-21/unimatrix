"use strict";

// remote-resolve.js — vnc-050 C9: single credential resolution for the MCP
// bridge and the observe path (ADR-006 #5818, ADR-007 #5817).
// Precedence per consumer: UNIMATRIX_BUNDLE > env pair > credential store.
// Empty string = unset (no trim). No error message ever carries a token or
// header value. Only throws ResolveError.

const { decodeBundle } = require("./bundle");
const credstore = require("./credstore");

const P = "UNIMATRIX_REMOTE_";
const VARS = {
  BUNDLE: "UNIMATRIX_BUNDLE", MCP_URL: P + "MCP_URL", URL: P + "URL",
  TOKEN: P + "TOKEN", FP: P + "FINGERPRINT", HEADERS: P + "HEADERS",
};
const { BUNDLE, MCP_URL, URL, TOKEN, FP, HEADERS } = VARS;
const RESERVED_HEADERS = new Set(["authorization", "mcp-session-id", "host",
  "content-length", "content-type", "transfer-encoding", "connection", "accept"]);

class ResolveError extends Error {
  constructor(code, message, varName) {
    super(message);
    this.name = "ResolveError";
    this.code = code;
    this.varName = varName || null;
  }
}

const nz = (v) => typeof v === "string" && v.length > 0;
const err = (c, m, v) => new ResolveError(c, m, v);
const bad = (m) => err("bad_headers", `${HEADERS}: ${m}`, HEADERS);
const absence = (env, n) => `${n} ${env[n] === "" ? "is empty" : "is not set"}`;

// Values must be HTAB or 0x20-0x7E (ADR-007): Node http mangles 0x80-0xFF as
// latin1 and throws mid-request above 0xFF, so reject before any connection.
// JSON.parse keeps the last of exact-duplicate keys; only case-variant
// duplicates are detectable here. Values never appear in messages.
function parseExtraHeaders(raw) {
  if (!nz(raw)) return [];
  let obj;
  try { obj = JSON.parse(raw); } catch (_) { throw bad("not valid JSON"); }
  if (!obj || typeof obj !== "object" || Array.isArray(obj)) throw bad("must be a JSON object");
  const entries = Object.entries(obj);
  if (entries.length > 32) throw bad("has more than 32 entries");
  const seen = new Set();
  for (const [n, v] of entries) {
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(n)) {
      throw bad(`header name ${JSON.stringify(n.slice(0, 64))} is not a valid token`);
    }
    const lc = n.toLowerCase(), h = `header ${n} `;
    if (RESERVED_HEADERS.has(lc)) throw bad(h + "is reserved and cannot be overridden");
    if (seen.has(lc)) throw bad(h + "is duplicated (case-insensitive)");
    seen.add(lc);
    if (typeof v !== "string") throw bad(h + "value must be a string");
    if (/[\r\n\0]/.test(v)) throw bad(h + "value contains CR, LF or NUL");
    if (/[^\t\x20-\x7e]/.test(v)) throw bad(h + "value must be printable ASCII (HTAB, 0x20-0x7E)");
    if (v.length > 8192) throw bad(h + "value exceeds 8 KiB"); // ASCII: chars == bytes
  }
  return entries;
}

// Returns { mcp, observe, headers, source }; tuples are
// { url, token, pinnedFp, tier } and observe also carries `timeouts`.
function resolveRemote(env, opts) {
  const o = opts || {};
  env = require("../project-state").effectiveEnv( // lazy: config<->project-state cycle
    env || {}, o.cwd || process.cwd(), o.home || require("os").homedir()); // D-091
  const headers = parseExtraHeaders(env[HEADERS]);
  const t = (url, token, pinnedFp, tier) => ({ url, token, pinnedFp, tier });
  const obs = (u, k, f, tier, timeouts) => Object.assign(t(u, k, f, tier), { timeouts: timeouts || null });

  if (nz(env[BUNDLE])) {
    for (const n of [MCP_URL, URL, TOKEN, FP]) {
      if (nz(env[n])) throw err("conflict", `${BUNDLE} cannot be combined with ${n}; unset one of them`, n);
    }
    let b;
    try { b = decodeBundle(env[BUNDLE]); } catch (e) {
      throw err("bad_bundle", `${BUNDLE} is invalid: ${e && e.message}`, BUNDLE);
    }
    const f = nz(b.fp) ? b.fp : null;
    return { mcp: t(b.mcp_url, b.token, f, "bundle"), observe: obs(b.observe_url, b.token, f, "bundle"),
      headers, source: "bundle" };
  }

  const mu = env[MCP_URL], ou = env[URL], tok = env[TOKEN], fp = env[FP];
  if (nz(fp) && !/^sha256:[0-9a-f]{64}$/.test(fp)) {
    throw err("bad_fingerprint", `${FP} must be sha256:<64 lowercase hex>`, FP);
  }
  if (nz(fp) && !nz(mu) && !nz(ou)) {
    throw err("fp_without_url", `${FP} is set but neither ${MCP_URL} nor ${URL} is set`, FP);
  }
  for (const u of [MCP_URL, URL]) { // F1: never send the Bearer token in cleartext
    if (nz(env[u]) && !/^https:\/\//i.test(env[u])) throw err("bad_url", `${u} must be an https:// URL`, u);
  }
  if (nz(tok) && !nz(mu) && !nz(ou)) throw err("partial_pair", `${absence(env, URL)}; ${TOKEN} is set`, URL);
  for (const u of [MCP_URL, URL]) {
    if (nz(env[u]) && !nz(tok)) throw err("partial_pair", `${absence(env, TOKEN)}; ${u} is set`, TOKEN);
  }
  const pf = nz(fp) ? fp : null;
  let mcp = nz(mu) ? t(mu, tok, pf, "env") : null;
  let observe = nz(ou) ? obs(ou, tok, pf, "env") : null;

  if ((!mcp || !observe) && nz(o.projectHash)) {
    let c;
    try { c = (o.readStore || credstore.read)(o.projectHash); } catch (e) {
      throw err("store_malformed", `credential store is unreadable: ${e && e.message}`, null);
    }
    if (c && nz(c.token)) {
      const f = nz(c.fingerprint) ? c.fingerprint : null;
      if (!mcp && nz(c.mcp_url)) mcp = t(c.mcp_url, c.token, f, "store");
      if (!observe && nz(c.observe_url)) observe = obs(c.observe_url, c.token, f, "store", c.timeouts);
    }
  }
  const tiers = new Set([mcp, observe].filter(Boolean).map((x) => x.tier));
  const source = tiers.size === 0 ? "none" : tiers.size > 1 ? "mixed" : [...tiers][0];
  return { mcp, observe, headers, source };
}

module.exports = { resolveRemote, parseExtraHeaders, ResolveError, RESERVED_HEADERS, VARS };
