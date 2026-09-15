# C1 — OpenCode plugin shim (TypeScript, net-new)

**Location:** `packages/unimatrix/opencode-plugin/` (net-new module tree; ships at/under cap).
**Dep:** `@opencode-ai/plugin@1.18.31` (`Plugin(input, opts) => Promise<Hooks>`).
**ADRs:** ADR-004 (shim = normalization boundary), ADR-007 (subagent observe channel), ADR-009
(degraded/experimental legs). **Risks:** R-01, R-06, R-07, R-11, R-12, R-13, R-14, R-15.
**Wave:** 3 (needs C4 `--model` contract + C2 opencode provider arm). **col-022:** its event map is
mirrored by C2/C3 and covered by C8.

## Purpose

OpenCode has no stdin command-hook contract; its only surface is an in-process TS plugin. This shim IS
the normalization boundary: it subscribes to OpenCode typed hooks + the event bus, maps each of the 7
canonical events to a Claude-shaped `HookInput`, resolves the backend model and subagent alignment, and
shells to `unimatrix hook <EVENT> --provider opencode --model <providerID>/<modelID>` over the existing
UDS transport. It reuses the entire proven Rust pipeline unchanged (NFR-02).

## Plugin entry & initialization

```
export const UnimatrixObservePlugin = async (input /* PluginInput */) => {
  // input: { client, project, directory, worktree, $ (Bun shell) }
  const ctx = {
    sh: input.$,                              // Bun shell for `unimatrix hook ...`
    cwd: input.worktree ?? input.directory,   // derive cwd (SessionStart has no cwd on the bus)
    client: input.client,                     // SDK client for parentID→cycle resolution (AC-04)
    activeSessions: new Map(),                // sessionID → { active, lastStopEmittedAt } (Stop over-count guard)
    preCompactEnabled: <flag default, OQ-7>,  // ADR-009 experimental gate
  };
  return {
    // typed hooks + bus subscriptions (below)
  };
};
```
- **Config note:** the exported symbol / file layout must match what C9 provisions into
  `.opencode/plugins/` and/or the `plugin:[]` array. C1 and C9 agree on the artifact name.

## The 7 event handlers → canonical HookInput

Common builder:
```
function buildHookInput(event, fields):
    return {
      hook_event_name: event,                 // canonical name (C2/C3 emit identity)
      session_id: fields.sessionID,
      cwd: fields.cwd ?? ctx.cwd,
      transcript_path: fields.transcriptPath ?? null,   // absent for bus-derived (R-14)
      prompt: fields.prompt ?? null,
      extra: fields.extra ?? {},              // tool_name/tool_input/tool_response/agent_type/exit_code
    }

async function emit(event, hookInput, model /* "providerID/modelID" | undefined */):
    // fail-open (R-13/NFR-02): never throw into the OpenCode session
    try:
        args = ["hook", event, "--provider", "opencode"]
        if model && isValidModelId(model): args.push("--model", model)
        await ctx.sh`unimatrix ${args}`.stdin(JSON.stringify(hookInput))   // stdin carries HookInput JSON
    catch (e):
        console.warn("[unimatrix] emit failed, session continues:", e)     // drop, do not block/crash
```

| Canonical | OpenCode source | Handler logic |
|---|---|---|
| **SessionStart** | `session.created` (+`session.updated`) | bus-derived. Derive `cwd` from PluginInput; no transcript. Mark active in `activeSessions`. If `parentID != null` → route to SubagentStart instead (see below). |
| **UserPromptSubmit** | `chat.message` | reachable. `prompt` from message/parts. Resolve model → `--model` (carries backend model). |
| **PreToolUse** | `tool.execute.before` | reachable. `extra.tool_name`, `extra.tool_input` from args. |
| **PostToolUse** | `tool.execute.after` | reachable. `extra.tool_response`; synthesize failure semantics from `output`/`metadata` → `extra.exit_code`. |
| **SubagentStart** | child `session.created` w/ `parentID` + `session.agent` | observe-only. See §Subagent. Injection NOT attempted (parity gap, FR-12). |
| **PreCompact** | `experimental.session.compacting` | gated by `ctx.preCompactEnabled` + try/catch (ADR-009). API absent/throws → no-op loud in plugin logs, other 6 unaffected. |
| **Stop** | `session.idle` | bus-derived. Over-count guard (see §Stop). Compute `duration`/`outcome` plugin-side; mark degraded. |

## Model resolution (AC-06)

