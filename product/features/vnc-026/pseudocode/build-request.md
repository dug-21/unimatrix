# build-request.js — HookRequest Parity Port

## Purpose
Pure function `buildRequest(effectiveEvent, input) -> HookRequest` — full port of
`hook.rs::build_request` (hook.rs:440-727) plus its private helpers and the topic-signal
chain (`unimatrix-observe/src/attribution.rs:15-92`) and cycle validation
(`infra/validation.rs::validate_cycle_params`, :354-510). Parity-tested against the
ADR-001 golden corpus (AC-01). No I/O except `process.ppid` and `process.cwd()` reads.

Size note: this module plus helpers approaches the 500-line gate — implement as
`build-request.js` (dispatch + arms) and keep `topicSignal`/cycle helpers in the same
file only if it stays under 500 lines; otherwise split `lib/hook-client/topic-signal.js`
(pure extractors) out first. Pseudocode below already isolates the seams.

## Constants
```
MIN_QUERY_WORDS = 5
MAX_GOAL_BYTES  = 1024
MAX_CYCLE_TOPIC_LEN = 128; MAX_PHASE_LEN = 64; MAX_OUTCOME_LEN = 512
CYCLE_START_EVENT = "cycle_start"; CYCLE_PHASE_END_EVENT = "cycle_phase_end"
CYCLE_STOP_EVENT = "cycle_stop"
MAX_FEATURE_ID_LEN = 128
```

## Main Dispatch

### buildRequest(event, input) -> HookRequest
```
function buildRequest(event, input):
  // session_id ppid fallback (hook.rs:449-453); process.ppid ≙ parent_id()
  session_id = input.session_id ?? ("ppid-" + process.ppid)
  cwd        = input.cwd ?? safeCwd()          // process.cwd() wrapped, "" on failure
                                               // (Rust: current_dir().to_string_lossy() else "")
  switch event:
    "SessionStart":
      return { type:"SessionRegister", session_id, cwd,
               agent_role: strOr(extraGet(input,"agent_role"), null),
               feature:    strOr(extraGet(input,"feature_cycle"), null) }   // null-encoded

    "Stop" | "TaskCompleted":
      return { type:"SessionClose", session_id, outcome:"success", duration_secs:0 }

    "Ping": return { type:"Ping" }

    "UserPromptSubmit":
      query = input.prompt ?? ""
      if query.trim() === "":                 return genericRecordEvent(event, session_id, input)
      if countWhitespaceWords(query) < MIN_QUERY_WORDS:
                                              return genericRecordEvent(event, session_id, input)
      return { type:"ContextSearch", query, session_id: input.session_id,  // null if absent (NOT ppid)
               role:null, task:null, feature:null, k:null, max_tokens:null }
               // source key OMITTED (None → skip_serializing_if)

    "PreCompact":
      return { type:"CompactPayload", session_id, injected_entry_ids: [],
               role:null, feature:null, token_limit:null }
               // transcript_excerpt OMITTED (None); F2 restores server-side

    "PostToolUse":        return buildPostToolUse(session_id, input)
    "PostToolUseFailure": // explicit arm — never the wildcard, never rework (hook.rs:641-661)
      return { type:"RecordEvent", ...implantEvent("PostToolUseFailure", session_id,
               payloadFromExtra(input), extractEventTopicSignal(event, input), input.provider) }
    "PreToolUse":         return buildPreToolUse(event, session_id, input)
    "SubagentStart":      return buildSubagentStart(event, session_id, input)
    default:              return genericRecordEvent(event, session_id, input)

function countWhitespaceWords(s):   // Rust split_whitespace(): Unicode-whitespace split,
  return s.split(/\s+/u).filter(w => w !== "").length   // empty tokens dropped

function extraGet(input, key):      // extra is object|null
  return (input.extra && typeof input.extra === "object") ? input.extra[key] : undefined
function strOr(v, dflt): return (typeof v === "string") ? v : dflt
function payloadFromExtra(input):   // Rust input.extra.clone(): {}|null|object as-is
  return input.extra               // null when parse failed; {} when no unknown keys
```

