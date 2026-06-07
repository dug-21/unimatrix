"use strict";

/**
 * config.js — Spawn-time config resolution, project root walk, hash, state dir.
 *
 * ADR-006: env vars UNIMATRIX_REMOTE_URL/UNIMATRIX_REMOTE_TOKEN win outright
 * (partial pair = misconfiguration); otherwise exactly ONE file is read:
 * {project_root}/.claude/settings.local.json, key unimatrix.remote. The SAME
 * projectRoot string feeds the config lookup and the state-dir hash (ADR-003) so
 * config identity and state identity never disagree.
 *
 * Hash parity oracle: project.rs::compute_project_hash (first 16 hex of SHA-256
 * over the UTF-8 path string). Never throws; no network I/O.
 */

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

const ENV_URL = "UNIMATRIX_REMOTE_URL"; // pinned at gate (Delivery Notes 4)
const ENV_TOKEN = "UNIMATRIX_REMOTE_TOKEN";

// ADR-005 defaults: connect 750 ms / sync 2,000 ms / fire-and-forget 3,000 ms.
const DEFAULT_TIMEOUTS = Object.freeze({
  connectMs: 750,
  syncMs: 2000,
  fnfMs: 3000,
});

const TIMEOUT_KEY_MAP = [
  ["connect_ms", "connectMs"],
  ["sync_ms", "syncMs"],
  ["fnf_ms", "fnfMs"],
];

const MAX_TIMEOUT_MS = 600000;

/**
 * Non-throwing port of init.js detectProjectRoot / Rust detect_project_root:
 * walk up from startDir to the first directory containing `.git` (dir OR file —
 * worktrees). No `.git` found → resolved startDir (ADR-006).
 *
 * Divergence from project.rs: Rust resolves `.git` worktree FILES to the real
 * gitdir; this walk stops at the containing directory. The hash is client-only
 * (state-dir identity), so worktree users get a per-worktree state dir.
 */
function walkToProjectRoot(startDir) {
  let current;
  try {
    current = path.resolve(startDir);
  } catch (_err) {
    return startDir;
  }
  for (;;) {
    try {
      if (fs.existsSync(path.join(current, ".git"))) {
        return current;
      }
    } catch (_err) {
      // non-throwing contract (existsSync should not throw)
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return path.resolve(startDir); // no .git anywhere → resolved cwd
    }
    current = parent;
  }
}

/**
 * project.rs::compute_project_hash: first 16 hex of SHA-256 over the UTF-8 path
 * string from walkToProjectRoot (native separators, no trailing slash).
 */
function computeProjectHash(projectRoot) {
  return crypto
    .createHash("sha256")
    .update(String(projectRoot), "utf8")
    .digest("hex")
    .slice(0, 16);
}

/**
 * Merge config timeout overrides over ADR-005 defaults. Keys:
 * unimatrix.remote.timeouts.{connect_ms,sync_ms,fnf_ms}. Invalid values ignored
 * (fail-open) — only finite numbers in [1, 600000] apply.
 */
function mergeTimeouts(t) {
  const out = {
    connectMs: DEFAULT_TIMEOUTS.connectMs,
    syncMs: DEFAULT_TIMEOUTS.syncMs,
    fnfMs: DEFAULT_TIMEOUTS.fnfMs,
  };
  if (t !== null && typeof t === "object" && !Array.isArray(t)) {
    for (const [src, dst] of TIMEOUT_KEY_MAP) {
      const v = t[src];
      if (typeof v === "number" && Number.isFinite(v) && v >= 1 && v <= MAX_TIMEOUT_MS) {
        out[dst] = Math.floor(v);
      }
    }
  }
  return out;
}

/** @returns {boolean} true iff v is a non-empty string. */
function nonEmpty(v) {
  return typeof v === "string" && v.length > 0;
}

/**
 * Host of a URL for the content-free breadcrumb. Parse failure → "" — not
 * rejected here; transport.post classifies it "connect" at send time (fail-open).
 */
