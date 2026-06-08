"use strict";

// UDS framing parity (vnc-027, parity-corpus-uds layer a — AC-01, R-18).
//
// The Rust wire (`wire.rs::write_frame` + `serialize_request`/
// `serialize_response`) is the oracle; committed goldens live under
// test/fixtures/parity/uds-framing/ (regen via scripts/regen-parity.sh, CI
// zero-diff drift gate). Each golden is a pair:
//   <name>.payload.json -- the EXACT serialized wire bytes (serde compact)
//   <name>.frame.bin    -- write_frame(payload) = 4-byte BE u32 len + payload
//
// Write direction: transportUds.encodeFrame(JSON.parse(payload)) must byte-equal
// the committed frame -- proving BOTH JSON serialization parity (serde compact
// == JSON.stringify) AND framing parity against wire.rs.
// Read direction: the committed response frames decode (4-byte BE header + body)
// and map through mapHookResponse to the ADR-002 SendResult.
//
// Boundary cases at EXACTLY 1,048,576 B run in both directions (R-18). No
// listener (offline byte-compare); live round-trip is Stage 3c.
//
// Cumulative infra: reuses the committed corpus + the real transport module.

const { describe, it, before } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const transport = require("../../lib/hook-client/transport-uds");
const { encodeFrame, mapHookResponse, MAX_PAYLOAD_SIZE } = transport;

const FRAMING_DIR = path.join(__dirname, "..", "fixtures", "parity", "uds-framing");

function readGolden(name) {
  const payload = fs.readFileSync(path.join(FRAMING_DIR, name + ".payload.json"));
  const frame = fs.readFileSync(path.join(FRAMING_DIR, name + ".frame.bin"));
  return { payload, frame };
}