## PostToolUse Arm (hook.rs:524-634)
```
function buildPostToolUse(session_id, input):
  provider = input.provider ?? "claude-code"
  topic_signal = extractEventTopicSignal("PostToolUse", input)
  if provider !== "claude-code":               // Gemini/Codex: never rework
    return { type:"RecordEvent", ...implantEvent("PostToolUse", session_id,
             payloadFromExtra(input), topic_signal, input.provider) }

  tool_name = strOr(extraGet(input, "tool_name"), "")
  if tool_name not in {"Bash","Edit","Write","MultiEdit"}:         // is_rework_eligible_tool
    return { type:"RecordEvent", ...implantEvent("PostToolUse", session_id,
             payloadFromExtra(input), topic_signal, input.provider) }

  if tool_name === "MultiEdit":
    pairs = extractReworkEventsForMultiEdit(input.extra)
    if pairs.length === 0:                     // missing/empty/non-array edits → generic
      return { type:"RecordEvent", ...implantEvent("PostToolUse", session_id,
               payloadFromExtra(input), topic_signal, input.provider) }
    return { type:"RecordEvents", events: pairs.map(([file_path, had_failure]) =>
      implantEvent("post_tool_use_rework_candidate", session_id,
        { tool_name:"MultiEdit", file_path,              // null when absent (json! null parity)
          had_failure,
          tool_input:  extraGet(input,"tool_input")  ?? null,   // json! serializes missing as null
          tool_response: extraGet(input,"tool_response") ?? null },
        topic_signal, input.provider)) }

  had_failure = (tool_name === "Bash") ? isBashFailure(input.extra) : false
  file_path   = extractFilePath(input.extra, tool_name)             // null unless Edit/Write hit
  return { type:"RecordEvent", ...implantEvent("post_tool_use_rework_candidate", session_id,
           { tool_name, file_path, had_failure,
             tool_input: extraGet(input,"tool_input") ?? null,
             tool_response: extraGet(input,"tool_response") ?? null },
           topic_signal, input.provider) }

function isBashFailure(extra):                  // hook.rs:889-903
  ec = extra?.exit_code
  if Number.isInteger(ec) && ec !== 0: return true   // as_i64 parity: 1.5/"1"/true → not integer → skip
  if extra?.interrupted === true: return true        // as_bool parity: only JSON true counts
  return false

function extractFilePath(extra, tool_name):     // hook.rs:910-924
  if tool_name === "Edit":  return strOr(extra?.tool_input?.path, null)
  if tool_name === "Write": return strOr(extra?.tool_input?.file_path, null)
  return null

function extractReworkEventsForMultiEdit(extra):  // hook.rs:931-951
  edits = extra?.tool_input?.edits
  if not Array.isArray(edits): return []
  return edits.map(edit => [ strOr(edit?.path, null), false ])   // edits can't fail
```

## PreToolUse Arm + context_cycle Interception (hook.rs:672-861)
```
function buildPreToolUse(event, session_id, input):
  bare = strOr(input.mcp_context?.tool_name, null)   // typed access; only when mcp_context is object
  if bare !== null:                                  // Gemini promotion (clone, then promote)
    promoted = shallowCloneInput(input)
    if promoted.extra is not object: promoted.extra = {}
    else: promoted.extra = { ...promoted.extra }     // never mutate caller's input (R-01 clone rule)
    promoted.extra.tool_name = bare
    return buildCycleEventOrFallthrough(event, session_id, promoted)
  return buildCycleEventOrFallthrough(event, session_id, input)

function buildCycleEventOrFallthrough(event, session_id, input):
  tool_name = strOr(extraGet(input,"tool_name"), "")
  if tool_name !== "context_cycle" and tool_name !== "mcp__unimatrix__context_cycle":
    return genericRecordEvent(event, session_id, input)   // EXACT equality (security gate F-02)

  tool_input = extraGet(input, "tool_input")
  if tool_input === undefined:                  // Rust: extra.get("tool_input") None
    stderr "unimatrix: context_cycle PreToolUse missing tool_input"
    return genericRecordEvent(event, session_id, input)

  v = validateCycleParams(strOr(tool_input?.type,""), strOr(tool_input?.topic,""),
                          strOrUndef(tool_input?.phase), strOrUndef(tool_input?.outcome),
                          strOrUndef(tool_input?.next_phase))
      // strOrUndef: non-string → undefined ≙ Rust as_str() None
  if v is Err: stderr one-liner; return genericRecordEvent(event, session_id, input)

  event_type = {start: CYCLE_START_EVENT, "phase-end": CYCLE_PHASE_END_EVENT,
                stop: CYCLE_STOP_EVENT}[v.cycle_type]

  // goal only on Start (hook.rs:808-823), BYTE truncation at UTF-8 boundary
  goal = null
  if v.cycle_type === "start" and typeof tool_input?.goal === "string":
    g = tool_input.goal
    if Buffer.byteLength(g) > MAX_GOAL_BYTES:
      stderr "[unimatrix hook] goal exceeds MAX_GOAL_BYTES, truncating"
      goal = truncateUtf8(g, MAX_GOAL_BYTES)    // shared with transcript.js (byte-boundary safe)
    else: goal = g

  payload = { feature_cycle: v.topic }          // insertion order: feature_cycle first (json! parity)
  if v.phase      !== null: payload.phase = v.phase
  if v.outcome    !== null: payload.outcome = v.outcome
  if v.next_phase !== null: payload.next_phase = v.next_phase
  if goal         !== null: payload.goal = goal
  return { type:"RecordEvent", ...implantEvent(event_type, session_id, payload,
           /*topic_signal=*/ v.topic, input.provider) }
```