function safeHostOf(url) {
  try {
    return new URL(url).host;
  } catch (_err) {
    return "";
  }
}

/**
 * State dir per ADR-003: ~/.unimatrix/{hash}/hook-client. os.homedir() throwing
 * or empty (no HOME) → null; state.js treats null as "persistence disabled,
 * sends still attempted".
 */
function stateDirFor(projectHash) {
  let home;
  try {
    home = os.homedir();
  } catch (_err) {
    return null;
  }
  if (!nonEmpty(home)) {
    return null;
  }
  return path.join(home, ".unimatrix", projectHash, "hook-client");
}

/** Build a successful ResolvedConfig (url, token, timeouts, source, root, hash, stateDir). */
function ok(url, token, timeouts, source, projectRoot, projectHash, stateDir) {
  return {
    ok: true,
    url,
    token,
    timeouts,
    source,
    projectRoot,
    projectHash,
    stateDir,
    urlHost: safeHostOf(url),
  };
}

/**
 * Resolve remote config per ADR-006 (first hit wins):
 *   1. env pair (partial pair = misconfiguration, reason "partial_env")
 *   2. {project_root}/.claude/settings.local.json → unimatrix.remote
 *   3. neither → { ok:false, reason:"missing" } (caller breadcrumbs + exits 0)
 * Never throws. No network I/O. Single file read, no probing.
 *
 * @param {string} cwd - resolved cwd (stdin.cwd if non-empty, else process.cwd()).
 * @returns {object} ResolvedConfig — { ok:true, ... } or { ok:false, reason,
 *   projectRoot, projectHash, stateDir }.
 */
function resolve(cwd) {
  const startDir = nonEmpty(cwd) ? cwd : safeProcessCwd();
  const projectRoot = walkToProjectRoot(startDir);
  const projectHash = computeProjectHash(projectRoot);
  const stateDir = stateDirFor(projectHash);

  const envUrl = process.env[ENV_URL];
  const envTok = process.env[ENV_TOKEN];
  if (nonEmpty(envUrl) && nonEmpty(envTok)) {
    // Env wins outright; the file is never consulted for url/token.
    return ok(envUrl, envTok, mergeTimeouts(null), "env", projectRoot, projectHash, stateDir);
  }
  if (nonEmpty(envUrl) || nonEmpty(envTok)) {
    // ADR-006: partial pair = misconfiguration (breadcrumb class "auth", exit 0).
    return { ok: false, reason: "partial_env", projectRoot, projectHash, stateDir };
  }

  // Single file, single read — no probing (ADR-006).
  const filePath = path.join(projectRoot, ".claude", "settings.local.json");
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (err) {
    const reason = err && err.code === "ENOENT" ? "missing" : "malformed";
    return { ok: false, reason, projectRoot, projectHash, stateDir };
  }

  const remote =
    parsed !== null && typeof parsed === "object" && parsed.unimatrix !== null &&
    typeof parsed.unimatrix === "object"
      ? parsed.unimatrix.remote
      : undefined;
  if (
    remote === null ||
    typeof remote !== "object" ||
    Array.isArray(remote) ||
    !nonEmpty(remote.url) ||
    !nonEmpty(remote.token)
  ) {
    // File present but key absent/incomplete → same as missing.
    return { ok: false, reason: "missing", projectRoot, projectHash, stateDir };
  }

  const timeouts = mergeTimeouts(remote.timeouts);
  return ok(remote.url, remote.token, timeouts, "file", projectRoot, projectHash, stateDir);
}

/** process.cwd() guarded — non-throwing module contract. */
function safeProcessCwd() {
  try {
    return process.cwd();
  } catch (_err) {
    return ".";
  }
}

module.exports = {
  ENV_URL,
  ENV_TOKEN,
  DEFAULT_TIMEOUTS,
  resolve,
  walkToProjectRoot,
  computeProjectHash,
  mergeTimeouts,
  safeHostOf,
};
