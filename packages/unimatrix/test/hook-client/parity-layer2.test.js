"use strict";

// ─────────────────────────────────────────────────────────────────────────
// Layer 2 integration suite — real client modules against the MERGED F2 server
// (vnc-025, PR #692 — C-08 satisfied). Owned by delta.md + parity-corpus.md
// Layer 2. Covers:
//   AC-05  Layer 2 content-equivalence modulo elision markers under streamed
//          deltas + injected drops.
//   AC-06  grow/hold transcript across spawns: POST presence/absence, declared
//          offset, AND persisted offset values.
//   AC-07  Layer-2 elision-mid-session run → correct PreCompact restoration with
//          the four pinned ADR-008 server-state assertions (verified through the
//          wire-observable PreCompact block — see "Observability note").
// AC-10 concurrency lives in parity-layer2-concurrency.test.js; the Layer 1
// PreCompact byte-identity half of AC-05 lives in parity-layer2-precompact.test.js.
//
// ── Observability note (C-07 — NO server-side production changes) ─────────────
// TranscriptBuffer's holes / high_water / base_offset / elided_bytes are PRIVATE
// and #[cfg(test)]-only (session_transcript.rs) — NOT reachable over the wire.
// The only content-bearing wire surface is the server-side PreCompact
// restoration block built from `TranscriptBuffer::contiguous_tail`
// (listener.rs::handle_compact_payload → BriefingContent → text/plain). The four
// pinned ADR-008 items (ADR-008 / brief WARN gate-note 3) are therefore asserted
// through their OBSERVABLE CONSEQUENCES in that block:
//   1. hole BEHIND content / high_water == file_len  → the span-start-anchor bug
//      (rejected alt) starves PreCompact right after the elision; end-anchored
//      frames keep it servable. Asserted: PreCompact serves real tail content
//      immediately after the elided frame.
//   2. high_water == file_len (coverage agreement)   → subsequent deltas extend
//      contiguously at file_len with no phantom hole. Asserted: post-elision
//      delta content reaches PreCompact.
//   3. contiguous_tail crosses the seam / pure client-tail → the tail bytes
//      shipped in the elided frame appear in the block; once a later delta
//      extends, the block crosses the seam naturally.
//   4. no NUL bytes EVER served (zero-fill never escapes contiguous_tail, R-06)
//      → scan every PreCompact body for 0x00.
// This is the legitimate query surface the brief mandates ("use existing server
// APIs only (C-07)").

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const { startRealServer } = require("../helpers/real-server");
const delta = require("../../lib/hook-client/delta");
const state = require("../../lib/hook-client/state");
const {
  exchange,
  liveConfig,
  deadConfig,
  spawnDelta,
  register,
  appendTranscript,
} = require("../helpers/layer2-fixtures");

// ── per-suite shared server (start once; cheap to reuse across cases) ────────
let server;
before(async () => {
  server = await startRealServer({ startTimeoutMs: 60000 });
});
after(async () => {
  if (server) await server.close();
});

