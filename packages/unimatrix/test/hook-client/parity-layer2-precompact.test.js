"use strict";

// ─────────────────────────────────────────────────────────────────────────
// AC-05 Layer 1 (the "two-layer parity" Layer-1 half) — PreCompact stdout is
// BYTE-IDENTICAL to the server's text/plain restoration body, with deterministic
// buffer pre-population isolated behind the ONE SR-11 helper
// (real-server.prepopulateBuffer).
//
// Per OVERVIEW pseudocode deviation #1: PreCompact does NOT read the transcript
// client-side — the merged F2 server restores it server-side and the client
// prints the text/plain body VERBATIM (+ exactly one newline; transform.js
// renderEnvelope plain path). So the Layer-1 byte-identity check is:
//   spawn(real client, PreCompact stdin) stdout  ==  server PreCompact body + "\n"
// against an identically pre-populated F2 buffer (SR-11). This is the spawn-level
// counterpart to the in-process Layer 2 buffer assertions in
// parity-layer2.test.js.
//
// Spawn gotchas honoured (pattern #4774): async spawn (never spawnSync);
// HOME=tmpRoot so client state lands in the temp tree; cwd at a .git root so
// config resolution is deterministic; raw session_id on the wire.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const { startRealServer } = require("../helpers/real-server");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");

let server;
before(async () => {
  server = await startRealServer({ startTimeoutMs: 60000 });
});
after(async () => {
  if (server) await server.close();
});

let tmpRoot;
function freshProject() {
  tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-l2pc-"));
  fs.mkdirSync(path.join(tmpRoot, ".git"), { recursive: true });
  return tmpRoot;
}
function rmTmp() {
  if (tmpRoot) {
    try {
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    } catch (_e) {
      /* best-effort */
    }
    tmpRoot = null;
  }
}

// Spawn the real client entry (async — never spawnSync; pattern #4774). Remote
// config via env vars pointing at the live server (config.js: env wins outright).
function runEntry(event, stdin) {
  const env = Object.assign({}, process.env, {
    HOME: tmpRoot,
    USERPROFILE: tmpRoot,
    UNIMATRIX_REMOTE_URL: server.url,
    UNIMATRIX_REMOTE_TOKEN: server.token,
    // vnc-038: the cloud listener is HTTPS-only with a self-signed cert. The
    // env-pair remote config carries no cert pin, so let the spawned client's
    // Node TLS accept the self-signed leaf (test-only; the harness server is a
    // localhost self-call). No client-code change — purely a child-process env.
    NODE_TLS_REJECT_UNAUTHORIZED: "0",
  });
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [ENTRY, event], { cwd: tmpRoot, env });
    const out = [];
    const errc = [];
    child.stdout.on("data", (c) => out.push(c));
    child.stderr.on("data", (c) => errc.push(c));
    child.on("error", reject);
    child.on("close", (code) => {
      resolve({ status: code, stdout: Buffer.concat(out), stderr: Buffer.concat(errc) });
    });
    child.stdin.on("error", () => {});
    child.stdin.end(Buffer.from(stdin === undefined ? "" : stdin, "utf8"));
  });
}

function exchange(tag, n) {
  return (
    JSON.stringify({
      type: "user",
      message: { content: [{ type: "text", text: tag + " user " + n }] },
    }) +
    "\n" +
    JSON.stringify({
      type: "assistant",
      message: { content: [{ type: "text", text: tag + " assistant " + n }] },
    }) +
    "\n"
  );
}

describe("AC-05 Layer 1 — PreCompact stdout byte-identity vs server body (SR-11)", () => {
  it("test_precompact_stdout_byte_identical_to_server_body", async () => {
    freshProject();
    const sid = "l2pc-basic";

    // Deterministic buffer pre-population — the ONE SR-11 helper.
    let bytes = "";
    for (let i = 0; i < 4; i += 1) bytes += exchange("PC", i);
    await server.prepopulateBuffer(sid, bytes);

    // Oracle: the server's text/plain PreCompact body for this session.
    const pc = await server.precompact(sid, { token_limit: 4000 });
    assert.strictEqual(pc.status, 200);
    assert.strictEqual(pc.contentType, "text/plain");
    assert.ok(pc.text.length > 0, "server produced a non-empty restoration body");

    // Spawn the real client with a PreCompact event → it prints the body verbatim
    // + one newline (transform plain path). Byte-identity is the AC-05 L1 check.
    const stdin = JSON.stringify({
      hook_event_name: "PreCompact",
      session_id: sid,
      cwd: tmpRoot,
    });
    const res = await runEntry("PreCompact", stdin);
    assert.strictEqual(res.status, 0, "client always exits 0 (C-05)");

    const expected = Buffer.from(pc.text + "\n", "utf8");
    assert.ok(
      res.stdout.equals(expected),
      "client PreCompact stdout must be byte-identical to server body + newline\n" +
        "  client : " +
        JSON.stringify(res.stdout.toString("utf8")) +
        "\n  server : " +
        JSON.stringify(expected.toString("utf8"))
    );
    // No NUL bytes escape onto the host context (R-06).
    assert.ok(!res.stdout.includes(0), "no NUL bytes on client stdout");
    rmTmp();
  });

  it("test_precompact_empty_buffer_no_stdout", async () => {
    // A registered session with NO transcript bytes: the server returns an empty
    // briefing body (no entries cached + no tail) → 204 / empty → client writes
    // NOTHING (transform: empty body → silent skip). Byte-identity at the empty
    // boundary (C-05: zero stdout on the no-content path).
    freshProject();
    const sid = "l2pc-empty";
    await server.post({
      type: "SessionRegister",
      session_id: sid,
      cwd: "/x",
      agent_role: null,
      feature: null,
    });

    const stdin = JSON.stringify({
      hook_event_name: "PreCompact",
      session_id: sid,
      cwd: tmpRoot,
    });
    const res = await runEntry("PreCompact", stdin);
    assert.strictEqual(res.status, 0);
    // Empty/no-content restoration → no stdout (the only safe host-context byte
    // count when there is nothing to restore).
    assert.strictEqual(res.stdout.length, 0, "no stdout when there is nothing to restore");
    rmTmp();
  });
});
