// @ts-check
// Shared test helpers: a spy for the Bun `$` shell boundary + a ctx/hooks factory.
// Component tests drive the plugin's `Hooks` handlers with OpenCode-shaped inputs
// and assert on the constructed HookInput frame and the shell invocation.

import { createContext } from "../lib/context.js";
import { makeHooks } from "../lib/events.js";

/**
 * A fake Bun `$` tagged-template. Records every invocation; behavior controls the
 * returned/thrown result for fail-open testing.
 *
 * @param {("resolve"|"reject"|"throw")} [behavior]
 */
export function shellSpy(behavior) {
  const calls = [];
  /** @type {any} */
  const $ = (strings, ...subs) => {
    calls.push({ strings: Array.from(strings), subs });
    if (behavior === "reject") {
      return Promise.reject(new Error("simulated non-zero exit"));
    }
    if (behavior === "throw") {
      throw new Error("simulated ENOENT");
    }
    return Promise.resolve();
  };
  $.calls = calls;
  return $;
}

/**
 * Build a plugin context + hooks with a spy shell.
 * @param {{ shell?: any, directory?: string, worktree?: string, options?: any }} [cfg]
 */
export function makeHarness(cfg) {
  const c = cfg || {};
  const $ = c.shell || shellSpy("resolve");
  const warnings = [];
  const options = Object.assign(
    { log: { warn: (m) => warnings.push(m) } },
    c.options || {},
  );
  const input = {
    $,
    directory: c.directory !== undefined ? c.directory : "/proj",
    worktree: c.worktree !== undefined ? c.worktree : "/proj/wt",
    client: {},
  };
  const ctx = createContext(input, options);
  const hooks = makeHooks(ctx);
  return { ctx, hooks, calls: $.calls, warnings };
}

/**
 * Decode a recorded emit call into { argv, frame }.
 * @param {{ subs: any[] }} call
 */
export function decode(call) {
  const argv = call.subs[0];
  const body = call.subs[1];
  const raw = Buffer.isBuffer(body) ? body.toString("utf8") : String(body);
  return { argv, frame: JSON.parse(raw) };
}

/** The single most-recent emit, decoded. */
export function lastEmit(calls) {
  return decode(calls[calls.length - 1]);
}
