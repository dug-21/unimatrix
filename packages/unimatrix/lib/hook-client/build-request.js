"use strict";

/**
 * build-request.js — HookRequest parity port (dispatch).
 *
 * Pure function `buildRequest(effectiveEvent, input) -> HookRequest`, a
 * byte-faithful port of `hook.rs::build_request` (hook.rs:440-727). The arm
 * builders, rework/cycle helpers, and topic-signal selection live in
 * `build-request-tools.js` to keep each file under the 500-line gate
 * (OVERVIEW.md). Read-only oracles under `crates/`.
 *
 * Purity contract: no I/O except `process.ppid` (parent_id fallback) and
 * `process.cwd()` (cwd fallback). Never throws — every malformed shape falls
 * through to `genericRecordEvent` or a defensive default (FR-03.7 parity). All
 * byte budgets use `Buffer.byteLength` (UTF-16 trap avoided). stderr one-liners
 * fire only where the Rust oracle `eprintln!`s — never stdout.
 */

const tools = require("./build-request-tools");

const {
  safeCwd,
  extraGet,
  strOr,
  payloadFromExtra,
  countWhitespaceWords,
  extractEventTopicSignal,
  recordEventFrame,
  genericRecordEvent,
  buildPostToolUse,
  buildPreToolUse,
  buildSubagentStart,
  MIN_QUERY_WORDS,
} = tools;

/**
 * Build a HookRequest from a normalized event name and parsed HookInput.
 * Pure; never throws.
 *
 * @param {string} event - canonical (normalized) event name
 * @param {object} input - parsed HookInput (see OVERVIEW shared types)
 * @returns {object} HookRequest
 */
function buildRequest(event, input) {
  // session_id ppid fallback (hook.rs:449-453); process.ppid ≙ parent_id().
  const sessionId =
    input.session_id == null ? "ppid-" + process.ppid : input.session_id;
  // cwd fallback to process.cwd(), "" on failure (hook.rs:455-459).
  const cwd = input.cwd == null ? safeCwd() : input.cwd;

  switch (event) {
    case "SessionStart":
      return {
        type: "SessionRegister",
        session_id: sessionId,
        cwd: cwd,
        agent_role: strOr(extraGet(input, "agent_role"), null),
        feature: strOr(extraGet(input, "feature_cycle"), null),
      };

    case "Stop":
    case "TaskCompleted":
      return {
        type: "SessionClose",
        session_id: sessionId,
        outcome: "success", // server overrides to "rework" if threshold crossed
        duration_secs: 0,
      };

    case "Ping":
      return { type: "Ping" };

    case "UserPromptSubmit": {
      const query = input.prompt == null ? "" : input.prompt;
      // Guard 1: empty / whitespace-only → RecordEvent (EC-01).
      if (query.trim() === "") {
        return genericRecordEvent(event, sessionId, input);
      }
      // Guard 2: word-count threshold (FR-05). Query value itself is NOT trimmed.
      if (countWhitespaceWords(query) < MIN_QUERY_WORDS) {
        return genericRecordEvent(event, sessionId, input);
      }
      return {
        type: "ContextSearch",
        query: query,
        session_id: input.session_id == null ? null : input.session_id,
        // source key OMITTED (None → skip_serializing_if)
        role: null,
        task: null,
        feature: null,
        k: null,
        max_tokens: null,
      };
    }

    case "PreCompact":
      return {
        type: "CompactPayload",
        session_id: sessionId,
        injected_entry_ids: [],
        role: null,
        feature: null,
        token_limit: null,
        // transcript_excerpt OMITTED (None); F2 restores server-side.
      };

    case "PostToolUse":
      return buildPostToolUse(sessionId, input);

    case "PostToolUseFailure":
      // Explicit arm — never wildcard, never rework (hook.rs:641-661).
      return recordEventFrame(
        "PostToolUseFailure",
        sessionId,
        payloadFromExtra(input),
        extractEventTopicSignal(event, input),
        input.provider
      );

    case "PreToolUse":
      return buildPreToolUse(event, sessionId, input);

    case "SubagentStart":
      return buildSubagentStart(event, sessionId, input);

    default:
      return genericRecordEvent(event, sessionId, input);
  }
}

module.exports = {
  buildRequest,
  // shared with delta.js (OVERVIEW.md helper contract)
  implantEvent: tools.implantEvent,
  nowSecs: tools.nowSecs,
  // re-exported for unit-test locality / parity coverage
  validateCycleParams: tools.validateCycleParams,
  isBashFailure: tools.isBashFailure,
  extractFilePath: tools.extractFilePath,
  extractReworkEventsForMultiEdit: tools.extractReworkEventsForMultiEdit,
  extractEventTopicSignal: tools.extractEventTopicSignal,
  countWhitespaceWords: tools.countWhitespaceWords,
  MIN_QUERY_WORDS: tools.MIN_QUERY_WORDS,
  MAX_GOAL_BYTES: tools.MAX_GOAL_BYTES,
  CYCLE_START_EVENT: tools.CYCLE_START_EVENT,
  CYCLE_PHASE_END_EVENT: tools.CYCLE_PHASE_END_EVENT,
  CYCLE_STOP_EVENT: tools.CYCLE_STOP_EVENT,
};