```
function resolveModel(evt):
    m = evt.model ?? evt.providerID_modelID   // in-process model:{providerID, modelID}
    if !m: return undefined                    // cloud/absent → no --model → model_id NULL
    return `${m.providerID}/${m.modelID}`      // e.g. "ollama/qwen3-coder"
```
- Reachable on `chat.message` (carries model). For events without a model, omit `--model`.
- Validate with `isValidModelId` (mirror C4/C3 charset: `^[a-z0-9._/-]{1,128}$`) before passing (R-15).

## Subagent alignment (AC-04, ADR-007) — §Subagent

```
on session.created (evt):
    if evt.parentID == null:
        emit SessionStart
    else:
        agent = evt.agent            // OpenCode-validated via agent.get() — trusted, LLM out of loop
        // resolve parent → owning feature/cycle
        cycle = await resolveParentCycle(evt.parentID)   // via ctx.client / parent's cycle stamp
        hookInput = buildHookInput("SubagentStart", {
            sessionID: evt.sessionID,
            extra: { agent_type: agent },                // OBSERVE CHANNEL only (R-07)
            // cycle alignment carried per existing SubagentStart handling / cycle_stamp
        })
        emit("SubagentStart", hookInput, resolveModel(evt))
```
- **R-07 (spoof):** the validated `agent` rides `extra.agent_type` ONLY. It is NEVER written into MCP
  `context_*` tool args. A conflicting agent value in tool args must not override the validated field.
- **R-06 (ordering race):** child `session.created` may arrive before parent registration.
  `resolveParentCycle` must retry/defer (bounded) or carry `parentID` so the server can correlate later —
  never drop the parent link. Flagged for delivery: whether correlation resolves plugin-side or
  parentID is carried for server-side correlation.

## Stop over-count guard (R-11, ADR-009) — §Stop

```
on session.idle (evt):
    s = ctx.activeSessions.get(evt.sessionID)
    if s && s.active:
        s.active = false                       // emit Stop once per active→idle transition
        emit("Stop", buildHookInput("Stop", { sessionID: evt.sessionID,
              extra: { duration: computeDuration(s), outcome: deriveOutcome(evt), degraded: true } }))
    // repeated idle within same active window → no-op (dedupe). Reset s.active on next chat.message.
```
- OQ-3/R-11: exact idle semantics need a delivery-time PoC; until measured, guard conservatively (dedupe).
- Mark Stop/SessionStart records as bus-derived/degraded (not full-fidelity), never present as full.

## Data flow

IN: OpenCode typed hooks + bus events (untrusted, R-15). OUT: `unimatrix hook` invocations over UDS →
C2 → C4 → C5. No direct DB access; reuses the Rust pipeline (queue/fail-open/drop-detector) unchanged.

## Error handling

- **Fail-open (R-13/NFR-02):** every `emit` wrapped in try/catch; binary-absent / socket-down / shell
  error → warn + drop the event, session continues. Never throw, never block.
- **PreCompact (R-12):** experimental API absent/changed → gated no-op, loud in plugin logs, non-fatal.
- **Bus-derived corruption (R-14):** missing `transcript_path` stored as null (not bogus); underivable
  `cwd` → fall to PluginInput; empty/malformed payload → skip emit, do not store corrupt fields.
- **Untrusted input (R-15):** validate `model_id` charset; never interpolate untrusted values into the
  shell command — pass as discrete args / stdin JSON, not string-built commands.

## Modularity (ADR-005)

Net-new module tree under `opencode-plugin/`; split by concern (entry, event-map, model-resolve,
subagent, stop-guard) so no file exceeds the cap. Ships at/under cap.

## Key test scenarios (hints for tester)

1. **7-event mapping** (AC-01): each OpenCode-shaped input → correct canonical `unimatrix hook` call;
   reachable + bus-derived land as stored records; SubagentStart injection recorded as parity gap (no
   false-pass, no silent drop, R-16).
2. **AC-06 end-path (GATING, R-01):** drive a real local-model event
   (`model:{providerID:"ollama", modelID:"qwen3-coder"}`) plugin→hook→wire→persist→store→query; queried
   record distinct from a cloud-model event; negative assertion (carrier dropped → fails).
3. **Subagent alignment** (AC-04/R-06): child `session.created`+`parentID`+`agent` → stored record
   aligned to owning feature/cycle, not orphaned; ordering race resolves.
4. **Spoof rejection** (R-07): `agent` rides `extra.agent_type`; tool-args agent value does not override.
5. **Stop over-count** (R-11): repeated `session.idle` → exactly one Stop.
6. **PreCompact fail-safe** (R-12): API present → lands; absent → no-op, session survives.
7. **Fail-open** (R-13): binary/socket unavailable → session not blocked/crashed.
8. **Bus-derived degradation** (R-14): missing transcript_path handled; degraded flag set.
</content>
