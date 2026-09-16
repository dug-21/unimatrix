"use strict";

/**
 * Event canonicalization — exact port of hook.rs:50-105 (read-only oracle).
 *
 * Pure string maps: exact-match, case-sensitive, no trimming — byte-for-byte
 * parity with the Rust `match`. Unknown names return the "__unknown__" sentinel;
 * the caller substitutes the raw event string (NFR-01).
 */

/** Sentinel returned for unrecognized event names. */
const UNKNOWN_EVENT = "__unknown__";

/** Provider inferred for unrecognized event names. */
const UNKNOWN_PROVIDER = "unknown";

/**
 * Provider allowlist — mirror of hook.rs:158 KNOWN_PROVIDERS (vnc-049 C3,
 * ADR-004). Single-source the whole provider contract, not just the event map
 * (#5302, R-03). Membership parity with the Rust const is asserted by the C8
 * corpus and the AC-02a-parallel unit test below.
 */
const KNOWN_PROVIDERS = ["claude-code", "gemini-cli", "codex-cli", "opencode"];

/** Provider stamped for OpenCode-origin events — mirror of OPENCODE_PROVIDER. */
const OPENCODE_PROVIDER = "opencode";

/** Max model_id carrier length (chars) — mirror of wire.rs MODEL_ID_MAX_LEN. */
const MODEL_ID_MAX_LEN = 128;

/**
 * Map any event name (Gemini-specific or canonical) to its canonical Unimatrix
 * name — port of hook.rs::map_to_canonical. F3 has no --provider flag, so
 * index.js uses only normalizeEventName; exported for completeness / parity.
 *
 * @param {string} event - Raw event name (argv[2]).
 * @returns {string} Canonical name, or "__unknown__" sentinel.
 */
function mapToCanonical(event) {
  switch (event) {
    // Gemini-unique names → canonical Claude Code equivalents
    case "BeforeTool":
      return "PreToolUse";
    case "AfterTool":
      return "PostToolUse";
    case "SessionEnd":
      return "Stop";
    // Canonical / shared names (Claude Code, Codex)
    case "PreToolUse":
      return "PreToolUse";
    case "PostToolUse":
      return "PostToolUse";
    case "SessionStart":
      return "SessionStart";
    case "Stop":
      return "Stop";
    case "TaskCompleted":
      return "TaskCompleted";
    case "Ping":
      return "Ping";
    case "UserPromptSubmit":
      return "UserPromptSubmit";
    case "PreCompact":
      return "PreCompact";
    case "PostToolUseFailure":
      return "PostToolUseFailure";
    case "SubagentStart":
      return "SubagentStart";
    case "SubagentStop":
      return "SubagentStop";
    // Unknown event name — caller detects the sentinel, uses raw event string
    default:
      return UNKNOWN_EVENT;
  }
}

/**
 * Translate a provider-specific event name to its canonical Unimatrix name and
 * infer the provider — port of hook.rs::normalize_event_name. Gemini-unique
 * names infer "gemini-cli"; known Claude Code / Codex names infer "claude-code";
 * unknown → ["__unknown__", "unknown"] (caller preserves raw name, NFR-01).
 *
 * @param {string} event - Raw event name (argv[2]).
 * @returns {[string, string]} [canonical, provider] pair.
 */
function normalizeEventName(event) {
  // vnc-049 C3 (ADR-004/005): OpenCode delegating arm — mirror of the hook.rs
  // guard (hook.rs:117-124). OpenCode-unique aliases route through
  // canonicalizeOpencode. The C1 shim emits canonical names and always passes
  // `--provider opencode` (the hint path), so this inference-path guard is inert
  // today (isOpencodeEventAlias is false for the seven shared canonical names).
  // It anchors parity (col-022, #5751) and keeps provider inference honest.
  if (isOpencodeEventAlias(event)) {
    return normalizeOpencode(event);
  }
  switch (event) {
    // Gemini-unique names — unambiguous provider inference
    case "BeforeTool":
      return ["PreToolUse", "gemini-cli"];
    case "AfterTool":
      return ["PostToolUse", "gemini-cli"];
    case "SessionEnd":
      return ["Stop", "gemini-cli"];
    // Canonical Claude Code names — pass through, default to "claude-code"
    case "PreToolUse":
      return ["PreToolUse", "claude-code"];
    case "PostToolUse":
      return ["PostToolUse", "claude-code"];
    case "SessionStart":
      return ["SessionStart", "claude-code"];
    case "Stop":
      return ["Stop", "claude-code"];
    case "TaskCompleted":
      return ["TaskCompleted", "claude-code"];
    case "Ping":
      return ["Ping", "claude-code"];
    case "UserPromptSubmit":
      return ["UserPromptSubmit", "claude-code"];
    case "PreCompact":
      return ["PreCompact", "claude-code"];
    case "PostToolUseFailure":
      return ["PostToolUseFailure", "claude-code"];
    case "SubagentStart":
      return ["SubagentStart", "claude-code"];
    case "SubagentStop":
      return ["SubagentStop", "claude-code"];
    // Unknown event: sentinel return (caller substitutes the raw event string).
    default:
      return [UNKNOWN_EVENT, UNKNOWN_PROVIDER];
  }
}

