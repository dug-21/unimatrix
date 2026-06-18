"use strict";

/**
 * credstore.js — sole owner of the out-of-tree per-projectHash credential store
 * (vnc-039 Scope B, ADR-003 + ADR-004). The credential lives at
 * ~/.unimatrix/<projectHash>/remote.json (mode 0600), colocated with the
 * existing per-project state (unimatrix.sock, hook-client/). One path
 * derivation, one canonical schema, one read, one idempotent merge-write — no
 * other module hand-rolls store access; both consumers (C2 bridge, C5 hook
 * client) and the writer (C4 init) go through this module.
 *
 * Pure Node stdlib only (fs, path, os, crypto) — zero runtime deps (AC-02).
 *
 * Canonical schema (ADR-004):
 *   { schema_version:1, mcp_url, observe_url, token, fingerprint, timeouts? }
 * fingerprint may be null on the legacy/unpinned path. timeouts is optional;
 * absent → consumer applies DEFAULT_TIMEOUTS.
 */

const fs = require("fs");
const os = require("os");
const path = require("path");

/** Single source of truth shared by reader + writer (R-13). */
const STORE_SCHEMA_VERSION = 1;
const STORE_FILENAME = "remote.json";
const STORE_MODE = 0o600;

/** @returns {boolean} true iff v is a non-empty string. */
function nonEmptyString(v) {
  return typeof v === "string" && v.length > 0;
}

/** @returns {boolean} true iff v is a plain (non-array, non-null) object. */
function isPlainObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

/**
 * pathFor(projectHash) → string | null
 *
 * Mirrors config.js:socketPathFor exactly (ADR-003): same home derivation, same
 * null-on-no-home posture. The projectHash is a 16-hex SHA-256 (fixed-grammar
 * derived value, no traversal surface) — it is NOT sanitized or normalized.
 * Colocated with unimatrix.sock + hook-client/ under one ~/.unimatrix/<hash>/
 * root.
 */
function pathFor(projectHash) {
  let home;
  try {
    home = os.homedir();
  } catch (_err) {
    return null;
  }
  if (!nonEmptyString(home)) {
    return null;
  }
  return path.join(home, ".unimatrix", String(projectHash), STORE_FILENAME);
}

/**
 * read(projectHash) → object | null
 *
 * Returns the parsed canonical schema object, or null on ENOENT / no homedir
 * (no credential for this project — caller decides fall-through vs loud).
 * THROWS a token-free Error on a non-ENOENT fs error, on malformed JSON, on a
 * non-object root, or on an unknown schema_version (R-13 / ADR-004): an unknown
 * version is TERMINAL and diagnosable, never a silent skip.
 *
 * Does NOT validate field completeness beyond schema_version — each consumer
 * validates the fields it owns, preserving the ENOENT-vs-incomplete distinction
 * the hook client relies on for UDS fall-through.
 *
 * Error messages NEVER contain token or fingerprint values (NFR-06 / R-09); the
 * store path carries no secret and is included for diagnosability.
 */
function read(projectHash) {
  const p = pathFor(projectHash);
  if (p === null) {
    // No homedir: same posture as ENOENT (R-13 §ENOENT).
    return null;
  }

  let raw;
  try {
    raw = fs.readFileSync(p, "utf8");
  } catch (err) {
    if (err && err.code === "ENOENT") {
      return null;
    }
    throw new Error(
      "credential store unreadable at " + p + ": " + (err && err.code)
    );
  }

  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (_err) {
    throw new Error("credential store malformed (invalid JSON) at " + p);
  }

  if (!isPlainObject(parsed)) {
    throw new Error("credential store malformed at " + p);
  }

  // schema_version gate — unknown/missing version is TERMINAL (ADR-004).
  if (parsed.schema_version !== STORE_SCHEMA_VERSION) {
    throw new Error(
      "credential store schema_version " +
        String(parsed.schema_version) +
        " unsupported (this client supports " +
        STORE_SCHEMA_VERSION +
        "); re-run init"
    );
  }

  return parsed;
}

/**
 * write(projectHash, cred, { dryRun }) → string[]
 *
 * Idempotent merge-write of the canonical schema at mode 0600. cred =
 * { mcp_url, observe_url, token, fingerprint, timeouts? } (fingerprint may be
 * null for legacy). Returns an actions array (mirrors init.js action strings;
 * NEVER carries token/fingerprint — NFR-06). THROWS on no-homedir or write
 * failure: credentials must persist (init exits 1, R-12). chmod failure is
 * swallowed (best-effort; Windows/unsupported fs).
 *
 * Merge posture (ADR-003 / ADR-004): the four canonical content fields plus
 * schema_version are always (over)written; unknown future fields in a readable
 * existing entry survive via Object.assign. A malformed existing file is
 * replaced by a valid one (recovery). Per-project directory separation means
 * two projects never share a file — no cross-project map merge (AC-08b).
 */
function write(projectHash, cred, opts) {
  const dryRun = !!(opts && opts.dryRun === true);
  const actions = [];
  const p = pathFor(projectHash);
  if (p === null) {
    throw new Error("cannot resolve credential store path (no home directory)");
  }

  // Idempotent merge: preserve unknown fields of a readable existing entry, but
  // tolerate absent/malformed by starting fresh (a write replaces a bad file).
  let existing = {};
  try {
    existing = read(projectHash) || {};
  } catch (_err) {
    existing = {};
  }

  const merged = Object.assign({}, existing, {
    schema_version: STORE_SCHEMA_VERSION,
    mcp_url: cred.mcp_url,
    observe_url: cred.observe_url,
    token: cred.token,
    fingerprint: cred.fingerprint === undefined ? null : cred.fingerprint,
  });

  if (isPlainObject(cred.timeouts)) {
    merged.timeouts = cred.timeouts;
  }
  // else: leave merged.timeouts as-is (absent → consumer DEFAULT_TIMEOUTS).

  if (dryRun) {
    actions.push(
      "[dry-run] Would write credential store " + p + " (mode 0600)"
    );
    return actions;
  }

  // Write at 0600, then re-assert mode (writeRemoteSettingsLocal pattern,
  // init.js): guarantees 0600 even if the file pre-existed with looser perms.
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, JSON.stringify(merged, null, 2) + "\n", {
    mode: STORE_MODE,
  });
  try {
    fs.chmodSync(p, STORE_MODE);
  } catch (_err) {
    // best-effort (Windows / unsupported fs); must not abort the write.
  }

  actions.push("Wrote credential store " + p + " (mode 0600)");
  return actions;
}

module.exports = { pathFor, read, write, STORE_SCHEMA_VERSION };
