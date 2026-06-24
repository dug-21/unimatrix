"use strict";

/**
 * build-request.js — HookRequest parity port (dispatch).
 *
 * Pure `buildRequest(effectiveEvent, input) -> HookRequest`, a byte-faithful port
 * of `hook.rs::build_request` (hook.rs:440-727). Arm builders, rework/cycle
 * helpers, and topic-signal selection live in build-request-tools.js for the
 * 500-line gate (OVERVIEW.md).
 *
 * Purity: no I/O except process.ppid (parent_id fallback) and process.cwd() (cwd
 * fallback). Never throws — malformed shapes fall through to genericRecordEvent
 * or a defensive default (FR-03.7). Byte budgets use Buffer.byteLength. stderr
 * one-liners fire only where the Rust oracle eprintln!s — never stdout.
 */

const tools = require("./build-request-tools");
const sessionIdMod = require("./session-id");

const {
  safeCwd,
  extraGet,
  strOr,
  countWhitespaceWords,
  genericRecordEvent,
  buildPostToolUse,
  buildPreToolUse,
  buildSubagentStart,
  MIN_QUERY_WORDS,
} = tools;

/**
 * Build a HookRequest from a normalized event name and parsed HookInput. Pure;
 * never throws.
 *
 * @param {string} event - canonical (normalized) event name
 * @param {object} input - parsed HookInput (see OVERVIEW shared types)
 * @returns {object} HookRequest
 */
function buildRequest(event, input) {
  // #832: deterministic CC session-id resolution shared by the cycle DECLARATION
  // and per-tool OBSERVE spawns so both key Path A (tracker) and Path B (registry)
  // on ONE id. input.session_id → cc-<hash>(transcript_path) → ppid- last resort.
  // The frame session_id IS the tracker key (decorateCycleStamp reads it back via
  // sessionIdOf), so fixing it here fixes both paths at once.
  const sessionId = sessionIdMod.resolveSessionId(input);
  // #832 B1 trace (gated to UNIMATRIX_HOOK_DEBUG, off the hot path otherwise) so a
  // live remote run can confirm declaration vs observe converge. Never throws.
  sessionIdMod.traceSessionId(
    event === "PreToolUse" ? "declaration?" : "observe",
    sessionId
  );
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
      // Guard 1 empty/whitespace-only (EC-01); Guard 2 word-count threshold
      // (FR-05, query value itself NOT trimmed) → RecordEvent.
      if (query.trim() === "" || countWhitespaceWords(query) < MIN_QUERY_WORDS) {
        return genericRecordEvent(event, sessionId, input);
      }
      return {
        type: "ContextSearch",
        query: query,
        // raw CC id (NOT the resolved tracker id), null when absent (#832); the
        // search session_id is omit-when-null parity, distinct from FNF framing.
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
      // Explicit arm — never wildcard, never rework (hook.rs:641-661). Shape is
      // identical to the generic frame (event/payload/topic_signal/provider).
      return genericRecordEvent("PostToolUseFailure", sessionId, input);

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
