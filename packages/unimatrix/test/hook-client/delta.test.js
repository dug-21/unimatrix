"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const delta = require("../../lib/hook-client/delta");

// ── Test infra ─────────────────────────────────────────────────────
//
// Adversarial strings are built via String.fromCharCode/fromCodePoint, never
// bare \uXXXX escape literals in source (Unimatrix pattern #4769). transport
// is stubbed by replacing its `post` so no network is touched; the stub records
// every call (bodies, concurrency) and returns scripted SendResults.

const transport = require("../../lib/hook-client/transport-http");

let tmpRoot;

function freshTmp() {
  tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-delta-test-"));
  return tmpRoot;
}

function cleanup() {
  if (tmpRoot) {
    try {
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    } catch (_e) {
      /* best-effort */
    }
    tmpRoot = null;
  }
}

function stateDir() {
  return path.join(tmpRoot, "hook-client");
}

function config() {
  return {
    url: "http://127.0.0.1:1/x",
    token: "t",
    timeouts: { connectMs: 50, syncMs: 100, fnfMs: 100 },
    stateDir: stateDir(),
  };
}

function transcriptFile(buf) {
  const p = path.join(tmpRoot, "transcript.jsonl");
  fs.writeFileSync(p, buf);
  return p;
}

function offsetFilePath(sessionId) {
  return path.join(stateDir(), "offsets", sessionId + ".json");
}

function readPersistedOffset(sessionId) {
  return JSON.parse(fs.readFileSync(offsetFilePath(sessionId), "utf8")).offset;
}

function offsetFileExists(sessionId) {
  return fs.existsSync(offsetFilePath(sessionId));
}

function queueFiles() {
  const dir = path.join(stateDir(), "queue");
  try {
    return fs.readdirSync(dir);
  } catch (_e) {
    return [];
  }
}

/**
 * Install a stub `transport.post`. `responder(callIndex, bodyBuf) -> SendResult`.
 * Returns a `calls` array of { bodyBuf, frame, ts }. Restored via the returned
 * `restore()`.
 */
function stubPost(responder) {
  const original = transport.post;
  const calls = [];
  transport.post = function (cfg, frame, opts) {
    const bodyBuf = opts && opts.bodyBuf ? opts.bodyBuf : null;
    const idx = calls.length;
    let parsed = null;
    try {
      parsed = bodyBuf ? JSON.parse(bodyBuf.toString("utf8")) : null;
    } catch (_e) {
      parsed = null;
    }
    calls.push({ bodyBuf, frame: parsed, ts: Date.now() });
    return Promise.resolve(responder(idx, bodyBuf));
  };
  return {
    calls,
    restore() {
      transport.post = original;
    },
  };
}

const OK = { ok: true, status: 200, contentType: "application/json", body: null, failureClass: null };
function failResult(cls, status) {
  return { ok: false, status: status || 0, contentType: null, body: null, failureClass: cls };
}

// Multi-byte builders (no bare escape literals — pattern #4769).
const SNOWMAN = String.fromCodePoint(0x2603); // 3-byte UTF-8
const EURO = String.fromCodePoint(0x20ac); // 3-byte
const NBSP = String.fromCharCode(0xa9); // 2-byte (©)
const EMOJI = String.fromCodePoint(0x1f600); // 4-byte (grinning face)

function repeat(s, n) {
  return s.repeat(n);
}

// ── UTF-8 boundary trim helpers (R-04) ──────────────────────────────

