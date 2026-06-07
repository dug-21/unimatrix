"use strict";

/**
 * transform.js — host-envelope stdout (vnc-026, ADR-002).
 *
 * The ONLY module that writes stdout. Envelopes are emitted from LITERAL template
 * strings; the sole serializer call is on the inner text scalar (string escaping
 * only) — no code path serializes a whole envelope object. The committed parity
 * goldens (expected-stdout.bin, ADR-001) are the byte authority for AC-04.
 *
 * Oracle: hook.rs:963-1028 (write_stdout, write_stdout_subagent_inject). No I/O
 * besides stdout, no network, no state.
 */

/**
 * Render the stdout bytes for a sync response body. Pure.
 *
 * @param {string} reqSource - request source event; "SubagentStart" selects
 *   the injection envelope, anything else the plain path.
 * @param {string} text - decoded UTF-8 response body.
 * @returns {Buffer|null} bytes to write, or null meaning "write nothing".
 */
function renderEnvelope(reqSource, text) {
  if (typeof text !== "string" || text.length === 0) {
    return null; // empty body → silent skip (write_stdout parity)
  }
  if (reqSource === "SubagentStart") {
    // Byte-pinned to hook.rs write_stdout_subagent_inject: compact separators,
    // hard-coded serde preserve_order key order, trailing newline. The
    // serializer below touches ONLY the text scalar.
    return Buffer.from(
      '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":' +
        JSON.stringify(text) +
        "}}\n",
      "utf8"
    );
  }
  // Plain path (UserPromptSubmit / PreCompact): body verbatim + ONE newline. The
  // server already formatted/budgeted the body (F1 format_injection) — no
  // client-side budget.
  return Buffer.from(text + "\n", "utf8");
}

/**
 * Write the sync-response stdout for a spawn. At most ONE stdout write. Silent on
 * failed sends, non-200 status (incl. 204), null/empty body, and any 200 whose
 * Content-Type is not text/plain (R-15).
 *
 * @param {string} reqSource - request source event (see renderEnvelope).
 * @param {{ok: boolean, status: number, contentType: (string|null),
 *          body: (Buffer|null)}} res - transport SendResult.
 */
function writeSyncOutput(reqSource, res) {
  if (!res || res.ok !== true || res.status !== 200) return;
  if (!res.body || res.body.length === 0) return;
  const ct = (res.contentType || "").toLowerCase();
  if (!ct.startsWith("text/plain")) return;
  const out = renderEnvelope(reqSource, res.body.toString("utf8"));
  if (out !== null) process.stdout.write(out);
}

module.exports = { renderEnvelope, writeSyncOutput };
