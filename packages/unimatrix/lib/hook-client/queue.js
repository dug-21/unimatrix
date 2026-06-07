"use strict";

/**
 * queue.js — minimal disk event queue (ADR-003 mini-spec).
 *
 * Enqueue-on-failure for NON-DELTA FNF frames; bounded lexicographic
 * replay-before-send; drop-oldest eviction; 24 h age prune; poison-pill immunity.
 * Lock-free — one O_EXCL file per frame, no shared mutable file. Runs ONLY on FNF
 * spawns (never the sync trio, SR-03).
 *
 * Layout (ADR-003): {stateDir}/queue/{ts_ms}-{pid}-{seq}.json — one frame/file.
 * Distinct from the Rust hook's `event-queue/` (no shared/cross-format reads).
 * transcript_delta frames are NEVER written (ADR-004); enqueue() carries a
 * structural guard so the at-rest guarantee holds even if a caller errs.
 *
 * Every op wrapped: NO function here throws. Full disk / unwritable dir / readdir
 * failure → swallowed; the send path proceeds (AC-15). No frame content is logged
 * (queued payloads are secrets-adjacent — R-16).
 */

const fs = require("fs");
const path = require("path");

const MAX_FILES = 500;
const MAX_TOTAL_BYTES = 5 * 1024 * 1024;
const MAX_AGE_MS = 24 * 60 * 60 * 1000;

const REPLAY_MAX_FRAMES = 32;
const REPLAY_MAX_BYTES = 262_144; // 256 KiB

const ENQUEUE_MAX_ATTEMPTS = 1000; // seq-bump retries on same-ms same-pid collision

/** True when stateDir is a usable path string (null when HOME is absent). */
function usable(stateDir) {
  return typeof stateDir === "string" && stateDir.length > 0;
}

/** Absolute path of the queue subdir for a state dir. */
function queueDir(stateDir) {
  return path.join(stateDir, "queue");
}

/** Zero-pad to `width` chars (lexicographic == numeric order). */
function pad(n, width) {
  const s = String(n);
  return s.length >= width ? s : "0".repeat(width - s.length) + s;
}

/**
 * Create `{stateDir}/queue` with mode 0700. Returns the dir path, or null on
 * failure (callers skip — sends proceed). Never throws.
 */
function ensureQueueDir(stateDir) {
  if (!usable(stateDir)) return null;
  const dir = queueDir(stateDir);
  try {
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    return dir;
  } catch (_err) {
    return null;
  }
}

/**
 * List `*.json` queue frame files sorted ascending — oldest first (zero-padded
 * `{ts_ms}` prefix is age order). Skips non-json names. Errors → []. Never throws.
 */
function listQueueFiles(dir) {
  let names;
  try {
    names = fs.readdirSync(dir);
  } catch (_err) {
    return [];
  }
  return names.filter((n) => n.endsWith(".json")).sort();
}

/** Parse the leading `{ts_ms}` from a queue filename; NaN when malformed. */
function tsFromName(name) {
  const dash = name.indexOf("-");
  const head = dash === -1 ? name : name.slice(0, dash);
  const ts = Number(head);
  return Number.isFinite(ts) ? ts : NaN;
}

/** Unlink a path, swallowing all errors (best-effort). */
function unlinkWrapped(p) {
  try {
    fs.unlinkSync(p);
  } catch (_err) {
    // best-effort
  }
}

/** True when frame is a transcript_delta RecordEvent (ADR-004 guard). */
function isDeltaFrame(frame) {
  return (
    frame !== null &&
    typeof frame === "object" &&
    frame.type === "RecordEvent" &&
    frame.event_type === "transcript_delta"
  );
}

/**
 * Prune + drop-oldest BEFORE a write of `incomingBytes`:
 *   1. age prune — files with leading ts older than 24 h (or malformed) unlinked.
 *   2. count/size — while (count+1 > MAX_FILES) OR
 *      (total + incomingBytes > MAX_TOTAL_BYTES): unlink the oldest remaining.
 * Each unlink wrapped. Never throws.
 */
function enforceBounds(dir, incomingBytes) {
  const now = Date.now();
  let names = listQueueFiles(dir);

  // 1. age prune (and malformed-name drop).
  const survivors = [];
  for (const name of names) {
    const ts = tsFromName(name);
    if (!Number.isFinite(ts) || now - ts > MAX_AGE_MS) {
      unlinkWrapped(path.join(dir, name));
    } else {
      survivors.push(name);
    }
  }
  names = survivors;

  // 2. count/size eviction, oldest first.
  let total = 0;
  const sizes = [];
  for (const name of names) {
    let size = 0;
    try {
      size = fs.statSync(path.join(dir, name)).size;
    } catch (_err) {
      // vanished concurrently — treat as 0, will fall out of the list below
    }
    sizes.push(size);
    total += size;
  }

  let i = 0;
  while (
    i < names.length &&
    (names.length - i + 1 > MAX_FILES ||
      total + incomingBytes > MAX_TOTAL_BYTES)
  ) {
    unlinkWrapped(path.join(dir, names[i]));
    total -= sizes[i];
    i += 1;
  }
}