describe("delta UTF-8 boundary trim (R-04)", function () {
  beforeEach(freshTmp);
  afterEach(cleanup);

  it("test_trim_mid_2byte", async function () {
    // "a" + first byte only of a 2-byte char.
    const full = Buffer.from("a" + NBSP, "utf8"); // 1 + 2 = 3 bytes
    const truncated = full.subarray(0, full.length - 1); // drops the trailing continuation byte
    const p = transcriptFile(truncated);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(out.attempted, true);
      assert.strictEqual(out.send.ok, true);
      // Only "a" survives — the mid-2-byte char is trimmed.
      assert.strictEqual(sb.calls[0].frame.payload.bytes, "a");
      assert.strictEqual(readPersistedOffset("s1"), 1);
    } finally {
      sb.restore();
    }
  });

  it("test_trim_mid_3byte", async function () {
    const full = Buffer.from("x" + SNOWMAN, "utf8"); // 1 + 3 = 4 bytes
    const truncated = full.subarray(0, full.length - 1); // mid-3-byte char
    const p = transcriptFile(truncated);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(sb.calls[0].frame.payload.bytes, "x");
      assert.strictEqual(readPersistedOffset("s1"), 1);
    } finally {
      sb.restore();
    }
  });

  it("test_trim_mid_4byte", async function () {
    const full = Buffer.from("y" + EMOJI, "utf8"); // 1 + 4 = 5 bytes
    const truncated = full.subarray(0, full.length - 2); // mid-4-byte char
    const p = transcriptFile(truncated);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(sb.calls[0].frame.payload.bytes, "y");
      assert.strictEqual(readPersistedOffset("s1"), 1);
    } finally {
      sb.restore();
    }
  });

  it("test_span_inside_one_multibyte_char", async function () {
    // First spawn ships "a" (offset 1). Then the file grows by ONLY the lead
    // byte of a multi-byte char → span is entirely inside one char → ship
    // nothing, offset unchanged.
    const p = transcriptFile(Buffer.from("a", "utf8"));
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(readPersistedOffset("s1"), 1);
      // Append the first byte of EMOJI (a lead byte 0xF0 — still incomplete).
      const lead = Buffer.from(EMOJI, "utf8").subarray(0, 1);
      fs.appendFileSync(p, lead);
      const before = sb.calls.length;
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(out.attempted, false);
      assert.strictEqual(out.reason, "empty_span");
      assert.strictEqual(sb.calls.length, before, "no POST for a sub-char span");
      assert.strictEqual(readPersistedOffset("s1"), 1, "offset unchanged");
    } finally {
      sb.restore();
    }
  });

  it("test_property_contiguous_prefix", async function () {
    // R-04 property: a single well-formed multi-byte stream is flushed to disk
    // in arbitrary byte-prefix increments (each write may end MID-CHARACTER).
    // Across spawns: concat(shipped spans) == contiguous prefix of the canonical
    // file, and last_offset == that prefix's byte length. Because every shipped
    // span ends on a char boundary and the next begins exactly there, the
    // reconstruction is byte-exact with no replacement chars (R-04 invariant).
    const alphabet = [SNOWMAN, EURO, NBSP, EMOJI, "a", "b", "Z", "0"];
    // Canonical, fully-formed content (all complete chars).
    let canonical = "";
    for (let i = 0; i < 300; i++) canonical += alphabet[(i * 7 + 3) % alphabet.length];
    const canonicalBuf = Buffer.from(canonical, "utf8");

    const p = transcriptFile(Buffer.alloc(0));
    let writtenBytes = 0; // bytes flushed to disk so far (may stop mid-char)
    let shipped = ""; // concatenation of all shipped payload.bytes
    const sb = stubPost(() => OK);
    try {
      let step = 0;
      while (writtenBytes < canonicalBuf.length) {
        // Flush 1-9 more bytes of the canonical buffer — deliberately allowed to
        // land mid-character so the trim logic is exercised.
        const grow = 1 + ((step * 5 + 1) % 9);
        writtenBytes = Math.min(canonicalBuf.length, writtenBytes + grow);
        fs.writeFileSync(p, canonicalBuf.subarray(0, writtenBytes));

        const before = sb.calls.length;
        await delta.maybeSendDelta(p, "s1", null, config());
        if (sb.calls.length > before) {
          shipped += sb.calls[sb.calls.length - 1].frame.payload.bytes;
        }
        const off = offsetFileExists("s1") ? readPersistedOffset("s1") : 0;
        const shippedBuf = Buffer.from(shipped, "utf8");
        // Invariant 1: offset == shipped prefix byte length.
        assert.strictEqual(off, shippedBuf.length, "offset == shipped prefix length");
        // Invariant 2: shipped == contiguous prefix of the canonical file.
        assert.ok(
          canonicalBuf.subarray(0, shippedBuf.length).equals(shippedBuf),
          "shipped == contiguous prefix at step " + step
        );
        // Invariant 3: no replacement char (clean boundaries throughout).
        assert.ok(
          !shipped.includes(String.fromCharCode(0xfffd)),
          "no replacement char at step " + step
        );
        step++;
      }
      // Terminal: the entire canonical file was shipped exactly.
      assert.strictEqual(shipped, canonical, "full file reconstructed");
      assert.strictEqual(readPersistedOffset("s1"), canonicalBuf.length);
    } finally {
      sb.restore();
    }
  });
});

// ── Frame shape + offset advance (AC-06 / AC-07 / ADR-008) ──────────

