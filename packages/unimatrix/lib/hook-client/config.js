"use strict";

/**
 * config.js — spawn-time config resolution, project root walk, hash, state dir.
 * ADR-006: env pair UNIMATRIX_REMOTE_URL/_TOKEN wins outright (partial pair =
 * misconfig); else ONE file: {root}/.claude/settings.local.json key
 * unimatrix.remote. The SAME projectRoot feeds config lookup and state-dir
 * hash (ADR-003). Hash oracle: project.rs::compute_project_hash. Never
 * throws; no network I/O.
 */

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

const ENV_URL = "UNIMATRIX_REMOTE_URL"; // pinned at gate
const ENV_TOKEN = "UNIMATRIX_REMOTE_TOKEN";

// ADR-005 defaults: connect / sync / fire-and-forget, in ms.
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
 * Non-throwing port of Rust detect_project_root: walk up to the first dir
 * with `.git`. Dir → root; FILE (worktree) → resolveGitFile → MAIN repo root.
 * None → resolved startDir (ADR-006). Realpath'd like `.canonicalize()` —
 * else a symlink alias (macOS /var) hashes to TWO state dirs and queued
 * frames are never replayed.
 */
function walkToProjectRoot(startDir) {
  let current;
  try {
    current = path.resolve(startDir);
  } catch (_err) {
    return startDir;
  }
  for (;;) {
    let st = null;
    try {
      st = fs.statSync(path.join(current, ".git"));
    } catch (_err) {
      // .git absent — keep walking
    }
    if (st) {
      return st.isFile()
        ? resolveGitFile(path.join(current, ".git"), current)
        : realpathOrSelf(current);
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return realpathOrSelf(path.resolve(startDir)); // no .git anywhere → resolved cwd
    }
    current = parent;
  }
}

/**
 * project.rs::resolve_git_file port. A `.git` FILE marks a worktree; its
 * `gitdir:` line points at <main>/.git/worktrees/<name> (relative paths
 * resolve against the containing dir). Realpath the target, walk UP to the
 * `.git` DIRECTORY ancestor, return its parent (realpath'd) — the main repo
 * root — so every worktree shares ONE hash and ONE settings.local.json. ANY
 * failure (unreadable, no gitdir line, dangling target, no .git-dir ancestor)
 * → realpath of the containing dir (project.rs:112-113; Rust errors on a
 * missing gitdir line, hook.rs then uses raw cwd — benign divergence).
 */
function resolveGitFile(gitFile, worktreeDir) {
  try {
    const line = fs
      .readFileSync(gitFile, "utf8")
      .split("\n")
      .find((l) => l.startsWith("gitdir:"));
    if (line) {
      const raw = line.slice("gitdir:".length).trim();
      let ancestor = fs.realpathSync(
        path.isAbsolute(raw) ? raw : path.join(worktreeDir, raw)
      );
      for (;;) {
        if (path.basename(ancestor) === ".git" && fs.statSync(ancestor).isDirectory()) {
          return realpathOrSelf(path.dirname(ancestor));
        }
        const parent = path.dirname(ancestor);
        if (parent === ancestor) break;
        ancestor = parent;
      }
    }
  } catch (_err) {
    // fall through
  }
  return realpathOrSelf(worktreeDir);
}

/**
 * fs.realpathSync with non-throwing fallback — project.rs propagates
 * canonicalize errors; the fail-open JS contract returns the input instead.
 */
function realpathOrSelf(p) {
  try {
    return fs.realpathSync(p);
  } catch (_err) {
    return p;
  }
}

/**
 * project.rs::compute_project_hash: first 16 hex of SHA-256 over the UTF-8
 * path from walkToProjectRoot (native separators, no trailing slash).
 */
function computeProjectHash(projectRoot) {
  return crypto
    .createHash("sha256")
    .update(String(projectRoot), "utf8")
    .digest("hex")
    .slice(0, 16);
}

/**
 * Merge unimatrix.remote.timeouts.{connect_ms,sync_ms,fnf_ms} over ADR-005
 * defaults. Invalid values ignored — only finite numbers in [1, 600000] apply.
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

/** URL host for the breadcrumb; parse failure → "" (transport classifies). */
function safeHostOf(url) {
  try {
    return new URL(url).host;
  } catch (_err) {
    return "";
  }
}

/**
 * ADR-003 state dir: ~/.unimatrix/{hash}/hook-client. No/empty homedir →
 * null (state.js: persistence disabled, sends still attempted).
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

/** Build a successful ResolvedConfig. */
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
 * Resolve remote config per ADR-006 (first hit wins): 1. env pair (partial →
 * "partial_env") 2. {root}/.claude/settings.local.json → unimatrix.remote
 * 3. neither → { ok:false, reason:"missing" }. Never throws; no network; one
 * file read. @param {string} cwd stdin.cwd if non-empty, else process.cwd().
 */
function resolve(cwd) {
  const startDir = nonEmpty(cwd) ? cwd : safeProcessCwd();
  const projectRoot = walkToProjectRoot(startDir);
  const projectHash = computeProjectHash(projectRoot);
  const stateDir = stateDirFor(projectHash);

  const envUrl = process.env[ENV_URL];
  const envTok = process.env[ENV_TOKEN];
  if (nonEmpty(envUrl) && nonEmpty(envTok)) {
    // Env wins outright; file never consulted.
    return ok(envUrl, envTok, mergeTimeouts(null), "env", projectRoot, projectHash, stateDir);
  }
  if (nonEmpty(envUrl) || nonEmpty(envTok)) {
    // ADR-006: partial pair = misconfig (breadcrumb "auth", exit 0).
    return { ok: false, reason: "partial_env", projectRoot, projectHash, stateDir };
  }

  // Single read, no probing (ADR-006).
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
    // Key absent/incomplete → same as missing.
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
  resolveGitFile,
  computeProjectHash,
  mergeTimeouts,
  safeHostOf,
};
