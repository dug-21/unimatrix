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

module.exports = {
  mapToCanonical,
  normalizeEventName,
  UNKNOWN_EVENT,
  UNKNOWN_PROVIDER,
};
