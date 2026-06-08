"use strict";

/**
 * state.js — client state dir, atomic writes, offsets, health breadcrumb.
 *
 * Owns `~/.unimatrix/{hash}/hook-client/` (ADR-003):
 *   offsets/{session_key}.json   { "offset": N, "updated": <unix secs> }
 *   queue/{ts}-{pid}-{seq}.json  (owned by queue.js)
 *   health.json                  (ADR-005 content-free breadcrumb)
 *
 * Best-effort: NO function here ever throws. Failures degrade to false/0/default
 * so the spawn always exits 0 (AC-15, R-10 scenario 3). Dir mode 0700, file mode
 * 0600; on Windows these are advisory no-ops that must not throw (R-14).
 */

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

/** Offset files older than this (since `updated`) are pruned. */
const OFFSET_PRUNE_SECS = 7 * 24 * 60 * 60;

const SESSION_KEY_RE = /^[A-Za-z0-9_-]{1,64}$/;

/** Current unix time in whole seconds. */
function nowSecs() {
  return Math.floor(Date.now() / 1000);
}

/** True when stateDir is a usable path string (null when HOME is absent). */
function usable(stateDir) {
  return typeof stateDir === "string" && stateDir.length > 0;
}

/**
 * Sanitize a session id into a filesystem-safe key (ADR-003). Conforming ids
 * pass through; anything else (traversal, absolute paths, NUL, 65+ chars,
 * Unicode) becomes the first 16 hex of its SHA-256. Idempotent.
 */
function sanitizeSessionKey(sessionId) {
  const id = typeof sessionId === "string" ? sessionId : String(sessionId);
  if (SESSION_KEY_RE.test(id)) return id;
  return crypto.createHash("sha256").update(id, "utf8").digest("hex").slice(0, 16);
}

/** Absolute path of a session's offset file (key sanitized internally). */
function offsetPath(stateDir, sessionId) {
  return path.join(stateDir, "offsets", sanitizeSessionKey(sessionId) + ".json");
}

/** Absolute path of the health breadcrumb. */
function healthPath(stateDir) {
  return path.join(stateDir, "health.json");
}

/**
 * Create `{stateDir}/offsets` (and stateDir) with mode 0700; queue/ is created
 * on demand by queue.js. Returns false when stateDir is unusable (no HOME) or
 * creation fails — callers skip persistence, sends proceed.
 */
function ensureStateDir(stateDir) {
  if (!usable(stateDir)) return false;
  try {
    fs.mkdirSync(path.join(stateDir, "offsets"), { recursive: true, mode: 0o700 });
    return true;
  } catch (_err) {
    return false;
  }
}

/**
 * Atomically write `jsonString` to `filePath` via temp file + rename in the same
 * directory. POSIX rename is atomic; Windows renameSync overwrites — acceptable,
 * last-writer-wins (FR-11). Returns false on error (tmp unlinked best-effort).
 */
function atomicWrite(filePath, jsonString) {
  const tmp =
    filePath + ".tmp-" + process.pid + "-" + crypto.randomBytes(4).toString("hex");
  try {
    fs.writeFileSync(tmp, jsonString, { mode: 0o600 });
    fs.renameSync(tmp, filePath);
    return true;
  } catch (_err) {
    try {
      fs.unlinkSync(tmp);
    } catch (_cleanupErr) {
      // best-effort
    }
    return false;
  }
}

/**
 * Read the persisted offset. Missing/corrupt/negative/non-integer/unsafe → 0;
 * re-shipping from 0 is SAFE (F2 merge is offset-bounded and idempotent).
 */
function readOffset(stateDir, sessionId) {
  if (!usable(stateDir)) return 0;
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(offsetPath(stateDir, sessionId), "utf8"));
  } catch (_err) {
    return 0;
  }
  const v = parsed && typeof parsed === "object" ? parsed.offset : undefined;
  return Number.isSafeInteger(v) && v >= 0 ? v : 0;
}

/** Persist a session's offset atomically. False on failure. */
function writeOffset(stateDir, sessionId, offset) {
  if (!usable(stateDir)) return false;
  if (!ensureStateDir(stateDir)) return false;
  const body = JSON.stringify({ offset: offset, updated: nowSecs() });
  return atomicWrite(offsetPath(stateDir, sessionId), body);
}

/**
 * Delete a session's offset file. Fired by index.js ONLY when the carrying send
 * succeeds AND the canonical event is TaskCompleted (ADR-006 vnc-027 — keyed by
 * canonical event name, NEVER frame type; Stop and TaskCompleted both build
 * SessionClose frames). Unreachable under current HOOK_EVENTS; pinned by unit
 * test. Fail-open: returns false on failure, never throws.
 */
function deleteOffset(stateDir, sessionId) {
  if (!usable(stateDir)) return false;
  try {
    fs.unlinkSync(offsetPath(stateDir, sessionId));
    return true;
  } catch (_err) {
    return false;
  }
}

/**
 * Prune offset files whose `updated` is older than 7 days (mtime fallback when
 * JSON is unreadable). Called opportunistically on FNF spawns after replay. A
 * pruned mid-session file degrades to offset 0 — safe (idempotent merge).
 */
