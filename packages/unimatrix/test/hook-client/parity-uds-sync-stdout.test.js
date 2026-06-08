"use strict";

// UDS sync-trio stdout parity (vnc-027, parity-corpus-uds layers b/c — AC-03,
// R-09). Post-reduction event set only (FR-21).
//
// Proves the FULL UDS sync leg end-to-end against the committed Rust-hook stdout
// goldens (test/fixtures/parity/<case>/expected-stdout.bin):
//
//   daemon HookResponse  -> transportUds.mapHookResponse -> SendResult
//                        -> transform.writeSyncOutput(reqSource, res) -> stdout
//
// The inner Text body is reconstructed from the committed golden (the same
// reconstruction parity-layer1 uses for AC-04), wrapped in a Text/Ack/Pong
// HookResponse, then mapped + written. The captured stdout must byte-equal the
// golden -- the server-side-preformatted path (ADR-001) carries the exact bytes
// the HTTP text/plain path would, so UDS and the Rust hook stdout coincide.
//
// Accepted divergences honored (FR-22): no lone-surrogate inputs; event-set
// divergence (retired PreToolUse / opt-in SubagentStop) excluded by design.
//
// Cumulative infra: reuses the committed corpus + the real transport/transform.

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const { mapHookResponse } = require("../../lib/hook-client/transport-uds");
const { writeSyncOutput, INJECTION_HEADER } = require("../../lib/hook-client/transform");

const PARITY_DIR = path.join(__dirname, "..", "fixtures", "parity");

const SUBAGENT_ENVELOPE_PREFIX =
  '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":';

/** Capture everything writeSyncOutput sends to stdout for one call. */
function captureStdout(fn) {
  const chunks = [];
  const orig = process.stdout.write;
  process.stdout.write = (chunk) => {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
    return true;
  };
  try {
    fn();
  } finally {
    process.stdout.write = orig;
  }
  return Buffer.concat(chunks);
}

function loadGolden(name) {
  return fs.readFileSync(path.join(PARITY_DIR, name, "expected-stdout.bin"));
}

function reqSourceOf(name) {
  const req = JSON.parse(fs.readFileSync(path.join(PARITY_DIR, name, "expected-request.json"), "utf8"));
  return req.type === "ContextSearch" && req.source !== undefined ? req.source : null;
}

// Recover the Text wire body the daemon would have returned from the golden
// (server formatting + client wrap baked in). Envelope golden -> inner
// additionalContext; plain golden -> body without the single trailing newline.
function wireBodyFromGolden(golden, reqSource) {
  if (golden.length === 0) return null; // empty injection -> Ack, not Text
  const s = golden.toString("utf8");
  if (reqSource === "SubagentStart" && s.startsWith(SUBAGENT_ENVELOPE_PREFIX)) {
    return JSON.parse(s).hookSpecificOutput.additionalContext;
  }
  return s.endsWith("\n") ? s.slice(0, -1) : s;
}

// Build the SendResult the UDS transport produces for a daemon response, then
// run the stdout writer -- the exact index.js sync tail over UDS.
function udsSyncStdout(reqSource, hookResponse) {
  const sendResult = mapHookResponse(hookResponse);
  return captureStdout(() => writeSyncOutput(reqSource, sendResult));
}

describe("UDS sync-trio stdout parity (AC-03 / R-09)", function () {
  it("test_context_search_plain_entries_byte_identical", function () {
    const name = "stdout-plain-entries";
    const golden = loadGolden(name);
    const reqSource = reqSourceOf(name); // null (UserPromptSubmit-class plain)
    const body = wireBodyFromGolden(golden, reqSource);
    assert.ok(body.startsWith(INJECTION_HEADER), "Entries body carries the injection header");
    const out = udsSyncStdout(reqSource, { type: "Text", body });
    assert.ok(out.equals(golden), "UDS plain Entries stdout diverges from the Rust golden");
  });

  it("test_subagent_start_envelope_byte_identical", function () {
    const name = "stdout-subagent-envelope";
    const golden = loadGolden(name);
    const reqSource = reqSourceOf(name); // SubagentStart
    assert.strictEqual(reqSource, "SubagentStart");
    const body = wireBodyFromGolden(golden, reqSource);
    assert.ok(body.startsWith(INJECTION_HEADER), "envelope inner body keeps the header (R-09 s2)");
    const out = udsSyncStdout(reqSource, { type: "Text", body });
    assert.ok(out.equals(golden), "UDS SubagentStart envelope stdout diverges from the golden");
  });

  it("test_briefing_content_verbatim_no_header", function () {
    // CompactPayload / BriefingContent: body verbatim, no injection header.
    const name = "stdout-briefing-content";
    const golden = loadGolden(name);
    const reqSource = reqSourceOf(name); // null (plain path)
    const body = wireBodyFromGolden(golden, reqSource);
    assert.ok(!body.startsWith(INJECTION_HEADER), "BriefingContent body has no header");
    const out = udsSyncStdout(reqSource, { type: "Text", body });
    assert.ok(out.equals(golden), "UDS BriefingContent stdout diverges from the golden");
  });

  it("test_empty_injection_ack_writes_nothing", function () {
    // Empty injection -> daemon returns Ack (204-equivalent), never Text. The
    // golden is empty; writeSyncOutput must emit nothing (ADR-001 §4).
    const golden = loadGolden("stdout-subagent-empty-entries");
    assert.strictEqual(golden.length, 0, "empty-injection golden is empty");
    const out = udsSyncStdout("SubagentStart", { type: "Ack" });
    assert.strictEqual(out.length, 0, "Ack -> 204 -> no stdout");
  });

  it("test_ping_pong_json_writes_nothing", function () {
    // Ping -> Pong -> 200 application/json; writeSyncOutput is text/plain-only.
    const out = udsSyncStdout(null, { type: "Pong", server_version: "0.7.2" });
    assert.strictEqual(out.length, 0, "Pong is not text/plain -> silent (R-15)");
  });
});