/**
 * enqueue(stateDir, frame) -> void (best-effort). Persist one non-delta FNF frame
 * as `queue/{ts_ms}-{pid}-{seq}.json` via O_EXCL (`flag:"wx"`, mode 0600) so
 * concurrent spawns never tear a write; same-ms same-pid collision bumps `seq`.
 * Bounds enforced before the write. transcript_delta dropped (ADR-004). All
 * errors swallowed — never throws or affects exit/stdout (AC-15).
 */
function enqueue(stateDir, frame) {
  if (!usable(stateDir)) return;
  if (isDeltaFrame(frame)) return; // defense-in-depth; ADR-004
  try {
    const dir = ensureQueueDir(stateDir);
    if (dir === null) return;
    const data = JSON.stringify(frame);
    enforceBounds(dir, Buffer.byteLength(data, "utf8"));
    const ts = Date.now();
    for (let seq = 0; seq < ENQUEUE_MAX_ATTEMPTS; seq += 1) {
      const name = pad(ts, 13) + "-" + process.pid + "-" + pad(seq, 4) + ".json";
      try {
        fs.writeFileSync(path.join(dir, name), data, { flag: "wx", mode: 0o600 });
        return;
      } catch (err) {
        if (err && err.code === "EEXIST") continue; // collision → bump seq
        return; // any other write error → swallow (full disk, EACCES, …)
      }
    }
  } catch (_err) {
    // ensureQueueDir / stringify / stat — swallow (AC-15)
  }
}

/**
 * replay(config, post) -> Promise<{sent, stoppedOnFailure}>. Replay-before-send
 * (FR-13/FR-15), called BEFORE the carrying POST on FNF spawns; does NOT gate it
 * (Rust run() parity, best-effort). Sends oldest-first, ≤ REPLAY_MAX_FRAMES /
 * REPLAY_MAX_BYTES per spawn; deletes a file only after a 2xx; stops at the first
 * failure (failed file + remainder kept). Corrupt frame → delete + continue
 * (poison-pill immunity). `post` is transport `(config, frame, opts)`. No throw.
 */
async function replay(config, post) {
  const stateDir =
    config && typeof config === "object" ? config.stateDir : null;
  if (!usable(stateDir)) return { sent: 0, stoppedOnFailure: false };

  const dir = queueDir(stateDir);
  const files = listQueueFiles(dir); // oldest first
  let sentFrames = 0;
  let sentBytes = 0;

  for (const name of files) {
    if (sentFrames >= REPLAY_MAX_FRAMES || sentBytes >= REPLAY_MAX_BYTES) {
      break; // budget exhausted — leave remainder
    }
    const p = path.join(dir, name);

    let raw;
    try {
      raw = fs.readFileSync(p);
    } catch (_err) {
      continue; // vanished (concurrent spawn) → skip
    }

    let frame;
    try {
      frame = JSON.parse(raw.toString("utf8"));
    } catch (_err) {
      unlinkWrapped(p); // poison pill: delete, keep going
      continue;
    }

    let res;
    try {
      res = await post(config, frame, { sync: false });
    } catch (_err) {
      return { sent: sentFrames, stoppedOnFailure: true }; // treat throw as failure
    }

    if (!res || !res.ok) {
      // stop at FIRST failure; file NOT deleted, remainder left
      return { sent: sentFrames, stoppedOnFailure: true };
    }

    unlinkWrapped(p); // delete only after 2xx
    sentFrames += 1;
    sentBytes += raw.length;
  }

  return { sent: sentFrames, stoppedOnFailure: false };
}

/**
 * prune(stateDir) -> void (best-effort). Age-prune only — files older than 24 h
 * unlinked. Called each FNF spawn before replay. Never throws.
 */
function prune(stateDir) {
  if (!usable(stateDir)) return;
  const dir = queueDir(stateDir);
  const now = Date.now();
  for (const name of listQueueFiles(dir)) {
    const ts = tsFromName(name);
    if (!Number.isFinite(ts) || now - ts > MAX_AGE_MS) {
      unlinkWrapped(path.join(dir, name));
    }
  }
}

/**
 * queueDepth(stateDir) -> number. Count of `*.json` frame files (breadcrumb
 * queue_depth). Errors → 0. Never throws.
 */
function queueDepth(stateDir) {
  if (!usable(stateDir)) return 0;
  return listQueueFiles(queueDir(stateDir)).length;
}

module.exports = {
  MAX_FILES,
  MAX_TOTAL_BYTES,
  MAX_AGE_MS,
  REPLAY_MAX_FRAMES,
  REPLAY_MAX_BYTES,
  queueDir,
  ensureQueueDir,
  listQueueFiles,
  enqueue,
  enforceBounds,
  replay,
  prune,
  queueDepth,
};
