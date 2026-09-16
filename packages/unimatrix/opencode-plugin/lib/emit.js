// @ts-check
/**
 * emit.js — the single shell-out boundary (fail-open, R-13/NFR-02).
 *
 * Builds `unimatrix hook <EVENT> --provider opencode [--model <id>]` and pipes
 * the Claude-shaped HookInput JSON on stdin, over the existing UDS transport
 * (the Rust `unimatrix hook` binary — the parity oracle). EVERYTHING is wrapped:
 * a missing binary, a down socket, a shell error, or a rejected model MUST NOT
 * throw into or block the OpenCode session — the event is dropped and the
 * session continues.
 *
 * R-15: untrusted values are passed as DISCRETE args (Bun escapes each array
 * element) and as stdin bytes — never string-built into the command line. The
 * model value is charset-validated before it is added.
 */

import { isValidModelId } from "./model.js";

/**
 * @param {{ sh: any, log: { warn: (m: string) => void } }} ctx
 * @param {string} event - canonical event name
 * @param {Record<string, unknown>} frame - flat HookInput frame
 * @param {(string|undefined)} model - validated "providerID/modelID" or undefined
 * @returns {Promise<void>}
 */
export async function emit(ctx, event, frame, model) {
  try {
    if (!ctx || typeof ctx.sh !== "function") {
      return; // no shell available → observation off, session unaffected
    }
    if (!frame || typeof event !== "string" || !event) {
      return; // never emit a corrupt/empty frame (R-14)
    }

    const argv = ["hook", event, "--provider", "opencode"];
    if (model && isValidModelId(model)) {
      argv.push("--model", model);
    }

    // stdin bytes carry the HookInput JSON. Bun Shell redirects stdin from a
    // Buffer via `< ${buf}`; the argv array is escaped + space-joined by Bun.
    const body = Buffer.from(JSON.stringify(frame));
    await ctx.sh`unimatrix ${argv} < ${body}`;
  } catch (_err) {
    // Fail-open: drop the event, keep the session alive. Static, content-free
    // message only — no session id / path / model / frame body.
    try {
      ctx.log.warn("[unimatrix] observe emit failed; event dropped, session continues");
    } catch (_logErr) {
      /* logging must never throw */
    }
  }
}
