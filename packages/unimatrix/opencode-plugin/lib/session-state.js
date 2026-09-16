// @ts-check
/**
 * session-state.js — per-session bookkeeping for the Stop over-count guard
 * (R-11, ADR-009) and the per-session model cache (AC-06).
 *
 * OpenCode's `session.idle` semantics are undocumented (OQ-3/R-11) — it may fire
 * on every idle transition. The guard emits Stop exactly ONCE per active→idle
 * transition: a session is marked active on session.created and re-armed on each
 * chat.message; `takeStop` flips it inactive and returns the transition once,
 * then no-ops until the session is re-armed. Conservative until a delivery-time
 * PoC refines it; it never silently over-counts.
 *
 * Model cache: only `chat.message` carries a typed backend model. We remember the
 * last validated model per session so tool/Stop/subagent frames for that session
 * can still carry `--model`, keeping local-vs-cloud attribution (AC-06) across the
 * whole session, not just the prompt event.
 */

/**
 * @typedef {{ active: boolean, startedAt: number, model: (string|undefined) }} SessionState
 */

/**
 * @param {{ sessions: Map<string, SessionState> }} ctx
 * @param {string} sessionID
 * @returns {SessionState}
 */
export function ensureState(ctx, sessionID) {
  let s = ctx.sessions.get(sessionID);
  if (!s) {
    s = { active: false, startedAt: Date.now(), model: undefined };
    ctx.sessions.set(sessionID, s);
  }
  return s;
}

/**
 * Mark a session active (arms the Stop guard). Called on session.created and
 * re-arm on chat.message.
 *
 * @param {{ sessions: Map<string, SessionState> }} ctx
 * @param {string} sessionID
 * @param {boolean} [resetStart] - reset startedAt (true on session.created)
 * @returns {SessionState}
 */
export function markActive(ctx, sessionID, resetStart) {
  const s = ensureState(ctx, sessionID);
  s.active = true;
  if (resetStart || !s.startedAt) {
    s.startedAt = Date.now();
  }
  return s;
}

/**
 * Remember the last validated model for a session (no-op when undefined).
 *
 * @param {{ sessions: Map<string, SessionState> }} ctx
 * @param {string} sessionID
 * @param {(string|undefined)} model
 */
export function markModel(ctx, sessionID, model) {
  if (!model) {
    return;
  }
  const s = ensureState(ctx, sessionID);
  s.model = model;
}

/**
 * Look up the cached model for a session (undefined if none seen).
 *
 * @param {{ sessions: Map<string, SessionState> }} ctx
 * @param {string} sessionID
 * @returns {(string|undefined)}
 */
export function sessionModel(ctx, sessionID) {
  const s = ctx.sessions.get(sessionID);
  return s ? s.model : undefined;
}

/**
 * Consume one active→idle transition. Returns the Stop payload exactly once per
 * active window; returns null when the session is not active (dedupe — R-11).
 *
 * @param {{ sessions: Map<string, SessionState> }} ctx
 * @param {string} sessionID
 * @returns {({ duration: (number|undefined), model: (string|undefined) }|null)}
 */
export function takeStop(ctx, sessionID) {
  const s = ctx.sessions.get(sessionID);
  if (!s || !s.active) {
    return null; // no active window → no Stop (repeated idle is a no-op)
  }
  s.active = false;
  return {
    duration: s.startedAt ? Date.now() - s.startedAt : undefined,
    model: s.model,
  };
}
