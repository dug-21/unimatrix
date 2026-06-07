"use strict";

/**
 * delta.js — transcript delta streaming (ADR-004 / ADR-007 / ADR-008).
 *
 * On FNF spawns with a non-empty transcript_path: stat the file, read the
 * unshipped span [last_offset, file_len), and ship ONE `transcript_delta`
 * RecordEvent as a separate concurrent POST. The persisted offset advances by
 * the UNIFORM rule `declared offset + byteLength(bytes)`, only on send success.
 *
 * Deltas are NEVER queued (ADR-004): send failure = offset non-advance; the next
 * FNF spawn re-derives the span. Zero transcript bytes at rest. End-anchored
 * elision (ADR-008): an oversized span ships head(48 KiB) ++ marker ++
 * tail(12 KiB) with `offset = effectiveEnd − byteLength(bytes)` so the frame ends
 * at file end — NEVER the span start (phantom hole + PreCompact starvation vs F2).
 *
 * Content-opaque (works on binary/non-JSONL). Every fs call wrapped; never throws
 * or rejects. No stdout/stderr — index.js owns outcome handling + breadcrumb.
 */

const fs = require("fs");
const state = require("./state");
const transport = require("./transport-http");

const DELTA_SOFT_CAP = 65536; // raw span bytes (64 KiB)
const HEAD_BYTES = 49152; // 48 KiB
const TAIL_BYTES = 12288; // 12 KiB
const BODY_GUARD = 1048576; // post-serialization (C-02 / SR-04)

/** Current unix time in whole seconds (matches state.js / OVERVIEW helper). */
function nowSecs() {
  return Math.floor(Date.now() / 1000);
}

/** Build an ImplantEvent. topic_signal/provider OMITTED when null (serde skip parity). */
function implantEvent(event_type, session_id, payload, topic_signal, provider) {
  const e = { event_type, session_id, timestamp: nowSecs(), payload };
  if (topic_signal !== null && topic_signal !== undefined) e.topic_signal = topic_signal;
  if (provider !== null && provider !== undefined) e.provider = provider;
  return e;
}

/**
 * UTF-8 lead-byte sequence length. 1 for ASCII, 2/3/4 for multi-byte leads; an
 * invalid lead (continuation byte or 0xF8+) → 1 (lossy-tolerant, content-opaque).
 */
function utf8SeqLen(b) {
  if (b < 0x80) return 1;
  if (b >= 0xc0 && b < 0xe0) return 2;
  if (b >= 0xe0 && b < 0xf0) return 3;
  if (b >= 0xf0 && b < 0xf8) return 4;
  return 1;
}

/**
 * Bytes to KEEP from the start of `buf` so it ends on a complete UTF-8 char.
 * Backs off at most 3 trailing continuation bytes.
 *   - whole buffer is continuation bytes → 0 (ship nothing; R-04 scenario 2);
 *   - >3 trailing continuation bytes → not valid UTF-8, keep everything.
 */
function trimEndToUtf8Boundary(buf) {
  let i = buf.length - 1;
  while (i >= 0 && i >= buf.length - 4 && (buf[i] & 0xc0) === 0x80) i -= 1;
  if (i < 0) return 0;
  if (i < buf.length - 4) return buf.length;
  const charLen = utf8SeqLen(buf[i]);
  return i + charLen <= buf.length ? buf.length : i;
}

/**
 * First index of `buf` whose byte is NOT a continuation byte (skips at most 3
 * leading continuation bytes). Used when a tail window starts mid-character.
 */
function trimStartToUtf8Boundary(buf) {
  let i = 0;
  while (i < buf.length && i < 4 && (buf[i] & 0xc0) === 0x80) i += 1;
  return i;
}

/**
 * Positioned read of up to `length` bytes at `position`. Returns a Buffer sized
 * to the ACTUAL bytes read. Throws on read error; callers → "ship nothing".
 */
function readAt(fd, position, length) {
  const buf = Buffer.allocUnsafe(length);
  const read = fs.readSync(fd, buf, 0, length, position);
  return read === length ? buf : buf.subarray(0, read);
}

/**
 * Serialize a delta frame and enforce the post-serialization body guard (SR-04).
 * Returns `{ bodyBuf, offset, byteLen }` or `null` to ship nothing. The guard is
 * a defensive backstop (64 KiB raw × ≤6x escape ≈ 384 KiB) — never throws/413.
 */
function assemble(offset, bytes, sessionId, provider) {
  const byteLen = Buffer.byteLength(bytes, "utf8");
  const frame = Object.assign(
    { type: "RecordEvent" },
    implantEvent("transcript_delta", sessionId, { offset, bytes }, null, provider)
  );
  let bodyBuf;
  try {
    bodyBuf = Buffer.from(JSON.stringify(frame), "utf8");
  } catch (_err) {
    return null;
  }
  if (bodyBuf.length > BODY_GUARD) return null; // backstop; ship nothing
  return { bodyBuf, offset, byteLen };
}

/**
 * Build an end-anchored elided frame for an oversized span (> DELTA_SOFT_CAP).
 * `headLimit`/`tailLimit` are byte budgets (halved on the one defensive rebuild).
 * Returns the assembled frame or `null` to ship nothing.
 */
