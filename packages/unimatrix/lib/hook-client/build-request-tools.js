"use strict";

/**
 * build-request-tools.js — helpers and arm-builders for the HookRequest parity
 * port. Split from build-request.js for the 500-line gate (OVERVIEW.md). Pure;
 * never throws.
 *
 * Holds: ImplantEvent/RecordEvent constructors, Value/Option-chaining accessors,
 * topic-signal selection, cycle validation (validation.rs::validate_cycle_params),
 * the PostToolUse rework helpers (hook.rs:881-951), and the PostToolUse /
 * PreToolUse / SubagentStart / generic arm builders (hook.rs:524-861). Dispatch
 * lives in build-request.js.
 */

const { truncateUtf8 } = require("./transcript");
const { extractTopicSignal } = require("./topic-signal");
const {
  validateCycleParams,
  validatePhaseField,
  CYCLE_START_EVENT,
  CYCLE_PHASE_END_EVENT,
  CYCLE_STOP_EVENT,
} = require("./cycle-validation");

// -- Constants (mirror hook.rs) --
const MIN_QUERY_WORDS = 5;
const MAX_GOAL_BYTES = 1024;

// -- Shared event helpers (OVERVIEW.md; consumed by delta.js too) --

/** Unix timestamp in seconds (hook.rs::now_secs). */
function nowSecs() {
  return Math.floor(Date.now() / 1000);
}

/** ImplantEvent. topic_signal/provider OMITTED when null/undefined (serde skip_serializing_if parity). */
function implantEvent(eventType, sessionId, payload, topicSignal, provider) {
  const e = { event_type: eventType, session_id: sessionId, timestamp: nowSecs(), payload: payload };
  // topic_signal then provider — omit-when-null (serde skip_serializing_if).
  if (topicSignal !== null && topicSignal !== undefined) e.topic_signal = topicSignal;
  if (provider !== null && provider !== undefined) e.provider = provider;
  return e;
}

/** RecordEvent frame = { type:"RecordEvent" } + flattened ImplantEvent. */
function recordEventFrame(eventType, sessionId, payload, topicSignal, provider) {
  return Object.assign({ type: "RecordEvent" }, implantEvent(eventType, sessionId, payload, topicSignal, provider));
}

// -- Small accessors (Rust Option/Value chaining parity) --

/** process.cwd() wrapped — "" on failure (Rust current_dir().unwrap_or_default()). */
function safeCwd() {
  try {
    return process.cwd();
  } catch (_e) {
    return "";
  }
}

/** Read a key from `input.extra` (object|null); undefined when absent. */
function extraGet(input, key) {
  const extra = input && input.extra;
  if (extra && typeof extra === "object" && !Array.isArray(extra)) {
    return extra[key];
  }
  return undefined;
}

/** Return `v` if it is a string, else `dflt` (Rust as_str().map parity). */
function strOr(v, dflt) {
  return typeof v === "string" ? v : dflt;
}

/** Return `v` if it is a string, else undefined (Rust as_str() None parity). */
function strOrUndef(v) {
  return typeof v === "string" ? v : undefined;
}

/** Payload from input.extra as-is — null on parse failure, {} when no unknown keys (Rust extra.clone()). */
function payloadFromExtra(input) {
  return input ? input.extra : null;
}

/** Whitespace-word count — Rust split_whitespace().count() (empty tokens dropped). */
function countWhitespaceWords(s) {
  return s.split(/\s+/u).filter((w) => w !== "").length;
}

// -- Topic-signal selection by event (hook.rs::extract_event_topic_signal) --

/**
 * Compute topic_signal for an event (hook.rs::extract_event_topic_signal). The
 * source text differs by event family; the extractor is the shared
 * extractTopicSignal chain.
 */
function extractEventTopicSignal(event, input) {
  switch (event) {
    case "PreToolUse":
    case "PostToolUse":
    case "PostToolUseFailure": {
      const v = extraGet(input, "tool_input");
      const text = typeof v === "string" ? v : v === undefined ? "" : JSON.stringify(v);
      return extractTopicSignal(text);
    }
    case "SubagentStart":
      return extractTopicSignal(strOr(extraGet(input, "agent_type"), ""));
    case "UserPromptSubmit":
      return extractTopicSignal(input.prompt == null ? "" : input.prompt);
    default:
      return input.extra === null ? null : extractTopicSignal(JSON.stringify(input.extra));
  }
}

// -- Generic arm (hook.rs:866-878) --

function genericRecordEvent(event, sessionId, input) {
  return recordEventFrame(event, sessionId, payloadFromExtra(input), extractEventTopicSignal(event, input), input.provider);
}

// -- PostToolUse rework helpers (hook.rs:881-951) --

/** rework-eligible (file-mutating) tools — hook.rs::is_rework_eligible_tool */
function isReworkEligibleTool(toolName) {
  return (
    toolName === "Bash" ||
    toolName === "Edit" ||
    toolName === "Write" ||
    toolName === "MultiEdit"
  );
}