/**
 * vnc-049 C3 — byte-parity mirror of `hook/opencode.rs::canonicalize` (col-022,
 * #5670). Rust hook.rs / opencode.rs are the read-only oracle (#4751).
 *
 * The C1 plugin shim emits already-canonical event names, so this is identity
 * for the seven canonical names plus any future OpenCode-unique alias (none
 * today). Unrecognized names return the "__unknown__" sentinel; the caller
 * substitutes the raw event string (NFR-01), matching Rust exactly.
 *
 * @param {string} event - OpenCode event name.
 * @returns {string} Canonical name, or "__unknown__" sentinel.
 */
function canonicalizeOpencode(event) {
  switch (event) {
    case "SessionStart":
    case "UserPromptSubmit":
    case "PreToolUse":
    case "PostToolUse":
    case "SubagentStart":
    case "PreCompact":
    case "Stop":
      return event; // C1 emits canonical names; identity map (matches Rust)
    // No OpenCode-unique aliases known today; this table is the parity anchor
    // and the seam for future OpenCode-specific remaps.
    default:
      return UNKNOWN_EVENT;
  }
}

/**
 * Canonicalize an OpenCode event and stamp the OpenCode provider in one step —
 * mirror of `opencode::normalize`. Returns `[canonical_name, provider]` matching
 * the normalizeEventName return contract (#5302: single-source the whole
 * contract). `provider` is always "opencode"; an unrecognized event still stamps
 * "opencode" while the name falls back to the "__unknown__" sentinel.
 *
 * @param {string} event - OpenCode event name.
 * @returns {[string, string]} [canonical, "opencode"] pair.
 */
function normalizeOpencode(event) {
  return [canonicalizeOpencode(event), OPENCODE_PROVIDER];
}

/**
 * True iff `event` is an OpenCode-specific alias that must be remapped on the
 * inference path (`--provider` absent) — mirror of
 * `opencode::is_opencode_event_alias`. Currently always `false`: OpenCode emits
 * canonical names and always supplies `--provider opencode` (the hint path). It
 * MUST stay `false` for the seven shared canonical names, or the delegating
 * guard in normalizeEventName would hijack claude-code inference for those names
 * (#5751). It is the honest, safe seam for a future OpenCode-unique alias.
 *
 * @param {string} _event - Event name (unused today).
 * @returns {boolean} Always false.
 */
function isOpencodeEventAlias(_event) {
  return false;
}

/**
 * Validate a `model_id` carrier value — byte-parity mirror of
 * `wire::is_valid_model_id` (vnc-049 C4, R-15). The carrier shape is
 * `"<providerID>/<modelID>"` (e.g. "ollama/qwen3-coder"), which contains `/`, so
 * the charset is intentionally broader than the source_domain contract:
 *
 *   `^[a-z0-9._/-]{1,128}$` — 1..128 chars, each in [a-z0-9._/-], non-empty.
 *
 * JS `$` without the `m` flag matches only the true end of input (no Python-style
 * trailing-newline leniency), so this is byte-for-byte the Rust predicate.
 * Fail-open callers drop an invalid value (frame.model_id omitted, matching
 * serde skip_serializing_if) rather than forwarding raw bytes.
 *
 * @param {*} s - Candidate model id.
 * @returns {boolean}
 */
function isValidModelId(s) {
  return (
    typeof s === "string" &&
    s.length >= 1 &&
    s.length <= MODEL_ID_MAX_LEN &&
    /^[a-z0-9._/-]+$/.test(s)
  );
}

module.exports = {
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
};
