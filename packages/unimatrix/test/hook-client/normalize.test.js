"use strict";

// Unit tests for lib/hook-client/normalize.js (vnc-026, test-plan/normalize.md).
// Oracle: crates/unimatrix-server/src/uds/hook.rs:50-105
// (map_to_canonical / normalize_event_name). Risk: R-01 (via parity corpus);
// these units give fast-fail locality. Authoritative coverage is Layer 1 parity.

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const {
  mapToCanonical,
  normalizeEventName,
  UNKNOWN_EVENT,
  UNKNOWN_PROVIDER,
} = require("../../lib/hook-client/normalize");

// 11 canonical names per the Rust oracle match arms (hook.rs:57-71).
const CANONICAL_EVENTS = [
  "PreToolUse",
  "PostToolUse",
  "SessionStart",
  "Stop",
  "TaskCompleted",
  "Ping",
  "UserPromptSubmit",
  "PreCompact",
  "PostToolUseFailure",
  "SubagentStart",
  "SubagentStop",
];

// Gemini alias → canonical (hook.rs:53-55).
const GEMINI_ALIASES = [
  ["BeforeTool", "PreToolUse"],
  ["AfterTool", "PostToolUse"],
  ["SessionEnd", "Stop"],
];

const CLOSED_SET = new Set(CANONICAL_EVENTS.concat([UNKNOWN_EVENT]));

describe("normalize", function () {
  describe("canonical mapping", function () {
    it("test_canonical_events_identity", function () {
      for (const event of CANONICAL_EVENTS) {
        assert.strictEqual(mapToCanonical(event), event, "mapToCanonical(" + event + ")");
        assert.deepStrictEqual(
          normalizeEventName(event),
          [event, "claude-code"],
          "normalizeEventName(" + event + ")"
        );
      }
    });

    it("test_gemini_aliases", function () {
      for (const [alias, canonical] of GEMINI_ALIASES) {
        assert.strictEqual(mapToCanonical(alias), canonical, "mapToCanonical(" + alias + ")");
        assert.deepStrictEqual(
          normalizeEventName(alias),
          [canonical, "gemini-cli"],
          "normalizeEventName(" + alias + ")"
        );
      }
    });

    it("test_unknown_event_sentinel", function () {
      // Raw name preserved for generic-observation passthrough: normalize
      // returns ONLY the sentinel; the caller (index.js) keeps the raw string
      // as effectiveEvent. Asserted jointly with build-request (AC-01).
      const raw = "SomeFutureEvent";
      assert.strictEqual(mapToCanonical(raw), UNKNOWN_EVENT);
      assert.deepStrictEqual(normalizeEventName(raw), [UNKNOWN_EVENT, UNKNOWN_PROVIDER]);
      assert.strictEqual(UNKNOWN_EVENT, "__unknown__");
      assert.strictEqual(UNKNOWN_PROVIDER, "unknown");
    });
  });

  describe("defensive behavior", function () {
    it("test_empty_event_name", function () {
      // "" hits the default arm exactly as in Rust — sentinel, no throw.
      assert.strictEqual(mapToCanonical(""), UNKNOWN_EVENT);
      assert.deepStrictEqual(normalizeEventName(""), [UNKNOWN_EVENT, UNKNOWN_PROVIDER]);
    });

    it("test_case_sensitivity_parity", function () {
      // Rust match is exact/case-sensitive — no lowercasing in hook.rs:50-105.
      for (const variant of ["pretooluse", "PRETOOLUSE", "preToolUse", "stop", "ping"]) {
        assert.strictEqual(mapToCanonical(variant), UNKNOWN_EVENT, variant);
        assert.deepStrictEqual(
          normalizeEventName(variant),
          [UNKNOWN_EVENT, UNKNOWN_PROVIDER],
          variant
        );
      }
    });

    it("test_whitespace_name", function () {
      // Rust match does no trimming — padded names hit the default arm.
      for (const padded of [" PreToolUse ", "PreToolUse ", " PreToolUse", "Stop\n", "\tStop"]) {
        assert.strictEqual(mapToCanonical(padded), UNKNOWN_EVENT, JSON.stringify(padded));
        assert.deepStrictEqual(
          normalizeEventName(padded),
          [UNKNOWN_EVENT, UNKNOWN_PROVIDER],
          JSON.stringify(padded)
        );
      }
    });
  });

  describe("concrete assertions", function () {
    it("test_purity_same_input_deep_equal_no_io", function () {
      // Pure: same input twice → deep-equal output, no fs activity.
      const spied = [];
      const origRead = fs.readFileSync;
      const origOpen = fs.openSync;
      fs.readFileSync = function () {
        spied.push("readFileSync");
        return origRead.apply(fs, arguments);
      };
      fs.openSync = function () {
        spied.push("openSync");
        return origOpen.apply(fs, arguments);
      };
      try {
        for (const event of ["PreToolUse", "BeforeTool", "nope", ""]) {
          assert.deepStrictEqual(normalizeEventName(event), normalizeEventName(event));
          assert.strictEqual(mapToCanonical(event), mapToCanonical(event));
        }
      } finally {
        fs.readFileSync = origRead;
        fs.openSync = origOpen;
      }
      assert.deepStrictEqual(spied, [], "normalize performed file I/O");
    });

    it("test_map_is_closed", function () {
      // Function never returns a name outside {11 canonical} ∪ {__unknown__}.
      const probes = CANONICAL_EVENTS.concat(
        GEMINI_ALIASES.map(function (p) {
          return p[0];
        }),
        ["", " ", "x", "__unknown__", "unknown", "BeforeTool ", "sessionend", "Sub", "0"]
      );
      for (const event of probes) {
        const canonical = mapToCanonical(event);
        assert.ok(CLOSED_SET.has(canonical), "mapToCanonical(" + event + ") → " + canonical);
        const [name, provider] = normalizeEventName(event);
        assert.ok(CLOSED_SET.has(name), "normalizeEventName(" + event + ") → " + name);
        assert.ok(
          ["claude-code", "gemini-cli", UNKNOWN_PROVIDER].indexOf(provider) !== -1,
          "provider " + provider
        );
      }
    });
  });
});