/**
 * Bash failure detection — hook.rs::is_bash_failure. Failure = exit_code is a
 * non-zero INTEGER, OR interrupted === true. as_i64 parity: 1.5/"1"/true are not
 * integers → skipped. as_bool parity: only JSON `true` counts.
 */
function isBashFailure(extra) {
  const ec = extra ? extra.exit_code : undefined;
  if (Number.isInteger(ec) && ec !== 0) {
    return true;
  }
  if ((extra ? extra.interrupted : undefined) === true) {
    return true;
  }
  return false;
}

/**
 * file_path for Edit/Write — hook.rs::extract_file_path. Edit → tool_input.path;
 * Write → tool_input.file_path; else null.
 */
function extractFilePath(extra, toolName) {
  const ti = extra ? extra.tool_input : undefined;
  const tiObj = ti && typeof ti === "object" ? ti : undefined;
  if (toolName === "Edit") {
    return strOr(tiObj ? tiObj.path : undefined, null);
  }
  if (toolName === "Write") {
    return strOr(tiObj ? tiObj.file_path : undefined, null);
  }
  return null;
}

/**
 * (file_path, had_failure) pairs for MultiEdit —
 * hook.rs::extract_rework_events_for_multiedit. Non-array/missing `edits` → [].
 */
function extractReworkEventsForMultiEdit(extra) {
  const ti = extra ? extra.tool_input : undefined;
  const edits = ti && typeof ti === "object" ? ti.edits : undefined;
  if (!Array.isArray(edits)) {
    return [];
  }
  return edits.map((edit) => {
    const path = edit && typeof edit === "object" ? edit.path : undefined;
    return [strOr(path, null), false];
  });
}

/**
 * Build the rework-candidate payload (Rust json!: missing tool_input/tool_response
 * → null). Order: tool_name, file_path, had_failure, tool_input, tool_response.
 */
function reworkPayload(toolName, filePath, hadFailure, input) {
  const ti = extraGet(input, "tool_input");
  const tr = extraGet(input, "tool_response");
  return {
    tool_name: toolName,
    file_path: filePath,
    had_failure: hadFailure,
    tool_input: ti === undefined ? null : ti,
    tool_response: tr === undefined ? null : tr,
  };
}

// -- PostToolUse arm (hook.rs:524-634) --

function buildPostToolUse(sessionId, input) {
  const provider = input.provider == null ? "claude-code" : input.provider;
  const topicSignal = extractEventTopicSignal("PostToolUse", input);
  // Plain (non-rework) PostToolUse frame — the 4 identical fallthrough returns.
  const plain = () =>
    recordEventFrame("PostToolUse", sessionId, payloadFromExtra(input), topicSignal, input.provider);

  if (provider !== "claude-code") {
    return plain();
  }

  const toolName = strOr(extraGet(input, "tool_name"), "");

  if (!isReworkEligibleTool(toolName)) {
    return plain();
  }

  if (toolName === "MultiEdit") {
    const pairs = extractReworkEventsForMultiEdit(input.extra);
    if (pairs.length === 0) {
      return plain();
    }
    const events = pairs.map(([filePath, hadFailure]) =>
      implantEvent(
        "post_tool_use_rework_candidate",
        sessionId,
        reworkPayload("MultiEdit", filePath, hadFailure, input),
        topicSignal,
        input.provider
      )
    );
    return { type: "RecordEvents", events: events };
  }

  const hadFailure = toolName === "Bash" ? isBashFailure(input.extra) : false;
  const filePath = extractFilePath(input.extra, toolName);
  const reworkP = reworkPayload(toolName, filePath, hadFailure, input);
  return recordEventFrame("post_tool_use_rework_candidate", sessionId, reworkP, topicSignal, input.provider);
}

// -- PreToolUse arm + context_cycle interception (hook.rs:663-861) --

function buildPreToolUse(event, sessionId, input) {
  const mcp = input.mcp_context;
  const bare =
    mcp && typeof mcp === "object" && !Array.isArray(mcp)
      ? strOr(mcp.tool_name, null)
      : null;

  if (bare !== null) {
    // Clone — never mutate the caller's input (R-01).
    const promoted = Object.assign({}, input);
    const e = promoted.extra;
    const plainExtra = e && typeof e === "object" && !Array.isArray(e);
    promoted.extra = plainExtra ? Object.assign({}, e) : {};
    promoted.extra.tool_name = bare;
    return buildCycleEventOrFallthrough(event, sessionId, promoted);
  }
  return buildCycleEventOrFallthrough(event, sessionId, input);
}

