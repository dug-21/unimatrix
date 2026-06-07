"use strict";

// AC-14 contract round-trip: CLIENT-produced frames vs the committed Rust ts-rs
// fixtures (crates/unimatrix-engine/bindings/fixtures/*.json).
//
// The existing crates/unimatrix-engine/bindings/contract.test.mjs proves the
// fixtures (Rust-emitted) narrow to the TS bindings. This suite extends that
// pattern to the OTHER direction: frames the JS hook client actually builds
// (buildRequest + the delta module) must carry the SAME wire shape as the
// fixtures -- discriminant, exact key set, field types, None-vs-omission. The
// fixtures remain the contract authority (C-01: never hand-mirrored).
//
// Includes transcript_delta_payload.json (the delta frame the client streams).
//
// Cumulative infra: reuses the real client modules and the bindings fixtures.

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const { buildRequest } = require("../../lib/hook-client/build-request");
const delta = require("../../lib/hook-client/delta");

const FIXTURES = path.resolve(
  __dirname,
  "../../../../crates/unimatrix-engine/bindings/fixtures"
);
const loadFixture = (name) => JSON.parse(fs.readFileSync(path.join(FIXTURES, name), "utf8"));

function mkInput(over) {
  return Object.assign(
    {
      hook_event_name: "",
      session_id: null,
      cwd: null,
      transcript_path: null,
      prompt: null,
      provider: "claude-code",
      mcp_context: null,
      extra: {},
    },
    over || {}
  );
}

// A client frame conforms to a fixture's contract iff: same discriminant, the
// SAME key set (None-vs-omission parity), and matching value TYPES per key.
// Values themselves differ (different inputs) -- the wire SHAPE is the contract.
function assertSameContract(frame, fixture, label) {
  assert.strictEqual(frame.type, fixture.type, label + ": discriminant must match");
  assert.deepStrictEqual(
    Object.keys(frame).sort(),
    Object.keys(fixture).sort(),
    label + ": key set must match the fixture exactly (None-vs-omission)"
  );
  for (const key of Object.keys(fixture)) {
    // `null` is the None form of an Option<T> on the wire -- type-compatible
    // with the fixture's populated value (Some) for the same field.
    if (frame[key] === null || fixture[key] === null) continue;
    const ft = Array.isArray(fixture[key]) ? "array" : typeof fixture[key];
    const at = Array.isArray(frame[key]) ? "array" : typeof frame[key];
    assert.strictEqual(at, ft, label + ": field " + key + " type must match the fixture");
  }
}

