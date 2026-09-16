// @ts-check
/**
 * context.js — per-plugin-instance context, derived once from PluginInput.
 *
 * Holds the Bun shell (emit boundary), the derived cwd (SessionStart/Stop are
 * bus-derived and carry no cwd — R-14), the SDK client, per-session state (Stop
 * over-count guard + model cache), the PreCompact gate (OQ-7), and a content-free
 * logger. No secrets, session ids, or paths are ever logged (NFR-02 contract).
 */

/**
 * Resolve the PreCompact experimental leg gate (ADR-009 / OQ-7).
 *
 * DELIVERY DECISION (OQ-7): default ENABLED. `experimental.session.compacting`
 * is registered by default because (a) if OpenCode renames/removes the
 * experimental API our handler simply never fires — an inert registration, no
 * crash — and (b) the handler body is fully try/catch-guarded so an unexpected
 * payload cannot break the session. The flag exists purely as an operator
 * kill-switch. Disable via plugin option `disablePreCompact: true` or env
 * `UNIMATRIX_OPENCODE_PRECOMPACT=0|false`. Documented degraded/experimental.
 *
 * @param {Record<string, unknown>|undefined} options
 * @returns {boolean}
 */
function resolvePreCompactEnabled(options) {
  if (options && options.disablePreCompact === true) {
    return false;
  }
  const env =
    typeof process !== "undefined" && process.env
      ? process.env.UNIMATRIX_OPENCODE_PRECOMPACT
      : undefined;
  if (env === "0" || env === "false") {
    return false;
  }
  return true;
}

/**
 * Content-free logger. Static messages only — never interpolate a session id,
 * path, frame body, or error object (which may embed a command line or path).
 * Writes to the plugin's own log channel (stderr via console.warn), never to the
 * OpenCode stdout injection channel.
 *
 * @param {Record<string, unknown>|undefined} options
 * @returns {{ warn: (msg: string) => void }}
 */
function makeLogger(options) {
  if (options && options.log && typeof (/** @type {any} */ (options.log).warn) === "function") {
    return /** @type {any} */ (options.log);
  }
  return {
    warn(msg) {
      try {
        // eslint-disable-next-line no-console
        console.warn(msg);
      } catch (_err) {
        /* logging must never throw */
      }
    },
  };
}

/**
 * Build the plugin context from OpenCode's PluginInput.
 *
 * @param {import("@opencode-ai/plugin").PluginInput} input
 * @param {Record<string, unknown>} [options]
 * @returns {{
 *   sh: any,
 *   cwd: (string|undefined),
 *   client: any,
 *   sessions: Map<string, { active: boolean, startedAt: number, model: (string|undefined) }>,
 *   preCompactEnabled: boolean,
 *   log: { warn: (msg: string) => void },
 * }}
 */
export function createContext(input, options) {
  const inp = input || /** @type {any} */ ({});
  const cwd =
    typeof inp.worktree === "string" && inp.worktree
      ? inp.worktree
      : typeof inp.directory === "string" && inp.directory
        ? inp.directory
        : undefined;
  return {
    sh: inp.$,
    cwd,
    client: inp.client,
    sessions: new Map(),
    preCompactEnabled: resolvePreCompactEnabled(options),
    log: makeLogger(options),
  };
}