describe("delta frame shape and offset advance", function () {
  beforeEach(freshTmp);
  afterEach(cleanup);

  it("test_normal_frame_declares_last_offset", async function () {
    const p = transcriptFile(Buffer.from("hello world\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", "claude", config());
      assert.strictEqual(out.attempted, true);
      const f = sb.calls[0].frame;
      assert.strictEqual(f.type, "RecordEvent");
      assert.strictEqual(f.event_type, "transcript_delta");
      assert.strictEqual(f.session_id, "s1");
      assert.strictEqual(f.provider, "claude");
      assert.strictEqual(f.payload.offset, 0, "declared offset == last_offset (span start)");
      assert.strictEqual(f.payload.bytes, "hello world\n");
      const advanced = f.payload.offset + Buffer.byteLength(f.payload.bytes, "utf8");
      assert.strictEqual(readPersistedOffset("s1"), advanced);
      assert.strictEqual(readPersistedOffset("s1"), 12);
    } finally {
      sb.restore();
    }
  });

  it("test_topic_signal_omitted_provider_present", async function () {
    const p = transcriptFile(Buffer.from("data\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "s1", "gemini", config());
      const f = sb.calls[0].frame;
      assert.ok(!("topic_signal" in f), "topic_signal omitted");
      assert.strictEqual(f.provider, "gemini");
      // payload is exactly {offset, bytes}.
      assert.deepStrictEqual(Object.keys(f.payload).sort(), ["bytes", "offset"]);
    } finally {
      sb.restore();
    }
  });

  it("test_provider_null_omits_key", async function () {
    const p = transcriptFile(Buffer.from("data\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "s1", null, config());
      const f = sb.calls[0].frame;
      assert.ok(!("provider" in f), "provider key omitted when null");
    } finally {
      sb.restore();
    }
  });

  it("test_no_growth_no_post", async function () {
    const p = transcriptFile(Buffer.from("seed\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "s1", null, config()); // ships, offset=5
      assert.strictEqual(readPersistedOffset("s1"), 5);
      const before = sb.calls.length;
      const out = await delta.maybeSendDelta(p, "s1", null, config()); // no growth
      assert.strictEqual(out.attempted, false);
      assert.strictEqual(out.reason, "unchanged");
      assert.strictEqual(sb.calls.length, before, "no POST when file_len == last_offset");
    } finally {
      sb.restore();
    }
  });

  it("test_elided_frame_end_anchored", async function () {
    // > 64 KiB span of ASCII → single elided frame, end-anchored.
    const fileLen = 200000;
    const buf = Buffer.alloc(fileLen, 0x61); // 'a'
    const p = transcriptFile(buf);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "sx", null, config());
      assert.strictEqual(out.attempted, true);
      assert.strictEqual(sb.calls.length, 1, "SINGLE frame");
      const f = sb.calls[0].frame;
      const bytes = f.payload.bytes;
      const byteLen = Buffer.byteLength(bytes, "utf8");
      // bytes = head ++ marker ++ tail.
      assert.ok(bytes.includes(" bytes elided]"), "elision marker present");
      assert.ok(
        bytes.startsWith(repeat("a", 100)),
        "head is span-start content"
      );
      assert.ok(bytes.endsWith(repeat("a", 100)), "tail is file-end content");
      // End-anchored: declared offset == file_len - byteLength(bytes).
      // (Pure ASCII → effectiveEnd == fileLen.)
      assert.strictEqual(f.payload.offset, fileLen - byteLen);
      assert.notStrictEqual(f.payload.offset, 0, "NOT span start last_offset");
      // offset + byteLen == file_len.
      assert.strictEqual(f.payload.offset + byteLen, fileLen);
      // Persisted offset advances to file_len (uniform rule).
      assert.strictEqual(readPersistedOffset("sx"), fileLen);
    } finally {
      sb.restore();
    }
  });

  it("test_elided_bytes_never_resent", async function () {
    const fileLen = 200000;
    const p = transcriptFile(Buffer.alloc(fileLen, 0x62)); // 'b'
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "sx", null, config());
      assert.strictEqual(readPersistedOffset("sx"), fileLen);
      // Append more; next delta starts at file_len, not re-sending elided bytes.
      fs.appendFileSync(p, Buffer.from("tail-extension\n", "utf8"));
      const before = sb.calls.length;
      await delta.maybeSendDelta(p, "sx", null, config());
      assert.strictEqual(sb.calls.length, before + 1);
      const f = sb.calls[before].frame;
      assert.strictEqual(f.payload.offset, fileLen, "next delta extends at file_len");
      assert.strictEqual(f.payload.bytes, "tail-extension\n");
    } finally {
      sb.restore();
    }
  });

  it("test_elision_truncation_at_multibyte_boundary", async function () {
    // Fill so the 48 KiB head-cut and 12 KiB tail-cut each land mid-char.
    // Use 3-byte chars throughout; 49152 % 3 == 0 and 12288 % 3 == 0, so shift
    // by inserting a 1-byte char up front to force misalignment.
    const body3 = repeat(SNOWMAN, 70000); // ~210 KB of 3-byte chars
    const buf = Buffer.concat([Buffer.from("x", "utf8"), Buffer.from(body3, "utf8")]);
    const p = transcriptFile(buf);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "sm", null, config());
      assert.strictEqual(out.attempted, true);
      const f = sb.calls[0].frame;
      // Byte-safe: re-encoding the shipped string must contain no replacement
      // char, and head/tail must be clean multi-byte sequences.
      assert.ok(!f.payload.bytes.includes(String.fromCharCode(0xfffd)), "no U+FFFD");
      // Round-trip: payload.bytes encodes to valid UTF-8 (no partial sequences).
      const reb = Buffer.from(f.payload.bytes, "utf8").toString("utf8");
      assert.strictEqual(reb, f.payload.bytes, "stable UTF-8 round trip");
    } finally {
      sb.restore();
    }
  });

  it("test_elision_file_ends_mid_char", async function () {
    // File ends mid-write during elision: effectiveEnd < fileLen by ≤3. The
    // declared offset is end-anchored to effectiveEnd (last COMPLETE char), and
    // the trailing partial bytes are left for the next spawn.
    const body = repeat(SNOWMAN, 70000); // 3-byte chars, ~210 KB
    const partial = Buffer.from(EMOJI, "utf8").subarray(0, 2); // 2 dangling bytes
    const buf = Buffer.concat([Buffer.from(body, "utf8"), partial]);
    const fileLen = buf.length;
    const p = transcriptFile(buf);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "sm", null, config());
      assert.strictEqual(out.attempted, true);
      const f = sb.calls[0].frame;
      const byteLen = Buffer.byteLength(f.payload.bytes, "utf8");
      // effectiveEnd = fileLen - 2 (the 2 dangling bytes are not anchored to).
      const effectiveEnd = fileLen - 2;
      assert.strictEqual(f.payload.offset, effectiveEnd - byteLen);
      assert.strictEqual(f.payload.offset + byteLen, effectiveEnd);
      // Persisted offset advances to effectiveEnd (NOT fileLen), so the dangling
      // bytes get re-derived next spawn.
      assert.strictEqual(readPersistedOffset("sm"), effectiveEnd);
      assert.ok(!f.payload.bytes.includes(String.fromCharCode(0xfffd)), "no U+FFFD");
    } finally {
      sb.restore();
    }
  });

  it("test_post_serialization_1mib_assert", async function () {
    // Escape-heavy content: quotes, backslashes, control chars inflate the
    // JSON serialization. Build a >64 KiB span of escape-dense bytes.
    const unit = '"\\' + String.fromCharCode(1) + String.fromCharCode(2) + "\t\n"; // 6 chars, all escaped
    const dense = repeat(unit, 40000); // ~240 KB raw, ~6x inflation when serialized
    const p = transcriptFile(Buffer.from(dense, "utf8"));
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "se", null, config());
      assert.strictEqual(out.attempted, true);
      const bodyBuf = sb.calls[0].bodyBuf;
      assert.ok(bodyBuf.length < 1048576, "serialized frame < 1 MiB: " + bodyBuf.length);
      // Frame parses (well-formed JSON despite escape density).
      const f = JSON.parse(bodyBuf.toString("utf8"));
      assert.strictEqual(f.event_type, "transcript_delta");
    } finally {
      sb.restore();
    }
  });

  it("test_frame_matches_binding_shape", async function () {
    // RecordEvent + TranscriptDeltaPayload contract: flattened ImplantEvent,
    // payload exactly {offset:number, bytes:string}.
    const p = transcriptFile(Buffer.from("contract\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "sid", "claude", config());
      const f = sb.calls[0].frame;
      assert.strictEqual(f.type, "RecordEvent");
      assert.strictEqual(typeof f.event_type, "string");
      assert.strictEqual(typeof f.session_id, "string");
      assert.strictEqual(typeof f.timestamp, "number");
      assert.strictEqual(typeof f.payload, "object");
      assert.strictEqual(typeof f.payload.offset, "number");
      assert.strictEqual(typeof f.payload.bytes, "string");
      assert.ok(Number.isSafeInteger(f.payload.offset));
    } finally {
      sb.restore();
    }
  });
});