function pruneOffsets(stateDir) {
  if (!usable(stateDir)) return;
  const dir = path.join(stateDir, "offsets");
  let names;
  try {
    names = fs.readdirSync(dir);
  } catch (_err) {
    return;
  }
  const cutoff = nowSecs() - OFFSET_PRUNE_SECS;
  for (const name of names) {
    if (!name.endsWith(".json")) continue; // skip .tmp-* remnants
    const fp = path.join(dir, name);
    let updated = null;
    try {
      const parsed = JSON.parse(fs.readFileSync(fp, "utf8"));
      const u = parsed && typeof parsed === "object" ? parsed.updated : undefined;
      if (Number.isSafeInteger(u) && u >= 0) updated = u;
    } catch (_err) {
      // fall through to mtime
    }
    if (updated === null) {
      try {
        updated = Math.floor(fs.statSync(fp).mtimeMs / 1000);
      } catch (_err) {
        continue;
      }
    }
    if (updated < cutoff) {
      try {
        fs.unlinkSync(fp);
      } catch (_err) {
        // best-effort
      }
    }
  }
}

/** Zeroed breadcrumb default (health.json missing/corrupt). */
function defaultBreadcrumb() {
  return {
    last_success: null,
    last_failure: null,
    failure_class: null,
    consecutive_failures: 0,
    queue_depth: 0,
    url_host: "",
  };
}

/** Read the health breadcrumb; missing/corrupt/mistyped fields degrade field-by-field to zeroed default. */
function readBreadcrumb(stateDir) {
  const def = defaultBreadcrumb();
  if (!usable(stateDir)) return def;
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(healthPath(stateDir), "utf8"));
  } catch (_err) {
    return def;
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return def;
  return {
    last_success: Number.isSafeInteger(parsed.last_success) ? parsed.last_success : null,
    last_failure: Number.isSafeInteger(parsed.last_failure) ? parsed.last_failure : null,
    failure_class:
      typeof parsed.failure_class === "string" ? parsed.failure_class : null,
    consecutive_failures:
      Number.isSafeInteger(parsed.consecutive_failures) && parsed.consecutive_failures >= 0
        ? parsed.consecutive_failures
        : 0,
    queue_depth:
      Number.isSafeInteger(parsed.queue_depth) && parsed.queue_depth >= 0
        ? parsed.queue_depth
        : 0,
    url_host: typeof parsed.url_host === "string" ? parsed.url_host : "",
  };
}

/**
 * Update the breadcrumb after a spawn that attempted ≥1 send (sync AND FNF —
 * R-10 scenario 4). `results` is SendResult-like objects, carrying-event-first;
 * null/undefined entries excluded. Aggregation (ADR-005 / state.md pinned rule):
 *   - all ok    → last_success=now, consecutive_failures=0
 *   - any fail  → last_failure=now, consecutive_failures=prev+1,
 *                 failure_class = first failure's class (carrying wins over delta)
 * Content-free: only timestamps, class, counters, url HOST. Best-effort.
 */
function recordSendOutcomes(stateDir, urlHost, results, queueDepth) {
  if (!usable(stateDir)) return false;
  const attempted = (Array.isArray(results) ? results : []).filter(
    (r) => r !== null && r !== undefined
  );
  if (attempted.length === 0) return false;
  const prev = readBreadcrumb(stateDir);
  const firstFailure = attempted.find((r) => !r.ok);
  const anyFail = firstFailure !== undefined;
  const now = nowSecs();
  const next = {
    last_success: anyFail ? prev.last_success : now,
    last_failure: anyFail ? now : prev.last_failure,
    failure_class: anyFail
      ? typeof firstFailure.failureClass === "string"
        ? firstFailure.failureClass
        : null
      : prev.failure_class,
    consecutive_failures: anyFail ? prev.consecutive_failures + 1 : 0,
    queue_depth:
      Number.isSafeInteger(queueDepth) && queueDepth >= 0 ? queueDepth : 0,
    url_host: typeof urlHost === "string" ? urlHost : prev.url_host,
  };
  if (!ensureStateDir(stateDir)) return false;
  return atomicWrite(healthPath(stateDir), JSON.stringify(next));
}

/**
 * Config-miss breadcrumb variant (no send attempted — index.js). Pinned rule:
 * config-miss DOES increment `consecutive_failures` and sets the class ("auth"
 * for partial_env, "connect" for missing/malformed) so a misconfigured install
 * shows a growing counter (SR-10). `url_host` keeps prior value or "".
 */
function writeBreadcrumb(stateDir, info) {
  if (!usable(stateDir)) return false;
  const prev = readBreadcrumb(stateDir);
  const next = {
    last_success: prev.last_success,
    last_failure: nowSecs(),
    failure_class:
      info && typeof info.failureClass === "string" ? info.failureClass : null,
    consecutive_failures: prev.consecutive_failures + 1,
    queue_depth: prev.queue_depth,
    url_host: prev.url_host || "",
  };
  if (!ensureStateDir(stateDir)) return false;
  return atomicWrite(healthPath(stateDir), JSON.stringify(next));
}

module.exports = {
  OFFSET_PRUNE_SECS,
  ensureStateDir,
  sanitizeSessionKey,
  offsetPath,
  healthPath,
  atomicWrite,
  readOffset,
  writeOffset,
  deleteOffset,
  pruneOffsets,
  readBreadcrumb,
  recordSendOutcomes,
  writeBreadcrumb,
};