### validateCycleParams(type, topic, phase, outcome, next_phase) — validation.rs:413-510 port
```
type ∉ {"start","phase-end","stop"}                          -> Err
topic === ""                                                 -> Err
clean = [...topic].filter(ch => isAscii(ch) && !isAsciiControl(ch)).slice(0,128).join("")
clean === "" or !isValidFeatureId(clean)                     -> Err
phase / next_phase: each via validatePhaseField:
  undefined -> null
  trim; empty -> Err; lowercase; >64 CODE POINTS -> Err ([...s].length, char-count parity)
  contains " " -> Err; any char ∉ [a-z0-9-_] -> Err; -> normalized
outcome: undefined -> null; >512 CODE POINTS -> Err;
  any code point <= 0x1F -> Err; -> as-is
return { cycle_type: type, topic: clean, phase, outcome, next_phase }

function isValidFeatureId(s):   // validation.rs:397-407 — BYTE length ≤128, ASCII-only body
  return s !== "" && Buffer.byteLength(s) <= 128 && s.includes("-")
      && !s.startsWith("-") && !s.endsWith("-")
      && /^[A-Za-z0-9\-_.]+$/.test(s)
```

## Generic Arm + Topic Signal (hook.rs:866-878, 376-437; attribution.rs:15-92)
```
function genericRecordEvent(event, session_id, input):
  return { type:"RecordEvent", ...implantEvent(event, session_id, payloadFromExtra(input),
           extractEventTopicSignal(event, input), input.provider) }

function extractEventTopicSignal(event, input):
  switch event:
    "PreToolUse" | "PostToolUse" | "PostToolUseFailure":
      v = extraGet(input,"tool_input")
      text = (typeof v === "string") ? v : (v === undefined ? "" : JSON.stringify(v))
      return extractTopicSignal(text)
    "SubagentStart":  return extractTopicSignal(strOr(extraGet(input,"agent_type"), ""))
    "UserPromptSubmit": return extractTopicSignal(input.prompt ?? "")
    default:
      if input.extra === null: return null           // Rust extra.is_null()
      return extractTopicSignal(JSON.stringify(input.extra))
      // NOTE: {} stringifies to "{}" → no signal; matches Rust (empty flatten object)

function extractTopicSignal(text):                   // attribution.rs:81-92 priority chain
  return extractFromPath(text) ?? extractFeatureIdPattern(text) ?? extractFromGitCheckout(text)

function extractFromPath(s):                         // attribution.rs:26-38
  scan every occurrence of "product/features/" left-to-right;
  segment = text after marker up to next "/" (or end);
  first segment passing isValidFeatureIdAttr → return it; else null

function extractFeatureIdPattern(s):                 // attribution.rs:44-55
  words = s.split(ch where ch is Unicode-whitespace or " ' ( ))      // /[\s"'()]/u
  for word: candidate = trim from BOTH ends every char that is NOT Unicode-alphanumeric
            and NOT "-"                              // \p{L}\p{N} regex; keeps -, trims ._ at ends
            if isValidFeatureIdAttr(candidate): return candidate
  return null

function extractFromGitCheckout(s):                  // attribution.rs:58-69
  idx = s.indexOf("feature/"); if -1: return null
  candidate = takeWhile(s.slice(idx+8), ch => Unicode-alphanumeric or "-")
  return isValidFeatureIdAttr(candidate) ? candidate : null

function isValidFeatureIdAttr(s):                    // attribution.rs:15-23 (BYTE length ≤128)
  same as isValidFeatureId above (shared implementation, both use byte length + ASCII body)
```

## Error Handling
- Pure and total: every malformed shape falls through to `genericRecordEvent` or a
  defensive default — never throws (FR-03.7 parity). All `?.` chains mirror Rust
  Option chaining.
- stderr one-liners only where hook.rs eprintln!s (cycle validation failure, missing
  tool_input) — same trigger points, never on stdout.

## Key Test Scenarios (corpus-driven — ADR-001 inventory IS the test list)
1. 4-vs-5-word UserPromptSubmit boundary; whitespace-only; empty; multi-space separators.
2. Bash exit_code 0 / 7 / missing / 1.5 / "2"; interrupted true/false/"true".
3. Edit `tool_input.path` vs Write `tool_input.file_path`; MultiEdit normal/empty/
   missing/non-array `edits` (empty → single generic RecordEvent, not RecordEvents).
4. context_cycle bare + prefixed; near-miss `"evil_context_cycle_bypass"` NOT intercepted;
   invalid params fall through; goal > 1024 B truncated at a multi-byte boundary;
   mcp_context promotion (Gemini) — promoted clone, caller input unmutated.
5. PostToolUseFailure with null/empty extra, missing tool_name.
6. ppid fallback: missing session_id → `ppid-{process.ppid}`; normalized to `ppid-X`
   in golden comparison.
7. Unknown event + unknown-extra-keys: payload preserves all stdin extras verbatim,
   insertion order (flatten parity, ass-071).
8. Topic-signal chain: `product/features/x-1/f`, bare `col-002` token, `feature/abc-12`,
   Unicode-whitespace separators, 129-byte candidate rejected.
9. AC-14: every produced frame round-trips against `bindings/fixtures/*.json`.