// ── Failure semantics (ADR-004 — amended AC-15) ─────────────────────

describe("delta failure semantics (ADR-004)", function () {
  beforeEach(freshTmp);
  afterEach(cleanup);

  it("test_delta_failure_no_advance_no_queue", async function () {
    const p = transcriptFile(Buffer.from("payload one\n", "utf8"));
    const sb = stubPost(() => failResult("http_5xx", 503));
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(out.attempted, true);
      assert.strictEqual(out.send.ok, false);
      // Offset file did NOT advance — it was never written (still 0 → no file).
      assert.strictEqual(offsetFileExists("s1"), false, "offset not advanced");
      // NO queue file for the delta (amended AC-15 letter).
      assert.strictEqual(queueFiles().length, 0, "deltas are never queued");
    } finally {
      sb.restore();
    }
  });

  it("test_delta_failure_then_redrive_larger_span", async function () {
    const p = transcriptFile(Buffer.from("first\n", "utf8"));
    let mode = "fail";
    const sb = stubPost(() => (mode === "fail" ? failResult("connect", 0) : OK));
    try {
      await delta.maybeSendDelta(p, "s1", null, config()); // fails, offset stays 0
      assert.strictEqual(offsetFileExists("s1"), false);
      // File grows; next spawn re-derives the LARGER span [0, newLen).
      fs.appendFileSync(p, Buffer.from("second\n", "utf8"));
      mode = "ok";
      await delta.maybeSendDelta(p, "s1", null, config());
      const f = sb.calls[sb.calls.length - 1].frame;
      assert.strictEqual(f.payload.offset, 0, "re-derive from last_offset 0");
      assert.strictEqual(f.payload.bytes, "first\nsecond\n", "larger span shipped");
      assert.strictEqual(readPersistedOffset("s1"), 13);
    } finally {
      sb.restore();
    }
  });

  it("test_livelock_bounded", async function () {
    // R-07: permanent 413 then 401 on the delta path. Offset never advances, no
    // queue file ever, per-spawn cost is exactly one fstat + one POST.
    const p = transcriptFile(Buffer.alloc(100000, 0x63)); // > soft cap → elided
    let i = 0;
    const sb = stubPost(() => (i++ < 1 ? failResult("http_4xx", 413) : failResult("auth", 401)));
    try {
      for (let spawn = 0; spawn < 5; spawn++) {
        const out = await delta.maybeSendDelta(p, "s1", null, config());
        assert.strictEqual(out.attempted, true);
        assert.strictEqual(out.send.ok, false);
        assert.strictEqual(offsetFileExists("s1"), false, "offset never advances");
        assert.strictEqual(queueFiles().length, 0, "no queue file");
      }
      // Exactly one POST per spawn (no growth in work).
      assert.strictEqual(sb.calls.length, 5, "one POST per spawn");
    } finally {
      sb.restore();
    }
  });
});