function loadManifest() {
  const p = path.join(FRAMING_DIR, "MANIFEST.json");
  assert.ok(fs.existsSync(p), "uds-framing/MANIFEST.json missing -- run scripts/regen-parity.sh");
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

describe("UDS framing parity (AC-01 / R-18)", function () {
  let manifest;

  before(function () {
    manifest = loadManifest();
    assert.ok(Array.isArray(manifest.requests) && manifest.requests.length > 0, "requests present");
    assert.ok(
      Array.isArray(manifest.responses) && manifest.responses.length > 0,
      "responses present"
    );
    assert.strictEqual(manifest.max_payload_size, MAX_PAYLOAD_SIZE, "1 MiB cap parity");
  });

  // ── write direction: encodeFrame == wire.rs write_frame ──────────────────

  describe("write direction", function () {
    it("test_request_frames_byte_identical_to_wire_rs", function () {
      const manifestNow = loadManifest();
      for (const name of manifestNow.requests) {
        const { payload, frame } = readGolden(name);
        // Header authority: 4-byte BE length == payload byte length.
        assert.strictEqual(
          frame.readUInt32BE(0),
          payload.length,
          name + ": frame header must declare the payload length"
        );
        assert.ok(
          frame.subarray(4).equals(payload),
          name + ": framed payload bytes must be verbatim"
        );
        // serde-compact == JSON.stringify (precise diagnostic on divergence).
        const parsed = JSON.parse(payload.toString("utf8"));
        assert.strictEqual(
          JSON.stringify(parsed),
          payload.toString("utf8"),
          name + ": JSON.stringify must reproduce the serde compact wire bytes"
        );
        // The client's actual encoder must produce the committed frame.
        const encoded = encodeFrame(parsed, {});
        assert.ok(
          encoded.equals(frame),
          name + ": encodeFrame output diverges from the wire.rs golden"
        );
      }
    });

    it("test_sync_accept_injection_matches_wire_golden", function () {
      // Sync injection-bearing frames carry accept:"text/plain". Deriving the
      // pre-injection request from the golden (delete accept) and re-encoding
      // with {sync:true} must reproduce the committed wire bytes -- proving the
      // client's serialization-time injection matches the Rust wire shape.
      const manifestNow = loadManifest();
      let checked = 0;
      for (const name of manifestNow.requests) {
        const { payload, frame } = readGolden(name);
        const parsed = JSON.parse(payload.toString("utf8"));
        if (parsed.accept !== "text/plain") continue;
        delete parsed.accept;
        const encoded = encodeFrame(parsed, { sync: true });
        assert.ok(
          encoded.equals(frame),
          name + ": sync accept injection diverges from the wire golden"
        );
        checked += 1;
      }
      assert.ok(checked >= 2, "expected >=2 sync injection-bearing frames (ContextSearch + Compact)");
    });

    it("test_write_boundary_exactly_1mib", function () {
      const { payload, frame } = readGolden("req-boundary-1mib");
      assert.strictEqual(payload.length, MAX_PAYLOAD_SIZE, "boundary payload is exactly 1 MiB");
      assert.strictEqual(frame.length, 4 + MAX_PAYLOAD_SIZE);
      const encoded = encodeFrame(JSON.parse(payload.toString("utf8")), {});
      assert.ok(encoded.equals(frame), "exactly-1-MiB frame must encode byte-identically");
    });

    it("test_write_over_1mib_rejected_no_frame", function () {
      // One byte past the cap -> client-side reject, no frame produced (R-18 s3).
      const { payload } = readGolden("req-boundary-1mib");
      const parsed = JSON.parse(payload.toString("utf8"));
      parsed.query += "a"; // 1 byte over MAX
      assert.strictEqual(
        Buffer.byteLength(JSON.stringify(parsed), "utf8"),
        MAX_PAYLOAD_SIZE + 1
      );
      assert.strictEqual(encodeFrame(parsed, {}), null);
    });
  });

  // ── read direction: committed response frames decode + map ───────────────

  describe("read direction", function () {
    function decodeBody(frame, payload) {
      const declared = frame.readUInt32BE(0);
      assert.strictEqual(declared, payload.length, "header declares the body length");
      const body = frame.subarray(4, 4 + declared);
      assert.ok(body.equals(payload), "framed body bytes verbatim");
      return JSON.parse(body.toString("utf8"));
    }

    it("test_text_entries_maps_200_text_plain_with_header", function () {
      const { payload, frame } = readGolden("res-text-entries");
      const res = mapHookResponse(decodeBody(frame, payload));
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.status, 200);
      assert.strictEqual(res.contentType, "text/plain");
      assert.ok(
        res.body.toString("utf8").startsWith("--- Unimatrix Context ---\n"),
        "Entries Text body carries the load-bearing injection header"
      );
    });

    it("test_text_briefing_maps_200_text_plain_verbatim", function () {
      const { payload, frame } = readGolden("res-text-briefing");
      const res = mapHookResponse(decodeBody(frame, payload));
      assert.strictEqual(res.status, 200);
      assert.strictEqual(res.contentType, "text/plain");
      assert.ok(!res.body.toString("utf8").startsWith("--- Unimatrix Context ---"));
    });

    it("test_ack_maps_204_null", function () {
      const { payload, frame } = readGolden("res-ack");
      const res = mapHookResponse(decodeBody(frame, payload));
      assert.deepStrictEqual(res, {
        ok: true,
        status: 204,
        contentType: null,
        body: null,
        failureClass: null,
      });
    });

    it("test_pong_maps_200_application_json", function () {
      const { payload, frame } = readGolden("res-pong");
      const res = mapHookResponse(decodeBody(frame, payload));
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.status, 200);
      assert.strictEqual(res.contentType, "application/json");
    });

    it("test_error_frames_map_to_4xx_and_5xx", function () {
      const e4 = readGolden("res-error-4xx");
      const r4 = mapHookResponse(decodeBody(e4.frame, e4.payload));
      assert.strictEqual(r4.ok, false);
      assert.strictEqual(r4.failureClass, "http_4xx");
      assert.strictEqual(r4.status, 400);

      const e5 = readGolden("res-error-5xx");
      const r5 = mapHookResponse(decodeBody(e5.frame, e5.payload));
      assert.strictEqual(r5.failureClass, "http_5xx");
      assert.strictEqual(r5.status, 503);
    });

    it("test_read_boundary_exactly_1mib", function () {
      const { payload, frame } = readGolden("res-boundary-1mib");
      assert.strictEqual(payload.length, MAX_PAYLOAD_SIZE, "boundary response is exactly 1 MiB");
      assert.strictEqual(frame.readUInt32BE(0), MAX_PAYLOAD_SIZE);
      const res = mapHookResponse(decodeBody(frame, payload));
      assert.strictEqual(res.ok, true);
      assert.strictEqual(res.status, 200);
    });
  });
});