function buildElidedFrame(fd, last, fileLen, sessionId, provider, headLimit, tailLimit) {
  // 1. Anchor: effectiveEnd = fileLen backed off ≤3 bytes so the tail ends on a
  //    complete char (file may end mid-write). Usually === fileLen for JSONL.
  const tailProbe = readAt(fd, fileLen - tailLimit, tailLimit);
  const keptLen = trimEndToUtf8Boundary(tailProbe);
  const effectiveEnd = fileLen - tailLimit + keptLen;

  // 2. Tail: ends at effectiveEnd; start advanced ≤3 bytes to a boundary.
  const tailStartRaw = effectiveEnd - tailLimit;
  const tailBuf = readAt(fd, tailStartRaw, tailLimit);
  const s = trimStartToUtf8Boundary(tailBuf);
  const tailStr = tailBuf.subarray(s).toString("utf8");

  // 3. Head: from span start, end-trimmed to a boundary.
  const headBuf = readAt(fd, last, headLimit);
  const e = trimEndToUtf8Boundary(headBuf);
  const headStr = headBuf.subarray(0, e).toString("utf8");

  // 4. Marker: N = raw bytes NOT shipped between head end and tail start.
  const nElided = tailStartRaw + s - (last + e);
  const marker = "…[" + nElided + " bytes elided]…"; // U+2026 each end (3 bytes)
  const bytes = headStr + marker + tailStr;

  // 5. End-anchored: frame ends exactly at effectiveEnd. NEVER offset = last for
  //    an elided frame (ADR-008 — phantom hole + starvation).
  const offset = effectiveEnd - Buffer.byteLength(bytes, "utf8");
  return assemble(offset, bytes, sessionId, provider);
}

/**
 * Build the delta frame for the unshipped span [last, fileLen). `null` ships
 * nothing (open/read error, or a span that trims away to nothing).
 */
function buildDeltaFrame(path_, last, fileLen, sessionId, provider) {
  let fd;
  try {
    fd = fs.openSync(path_, "r");
  } catch (_err) {
    return null;
  }
  try {
    const span = fileLen - last;
    if (span <= DELTA_SOFT_CAP) {
      // Normal frame: declared offset = span start.
      const raw = readAt(fd, last, span);
      const end = trimEndToUtf8Boundary(raw);
      if (end === 0) return null; // grew by continuation bytes only (R-04 sc.2)
      const bytes = raw.subarray(0, end).toString("utf8");
      return assemble(last, bytes, sessionId, provider);
    }
    // Elided frame: end-anchored (ADR-008).
    let frame = buildElidedFrame(
      fd,
      last,
      fileLen,
      sessionId,
      provider,
      HEAD_BYTES,
      TAIL_BYTES
    );
    if (frame === null) {
      // One defensive rebuild with halved budgets, same end-anchored math.
      frame = buildElidedFrame(
        fd,
        last,
        fileLen,
        sessionId,
        provider,
        HEAD_BYTES / 2,
        TAIL_BYTES / 2
      );
    }
    return frame; // null here → ship nothing, never throw, never 413
  } catch (_err) {
    return null; // any read error → ship nothing, offset unchanged
  } finally {
    try {
      fs.closeSync(fd);
    } catch (_closeErr) {
      // best-effort
    }
  }
}

/**
 * Stat the transcript, ship one delta for any growth, advance the offset only on
 * send success. Never throws / rejects.
 *
 * @returns {Promise<object>} DeltaOutcome:
 *   { attempted:false, reason } — no POST (stat error, rewind, no growth, empty);
 *   { attempted:true, send:SendResult } — a POST was attempted. index.js feeds
 *     attempted:true outcomes into the breadcrumb.
 */
async function maybeSendDelta(transcriptPath, sessionId, provider, config) {
  const last = state.readOffset(config.stateDir, sessionId); // corrupt/missing/neg → 0

  let fileLen;
  try {
    fileLen = fs.statSync(transcriptPath).size; // ONE fstat (ADR-007 cheap gate)
  } catch (_err) {
    return { attempted: false, reason: "stat" }; // missing/dir/TOCTOU
  }

  if (fileLen < last) {
    // Rewrite guard (A-4 / FR-11): reset, NEVER a negative span, ship nothing.
    state.writeOffset(config.stateDir, sessionId, fileLen);
    return { attempted: false, reason: "rewind" };
  }
  if (fileLen === last) {
    return { attempted: false, reason: "unchanged" }; // AC-06: no POST
  }

  const frame = buildDeltaFrame(transcriptPath, last, fileLen, sessionId, provider);
  if (frame === null) {
    return { attempted: false, reason: "empty_span" };
  }

  const send = await transport.post(config, null, {
    sync: false,
    bodyBuf: frame.bodyBuf,
  });
  if (send.ok) {
    // UNIFORM advance: normal = trimmed span end; elided = effectiveEnd.
    state.writeOffset(config.stateDir, sessionId, frame.offset + frame.byteLen);
  }
  // Failure: do NOT advance, do NOT enqueue (ADR-004) — next spawn re-derives.
  return { attempted: true, send };
}

module.exports = {
  maybeSendDelta,
  buildDeltaFrame,
  trimEndToUtf8Boundary,
  trimStartToUtf8Boundary,
  DELTA_SOFT_CAP,
  HEAD_BYTES,
  TAIL_BYTES,
  BODY_GUARD,
};