// ── Rewrite guard + TOCTOU (FR-11 / A-4) ────────────────────────────

describe("delta rewrite guard and TOCTOU", function () {
  beforeEach(freshTmp);
  afterEach(cleanup);

  it("test_rewrite_guard", async function () {
    const p = transcriptFile(Buffer.from("0123456789abcdef\n", "utf8")); // 17 bytes
    const sb = stubPost(() => OK);
    try {
      await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(readPersistedOffset("s1"), 17);
      // Shrink the file below the persisted offset.
      fs.writeFileSync(p, Buffer.from("short\n", "utf8")); // 6 bytes
      const before = sb.calls.length;
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(out.attempted, false);
      assert.strictEqual(out.reason, "rewind");
      assert.strictEqual(sb.calls.length, before, "nothing shipped");
      // Offset RESET to new file_len, never a negative span.
      assert.strictEqual(readPersistedOffset("s1"), 6);
    } finally {
      sb.restore();
    }
  });

  it("test_toctou_delete_between_stat_and_read", async function () {
    // Stat succeeds, then the open/read fails because the file is gone. Simulate
    // by pointing at a file that statSync can see but readSync cannot: delete it
    // after we capture size via a custom path that no longer exists at open.
    // Practically: write file, record offset 0, then in buildDeltaFrame the
    // open fails. We force this by deleting between our own stat and the call —
    // emulate by stubbing nothing and deleting a file whose dir we keep.
    const p = transcriptFile(Buffer.from("about to vanish\n", "utf8"));
    const sb = stubPost(() => OK);
    // Monkeypatch statSync to report a size for a path we then delete.
    const realStat = fs.statSync;
    fs.statSync = function (target) {
      if (target === p) {
        try {
          fs.unlinkSync(p);
        } catch (_e) {
          /* ignore */
        }
        return { size: 16 };
      }
      return realStat.apply(fs, arguments);
    };
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      // Stat said grow; open fails → empty_span (ship nothing).
      assert.strictEqual(out.attempted, false);
      assert.strictEqual(out.reason, "empty_span");
      assert.strictEqual(sb.calls.length, 0, "no POST");
      assert.strictEqual(offsetFileExists("s1"), false, "offset unchanged");
    } finally {
      fs.statSync = realStat;
      sb.restore();
    }
  });

  it("test_stat_missing_file", async function () {
    const p = path.join(tmpRoot, "does-not-exist.jsonl");
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(out.attempted, false);
      assert.strictEqual(out.reason, "stat");
      assert.strictEqual(sb.calls.length, 0);
    } finally {
      sb.restore();
    }
  });

  it("test_corrupt_offset_file", async function () {
    const p = transcriptFile(Buffer.from("clean\n", "utf8"));
    // Pre-seed a corrupt offset file.
    fs.mkdirSync(path.join(stateDir(), "offsets"), { recursive: true });
    fs.writeFileSync(offsetFilePath("s1"), "{not json", "utf8");
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      assert.strictEqual(out.attempted, true, "corrupt offset → treated as 0, re-ship");
      assert.strictEqual(sb.calls[0].frame.payload.offset, 0);
      assert.strictEqual(sb.calls[0].frame.payload.bytes, "clean\n");
    } finally {
      sb.restore();
    }
  });

  it("test_binary_transcript_content_opaque", async function () {
    // Random binary, > soft cap → caps/elides without throwing.
    const bin = Buffer.alloc(120000);
    for (let i = 0; i < bin.length; i++) bin[i] = (i * 31 + 7) & 0xff;
    const p = transcriptFile(bin);
    const sb = stubPost(() => OK);
    try {
      const out = await delta.maybeSendDelta(p, "s1", null, config());
      // Either ships (elided) or skips, but never throws.
      assert.ok(out.attempted === true || out.attempted === false);
      if (out.attempted) {
        assert.strictEqual(out.send.ok, true);
      }
    } finally {
      sb.restore();
    }
  });
});