function buildCycleEventOrFallthrough(event, sessionId, input) {
  const toolName = strOr(extraGet(input, "tool_name"), "");

  // Exact equality (security gate F-02): "evil_context_cycle_bypass" must fail.
  // ADR-004 §1: non-cycle PreToolUse → null no-send sentinel (observation
  // retired); index.js short-circuits to exit 0. The F-02 exact-equality gate
  // is preserved as defense-in-depth even though the narrowed install matcher
  // already prevents most spawns.
  if (
    toolName !== "context_cycle" &&
    toolName !== "mcp__unimatrix__context_cycle"
  ) {
    return null;
  }

  const toolInput = extraGet(input, "tool_input");
  if (toolInput === undefined) {
    process.stderr.write("unimatrix: context_cycle PreToolUse missing tool_input\n");
    return null; // sentinel — stderr diagnostic retained (R-11 s2)
  }
  const tiObj =
    toolInput && typeof toolInput === "object" && !Array.isArray(toolInput) ? toolInput : {};

  const validated = validateCycleParams(
    strOr(tiObj.type, ""),
    strOr(tiObj.topic, ""),
    strOrUndef(tiObj.phase),
    strOrUndef(tiObj.outcome),
    strOrUndef(tiObj.next_phase)
  );
  if (!validated.ok) {
    process.stderr.write(
      "unimatrix: context_cycle validation failed in hook (tool_name=" + toolName + ")\n"
    );
    return null; // sentinel — stderr diagnostic retained (R-11 s2)
  }

  const eventType =
    validated.cycleType === "start"
      ? CYCLE_START_EVENT
      : validated.cycleType === "phase-end"
        ? CYCLE_PHASE_END_EVENT
        : CYCLE_STOP_EVENT;

  // goal only on Start, byte-truncated at a UTF-8 boundary (hook.rs:808-823)
  let goal = null;
  if (validated.cycleType === "start" && typeof tiObj.goal === "string") {
    const g = tiObj.goal;
    if (Buffer.byteLength(g, "utf8") > MAX_GOAL_BYTES) {
      process.stderr.write("[unimatrix hook] goal exceeds MAX_GOAL_BYTES, truncating\n");
      goal = truncateUtf8(g, MAX_GOAL_BYTES);
    } else {
      goal = g;
    }
  }

  // Insertion order: feature_cycle first (json! parity), then optionals — each
  // key set only when non-null (omit-when-null). Sequence pins the key order.
  const payload = { feature_cycle: validated.topic };
  const put = (key, val) => { if (val !== null) payload[key] = val; };
  put("phase", validated.phase);
  put("outcome", validated.outcome);
  put("next_phase", validated.nextPhase);
  put("goal", goal);

  // tags only on Start (vnc-047, hook.rs:860-917). Value-opacity: strings only,
  // dropped when blank-after-trim. The ABSENCE of a byte/count cap is INTENTIONAL
  // — the oracle (hook.rs Step 4c) has none (value-opacity, no MAX_*_BYTES);
  // do NOT add one here or it breaks parity with the oracle and the goldens
  // (contrast MAX_GOAL_BYTES on goal). Omit the key entirely when nothing
  // survives, so a tagless/all-blank start leaves the whole-set-once lock
  // unburned (server C5 routes an empty-tags start to the unchanged arm).
  // Placed AFTER put("goal") and fully guarded: any malformed input (non-array,
  // null, nested junk, non-string members) degrades to no key and MUST NEVER
  // throw — a throw here drops the entire cycle frame (topic + goal + tags).
  let tags = null;
  if (validated.cycleType === "start") {
    try {
      const raw = tiObj.tags;
      if (Array.isArray(raw)) {
        const filtered = raw.filter((t) => typeof t === "string" && t.trim() !== "");
        if (filtered.length > 0) tags = filtered;
      }
    } catch (_e) {
      tags = null; // infallible (FR-03.7): treat any problem as no tags
    }
  }
  put("tags", tags);

  // topic_signal = topic (the cycle declaration keeps it).
  return recordEventFrame(eventType, sessionId, payload, validated.topic, input.provider);
}

// -- SubagentStart arm (hook.rs:698-723) --

function buildSubagentStart(event, sessionId, input) {
  const query = strOr(extraGet(input, "prompt_snippet"), "");
  if (query.trim() === "") {
    return genericRecordEvent(event, sessionId, input);
  }
  return {
    type: "ContextSearch",
    query: query,
    session_id: input.session_id == null ? null : input.session_id,
    source: "SubagentStart",
    role: null,
    task: null,
    feature: null,
    k: null,
    max_tokens: null,
  };
}

module.exports = {
  // shared event helpers
  nowSecs,
  implantEvent,
  recordEventFrame,
  // accessors
  safeCwd,
  extraGet,
  strOr,
  strOrUndef,
  payloadFromExtra,
  countWhitespaceWords,
  extractEventTopicSignal,
  // arm builders
  genericRecordEvent,
  buildPostToolUse,
  buildPreToolUse,
  buildSubagentStart,
  // validation + rework helpers (unit-test locality / parity coverage)
  validateCycleParams,
  validatePhaseField,
  isReworkEligibleTool,
  isBashFailure,
  extractFilePath,
  extractReworkEventsForMultiEdit,
  reworkPayload,
  // constants
  MIN_QUERY_WORDS,
  MAX_GOAL_BYTES,
  CYCLE_START_EVENT,
  CYCLE_PHASE_END_EVENT,
  CYCLE_STOP_EVENT,
};
