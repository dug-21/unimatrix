// @ts-check
/**
 * unimatrix.js — OpenCode observation plugin entry (vnc-049 C18, ADR-004).
 *
 * OpenCode has no stdin command-hook contract; its only extension surface is an
 * in-process TypeScript/JS plugin. This module IS the normalization boundary: it
 * subscribes to OpenCode typed hooks + the event bus, maps each of the 7 canonical
 * events to a Claude-shaped `HookInput` frame, resolves the backend model and
 * subagent alignment, and shells to
 *   `unimatrix hook <EVENT> --provider opencode --model <providerID>/<modelID>`
 * over the existing (proven) UDS transport — reusing the whole Rust pipeline
 * unchanged (NFR-02 fail-open).
 *
 * PACKAGE IDENTITY (C9 coordination — MANDATORY): C9's installer provisions this
 * plugin by the constant PLUGIN_PACKAGE="@dug-21/unimatrix-opencode-plugin",
 * export UnimatrixObservePlugin, shim file unimatrix.js. Those three MUST match
 * `packages/unimatrix/lib/opencode-install.js` exactly or the installer cannot
 * provision it. See that module's PLUGIN_PACKAGE/PLUGIN_EXPORT/PLUGIN_ENTRY_FILE.
 *
 * Plugin signature (ADR-004): `Plugin(input, options?) => Promise<Hooks>`.
 */

import { createContext } from "./lib/context.js";
import { makeHooks } from "./lib/events.js";

/**
 * OpenCode plugin factory. Fail-open: initialization never throws into the host;
 * any setup failure degrades to an inert (no-op) hook set so the OpenCode session
 * is never blocked or crashed (R-13/NFR-02).
 *
 * @param {import("@opencode-ai/plugin").PluginInput} input
 * @param {Record<string, unknown>} [options]
 * @returns {Promise<import("@opencode-ai/plugin").Hooks>}
 */
export const UnimatrixObservePlugin = async (input, options) => {
  try {
    const ctx = createContext(input, options);
    return makeHooks(ctx);
  } catch (_err) {
    // Never throw during plugin load — return an empty hook set (observation off,
    // session unaffected).
    return {};
  }
};

export default UnimatrixObservePlugin;
