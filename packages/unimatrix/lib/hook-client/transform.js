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
 * The header format_injection (hook.rs, single formatting truth — AC-07)
 * unconditionally prepends to EVERY Entries body; an Entries response with no
 * renderable text yields 204 (no body), never a headerless 200. Over the
 * ADR-003 text/plain wire this header is therefore a structural invariant of
 * the Entries variant — the only wire discriminator between Entries and
 * BriefingContent on the SubagentStart path (both arrive as 200 text/plain;
 * BriefingContent is production-reachable via the col-025 goal-present branch
 * and always starts with the CONTEXT_GET_INSTRUCTION constant,
 * index_briefing.rs:41). Dispatching on it mirrors the oracle's enum match in
 * write_stdout_subagent_inject_response (Entries → envelope, other → plain).
 * This is contract-keyed dispatch, not content sniffing: the header is emitted
 * by the same server code the goldens pin, so a header change surfaces as a
 * loud Layer 1 byte diff (ADR-001 drift check), never a silent misroute.
 */
const INJECTION_HEADER = "--- Unimatrix Context ---\n";

/**
 * Render the stdout bytes for a sync response body. Pure.
 *
 * @param {string} reqSource - request source event; "SubagentStart" selects
 *   the injection envelope for Entries-shaped bodies (INJECTION_HEADER
 *   prefix), the plain path otherwise — mirroring the oracle's
 *   write_stdout_subagent_inject_response match.
 * @param {string} text - decoded UTF-8 response body.
 * @returns {Buffer|null} bytes to write, or null meaning "write nothing".
 */
function renderEnvelope(reqSource, text) {
  if (typeof text !== "string" || text.length === 0) {
    return null; // empty body → silent skip (write_stdout parity)
  }
  if (reqSource === "SubagentStart" && text.startsWith(INJECTION_HEADER)) {
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
  // Plain path: UserPromptSubmit / PreCompact bodies, and SubagentStart
  // non-Entries bodies (BriefingContent — the oracle's write_stdout fallthrough,
  // hook.rs:1024-1027): body verbatim + ONE newline (println! parity). The
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

module.exports = { renderEnvelope, writeSyncOutput, INJECTION_HEADER };
