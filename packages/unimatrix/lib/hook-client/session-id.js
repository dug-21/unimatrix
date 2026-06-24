"use strict";

/**
 * session-id.js — deterministic Claude Code session-id resolution (#832).
 *
 * Both the context_cycle DECLARATION hook spawn and every per-tool OBSERVE hook
 * spawn must key on the SAME id: the tracker file (Path A) and the registry
 * Declared feature (Path B) are keyed by it, and the cycle-review join links a
 * cycle to its observations only through it. Over STDIO/UDS this holds because
 * every spawn carries the same CC `input.session_id`. Over HTTP the declaration
 * spawn was observed to carry a different/absent id than the observe spawns,
 * splitting both paths (GH #832).
 *
 * Correct-by-construction: resolve ONE id from stable CC-supplied inputs, in a
 * fixed order, so two spawns of the same conversation CANNOT diverge:
 *   1. input.session_id            — the CC session id (the STDIO anchor).
 *   2. cc-<hash>(transcript_path)  — stable per CC conversation; written onto
 *      BOTH spawn types' stdin, so when CC omits session_id on one spawn both
 *      still compute the identical id (NOT a per-spawn ppid). This is the B1
 *      fix: forcing "onto input.session_id" is a no-op when it is null.
 *   3. ppid-<ppid>                 — last resort only when neither CC field
 *      exists; a single spawn with no conversational anchor at all.
 *
 * The cc-<16 hex> id is filesystem/registry safe by construction, so it survives
 * state.sanitizeSessionKey and cycles.js path-safety unchanged (N5). Never throws.
 */

const crypto = require("crypto");

const nonEmpty = (v) => (typeof v === "string" && v.length > 0 ? v : null);

/** Resolved CC session id (never null/empty) from the fixed precedence. */
function resolveSessionId(input) {
  const sid = input && nonEmpty(input.session_id);
  if (sid) return sid;
  const tp = input && nonEmpty(input.transcript_path);
  if (tp) {
    return "cc-" + crypto.createHash("sha256").update(tp, "utf8").digest("hex").slice(0, 16);
  }
  return "ppid-" + process.ppid;
}

/** Source label inferred from the id prefix (trace/diagnostic only). */
const sourceOf = (id) =>
  id.indexOf("cc-") === 0 ? "transcript_path" : id.indexOf("ppid-") === 0 ? "ppid" : "input.session_id";

/**
 * B1 trace: one structured stderr line, gated to UNIMATRIX_HOOK_DEBUG. Session
 * ids are not secrets (trusted-after-sanitize); no paths/tokens. Never throws.
 */
function traceSessionId(kind, id) {
  if (!process.env.UNIMATRIX_HOOK_DEBUG) return;
  try {
    process.stderr.write(
      "unimatrix: session-id: kind=" + kind + " source=" + sourceOf(id) + " id=" + id + "\n"
    );
  } catch (_e) {
    /* swallow */
  }
}

module.exports = { resolveSessionId, sourceOf, traceSessionId };