describe("AC-14 contract round-trip - client frames vs Rust fixtures", () => {
  it("test_ping_frame_matches_fixture", () => {
    const frame = buildRequest("Ping", mkInput());
    assertSameContract(frame, loadFixture("request_ping.json"), "Ping");
  });

  it("test_session_register_frame_matches_fixture", () => {
    const frame = buildRequest(
      "SessionStart",
      mkInput({
        session_id: "s1",
        cwd: "/work",
        extra: { agent_role: "developer", feature_cycle: "vnc-026" },
      })
    );
    assertSameContract(frame, loadFixture("request_session_register.json"), "SessionRegister");
  });

  it("test_session_close_frame_matches_fixture", () => {
    const frame = buildRequest("Stop", mkInput({ session_id: "s1" }));
    assertSameContract(frame, loadFixture("request_session_close.json"), "SessionClose");
  });

  it("test_record_event_frame_matches_fixture", () => {
    // A context_cycle start -> RecordEvent carrying BOTH topic_signal AND
    // provider present (the fixture's "present" shape).
    const frame = buildRequest(
      "PreToolUse",
      mkInput({
        session_id: "s1",
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "vnc-024", goal: "do the thing" },
        },
      })
    );
    assertSameContract(frame, loadFixture("request_record_event.json"), "RecordEvent");
  });

  it("test_context_search_frame_matches_fixture", () => {
    // SubagentStart with prompt_snippet -> ContextSearch carrying `source`.
    const frame = buildRequest(
      "SubagentStart",
      mkInput({
        session_id: "s1",
        extra: { agent_type: "developer", prompt_snippet: "explain the auth flow" },
      })
    );
    assertSameContract(frame, loadFixture("request_context_search.json"), "ContextSearch");
  });

  it("test_compact_payload_frame_matches_fixture_no_excerpt", () => {
    // PreCompact -> CompactPayload; the client never sends transcript_excerpt,
    // so it matches the *_no_excerpt fixture's key set.
    const frame = buildRequest("PreCompact", mkInput({ session_id: "s1" }));
    assertSameContract(
      frame,
      loadFixture("request_compact_payload_no_excerpt.json"),
      "CompactPayload"
    );
  });

  it("test_context_search_no_source_matches_fixture", () => {
    // UserPromptSubmit (>=5 words) -> ContextSearch WITHOUT source (omitted).
    const frame = buildRequest(
      "UserPromptSubmit",
      mkInput({ session_id: "s1", prompt: "explain the auth flow today please" })
    );
    assertSameContract(
      frame,
      loadFixture("request_context_search_no_source.json"),
      "ContextSearch(no source)"
    );
  });

  it("test_record_event_omitted_topic_provider_matches_fixture", () => {
    // An unknown event with null provider and no topic signal -> RecordEvent
    // with topic_signal AND provider OMITTED (the *_omitted fixture shape).
    const frame = buildRequest(
      "TotallyUnknownEvent",
      mkInput({ session_id: "s1", provider: null, extra: {} })
    );
    assertSameContract(
      frame,
      loadFixture("request_record_event_omitted.json"),
      "RecordEvent(omitted)"
    );
  });
});

describe("AC-14 contract round-trip - transcript_delta payload", () => {
  // Build a REAL delta frame from a temp transcript via the delta module, then
  // assert its payload matches transcript_delta_payload.json's contract: exactly
  // {offset, bytes}, numeric safe-integer offset, string bytes.
  it("test_delta_frame_payload_matches_fixture", () => {
    const fixture = loadFixture("transcript_delta_payload.json");

    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-delta-contract-"));
    const tpath = path.join(tmp, "transcript.jsonl");
    const content = "user: explain the auth flow\nassistant: tok example span\n";
    fs.writeFileSync(tpath, content);

    try {
      const built = delta.buildDeltaFrame(tpath, 0, Buffer.byteLength(content), "sess-1", "claude-code");
      assert.ok(built !== null, "delta frame must build for a grown transcript");

      const frame = JSON.parse(built.bodyBuf.toString("utf8"));
      assert.strictEqual(frame.type, "RecordEvent");
      assert.strictEqual(frame.event_type, "transcript_delta");

      const payload = frame.payload;
      assert.deepStrictEqual(
        Object.keys(payload).sort(),
        Object.keys(fixture).sort(),
        "delta payload keys must be exactly {offset, bytes} (TranscriptDeltaPayload)"
      );
      assert.strictEqual(typeof payload.offset, "number", "offset is a wire number");
      assert.ok(Number.isSafeInteger(payload.offset), "offset within 2^53 (lossless round-trip)");
      assert.strictEqual(typeof payload.bytes, "string", "bytes is a string");
      assert.ok(payload.bytes.length > 0, "bytes non-empty for a grown span");
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("test_delta_large_offset_stays_safe_integer", () => {
    // The fixture pins offset=4294967296 (>2^32) as a lossless safe integer;
    // the client's uniform advance (offset + byteLength) must stay in that range
    // for any realistic transcript. Assert the arithmetic the client uses.
    const fixture = loadFixture("transcript_delta_payload.json");
    assert.ok(Number.isSafeInteger(fixture.offset));
    const advanced = fixture.offset + Buffer.byteLength(fixture.bytes, "utf8");
    assert.ok(
      Number.isSafeInteger(advanced),
      "uniform offset advance must remain a safe integer"
    );
  });
});
