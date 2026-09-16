// @ts-check
/**
 * events.js — the 7-event map: OpenCode typed hooks + event bus → canonical
 * HookInput frames → emit (AC-01). Every handler is fully wrapped: a handler
 * MUST NOT throw into the OpenCode session (R-13/NFR-02).
 *
 * Canonical ← OpenCode source:
 *   SessionStart    ← bus session.created (parentID == null)
 *   SubagentStart   ← bus session.created (parentID != null) + validated agent
 *   UserPromptSubmit← chat.message              (carries the backend model)
 *   PreToolUse      ← tool.execute.before
 *   PostToolUse     ← tool.execute.after
 *   PreCompact      ← experimental.session.compacting (gated, ADR-009)
 *   Stop            ← bus session.idle          (over-count guarded, ADR-009)
 */

import { buildHookInput } from "./frame.js";
import { emit } from "./emit.js";
import { resolveModel } from "./model.js";
import { markActive, markModel, sessionModel, takeStop } from "./session-state.js";
import { isSubagentSession, buildSubagentFrame } from "./subagent.js";

/**
 * Extract prompt text from a chat.message output (message + parts).
 * @param {any} output
 * @returns {(string|undefined)}
 */
function extractPrompt(output) {
  try {
    if (!output) {
      return undefined;
    }
    const parts = Array.isArray(output.parts) ? output.parts : [];
    const text = parts
      .filter((p) => p && p.type === "text" && typeof p.text === "string")
      .map((p) => p.text)
      .join("");
    if (text) {
      return text;
    }
    const msg = output.message;
    if (msg && typeof msg.content === "string" && msg.content) {
      return msg.content;
    }
    return undefined;
  } catch (_err) {
    return undefined;
  }
}

/**
 * Synthesize exit-code failure semantics for PostToolUse from output/metadata.
 * Conservative: only signals failure on an explicit marker; defaults to 0.
 * @param {any} output
 * @returns {number}
 */
function deriveExitCode(output) {
  try {
    const md = output && output.metadata;
    if (md && typeof md === "object") {
      if (md.error || md.failed === true || md.success === false) {
        return 1;
      }
      if (typeof md.exit === "number") {
        return md.exit;
      }
      if (typeof md.exitCode === "number") {
        return md.exitCode;
      }
    }
    return 0;
  } catch (_err) {
    return 0;
  }
}

/**
 * Bus event dispatch (session.created / session.idle).
 * @param {any} ctx
 * @param {any} info
 */
async function onSessionCreated(ctx, info) {
  if (!info || typeof info.id !== "string" || !info.id) {
    return; // malformed bus payload → skip, no corrupt frame (R-14)
  }
  const s = markActive(ctx, info.id, true);
  if (isSubagentSession(info)) {
    const frame = buildSubagentFrame(info, ctx.cwd);
    await emit(ctx, "SubagentStart", frame, s.model);
    return;
  }
  const frame = buildHookInput(
    "SessionStart",
    { session_id: info.id, cwd: info.directory, extra: { bus_derived: true } },
    ctx.cwd,
  );
  await emit(ctx, "SessionStart", frame, undefined);
}

/**
 * @param {any} ctx
 * @param {string} sessionID
 */
async function onSessionIdle(ctx, sessionID) {
  if (typeof sessionID !== "string" || !sessionID) {
    return;
  }
  const stop = takeStop(ctx, sessionID);
  if (!stop) {
    return; // repeated idle within the same active window → no-op (R-11)
  }
  /** @type {Record<string, unknown>} */
  const extra = { outcome: "idle", degraded: true, bus_derived: true };
  if (typeof stop.duration === "number") {
    extra.duration = stop.duration;
  }
  const frame = buildHookInput("Stop", { session_id: sessionID, extra }, ctx.cwd);
  await emit(ctx, "Stop", frame, stop.model);
}

/**
 * Build the OpenCode `Hooks` object.
 * @param {any} ctx
 * @returns {import("@opencode-ai/plugin").Hooks}
 */
export function makeHooks(ctx) {
  return {
    // --- Event bus: SessionStart / SubagentStart / Stop -------------------
    async event({ event }) {
      try {
        if (!event || typeof event.type !== "string") {
          return;
        }
        if (event.type === "session.created") {
          await onSessionCreated(ctx, event.properties && event.properties.info);
        } else if (event.type === "session.idle") {
          await onSessionIdle(ctx, event.properties && event.properties.sessionID);
        }
      } catch (_err) {
        /* fail-open: never throw into the session */
      }
    },

    // --- UserPromptSubmit ← chat.message (carries the model) --------------
    async "chat.message"(input, output) {
      try {
        if (!input || typeof input.sessionID !== "string" || !input.sessionID) {
          return;
        }
        const model = resolveModel(input.model);
        markActive(ctx, input.sessionID, false); // re-arm the Stop guard
        markModel(ctx, input.sessionID, model);
        const frame = buildHookInput(
          "UserPromptSubmit",
          { session_id: input.sessionID, prompt: extractPrompt(output) },
          ctx.cwd,
        );
        await emit(ctx, "UserPromptSubmit", frame, model);
      } catch (_err) {
        /* fail-open */
      }
    },

    // --- PreToolUse ← tool.execute.before ---------------------------------
    // R-07: tool args (output.args) are NEVER read for agent_type — only
    // tool_name/tool_input are lifted, so a spoofed `agent` in args cannot
    // populate the observe channel.
    async "tool.execute.before"(input, output) {
      try {
        if (!input || typeof input.sessionID !== "string" || !input.sessionID) {
          return;
        }
        const frame = buildHookInput(
          "PreToolUse",
          {
            session_id: input.sessionID,
            extra: { tool_name: input.tool, tool_input: output && output.args },
          },
          ctx.cwd,
        );
        await emit(ctx, "PreToolUse", frame, sessionModel(ctx, input.sessionID));
      } catch (_err) {
        /* fail-open */
      }
    },

    // --- PostToolUse ← tool.execute.after ---------------------------------
    async "tool.execute.after"(input, output) {
      try {
        if (!input || typeof input.sessionID !== "string" || !input.sessionID) {
          return;
        }
        const frame = buildHookInput(
          "PostToolUse",
          {
            session_id: input.sessionID,
            extra: {
              tool_name: input.tool,
              tool_input: input.args,
              tool_response: output && output.output,
              exit_code: deriveExitCode(output),
            },
          },
          ctx.cwd,
        );
        await emit(ctx, "PostToolUse", frame, sessionModel(ctx, input.sessionID));
      } catch (_err) {
        /* fail-open */
      }
    },

    // --- PreCompact ← experimental.session.compacting (gated, ADR-009) ----
    async "experimental.session.compacting"(input, _output) {
      try {
        if (!ctx.preCompactEnabled) {
          return; // OQ-7 kill-switch
        }
        if (!input || typeof input.sessionID !== "string" || !input.sessionID) {
          return;
        }
        const frame = buildHookInput(
          "PreCompact",
          { session_id: input.sessionID, extra: { experimental: true } },
          ctx.cwd,
        );
        await emit(ctx, "PreCompact", frame, sessionModel(ctx, input.sessionID));
      } catch (_err) {
        // Experimental API drift must never crash the session or the other legs.
        try {
          ctx.log.warn("[unimatrix] precompact leg skipped (experimental API)");
        } catch (_logErr) {
          /* logging must never throw */
        }
      }
    },
  };
}
