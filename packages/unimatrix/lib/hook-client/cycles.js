"use strict";

/**
 * cycles.js — per-session cycle tracker file lifecycle (ADR-001).
 *
 * Owns `{stateDir}/cycles/{session_key}.json` (beside offsets/):
 *   { "topic": string, "phase": string|null, "declared_at": <secs>, "updated": <secs> }
 *
 * The tracker is the cycle stamp's source of truth. It survives all three
 * server-state-loss events (per-turn drain, resume/compact re-register, server
 * restart) because it is a disk file keyed by the root session_id. Reuses
 * state.js machinery (atomicWrite, sanitizeSessionKey, nowSecs) — no new
 * failure modes in atomics/sanitize.
 *
 * Best-effort: NO function here ever throws. Failures degrade to the never-throw
 * sentinel (null/false) so the spawn always exits 0 (C-04 fail-open). No stdout,
 * no stderr, no secrets in any output.
 *
 * C-11: all paths derive from the caller-supplied `stateDir`
 * (= config.resolve(cwd).stateDir, which routes through the worktree gitdir
 * port). This module NEVER hashes a raw cwd.
 *
 * Sanitization happens INSIDE this module (pattern #4772 — never pre-sanitize at
 * call sites). Raw `session_id` is passed in.
 */

const fs = require("fs");
const path = require("path");
const state = require("./state");

/** Tracker files older than this (since `updated`) are pruned — matches offsets. */
const PRUNE_SECS = 7 * 24 * 60 * 60;

/** True when stateDir is a usable path string (null when HOME is absent). */
function usable(stateDir) {
  return typeof stateDir === "string" && stateDir.length > 0;
}

/** Absolute path of the cycles directory under stateDir. */
function cyclesDir(stateDir) {
  return path.join(stateDir, "cycles");
}

/** Absolute path of a session's tracker file (key sanitized internally, #4772). */
function cyclePath(stateDir, sessionId) {
  return path.join(stateDir, "cycles", state.sanitizeSessionKey(sessionId) + ".json");
}

/**
 * Create `{stateDir}/cycles` with mode 0700 (lazily, on first write). Local to
 * cycles.js so state.ensureStateDir stays untouched. Returns false when stateDir
 * is unusable or creation fails — callers skip persistence, sends proceed.
 */
function ensureCyclesDir(stateDir) {
  if (!usable(stateDir)) return false;
  try {
    fs.mkdirSync(cyclesDir(stateDir), { recursive: true, mode: 0o700 });
    return true;
  } catch (_err) {
    return false;
  }
}

/**
 * Read the cycle tracker. Returns ONLY the stamp surface `{topic, phase}`;
 * `declared_at`/`updated` are file-internal. Missing/unreadable/corrupt JSON,
 * non-object, or mistyped/empty topic → null (event sent unstamped). Never
 * throws (R-03/R-06).
 */
function readCycle(stateDir, sessionId) {
  if (!usable(stateDir)) return null;
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(cyclePath(stateDir, sessionId), "utf8"));
  } catch (_err) {
    return null;
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
  const topic = parsed.topic;
  if (typeof topic !== "string" || topic === "") return null;
  const phase = typeof parsed.phase === "string" ? parsed.phase : null;
  return { topic, phase };
}

/**
 * Create-or-overwrite the tracker (last-writer-wins, ADR-001 scenario 17). Full
 * file write via state.atomicWrite (temp+rename); `declared_at := now` on every
 * write (a re-declaration is a fresh declaration; declared_at is informational,
 * never read back, so reset-on-overwrite keeps this a pure overwrite — no RMW).
 * Topic is stored verbatim (validation is the frame-construction gate, R-10).
 * Returns false on any failure (disk-full → event still sent unstamped).
 */
function writeCycle(stateDir, sessionId, topic, phase) {
  if (!usable(stateDir)) return false;
  if (!ensureCyclesDir(stateDir)) return false;
  const now = Math.floor(Date.now() / 1000);
  const body = JSON.stringify({
    topic: topic,
    phase: phase === undefined || phase === null ? null : phase,
    declared_at: now,
    updated: now,
  });
  return state.atomicWrite(cyclePath(stateDir, sessionId), body);
}

/**
 * Read-modify-write the tracker's phase + `updated`, preserving topic and
 * declared_at. A MISSING/corrupt file → no-op `false`; the tracker is NEVER
 * recreated (a phase-end without a prior start is a protocol violation; degrade,
 * R-22 / ADR-001). Never throws.
 */
function updatePhase(stateDir, sessionId, phase) {
  if (!usable(stateDir)) return false;
  let existing;
  try {
    existing = JSON.parse(fs.readFileSync(cyclePath(stateDir, sessionId), "utf8"));
  } catch (_err) {
    return false; // missing / unreadable / bad JSON → no-op, never recreate
  }
  if (
    !existing ||
    typeof existing !== "object" ||
    Array.isArray(existing) ||
    typeof existing.topic !== "string"
  ) {
    return false;
  }
  const now = Math.floor(Date.now() / 1000);
  const body = JSON.stringify({
    topic: existing.topic,
    phase: phase === undefined || phase === null ? null : phase,
    declared_at: Number.isSafeInteger(existing.declared_at) ? existing.declared_at : now,
    updated: now,
  });
  return state.atomicWrite(cyclePath(stateDir, sessionId), body);
}

/** Delete the tracker file. Already-gone / unwritable → false, never throws. */
function deleteCycle(stateDir, sessionId) {
  if (!usable(stateDir)) return false;
  try {
    fs.unlinkSync(cyclePath(stateDir, sessionId));
    return true;
  } catch (_err) {
    return false;
  }
}

/**
 * Prune tracker files whose `updated` is older than 7 days (mtime fallback when
 * JSON is unreadable). Mirrors state.pruneOffsets exactly, over cycles/. Called
 * opportunistically on the FNF path where queue.prune / pruneOffsets already
 * run — piggyback, best-effort. Missing cycles/ dir → no-op, no throw.
 */
function pruneCycles(stateDir) {
  if (!usable(stateDir)) return;
  const dir = cyclesDir(stateDir);
  let names;
  try {
    names = fs.readdirSync(dir);
  } catch (_err) {
    return;
  }
  const cutoff = Math.floor(Date.now() / 1000) - PRUNE_SECS;
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

module.exports = {
  PRUNE_SECS,
  cyclesDir,
  cyclePath,
  ensureCyclesDir,
  readCycle,
  writeCycle,
  updatePhase,
  deleteCycle,
  pruneCycles,
};
