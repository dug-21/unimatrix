"use strict";

// ─────────────────────────────────────────────────────────────────────────
// AC-10 / FR-26 — Layer 2 concurrency: ≥8 interleaved sessions with injected
// drops → each merged-F2 server buffer holds ONLY its own session's bytes
// (per-session byte tagging, ass-069 PoC). Raw session_id on the wire; the
// server mints http-{session_id} (no double prefix). Cross-contamination would
// show another session's tag in a session's PreCompact restoration block.
//
// Observability surface: same as parity-layer2.test.js — the wire PreCompact
// block (contiguous_tail → text/plain) is the only content-bearing query
// surface (C-07: no server-side production changes).

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const { startRealServer } = require("../helpers/real-server");
const delta = require("../../lib/hook-client/delta");
const { exchange } = require("../helpers/layer2-fixtures");

let server;
before(async () => {
  server = await startRealServer({ startTimeoutMs: 60000 });
});
after(async () => {
  if (server) await server.close();
});

describe("Layer 2 — AC-10 concurrency: ≥8 interleaved sessions, byte isolation", () => {
  it("test_l2_concurrency_attribution", async () => {
    const N = 8;
    const sessions = [];
    for (let i = 0; i < N; i += 1) {
      const sid = "l2-conc-" + i;
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-l2-conc-" + i + "-"));
      sessions.push({
        sid,
        tag: "SESSION-" + i + "-TAG",
        dir,
        tpath: path.join(dir, "transcript.jsonl"),
        sdir: path.join(dir, "hook-client"),
      });
    }

    try {
      // Register all sessions (raw ids on the wire).
      await Promise.all(
        sessions.map((s) =>
          server.post({
            type: "SessionRegister",
            session_id: s.sid,
            cwd: "/x",
            agent_role: null,
            feature: null,
          })
        )
      );

      const cfgFor = (s) => ({
        url: server.url,
        token: server.token,
        // vnc-038: the cloud listener is HTTPS-only; pin the self-signed leaf so
        // the shipped transport completes the handshake and trusts by fingerprint.
        pinnedFp: server.pinnedFp,
        timeouts: { connectMs: 2000, syncMs: 4000, fnfMs: 4000 },
        stateDir: s.sdir,
      });
      const deadCfgFor = (s) => ({
        url: "http://127.0.0.1:1",
        token: server.token,
        timeouts: { connectMs: 150, syncMs: 150, fnfMs: 150 },
        stateDir: s.sdir,
      });

      // Interleave: each round, every session grows by its own tagged exchange
      // and ships a delta. Inject a drop on a rotating session each round so the
      // re-derive path is exercised under concurrency.
      for (let round = 0; round < 4; round += 1) {
        // Grow all sessions for this round first.
        for (const s of sessions) {
          fs.appendFileSync(s.tpath, exchange(s.tag, round));
        }
        // Then ship all deltas concurrently (interleaved on the wire).
        await Promise.all(
          sessions.map((s, i) => {
            const drop = i === round % N; // rotating injected drop
            return delta.maybeSendDelta(
              s.tpath,
              s.sid,
              "claude-code",
              drop ? deadCfgFor(s) : cfgFor(s)
            );
          })
        );
      }

      // Final reconciliation pass so every session's full content has landed
      // (dropped rounds re-derive on the next successful spawn).
      await Promise.all(
        sessions.map((s) => delta.maybeSendDelta(s.tpath, s.sid, "claude-code", cfgFor(s)))
      );

      // Assert byte isolation via PreCompact: each session's block contains ONLY
      // its own tag and NONE of the other sessions' tags.
      for (const s of sessions) {
        const pc = await server.precompact(s.sid, { token_limit: 4000 });
        assert.strictEqual(pc.status, 200, s.sid + " PreCompact served");
        assert.ok(!pc.raw.includes(0), s.sid + ": no NUL bytes served");
        assert.ok(pc.text.includes(s.tag), s.sid + ": own tag present in its buffer");
        for (const other of sessions) {
          if (other.sid === s.sid) continue;
          assert.ok(
            !pc.text.includes(other.tag),
            s.sid + " buffer leaked " + other.tag + " (cross-session contamination)"
          );
        }
      }
    } finally {
      for (const s of sessions) {
        try {
          fs.rmSync(s.dir, { recursive: true, force: true });
        } catch (_e) {
          /* best-effort */
        }
      }
    }
  });

  it("test_l2_raw_session_id_on_wire_server_mints_http_prefix", async () => {
    // The client sends the RAW session_id; the server keys the buffer under
    // http-{session_id}. Proof: register + delta + PreCompact all use the raw id
    // and reconcile to the SAME buffer (the server-side prefix is applied
    // uniformly). A double-prefix on the client would land the delta under
    // http-http-… and PreCompact (raw id → http-id) would find an empty buffer.
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-l2-wire-"));
    const sid = "l2-wire-raw";
    const tpath = path.join(dir, "transcript.jsonl");
    const sdir = path.join(dir, "hook-client");
    try {
      await server.post({
        type: "SessionRegister",
        session_id: sid,
        cwd: "/x",
        agent_role: null,
        feature: null,
      });
      const tag = sid + "-WIRE-TAG";
      fs.writeFileSync(
        tpath,
        JSON.stringify({
          type: "user",
          message: { content: [{ type: "text", text: tag + " q" }] },
        }) +
          "\n" +
          JSON.stringify({
            type: "assistant",
            message: { content: [{ type: "text", text: tag + " a" }] },
          }) +
          "\n"
      );
      const out = await delta.maybeSendDelta(tpath, sid, "claude-code", {
        url: server.url,
        token: server.token,
        pinnedFp: server.pinnedFp, // vnc-038 HTTPS cloud listener — pin self-signed leaf
        timeouts: { connectMs: 2000, syncMs: 4000, fnfMs: 4000 },
        stateDir: sdir,
      });
      assert.ok(out.attempted && out.send.ok, "raw-id delta Acked");
      const pc = await server.precompact(sid, { token_limit: 4000 });
      assert.strictEqual(pc.status, 200);
      assert.ok(
        pc.text.includes(tag),
        "raw session_id reconciles delta and PreCompact to the same server buffer"
      );
    } finally {
      try {
        fs.rmSync(dir, { recursive: true, force: true });
      } catch (_e) {
        /* best-effort */
      }
    }
  });
});
