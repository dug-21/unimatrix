# Component: build-request-sentinel (`lib/hook-client/build-request-tools.js`)

ADR-004 §1. FR-27, AC-08. Risk R-11. Merge step 4.
Existing source: build-request-tools.js (read in full).

## Purpose

Retire standalone PreToolUse observation at the client level: every non-cycle
PreToolUse path returns a `null` no-send sentinel instead of `genericRecordEvent`,
while cycle interception (`cycle_start`/`phase-end`/`stop`) and the F-02
exact-tool-name security gate are fully preserved. Only PreToolUse gets the sentinel;
all other events' fallthrough observation is untouched. index.js converts `null` →
immediate exit-0 return (see index-dispatch.md).

## Modified: `buildCycleEventOrFallthrough(event, sessionId, input)` (line 314)

Currently returns `genericRecordEvent(...)` on three non-cycle paths. Change each of
those three to return `null`; keep the cycle frame construction and ALL stderr lines
exactly as today (R-11 s2: stderr retained on missing tool_input / failed validation).

```
FUNCTION buildCycleEventOrFallthrough(event, sessionId, input):
  toolName = strOr(extraGet(input, "tool_name"), "")

  // F-02 security gate: EXACT equality (substring "evil_context_cycle_bypass" fails)
  IF toolName !== "context_cycle" AND toolName !== "mcp__unimatrix__context_cycle":
      RETURN null                              // CHANGED: was genericRecordEvent — no-send sentinel

  toolInput = extraGet(input, "tool_input")
  IF toolInput === undefined:
      stderr("unimatrix: context_cycle PreToolUse missing tool_input")  // RETAINED
      RETURN null                              // CHANGED

  tiObj = (toolInput is plain object) ? toolInput : {}
  validated = validateCycleParams(strOr(tiObj.type,""), strOr(tiObj.topic,""),
                                  strOrUndef(tiObj.phase), strOrUndef(tiObj.outcome),
                                  strOrUndef(tiObj.next_phase))
  IF NOT validated.ok:
      stderr("unimatrix: context_cycle validation failed in hook (tool_name=" + toolName + ")")  // RETAINED
      RETURN null                              // CHANGED

  // --- cycle frame construction: UNCHANGED (stays fully parity-tested) ---
  eventType = (start → CYCLE_START_EVENT | phase-end → CYCLE_PHASE_END_EVENT | stop → CYCLE_STOP_EVENT)
  goal = (start && string goal) ? truncateUtf8-if-over-MAX_GOAL_BYTES : null   // + existing stderr on truncate
  payload = { feature_cycle: validated.topic, [phase], [outcome], [next_phase], [goal] }  // insertion order unchanged
  RETURN recordEventFrame(eventType, sessionId, payload, validated.topic, input.provider)
```

The cycle frame path is byte-identical to today and to the Rust hook (cycle frames
remain in the parity corpus — ADR-004 §4). Only the three fallthrough returns change
from `genericRecordEvent` to `null`.

## `buildPreToolUse(event, sessionId, input)` (line 289) — unchanged logic

The `mcp_context.tool_name` promotion (R-01 clone, never mutate caller input) is
unchanged; it still calls `buildCycleEventOrFallthrough` on the promoted/raw input.
Because both promoted and non-promoted paths route through the modified function, a
bare-tool PreToolUse whose promoted tool_name is not a cycle tool also returns `null`
— correct (it was observation-only before).

## NOT changed (scope guard, ADR-004 §1, R-11 s6)

- `genericRecordEvent` itself — still used by PostToolUse arm fallthroughs,
  SubagentStart non-snippet path, and the generic dispatch arm.
- `buildPostToolUse`, `buildSubagentStart`, all rework helpers, all accessors.
- `validateCycleParams` and the cycle constants.
- Only PreToolUse loses fallthrough observation. PostToolUse / PostToolUseFailure
  fallthrough observation is untouched.

## Data flow

`build-request.js` dispatch → `buildPreToolUse` → `buildCycleEventOrFallthrough` →
{cycle RecordEvent frame | `null`}. The `null` propagates up through `buildRequest`
to index.js, which short-circuits (exit 0, no send). A valid cycle event flows
through the normal FNF path (RecordEvent → fire-and-forget).

## Error handling

Pure / never throws (module contract). Sentinel is a value, not an exception. stderr
writes stay wrapped where the file already wraps them; the existing direct
`process.stderr.write` calls for cycle diagnostics are retained verbatim (they are
the only intended user signal on a malformed cycle invocation).

## Key test scenarios (hints for tester)

1. Sentinel matrix: non-cycle tool name → `null` (no frame); missing `tool_input` →
   `null` + retained stderr; failed `validateCycleParams` → `null` + retained
   stderr; valid cycle → frame identical to the Rust hook's — R-11 s2, AC-08.
2. F-02 defense-in-depth: `evil_context_cycle_bypass` (regex-substring) → `null`
   (exact-equality gate holds) — R-11 s3.
3. `buildPreToolUse` bare-tool promotion still clones input (no caller mutation) and
   routes through the sentinel — regression guard.
4. PostToolUse / PostToolUseFailure fallthrough observation untouched (only PreToolUse
   gets the sentinel) — R-11 s6.
5. Cycle frames (start/phase-end/stop) remain byte-parity-tested in the corpus —
   ADR-004 §4.
