// @ts-check
/**
 * frame.js — Claude-shaped HookInput frame builder.
 *
 * The Rust `HookInput` (unimatrix-engine/src/wire.rs) declares named fields
 * (hook_event_name, session_id, cwd, transcript_path, prompt, mcp_context) plus
 * a `#[serde(flatten)] extra: serde_json::Value` catch-all. So a Claude-shaped
 * frame is FLAT: tool_name / tool_input / tool_response / agent_type / exit_code
 * (and our additive parent_id / bus_derived / duration / outcome markers) ride at
 * the TOP LEVEL and land under `extra` on the Rust side. This mirrors the flat
 * shape Claude Code pipes on stdin — verified against hook.rs `input.extra.get(...)`.
 *
 * RECONCILIATION (vs C1 pseudocode): the pseudocode's `buildHookInput` nested an
 * `extra: {}` object; that would deserialize to `extra.extra` under the flatten
 * contract. The wire truth is flat, so this builder spreads the `extra` bag onto
 * the frame's top level.
 *
 * NOTE: `provider` and `model_id` are NOT frame fields — hook.rs populates them
 * from the `--provider` / `--model` CLI args, never from stdin JSON (wire.rs).
 * They are carried by emit.js on the command line.
 *
 * R-14 honesty: absent fields (no transcript_path on bus-derived events, empty
 * prompt) are OMITTED, never emitted as bogus empty values.
 */

/**
 * @param {string} event - canonical event name (SessionStart, UserPromptSubmit, ...)
 * @param {{
 *   session_id?: (string|null|undefined),
 *   cwd?: (string|null|undefined),
 *   transcript_path?: (string|null|undefined),
 *   prompt?: (string|null|undefined),
 *   extra?: (Record<string, unknown>|undefined),
 * }} fields
 * @param {(string|undefined)} defaultCwd - fallback cwd (from PluginInput) for bus events
 * @returns {Record<string, unknown>}
 */
export function buildHookInput(event, fields, defaultCwd) {
  const f = fields || {};
  /** @type {Record<string, unknown>} */
  const frame = { hook_event_name: event };

  if (typeof f.session_id === "string" && f.session_id) {
    frame.session_id = f.session_id;
  }

  const cwd = f.cwd != null && f.cwd !== "" ? f.cwd : defaultCwd;
  if (typeof cwd === "string" && cwd) {
    frame.cwd = cwd;
  }

  // transcript_path: omitted for bus-derived events (never stored as bogus, R-14).
  if (typeof f.transcript_path === "string" && f.transcript_path) {
    frame.transcript_path = f.transcript_path;
  }

  if (typeof f.prompt === "string" && f.prompt) {
    frame.prompt = f.prompt;
  }

  // Flatten the extra bag onto the top level (serde flatten contract). Undefined
  // values are dropped so the frame never claims a field it does not have.
  const extra = f.extra;
  if (extra && typeof extra === "object") {
    for (const key of Object.keys(extra)) {
      const value = extra[key];
      if (value !== undefined) {
        frame[key] = value;
      }
    }
  }

  return frame;
}
