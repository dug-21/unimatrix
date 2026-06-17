"use strict";

// Shared Layer 2 fixture helpers (vnc-026). Cumulative test infra: the
// parity-layer2*.test.js suites all build transcript content and drive the real
// client delta path against the merged-F2 server through these helpers, so the
// per-session byte-tagging convention (ass-069 PoC) and the JSONL exchange shape
// live in ONE place.

const fs = require("fs");
const delta = require("../../lib/hook-client/delta");

// One JSONL exchange (user line + assistant line). `tag` makes the content
// recognizable in the PreCompact restoration block (per-session byte tagging).
function exchange(tag, n) {
  const u =
    JSON.stringify({
      type: "user",
      message: { content: [{ type: "text", text: tag + " user msg " + n }] },
    }) + "\n";
  const a =
    JSON.stringify({
      type: "assistant",
      message: { content: [{ type: "text", text: tag + " assistant reply " + n }] },
    }) + "\n";
  return u + a;
}

// A live ResolvedConfig pointing delta.maybeSendDelta at the real server.
// vnc-038: the cloud listener is HTTPS-only (self-provisioned self-signed cert);
// `server.url` is the per-slug https observe URL and `server.pinnedFp` pins the
// served leaf so the shipped transport completes the handshake and trusts by
// fingerprint (the OSS trust model — cert-pin.js), never CA chain.
function liveConfig(server, stateDir) {
  return {
    url: server.url,
    token: server.token,
    pinnedFp: server.pinnedFp,
    timeouts: { connectMs: 2000, syncMs: 4000, fnfMs: 4000 },
    stateDir,
  };
}

// A ResolvedConfig pointing at a dead port → connect failure (injected drop).
function deadConfig(server, stateDir) {
  return {
    url: "http://127.0.0.1:1",
    token: server.token,
    timeouts: { connectMs: 150, syncMs: 150, fnfMs: 150 },
    stateDir,
  };
}

// Run ONE delta "spawn": stat + ship [last_offset, file_len) against `config`,
// exactly as index.js does on a FNF event. Returns the DeltaOutcome.
function spawnDelta(transcriptPath, sessionId, config) {
  return delta.maybeSendDelta(transcriptPath, sessionId, "claude-code", config);
}

// Register a session over the wire (raw session_id; server mints http-).
async function register(server, sessionId) {
  const r = await server.post({
    type: "SessionRegister",
    session_id: sessionId,
    cwd: "/x",
    agent_role: null,
    feature: null,
  });
  if (r.status !== 204) {
    throw new Error("SessionRegister must Ack 204, got " + r.status);
  }
}

function appendTranscript(transcriptPath, data) {
  fs.appendFileSync(transcriptPath, data);
}

module.exports = {
  exchange,
  liveConfig,
  deadConfig,
  spawnDelta,
  register,
  appendTranscript,
};