// ── Concurrency (R-05) ──────────────────────────────────────────────

describe("delta concurrency (R-05)", function () {
  beforeEach(freshTmp);
  afterEach(cleanup);

  it("test_concurrent_spawn_offset_race", async function () {
    const p = transcriptFile(Buffer.from("racey content here\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      // Two interleaved spawns of one session.
      const [a, b] = await Promise.all([
        delta.maybeSendDelta(p, "s1", null, config()),
        delta.maybeSendDelta(p, "s1", null, config()),
      ]);
      assert.strictEqual(a.attempted, true);
      assert.strictEqual(b.attempted, true);
      // Offset file is always valid JSON (atomic rename), final value is span end.
      const off = readPersistedOffset("s1");
      assert.strictEqual(off, 19);
    } finally {
      sb.restore();
    }
  });

  it("test_independence_via_promise_allsettled", async function () {
    // ADR-007 orchestration shape: delta issued concurrently with a carrying
    // event via Promise.allSettled, independent outcomes. Here we model the
    // carrying POST as a separate stubbed promise and assert the delta outcome
    // is unaffected by the carrying result.
    const p = transcriptFile(Buffer.from("carry\n", "utf8"));
    const sb = stubPost(() => OK);
    try {
      const carrying = Promise.resolve(failResult("connect", 0)); // carrying fails
      const settled = await Promise.allSettled([
        carrying,
        delta.maybeSendDelta(p, "s1", null, config()),
      ]);
      assert.strictEqual(settled[0].status, "fulfilled");
      assert.strictEqual(settled[0].value.ok, false, "carrying failed");
      assert.strictEqual(settled[1].status, "fulfilled");
      assert.strictEqual(settled[1].value.attempted, true);
      assert.strictEqual(settled[1].value.send.ok, true, "delta succeeded independently");
      // Delta offset advanced despite carrying failure.
      assert.strictEqual(readPersistedOffset("s1"), 6);
    } finally {
      sb.restore();
    }
  });
});
