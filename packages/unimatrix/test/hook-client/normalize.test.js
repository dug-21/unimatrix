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
  canonicalizeOpencode,
  normalizeOpencode,
  isOpencodeEventAlias,
  isValidModelId,
  KNOWN_PROVIDERS,
  OPENCODE_PROVIDER,
  MODEL_ID_MAX_LEN,
  UNKNOWN_EVENT,
  UNKNOWN_PROVIDER,
} = require("../../lib/hook-client/normalize");

// vnc-049 C3 — the seven canonical OpenCode events (mirror of
// opencode.rs OPENCODE_CANONICAL_EVENTS). Kept in this order for 1:1 parity
// with the Rust test cases (test-plan/c3-provider-arm-js.md).
const OPENCODE_CANONICAL_EVENTS = [
  "SessionStart",
  "UserPromptSubmit",
  "PreToolUse",
  "PostToolUse",
  "SubagentStart",
  "PreCompact",
  "Stop",
];

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

// vnc-049 C3 — OpenCode provider normalization arm (JS mirror of
// hook/opencode.rs, col-022 split-brain). These units give fast-fail locality;
// the authoritative cross-language parity assertion lives in C8's corpus.
// Each case mirrors a Rust test in opencode.rs so drift on either side is caught.
describe("opencode arm (vnc-049 C3)", function () {
  describe("canonicalization mirror (FR-04, AC-02, R-03)", function () {
    // One test per canonical event asserting the FULL (name, provider) contract
    // — not just the name (#5302) — paralleling opencode.rs test_opencode_*.
    const cases = [
      ["test_normalize_opencode_sessionstart", "SessionStart"],
      ["test_normalize_opencode_userpromptsubmit", "UserPromptSubmit"],
      ["test_normalize_opencode_pretooluse", "PreToolUse"],
      ["test_normalize_opencode_posttooluse", "PostToolUse"],
      ["test_normalize_opencode_subagentstart", "SubagentStart"],
      ["test_normalize_opencode_precompact", "PreCompact"],
      ["test_normalize_opencode_stop", "Stop"],
    ];
    for (const [name, event] of cases) {
      it(name, function () {
        assert.strictEqual(canonicalizeOpencode(event), event, "canonicalize(" + event + ")");
        assert.deepStrictEqual(
          normalizeOpencode(event),
          [event, "opencode"],
          "normalize(" + event + ")"
        );
      });
    }

    it("test_opencode_canonicalize_all_seven_identity", function () {
      // Mirror of opencode.rs test_opencode_canonicalize_all_seven_identity.
      for (const event of OPENCODE_CANONICAL_EVENTS) {
        assert.strictEqual(canonicalizeOpencode(event), event, "identity for " + event);
        assert.deepStrictEqual(normalizeOpencode(event), [event, OPENCODE_PROVIDER]);
      }
    });

    it("test_opencode_canonicalize_unknown_returns_sentinel", function () {
      // Unrecognized OpenCode source names (raw bus names, not canonical) → sentinel.
      // Mirror of opencode.rs test_opencode_canonicalize_unknown_returns_sentinel.
      assert.strictEqual(canonicalizeOpencode("session.created"), UNKNOWN_EVENT);
      assert.strictEqual(canonicalizeOpencode("CompletelyUnknownEvent"), UNKNOWN_EVENT);
      assert.deepStrictEqual(normalizeOpencode("nope"), [UNKNOWN_EVENT, OPENCODE_PROVIDER]);
    });
  });

  describe("alias guard must not hijack shared canonical names (#5751)", function () {
    it("test_opencode_alias_does_not_hijack_canonical_names", function () {
      // Mirror of opencode.rs test_opencode_alias_does_not_hijack_canonical_names.
      // If the guard claimed a shared name, normalizeEventName would misattribute
      // claude-code events to opencode on the inference path.
      for (const event of OPENCODE_CANONICAL_EVENTS) {
        assert.strictEqual(
          isOpencodeEventAlias(event),
          false,
          event + " is shared; isOpencodeEventAlias must be false"
        );
      }
      assert.strictEqual(isOpencodeEventAlias("BeforeTool"), false);
      assert.strictEqual(isOpencodeEventAlias("session.idle"), false);
    });

    it("test_normalize_existing_providers_unchanged", function () {
      // Non-regression: the inert opencode guard leaves claude-code / gemini-cli
      // inference byte-for-byte unchanged (the guard returns false, so the
      // existing match arms run exactly as before).
      assert.deepStrictEqual(normalizeEventName("PreToolUse"), ["PreToolUse", "claude-code"]);
      assert.deepStrictEqual(normalizeEventName("SessionStart"), ["SessionStart", "claude-code"]);
      assert.deepStrictEqual(normalizeEventName("BeforeTool"), ["PreToolUse", "gemini-cli"]);
      assert.deepStrictEqual(normalizeEventName("SessionEnd"), ["Stop", "gemini-cli"]);
      assert.deepStrictEqual(normalizeEventName("nope"), [UNKNOWN_EVENT, UNKNOWN_PROVIDER]);
    });
  });

  describe("provider allowlist mirror (hook.rs:158)", function () {
    it("test_known_providers_includes_opencode", function () {
      // AC-02a-parallel membership check; mirror of the Rust KNOWN_PROVIDERS test.
      assert.ok(KNOWN_PROVIDERS.indexOf("opencode") !== -1, "opencode in KNOWN_PROVIDERS");
      for (const p of ["claude-code", "gemini-cli", "codex-cli"]) {
        assert.ok(KNOWN_PROVIDERS.indexOf(p) !== -1, p + " preserved in KNOWN_PROVIDERS");
      }
    });
  });

  describe("model_id carrier validation (mirror of wire::is_valid_model_id)", function () {
    it("test_is_valid_model_id_accepts", function () {
      // Mirror of opencode.rs test_opencode_model_id_validation_reexport + the
      // wire.rs charset contract (^[a-z0-9._/-]{1,128}$).
      for (const ok of ["ollama/qwen3-coder", "a", "gpt-4o-mini", "x.y_z/1-2", "0"]) {
        assert.strictEqual(isValidModelId(ok), true, "accept " + JSON.stringify(ok));
      }
      assert.strictEqual(isValidModelId("a".repeat(MODEL_ID_MAX_LEN)), true, "128 chars ok");
    });

    it("test_is_valid_model_id_rejects", function () {
      const bad = [
        "",                                // empty
        "a".repeat(MODEL_ID_MAX_LEN + 1),  // 129 chars — over cap
        "model with space",                // space
        "Ollama/Qwen",                     // uppercase
        "model:tag",                       // colon not in charset
        "modè",                            // non-ascii
        "ollama/qwen3-coder\n",            // trailing newline (JS $ must not match)
        "a\tb",                            // control char
      ];
      for (const b of bad) {
        assert.strictEqual(isValidModelId(b), false, "reject " + JSON.stringify(b));
      }
      // Non-string inputs (serde Option<String> parity) → false, never throws.
      for (const nv of [null, undefined, 42, {}, ["ollama/x"]]) {
        assert.strictEqual(isValidModelId(nv), false, "reject non-string " + JSON.stringify(nv));
      }
    });
  });
});
