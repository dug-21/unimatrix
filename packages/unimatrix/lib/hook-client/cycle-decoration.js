"use strict";

/**
 * cycle-decoration.js — vnc-030 FNF cycle-stamp decoration seam (ADR-002).
 *
 * Split from index.js for the modular-file limit; the single seam between
 * buildRequest and dispatch. `decorateCycleStamp` mutates the in-memory `request`
 * in place, upstream of selectTransport (index.js:410) so BOTH transports
 * JSON.stringify the same decorated object → cycle_stamp byte-identical on the UDS
 * frame and the HTTP body (AC-10/FR-29). All tracker I/O lives here (only the
 * caller has config.stateDir); build-request*.js gets zero vnc-030 logic (SR-09).
 *
 * Fail-open (C-04): the whole body is try/catch-wrapped and every internal call
 * is a never-throw cycles/state helper. A failure leaves `request` unstamped and
 * the event is still sent (exit 0, no stdout).
 *
 * OQ-E (ADR-006 §7) — BRANCH A: the "I am a subagent" marker
 * (`input.extra.agent_type`) rides a structurally distinct stdin channel from the
 * named top-level `session_id` field, so a CLI regression that breaks root-id
 * inheritance does NOT strip the marker. The production canary fires under drift
 * and ships ACTIVE. The test-time zero-tolerance invariant ships either way.
 */

const cycles = require("./cycles");
const state = require("./state");
const buildRequestMod = require("./build-request");

// Cycle event_type constants re-exported by build-request.js (build-request.js:143-145).
const {
  CYCLE_START_EVENT,
  CYCLE_PHASE_END_EVENT,
  CYCLE_STOP_EVENT,
} = buildRequestMod;

/** True iff v is a non-null, non-array plain object. */
function isPlainObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

/**
 * The ImplantEvents a frame carries. The decoration must iterate BOTH single and
 * batch shapes (R-06). A RecordEvent frame IS a flattened ImplantEvent, so
 * mutating the frame object mutates the event. SessionRegister/SessionClose carry
 * none → skipped (no stamp, no lifecycle, no canary).
 */
function frameEvents(request) {
  switch (request.type) {
    case "RecordEvent":
      return [request];
    case "RecordEvents":
      return Array.isArray(request.events) ? request.events : [];
    default:
      return [];
  }
}

/** True iff an event is a CYCLE_* declaration frame (keeps its topic_signal). */
function isCycleEvent(ev) {
  return (
    ev.event_type === CYCLE_START_EVENT ||
    ev.event_type === CYCLE_PHASE_END_EVENT ||
    ev.event_type === CYCLE_STOP_EVENT
  );
}

/** payload.next_phase iff a string, else null (lifecycle dispatch helper). */
function payloadNextPhase(ev) {
  return ev.payload && typeof ev.payload.next_phase === "string"
    ? ev.payload.next_phase
    : null;
}

/**
 * Detect "I am a subagent" from hook stdin, INDEPENDENTLY of root-id inheritance
 * (OQ-E Branch A). The signal is the subagent role marker `input.extra.agent_type`
 * — populated by Claude Code's subagent-spawn lifecycle on a structurally
 * distinct channel from the named top-level `session_id` field. A CLI regression
 * that breaks root-id inheritance (stops propagating the root id to subagent
 * stdin) does NOT strip `agent_type`: the two are produced by different
 * subsystems, so the canary still fires under drift (ADR-006 §7).
 *
 * rootSessionId is the inherited root id the subagent event carries (= the frame's
 * session id today, depth-1; a depth>1 grandchild carries an intermediate id —
 * same shape).
 * @returns {{ isSubagent: boolean, rootSessionId: string|null }}
 */
function subagentContext(input, sid) {
  const isSubagent =
    isPlainObject(input.extra) && typeof input.extra.agent_type === "string";
  return {
    isSubagent,
    rootSessionId: typeof sid === "string" && sid.length > 0 ? sid : null,
  };
}

/**
 * FNF decoration seam (ADR-002 §2). Mutates `request` in place; returns nothing;
 * NEVER throws (C-04).
 *
 * Ordering (ADR-002 §2.4-2.5): lifecycle dispatch BEFORE decoration (so the
 * cycle_stop frame goes unstamped — its tracker is gone); the caller invokes this
 * BEFORE runFireAndForget (so queue.enqueue on send-failure stores the
 * post-decoration request and replayed frames carry the stamp true at event time).
 *
 * @param {object} request HookRequest (mutated in place)
 * @param {object} input   parsed HookInput (subagent marker source)
 * @param {object} config  resolved config (stateDir)
 * @param {function} sessionIdOf index.js helper: HookRequest → session id
 */
function decorateCycleStamp(request, input, config, sessionIdOf) {
  try {
    const stateDir = config.stateDir;
    const sid = sessionIdOf(request);
    if (typeof sid !== "string" || sid.length === 0) return; // nothing to key on
    const events = frameEvents(request);
    if (events.length === 0) return; // SessionRegister/SessionClose — skip entirely

    // (1) LIFECYCLE DISPATCH — keys on frame event_type, NEVER canonical name.
    for (const ev of events) {
      if (ev.event_type === CYCLE_START_EVENT) {
        const topic = ev.payload ? ev.payload.feature_cycle : undefined;
        cycles.writeCycle(stateDir, sid, topic, payloadNextPhase(ev));
      } else if (ev.event_type === CYCLE_PHASE_END_EVENT) {
        cycles.updatePhase(stateDir, sid, payloadNextPhase(ev));
      } else if (ev.event_type === CYCLE_STOP_EVENT) {
        cycles.deleteCycle(stateDir, sid);
      }
    }

    // (2) DECORATION — one readCycle.
    const tracker = cycles.readCycle(stateDir, sid); // {topic, phase}|null
    if (tracker !== null) {
      for (const ev of events) {
        ev.cycle_stamp = { topic: tracker.topic };
        if (tracker.phase !== null && tracker.phase !== undefined) {
          ev.cycle_stamp.phase = tracker.phase; // omit-when-null parity (ADR-003 §4)
        }
        if (!isCycleEvent(ev)) {
          delete ev.topic_signal; // SUPPRESSION (AC-03); CYCLE_* keep it (the declaration)
        }
      }
      return;
    }

    // (3) CANARY (subagent-gated; ADR-006 rev2) — miss branch only.
    const ctx = subagentContext(input, sid);
    if (ctx.isSubagent && ctx.rootSessionId) {
      // The carried root id has no tracker = inheritance drift. Depth-1: this is
      // redundant with the readCycle(sid) above (rootSessionId === sid); written
      // explicitly so a depth>1 grandchild carrying an intermediate id still trips
      // the canary (ADR-006 §4-5, R-14). Depth-0 / non-subagent miss → no
      // increment, no extra read (never-declare = structural noise).
      if (cycles.readCycle(stateDir, ctx.rootSessionId) === null) {
        state.bumpStampMiss(stateDir);
      }
    }
  } catch (_e) {
    // Last-resort: decoration never escalates; request sent as-is (unstamped).
  }
}

module.exports = {
  decorateCycleStamp,
  frameEvents,
  isCycleEvent,
  subagentContext,
};
