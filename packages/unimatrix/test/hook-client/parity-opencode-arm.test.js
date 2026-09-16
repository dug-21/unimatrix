"use strict";

// OpenCode provider-arm parity suite (vnc-049 C8, ADR-004; col-022 split-brain).
//
// The shipped Rust hook/opencode.rs (C2) is the oracle (#4751/#5754). The Rust
// parity generator (parity_corpus_opencode.rs) emits opencode-arm-goldens.json
// from the REAL arm functions; this suite replays the JS mirror
// (normalize.js, C3) over the SAME inputs and asserts byte-parity. Together with
// the Rust-side zero-diff drift gate (scripts/check-parity-drift.sh) this is the
// permanent col-022 guard (NFR-04, R-03):
//
//   * edit hook/opencode.rs alone  -> regenerate diverges from the committed
//     golden -> drift gate RED (and opencode.rs's own unit tests RED).
//   * edit normalize.js alone      -> this suite diverges from the committed
//     golden -> RED here.
//
// No hand-written expected values (#2984/#5302): goldens only. A missing golden
// is a hard failure, never a skip. The golden is a flat file (the arm is
// unreachable through the stdin->request pipeline: the JS client has no
// --provider/--model flag — the OpenCode plugin shells to the Rust binary,
// #5754), mirroring the project-hash-goldens.json precedent exactly.

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const {
  normalizeEventName,
  normalizeOpencode,
  canonicalizeOpencode,
  isOpencodeEventAlias,
  isValidModelId,
  KNOWN_PROVIDERS,
  OPENCODE_PROVIDER,
  UNKNOWN_EVENT,
} = require("../../lib/hook-client/normalize");

const GOLDEN_PATH = path.join(
  __dirname,
  "..",
  "fixtures",
  "parity",
  "opencode-arm-goldens.json"
);

const GOLDEN = (() => {
  assert.ok(
    fs.existsSync(GOLDEN_PATH),
    "opencode-arm-goldens.json missing — run the Rust generator (scripts/regen-parity.sh)"
  );
  return JSON.parse(fs.readFileSync(GOLDEN_PATH, "utf8"));
})();

// ── constant / contract parity (AC-02c, #5754 gotcha 1) ─────────────

describe("opencode arm parity - constants (col-022)", () => {
  it("test_opencode_provider_matches_oracle", () => {
    assert.strictEqual(OPENCODE_PROVIDER, GOLDEN.provider);
  });

  it("test_unknown_event_sentinel_matches_oracle", () => {
    assert.strictEqual(UNKNOWN_EVENT, GOLDEN.unknown_event_sentinel);
  });

  it("test_known_providers_membership_matches_oracle", () => {
    // KNOWN_PROVIDERS did not exist in the JS client before vnc-049 C3; the
    // whole allowlist is mirrored from Rust hook.rs:35 as the single-source
    // anchor (#5754 gotcha 1). Order + membership must match byte-for-byte.
    assert.deepStrictEqual(KNOWN_PROVIDERS, GOLDEN.known_providers);
    assert.ok(KNOWN_PROVIDERS.includes(OPENCODE_PROVIDER));
  });
});

// ── normalize / canonicalize / alias-guard parity (AC-02b, R-03) ────

describe("opencode arm parity - normalize goldens (col-022)", () => {
  it("test_normalize_goldens_nonempty", () => {
    // Non-vacuity guard (#4452): the 7 canonical events + raw-source probes.
    assert.ok(
      Array.isArray(GOLDEN.normalize) && GOLDEN.normalize.length >= 7,
      "normalize goldens must cover >= 7 events"
    );
  });

  it("test_canonical_events_cover_seven", () => {
    assert.deepStrictEqual(GOLDEN.canonical_events, [
      "SessionStart",
      "UserPromptSubmit",
      "PreToolUse",
      "PostToolUse",
      "SubagentStart",
      "PreCompact",
      "Stop",
    ]);
    // Each canonical event round-trips through the arm identically.
    for (const ev of GOLDEN.canonical_events) {
      assert.strictEqual(canonicalizeOpencode(ev), ev, `identity for ${ev}`);
    }
  });

  for (const c of GOLDEN.normalize) {
    it("test_normalize_opencode_" + c.event_in + "_matches_golden", () => {
      // Full contract, not just the name (#5302): (canonical, provider).
      assert.deepStrictEqual(
        normalizeOpencode(c.event_in),
        [c.canonical, c.provider],
        "normalizeOpencode diverges from the Rust oracle for " + c.event_in
      );
      assert.strictEqual(canonicalizeOpencode(c.event_in), c.canonical);
      assert.strictEqual(c.provider, OPENCODE_PROVIDER);
      assert.strictEqual(
        isOpencodeEventAlias(c.event_in),
        c.is_opencode_event_alias,
        "isOpencodeEventAlias diverges for " + c.event_in
      );
    });
  }
});

// ── alias-guard must not hijack claude-code inference (#5751) ────────

describe("opencode arm parity - alias guard does not hijack canonical inference", () => {
  // The alias guard is false for the 7 shared canonical names, so the
  // INFERENCE path (no --provider) must still stamp claude-code for them —
  // never opencode. This is the load-bearing #5751 property both arms share.
  for (const ev of GOLDEN.canonical_events) {
    it("test_inference_path_stays_claude_code_" + ev, () => {
      assert.strictEqual(isOpencodeEventAlias(ev), false);
      assert.deepStrictEqual(normalizeEventName(ev), [ev, "claude-code"]);
    });
  }
});

// ── model_id carrier validation parity (OQ-5, R-15, #5754 gotcha 2) ──

describe("opencode arm parity - model_id validation goldens", () => {
  it("test_model_id_goldens_nonempty", () => {
    assert.ok(
      Array.isArray(GOLDEN.model_id_validation) &&
        GOLDEN.model_id_validation.length >= 5,
      "model_id validation goldens must be non-trivial"
    );
  });

  for (const c of GOLDEN.model_id_validation) {
    // Label uses a stable index-free preview so newlines/emoji don't break names.
    const label = JSON.stringify(c.model_id).slice(0, 40);
    it("test_is_valid_model_id_" + label + "_matches_golden", () => {
      assert.strictEqual(
        isValidModelId(c.model_id),
        c.valid,
        "isValidModelId diverges from the Rust oracle for " + label
      );
    });
  }
});
