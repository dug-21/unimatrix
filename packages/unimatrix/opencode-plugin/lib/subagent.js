// @ts-check
/**
 * subagent.js — subagent alignment on the OBSERVE channel (AC-04, ADR-007, R-06/R-07).
 *
 * A subagent spawn is reachable only as a bus-derived child `session.created`
 * with `parentID != null`. OpenCode validates the subagent type (agent.get(),
 * LLM out of the loop) and stamps the validated agent onto the child session.
 *
 * R-07 (spoof rejection): the validated agent rides `extra.agent_type` ONLY — the
 * observe channel the pipeline already understands (hook.rs reads
 * `input.extra["agent_type"]`). It is NEVER lifted from tool args (a spoofable,
 * LLM-controlled channel). This module reads the agent solely from the harness
 * session field; the tool-execution handlers never populate agent_type at all, so
 * a conflicting agent value inside tool args cannot override it.
 *
 * R-06 (ordering race): a child `session.created` may arrive before the parent is
 * registered server-side. Rather than resolve the parent cycle eagerly plugin-side
 * (which would drop the link on the race), the frame CARRIES `parent_id` so the
 * server correlates parent→feature/cycle when both are known. The link is never
 * dropped. (DELIVERY DECISION: server-side correlation via carried parentID —
 * resolves the ordering race by construction; a delivery PoC may add bounded
 * plugin-side pre-resolution later without changing this contract.)
 *
 * SDK NOTE: `@opencode-ai/sdk` v1.18.31 `Session` does not list an `agent` field;
 * the runtime may still carry it. `deriveValidatedAgent` reads it defensively and
 * NEVER fabricates one — absent → agent_type omitted (honest), parent_id still
 * carried so alignment is not lost.
 */

import { buildHookInput } from "./frame.js";

/**
 * Read the harness-validated agent from a child session, defensively.
 * @param {any} info - session.created `properties.info`
 * @returns {(string|undefined)}
 */
export function deriveValidatedAgent(info) {
  if (info && typeof info.agent === "string" && info.agent) {
    return info.agent;
  }
  return undefined;
}

/**
 * True iff the created session is a subagent (has a non-empty parentID).
 * @param {any} info
 * @returns {boolean}
 */
export function isSubagentSession(info) {
  return !!(info && typeof info.parentID === "string" && info.parentID);
}

/**
 * Build the SubagentStart observe frame. Injection is NOT attempted (structurally
 * unreachable in OpenCode — accepted parity gap, FR-12); this emits the observe
 * frame only.
 *
 * @param {any} info - session.created `properties.info`
 * @param {(string|undefined)} defaultCwd
 * @returns {Record<string, unknown>}
 */
export function buildSubagentFrame(info, defaultCwd) {
  const agent = deriveValidatedAgent(info);
  /** @type {Record<string, unknown>} */
  const extra = { parent_id: info.parentID, bus_derived: true };
  if (agent) {
    // OBSERVE CHANNEL ONLY (R-07). Never written into MCP tool args.
    extra.agent_type = agent;
  }
  return buildHookInput(
    "SubagentStart",
    { session_id: info.id, cwd: info.directory, extra },
    defaultCwd,
  );
}