// ── per-case temp transcript + state dir ────────────────────────────────────
let tmpRoot;
function freshTmp(tag) {
  tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-l2-" + (tag || "") + "-"));
  return tmpRoot;
}
function rmTmp() {
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
function transcriptPath() {
  return path.join(tmpRoot, "transcript.jsonl");
}
function readOffset(sessionId) {
  return state.readOffset(stateDir(), sessionId);
}
function liveSpawn(sessionId) {
  return spawnDelta(transcriptPath(), sessionId, liveConfig(server, stateDir()));
}
function append(data) {
  appendTranscript(transcriptPath(), data);
}
function precompact(sessionId) {
  return server.precompact(sessionId, { token_limit: 4000 });
}

// ═════════════════════════════════════════════════════════════════════════
// AC-06 — grow/hold transcript across spawns: POST presence/absence + declared
// offset + PERSISTED offset values (AC-06 explicitly requires asserting offset
// VALUES, not just POST presence — delta.md test_property_contiguous_prefix).
// ═════════════════════════════════════════════════════════════════════════

describe("Layer 2 — AC-06 grow/hold across spawns (offset values)", () => {
  it("test_l2_grow_hold_grow_offset_values", async () => {
    freshTmp("ac06");
    const sid = "l2-ac06-grow";
    await register(server, sid);
    fs.writeFileSync(transcriptPath(), "");

    // Spawn 1: empty file → NO POST (fstat-gated), offset stays 0.
    let out = await liveSpawn(sid);
    assert.strictEqual(out.attempted, false, "empty file: no POST");
    assert.strictEqual(readOffset(sid), 0, "offset unchanged on empty");

    // Grow by exchange A, spawn 2: one POST, offset advances to file length.
    append(exchange(sid, 1));
    const lenAfterA = fs.statSync(transcriptPath()).size;
    out = await liveSpawn(sid);
    assert.strictEqual(out.attempted, true, "growth: a POST is attempted");
    assert.ok(out.send.ok, "server Acked the delta");
    assert.strictEqual(
      readOffset(sid),
      lenAfterA,
      "persisted offset == file length after first growth"
    );

    // HOLD: no growth, spawn 3: NO POST, offset held.
    out = await liveSpawn(sid);
    assert.strictEqual(out.attempted, false, "no growth → no POST (AC-06 hold)");
    assert.strictEqual(out.reason, "unchanged");
    assert.strictEqual(readOffset(sid), lenAfterA, "offset held across the no-op spawn");

    // Grow again by exchange B, spawn 4: POST, offset advances to new length.
    append(exchange(sid, 2));
    const lenAfterB = fs.statSync(transcriptPath()).size;
    out = await liveSpawn(sid);
    assert.strictEqual(out.attempted, true);
    assert.ok(out.send.ok);
    assert.strictEqual(readOffset(sid), lenAfterB, "offset == new file length after second growth");

    // The catch-up reached the server: PreCompact serves the newest exchange.
    const pc = await precompact(sid);
    assert.strictEqual(pc.status, 200);
    assert.strictEqual(pc.contentType, "text/plain");
    assert.ok(!pc.raw.includes(0), "no NUL bytes served (R-06)");
    assert.ok(
      pc.text.includes(sid + " assistant reply 2"),
      "PreCompact reflects the most recent shipped exchange"
    );
    rmTmp();
  });

  it("test_l2_adversarial_growth_sequence_contiguous_prefix", async () => {
    // delta.md test_growth_replay_sequence / test_property_contiguous_prefix:
    // adversarial increments across many spawns; persisted offset always equals
    // the contiguous prefix length, and the final PreCompact reflects the tail.
    freshTmp("ac06seq");
    const sid = "l2-ac06-seq";
    await register(server, sid);
    fs.writeFileSync(transcriptPath(), "");

    let lastTag = "";
    for (let i = 0; i < 12; i += 1) {
      lastTag = sid + " assistant reply " + i;
      append(exchange(sid, i));
      const flen = fs.statSync(transcriptPath()).size;
      const out = await liveSpawn(sid);
      assert.strictEqual(out.attempted, true, "spawn " + i + " ships growth");
      assert.ok(out.send.ok, "spawn " + i + " Acked");
      // AC-06: persisted offset == contiguous prefix length (the whole file).
      assert.strictEqual(readOffset(sid), flen, "offset == prefix length at spawn " + i);
    }
    const pc = await precompact(sid);
    assert.ok(!pc.raw.includes(0), "no NUL bytes served");
    assert.ok(pc.text.includes(lastTag), "final PreCompact reflects the last shipped exchange");
    rmTmp();
  });
});

// ═════════════════════════════════════════════════════════════════════════
// AC-05 — streamed deltas with injected drops → content-equivalence modulo
// elision markers (FR-24). A "drop" = a spawn whose POST fails (offset does NOT
// advance, ADR-004); the NEXT spawn re-derives [last_offset, file_len) and the
// content still lands. We inject the drop by pointing one spawn at a dead URL.
// ═════════════════════════════════════════════════════════════════════════

describe("Layer 2 — AC-05 streamed deltas with injected drops", () => {
  it("test_l2_drops_content_equivalence", async () => {
    freshTmp("ac05");
    const sid = "l2-ac05-drops";
    await register(server, sid);
    fs.writeFileSync(transcriptPath(), "");

    let lastTag = "";
    for (let i = 0; i < 8; i += 1) {
      lastTag = sid + " assistant reply " + i;
      append(exchange(sid, i));
      const flen = fs.statSync(transcriptPath()).size;
      if (i % 3 === 1) {
        // Injected drop: this spawn fails to send.
        const out = await spawnDelta(transcriptPath(), sid, deadConfig(server, stateDir()));
        assert.strictEqual(out.attempted, true, "drop spawn attempted a POST");
        assert.ok(!out.send.ok, "drop spawn POST failed");
        // ADR-004: offset must NOT advance on a failed delta.
        assert.ok(readOffset(sid) < flen, "failed delta did NOT advance offset");
      } else {
        const out = await liveSpawn(sid);
        assert.strictEqual(out.attempted, true);
        assert.ok(out.send.ok, "live spawn Acked at i=" + i);
        // After a successful spawn the offset re-derives to the full prefix —
        // the dropped span is re-shipped contiguously (no gap).
        assert.strictEqual(readOffset(sid), flen, "offset re-derives to full prefix at i=" + i);
      }
    }

    // Reconciliation spawn: if the loop ended on a drop, the re-derive path
    // ships the still-unshipped tail (ADR-004). After this, the full prefix has
    // landed regardless of where drops fell.
    const reconcile = await liveSpawn(sid);
    if (reconcile.attempted) {
      assert.ok(reconcile.send.ok, "reconciliation spawn Acked");
    }
    const finalLen = fs.statSync(transcriptPath()).size;
    assert.strictEqual(readOffset(sid), finalLen, "offset reconciles to full prefix after drops");

    // Content-equivalence: despite the drops, every exchange landed (re-derive),
    // and PreCompact serves the most-recent content with no NUL fill.
    const pc = await precompact(sid);
    assert.strictEqual(pc.status, 200);
    assert.ok(!pc.raw.includes(0), "no NUL bytes served under drops (R-06)");
    assert.ok(pc.text.includes(lastTag), "final content present despite injected drops");
    rmTmp();
  });
});

// ═════════════════════════════════════════════════════════════════════════
// AC-07 — Layer-2 elision-mid-session run with the FOUR pinned ADR-008
// assertions (gate-binding). See the Observability note at the top of the file
// for how each pinned item maps to an observable PreCompact consequence.
// ═════════════════════════════════════════════════════════════════════════

describe("Layer 2 — AC-07 elision-mid-session (four pinned ADR-008 items)", () => {
  it("test_l2_elision_mid_session", async () => {
    freshTmp("ac07");
    const sid = "l2-ac07-elision";
    await register(server, sid);
    fs.writeFileSync(transcriptPath(), "");

    // 1. Pre-outage: ship one normal exchange so last_offset > 0.
    append(exchange(sid, "pre"));
    let out = await liveSpawn(sid);
    assert.ok(out.attempted && out.send.ok, "pre-outage delta Acked");
    const offsetBeforeOutage = readOffset(sid);

    // 2. OUTAGE: the file grows by WAY more than DELTA_SOFT_CAP (64 KiB) while no
    //    spawn ships. Build a large run of exchanges, then a RECOGNIZABLE tail
    //    exchange as the very last lines (these become the elided frame's tail
    //    + contiguous_tail window).
    let bulk = "";
    let n = 0;
    while (Buffer.byteLength(bulk, "utf8") < 120 * 1024) {
      bulk += exchange(sid, "bulk-" + n);
      n += 1;
    }
    append(bulk);
    const tailTag = sid + "-ELISION-TAIL";
    append(
      JSON.stringify({
        type: "user",
        message: { content: [{ type: "text", text: tailTag + " question" }] },
      }) +
        "\n" +
        JSON.stringify({
          type: "assistant",
          message: { content: [{ type: "text", text: tailTag + " answer" }] },
        }) +
        "\n"
    );
    const fileLenAtElision = fs.statSync(transcriptPath()).size;
    assert.ok(
      fileLenAtElision - offsetBeforeOutage > delta.DELTA_SOFT_CAP,
      "outage span exceeds the elision soft cap"
    );

    // 3. Catch-up spawn: ships a SINGLE end-anchored elided frame.
    out = await liveSpawn(sid);
    assert.strictEqual(out.attempted, true, "catch-up attempted");
    assert.ok(out.send.ok, "catch-up Acked");

    // ADR-008 end-anchored: persisted offset advances to file_len exactly
    // (declared offset = file_len − byteLen; advance = offset + byteLen =
    // file_len). This is the client-side proof of pinned item (b): high_water ==
    // file_len (server coverage == client last_offset). We additionally assert
    // it is NOT the span-start anchor.
    assert.strictEqual(
      readOffset(sid),
      fileLenAtElision,
      "PINNED (b): last_offset advances to file_len after the elided frame"
    );
    assert.notStrictEqual(
      readOffset(sid),
      offsetBeforeOutage,
      "elided frame is NOT declared at the span-start offset (ADR-008 rejected alt)"
    );

    // PINNED (a)+(c): PreCompact must serve REAL client-tail content right after
    // the elided frame (the span-start-anchor bug would starve here — the whole
    // catch-up unservable). The recognizable tail exchange must appear.
    let pc = await precompact(sid);
    assert.strictEqual(pc.status, 200, "PreCompact served (not starved) after elision");
    assert.strictEqual(pc.contentType, "text/plain");
    // PINNED (4): no NUL bytes ever served (zero-fill never escapes).
    assert.ok(!pc.raw.includes(0), "PINNED (4): no NUL bytes served after elision");
    assert.ok(
      pc.text.includes(tailTag),
      "PINNED (a/c): PreCompact serves the elided frame's client-tail content"
    );
    // The elision MARKER itself is a non-JSON line → filtered by the JSONL block
    // builder; it must NOT leak into the restoration block.
    assert.ok(
      !pc.text.includes("bytes elided"),
      "elision marker filtered out of the restoration block"
    );

    // 4. POST-ELISION CONTIGUITY (pinned item c, second half): a subsequent
    //    NORMAL delta extends at file_len with no further hole; PreCompact then
    //    crosses the elision seam naturally and serves the new content.
    const postTag = sid + "-POST-ELISION";
    append(
      JSON.stringify({
        type: "user",
        message: { content: [{ type: "text", text: postTag + " q" }] },
      }) +
        "\n" +
        JSON.stringify({
          type: "assistant",
          message: { content: [{ type: "text", text: postTag + " a" }] },
        }) +
        "\n"
    );
    const fileLenPost = fs.statSync(transcriptPath()).size;
    out = await liveSpawn(sid);
    assert.strictEqual(out.attempted, true, "post-elision delta attempted");
    assert.ok(out.send.ok, "post-elision delta Acked");
    assert.strictEqual(
      readOffset(sid),
      fileLenPost,
      "post-elision delta extends contiguously to the new file_len (no phantom hole)"
    );

    pc = await precompact(sid);
    assert.strictEqual(pc.status, 200, "PreCompact still served after post-elision delta");
    assert.ok(!pc.raw.includes(0), "PINNED (4): no NUL bytes served after post-elision delta");
    assert.ok(
      pc.text.includes(postTag),
      "PINNED (c): post-elision content reaches PreCompact (seam crossed contiguously)"
    );
    rmTmp();
  });
});
