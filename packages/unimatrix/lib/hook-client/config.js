"use strict";

/**
 * config.js — spawn-time config + transport resolution, project root walk,
 * hash, state dir, socket path. ADR-002 §3 / vnc-039 ADR-004: env pair
 * UNIMATRIX_REMOTE_URL/_TOKEN → http unpinned (partial pair = terminal
 * misconfig); else the out-of-tree store ~/.unimatrix/<projectHash>/remote.json
 * (credstore.read) → http to observe_url, PINNED on the store fingerprint; else
 * local "uds" mode with a derived socketPath (ADR-007). The terminal "missing"
 * path is retired. The SAME projectRoot feeds config lookup, state-dir hash, and
 * socketPath (ADR-003, ADR-007 single derivation). Hash oracle:
 * project.rs::compute_project_hash. Never throws; no network I/O.
 */

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

const credstore = require("./credstore.js");

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

/**
 * ADR-007 §1 UDS socket path: ~/.unimatrix/{hash}/unimatrix.sock. SAME home +
 * projectHash root as stateDirFor — single derivation, so state dir and socket
 * path can never disagree (invariant: dirname(socketPath)===dirname(stateDir)).
 * No/empty homedir → null (no socket can be derived → honest terminal misconfig).
 */
function socketPathFor(projectHash) {
  let home;
  try {
    home = os.homedir();
  } catch (_err) {
    return null;
  }
  if (!nonEmpty(home)) {
    return null;
  }
  return path.join(home, ".unimatrix", projectHash, "unimatrix.sock");
}

/**
 * Build a successful HTTP-mode ResolvedConfig (ADR-002 §3). `pinnedFp` is the
 * LAST positional arg (vnc-039 ADR-004): env-path omits it → null (unpinned by
 * design); the file path passes the store `fingerprint`. transport-http.post
 * reads config.pinnedFp to gate the pinned flush, so populating it here is what
 * makes file-mode remote observe run over pinned HTTPS (the break fix, R-06).
 */
function okHttp(url, token, timeouts, source, projectRoot, projectHash, stateDir, pinnedFp) {
  return {
    ok: true,
    mode: "http",
    url,
    token,
    timeouts,
    source,
    projectRoot,
    projectHash,
    stateDir,
    urlHost: safeHostOf(url),
    pinnedFp: pinnedFp || null,
  };
}

/**
 * Build a successful UDS-mode ResolvedConfig (ADR-002 §3, ADR-007). urlHost ""
 * (no remote host) keeps state.recordSendOutcomes/breadcrumbs working unchanged.
 */
function okUds(socketPath, projectRoot, projectHash, stateDir) {
  return {
    ok: true,
    mode: "uds",
    socketPath,
    source: "local",
    projectRoot,
    projectHash,
    stateDir,
    urlHost: "",
  };
}

/**
 * UDS fall-through (ADR-002 §3, ADR-007): no remote config → local mode.
 * Derives socketPath from the SAME projectHash as stateDir. No HOME → cannot
 * derive a socket → honest terminal "malformed" (not the retired "missing").
 */
function resolveUds(projectRoot, projectHash, stateDir) {
  const socketPath = socketPathFor(projectHash);
  if (socketPath === null) {
    return { ok: false, reason: "malformed", projectRoot, projectHash, stateDir };
  }
  return okUds(socketPath, projectRoot, projectHash, stateDir);
}

/**
 * Resolve transport config (ADR-002 §3, vnc-039 ADR-004, first hit wins):
 * 1. env pair → http, UNPINNED (partial → terminal "partial_env")
 * 2. out-of-tree store ~/.unimatrix/<projectHash>/remote.json (credstore.read)
 *    → http to observe_url, PINNED on fingerprint (malformed/unknown
 *    schema_version → terminal "malformed"; ENOENT/incomplete → UDS fall-through)
 * 3. no remote config → local "uds" mode with a derived socketPath.
 * The terminal "missing" breadcrumb is retired: absent config means local UDS,
 * not a failure. Never throws; no network; one store read.
 * @param {string} cwd stdin.cwd if non-empty.
 */
function resolve(cwd) {
  const startDir = nonEmpty(cwd) ? cwd : safeProcessCwd();
  const projectRoot = walkToProjectRoot(startDir);
  const projectHash = computeProjectHash(projectRoot);
  const stateDir = stateDirFor(projectHash);

  const envUrl = process.env[ENV_URL];
  const envTok = process.env[ENV_TOKEN];
  if (nonEmpty(envUrl) && nonEmpty(envTok)) {
    // Env pair → HTTP, wins outright even if a local socket is live (OQ1); store
    // never consulted, no probe (ADR-002 §3). Env stays UNPINNED by design
    // (ADR-004 precedence note): pinnedFp null.
    return okHttp(envUrl, envTok, mergeTimeouts(null), "env", projectRoot, projectHash, stateDir, null);
  }
  if (nonEmpty(envUrl) || nonEmpty(envTok)) {
    // Partial pair = misconfig: signals intent to use remote → terminal.
    return { ok: false, reason: "partial_env", projectRoot, projectHash, stateDir };
  }

  // STORE FILE (vnc-039 ADR-004): repointed from <root>/.claude/settings.local.json
  // to the out-of-tree ~/.unimatrix/<projectHash>/remote.json via credstore.read.
  // Reads the canonical schema — observe_url (NOT the never-written url) as the
  // post target, fingerprint → pinnedFp. Single read, no probing.
  let cred;
  try {
    cred = credstore.read(projectHash);
  } catch (_err) {
    // Parse failure / unknown schema_version signals intent to use remote →
    // terminal (R-13; same posture the old code gave on non-ENOENT parse error).
    return { ok: false, reason: "malformed", projectRoot, projectHash, stateDir };
  }
  if (cred === null) {
    // ENOENT (or no home): NOT a misconfig → fall through to local UDS.
    return resolveUds(projectRoot, projectHash, stateDir);
  }

  const observeUrl = cred.observe_url;
  const token = cred.token;
  if (!nonEmpty(observeUrl) || !nonEmpty(token)) {
    // Incomplete entry (missing observe_url/token) → UDS fall-through (R-13).
    return resolveUds(projectRoot, projectHash, stateDir);
  }

  const timeouts = mergeTimeouts(cred.timeouts);
  // fingerprint → pinnedFp; null on the legacy path → unpinned (R-15), preserving
  // today's legacy behavior. NOT pin-or-fail on null.
  const pinnedFp = nonEmpty(cred.fingerprint) ? cred.fingerprint : null;
  return okHttp(observeUrl, token, timeouts, "file", projectRoot, projectHash, stateDir, pinnedFp);
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
  socketPathFor,
  mergeTimeouts,
  safeHostOf,
};
