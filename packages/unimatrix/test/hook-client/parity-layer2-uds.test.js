"use strict";

// ─────────────────────────────────────────────────────────────────────────
// Layer 2 — LIVE UDS hook listener integration (vnc-027, parity-corpus-uds
// layers b/c + cross-transport replay). The integration backbone deferred from
// Stage 3b: everything that needs the cargo-built daemon's real Unix socket.
//
// Drives the SHIPPED transport-uds module and the real client entry against the
// daemon started by the extended real-server.js helper (which now polls the hook
// socket {dataDir}/unimatrix.sock into existence and exposes udsPost/udsConnectRaw
// — cumulative infra, never a parallel framer). Requires a cargo-built binary;
// real-server.js hard-fails (never skips, #4452) if absent.
//
// Coverage:
//   AC-03  live round-trip: identical HookRequest frames decode; sync trio I/O.
//   AC-04 / R-10  cross-transport replay BOTH directions; session-id split pinned.
//   AC-06 / R-17  PreCompact single server-built block over UDS (no client prepend).
//   AC-07  transcript_delta over UDS merges into the F2 buffer (CONTENT asserted).
//   AC-08 / R-12  no-SubagentStop full lifecycle: buffers finalize without it.
//   AC-11 / R-08 s4  compiled FROZEN Rust hook end-to-end vs TS client, same
//                    daemon → byte-identical stdout (THE deployed-hook safety proof).
//   AC-05 / R-15  p95 latency over UDS < 20 ms (FNF vs sync measured separately);
//                 40 ms timeout constant asserted.
//   R-01 s1/s2 / R-18  FNF large-frame recorded complete; truncated frame NEVER
//                      silently recorded (server-side truncation contract).
//
// Spawn discipline (#4774): async spawn only, never spawnSync; HOME under the temp
// tree; stdin.cwd at a .git root so config resolution is deterministic; raw
// session ids on the UDS wire (the daemon mints http- only on the HTTP path).

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const { startRealServer, resolveServerBinary } = require("../helpers/real-server");
const transportUds = require("../../lib/hook-client/transport-uds");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");
const INJECTION_HEADER = "--- Unimatrix Context ---\n";

let server;
before(async () => {
  server = await startRealServer({ startTimeoutMs: 60000 });
});
after(async () => {
  if (server) await server.close();
});

// ── helpers ──────────────────────────────────────────────────────────────

/** One user/assistant transcript exchange (matches the Layer-2 fixtures). */
function exchange(tag, n) {
  return (
    JSON.stringify({ type: "user", message: { content: [{ type: "text", text: tag + " user " + n }] } }) +
    "\n" +
    JSON.stringify({ type: "assistant", message: { content: [{ type: "text", text: tag + " assistant " + n }] } }) +
    "\n"
  );
}

function deltaFrame(sessionId, offset, bytesStr) {
  return {
    type: "RecordEvent",
    event_type: "transcript_delta",
    session_id: sessionId,
    timestamp: Math.floor(Date.now() / 1000),
    payload: { offset, bytes: bytesStr },
  };
}

function compactFrame(sessionId) {
  return {
    type: "CompactPayload",
    session_id: sessionId,
    injected_entry_ids: [],
    role: null,
    feature: null,
    token_limit: 4000,
  };
}

/** Register a session and stream `bytes` as one transcript_delta over UDS (FNF). */
async function udsPrepopulate(sessionId, bytes) {
  const reg = await server.udsPost(
    { type: "SessionRegister", session_id: sessionId, cwd: "/x", agent_role: null, feature: null },
    { sync: false }
  );
  assert.ok(reg.ok, "UDS SessionRegister accepted");
  const d = await server.udsPost(deltaFrame(sessionId, 0, bytes), { sync: false });
  assert.ok(d.ok, "UDS transcript_delta accepted (FNF status 0)");
  assert.strictEqual(d.status, 0, "FNF success is status 0 (ADR-002 §2)");
  // Give the daemon a beat to apply the buffered write before a sync read.
  await sleep(150);
}

/** Read a session's PreCompact restoration block over UDS (raw id). */
async function udsCompactBody(sessionId) {
  const res = await server.udsPost(compactFrame(sessionId), { sync: true });
  return { res, text: res.body ? res.body.toString("utf8") : "" };
}

/** Spawn the real client entry. extraEnv overrides; remote vars cleared unless set. */
function spawnClient(event, stdin, extraEnv) {
  const env = Object.assign({}, process.env, { HOME: server.home, USERPROFILE: server.home });
  delete env.UNIMATRIX_REMOTE_URL;
  delete env.UNIMATRIX_REMOTE_TOKEN;
  Object.assign(env, extraEnv || {});
  return runProc(process.execPath, [ENTRY, event], stdin, env);
}

/** Spawn the compiled FROZEN Rust hook (no `accept`) against the live daemon. */
function spawnRustHook(event, stdin, extraEnv) {
  const bin = resolveServerBinary();
  const env = Object.assign({}, process.env, { HOME: server.home, USERPROFILE: server.home });
  delete env.UNIMATRIX_REMOTE_URL;
  delete env.UNIMATRIX_REMOTE_TOKEN;
  Object.assign(env, extraEnv || {});
  return runProc(bin, ["--project-dir", server.projectDir, "hook", event], stdin, env);
}

function runProc(bin, args, stdin, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(bin, args, { env });
    const out = [];
    const errc = [];
    child.stdout.on("data", (c) => out.push(c));
    child.stderr.on("data", (c) => errc.push(c));
    child.on("error", reject);
    child.on("close", (code) => resolve({ status: code, stdout: Buffer.concat(out), stderr: Buffer.concat(errc) }));
    child.stdin.on("error", () => {});
    child.stdin.end(Buffer.from(stdin === undefined ? "" : stdin, "utf8"));
  });
}

/** A fresh temp HOME + .git project root (isolated stateDir/socket derivation). */
function freshHomeProject(tag) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "uds-" + tag + "-home-"));
  const proj = fs.mkdtempSync(path.join(os.tmpdir(), "uds-" + tag + "-proj-"));
  fs.mkdirSync(path.join(proj, ".git"), { recursive: true });
  return { home, proj };
}

function rmAll() {
  for (const d of arguments) {
    try {
      fs.rmSync(d, { recursive: true, force: true });
    } catch (_e) {
      /* best-effort */
    }
  }
}

const crypto = require("crypto");
function projectHash(projectRoot) {
  return crypto.createHash("sha256").update(fs.realpathSync(projectRoot), "utf8").digest("hex").slice(0, 16);
}
function stateDirFor(home, proj) {
  return path.join(home, ".unimatrix", projectHash(proj), "hook-client");
}

/** Seed the on-disk queue directly (oldest-first names), bypassing the
 *  enqueue() delta guard so a transcript_delta can be staged for a replay test
 *  (test-only; production never queues deltas). */
function seedQueue(stateDir, frames) {
  const dir = path.join(stateDir, "queue");
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const base = Date.now();
  frames.forEach((frame, i) => {
    const name = String(base + i).padStart(13, "0") + "-00000-" + String(i).padStart(4, "0") + ".json";
    fs.writeFileSync(path.join(dir, name), JSON.stringify(frame), { mode: 0o600 });
  });
  return dir;
}
function queueDepth(stateDir) {
  try {
    return fs.readdirSync(path.join(stateDir, "queue")).filter((n) => n.endsWith(".json")).length;
  } catch (_e) {
    return 0;
  }
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function newWarnErrors(before, after) {
  const a = after.split("\n");
  return a
    .slice(before.split("\n").length)
    .filter((l) => /\bERROR\b|panic|PANIC/.test(l) && !/early eof/.test(l));
}

// ── AC-03: live round-trip framing + sync I/O ──────────────────────────────

describe("AC-03 — live UDS round-trip: frames decode, sync trio behaves", () => {
  it("test_uds_ping_sync_roundtrip_pong", async () => {
    const res = await server.udsPost({ type: "Ping" }, { sync: true });
    assert.ok(res.ok, "Ping round-trips over the live socket");
    assert.strictEqual(res.status, 200);
    const pong = JSON.parse(res.body.toString("utf8"));
    assert.strictEqual(pong.type, "Pong", "daemon decoded the framed Ping and replied Pong");
  });

  it("test_uds_fnf_session_register_status0", async () => {
    const res = await server.udsPost(
      { type: "SessionRegister", session_id: "ac03-reg", cwd: "/x", agent_role: null, feature: null },
      { sync: false }
    );
    assert.ok(res.ok && res.status === 0 && res.failureClass === null, "FNF success → status 0, no failure");
  });

  it("test_uds_context_search_empty_db_no_injection", async () => {
    // Empty DB → no entries → format_injection None → Ack (204-equivalent), no
    // Text. The TS client writes nothing; the frozen Rust hook writes nothing.
    const res = await server.udsPost(
      {
        type: "ContextSearch",
        query: "how should the hook client resolve config",
        session_id: "ac03-cs",
        role: null,
        task: null,
        feature: null,
        k: null,
        max_tokens: null,
      },
      { sync: true }
    );
    assert.ok(res.ok, "ContextSearch round-trips");
    // Ack → 204-equivalent (no text/plain body) on empty injection.
    assert.ok(res.status === 204 || (res.body && res.body.length === 0), "empty injection → no Text body");
  });
});

// ── AC-07: transcript_delta over UDS merges into the F2 buffer ──────────────

describe("AC-07 — transcript_delta over UDS merges into the F2 buffer (content)", () => {
  it("test_transcript_delta_over_uds_merges_into_f2_buffer", async () => {
    const sid = "ac07-merge";
    let bytes = "";
    for (let i = 0; i < 4; i += 1) bytes += exchange("AC07", i);
    await udsPrepopulate(sid, bytes);

    const { res, text } = await udsCompactBody(sid);
    assert.ok(res.ok && res.status === 200 && res.contentType === "text/plain", "UDS CompactPayload → Text body");
    assert.ok(text.includes("AC07 user 3"), "buffer CONTENT (not just acceptance) reflects the UDS-streamed delta");
    assert.ok(text.includes("AC07 assistant 3"), "assistant turn also merged");
    assert.ok(res.body && !res.body.includes(0), "no NUL bytes escape the restoration block (R-06)");
  });
});

// ── AC-06: PreCompact single server-built block over UDS ────────────────────

describe("AC-06 / R-17 — PreCompact single server-built block over UDS", () => {
  it("test_uds_precompact_single_server_built_block", async () => {
    const sid = "ac06-single";
    let bytes = "";
    for (let i = 0; i < 4; i += 1) bytes += exchange("AC06", i);
    await udsPrepopulate(sid, bytes);

    // Oracle: the daemon's restoration body read over UDS (raw id).
    const { text: body } = await udsCompactBody(sid);
    assert.ok(body.length > 0, "daemon built a non-empty restoration block");

    // Client over UDS: prints the server body VERBATIM + one newline; never
    // client-side-prepends (TS deviation). Exactly one block ⇒ byte-identity.
    const stdin = JSON.stringify({ hook_event_name: "PreCompact", session_id: sid, cwd: server.projectDir });
    const res = await spawnClient("PreCompact", stdin);
    assert.strictEqual(res.status, 0, "client always exits 0");
    assert.ok(
      res.stdout.equals(Buffer.from(body + "\n", "utf8")),
      "client PreCompact stdout == server block + newline (single server-built block, no client prepend)"
    );
  });
});

// ── AC-11 / R-08 s4: frozen Rust hook vs TS client, same daemon ─────────────

describe("AC-11 / R-08 s4 — FROZEN Rust hook e2e vs TS client (byte-identical)", () => {
  it("test_frozen_rust_hook_precompact_byte_identical_to_ts_client", async () => {
    // THE deployed-frozen-hook safety proof. The frozen Rust binary sends NO
    // `accept` → the daemon returns typed BriefingContent (never Text), and the
    // hook formats client-side. The TS client sends accept:"text/plain" → the
    // daemon returns a server-built Text body. Against the SAME buffer, both
    // stdouts must be byte-identical — additive wire change cannot crash or
    // diverge a deployed hook (ADR-001 accept↔Text coupling + shared core).
    const sid = "ac11-pc";
    let bytes = "";
    for (let i = 0; i < 5; i += 1) bytes += exchange("AC11", i);
    await udsPrepopulate(sid, bytes);

    const stdin = JSON.stringify({ hook_event_name: "PreCompact", session_id: sid, cwd: server.projectDir });
    const ts = await spawnClient("PreCompact", stdin);
    const rust = await spawnRustHook("PreCompact", stdin);

    assert.strictEqual(ts.status, 0, "TS client exits 0");
    assert.strictEqual(rust.status, 0, "frozen Rust hook exits 0 against the updated daemon");
    assert.ok(rust.stdout.length > 0, "frozen hook produced a restoration block (non-empty proof)");
    assert.ok(
      rust.stdout.equals(ts.stdout),
      "frozen Rust hook stdout must be byte-identical to the TS client over the same daemon\n" +
        "  rust: " + JSON.stringify(rust.stdout.toString("utf8").slice(0, 120)) + "\n" +
        "  ts  : " + JSON.stringify(ts.stdout.toString("utf8").slice(0, 120))
    );
  });

  it("test_frozen_rust_hook_userpromptsubmit_empty_db_parity", async () => {
    // ContextSearch leg: empty DB → both write nothing. Proves the frozen hook
    // (no accept) and the TS client (accept) agree on the empty-injection path
    // and neither crashes on the additive wire surface.
    const stdin = JSON.stringify({ hook_event_name: "UserPromptSubmit", session_id: "ac11-ups", cwd: server.projectDir, prompt: "resolve config precedence" });
    const ts = await spawnClient("UserPromptSubmit", stdin);
    const rust = await spawnRustHook("UserPromptSubmit", stdin);
    assert.strictEqual(ts.status, 0);
    assert.strictEqual(rust.status, 0);
    assert.ok(ts.stdout.equals(rust.stdout), "empty-injection stdout parity (both empty)");
    assert.strictEqual(ts.stdout.length, 0, "no injection on empty DB → no stdout");
  });
});

// ── R-01 / R-18: FNF large frame + truncation contract ─────────────────────

describe("R-01 / R-18 — FNF frame size + truncation contract", () => {
  it("test_frame_cap_boundary_exact_and_over", () => {
    // The wire cap is byte-shared with wire.rs: exactly 1,048,576 B payload
    // encodes; one byte over rejects BEFORE any write (live framing boundary;
    // offline fixtures pin the bytes).
    const exact = transportUds.encodeFrame({ type: "RecordEvent", pad: "x" }, {});
    assert.ok(exact !== null);
    assert.strictEqual(transportUds.MAX_PAYLOAD_SIZE, 1048576, "1 MiB cap matches wire.rs");
    // Build a payload exactly at the cap and one over.
    const overhead = Buffer.byteLength(JSON.stringify({ type: "RecordEvent", pad: "" }), "utf8");
    const atCap = { type: "RecordEvent", pad: "a".repeat(transportUds.MAX_PAYLOAD_SIZE - overhead) };
    const overCap = { type: "RecordEvent", pad: "a".repeat(transportUds.MAX_PAYLOAD_SIZE - overhead + 1) };
    assert.ok(transportUds.encodeFrame(atCap, {}) !== null, "exactly 1 MiB payload encodes");
    assert.strictEqual(transportUds.encodeFrame(overCap, {}), null, ">1 MiB payload rejected before write");
  });

  it("test_fnf_large_frame_recorded_complete", async () => {
    // A large (~900 KiB) FNF transcript_delta is recorded COMPLETE: the tail
    // sentinel survives into the restoration block (no Node write-buffer drop;
    // flush-before-FIN, ADR-003 §6 / R-01 s1).
    const sid = "r01-large";
    // ~700 KiB of transcript — large, but the framed JSON payload stays under the
    // 1 MiB wire cap (encodeFrame would reject anything over).
    const one = exchange("BIG", 0);
    const reps = Math.floor((700 * 1024) / Buffer.byteLength(one, "utf8"));
    const filler = one.repeat(reps);
    const sentinel = "SENTINEL_TAIL_MARKER_" + Date.now();
    // Sentinel embedded in a valid JSONL turn (the restoration block parses turns
    // and drops bare lines), placed last so it lands in the contiguous tail window.
    const tailTurn =
      JSON.stringify({ type: "assistant", message: { content: [{ type: "text", text: "TAIL " + sentinel }] } }) + "\n";
    const bytes = filler + tailTurn;
    const framedLen = Buffer.byteLength(JSON.stringify(deltaFrame(sid, 0, bytes)), "utf8");
    assert.ok(Buffer.byteLength(bytes, "utf8") > 600 * 1024, "payload is large (~>600 KiB)");
    assert.ok(framedLen < transportUds.MAX_PAYLOAD_SIZE, "framed payload stays under the 1 MiB wire cap");

    const reg = await server.udsPost(
      { type: "SessionRegister", session_id: sid, cwd: "/x", agent_role: null, feature: null },
      { sync: false }
    );
    assert.ok(reg.ok);
    const d = await server.udsPost(deltaFrame(sid, 0, bytes), { sync: false });
    assert.ok(d.ok && d.status === 0, "large FNF delta flushed (status 0)");
    await sleep(250);

    const { text } = await udsCompactBody(sid);
    assert.ok(text.includes(sentinel), "tail sentinel of the large frame reached the buffer — recorded complete");
  });

  it("test_fnf_truncated_frame_not_silently_recorded", async () => {
    // Adversarial: declare a length LARGER than the bytes actually written, then
    // destroy mid-write. The daemon must reject the incomplete frame (early eof)
    // and NEVER record a silently-truncated event (R-01 s2 / ADR-003 §6).
    const sid = "r01-trunc";
    const reg = await server.udsPost(
      { type: "SessionRegister", session_id: sid, cwd: "/x", agent_role: null, feature: null },
      { sync: false }
    );
    assert.ok(reg.ok);

    const payload = Buffer.from(JSON.stringify(deltaFrame(sid, 0, "TRUNCATED_POISON".repeat(80))), "utf8");
    const header = Buffer.alloc(4);
    header.writeUInt32BE(payload.length + 4096, 0); // declare MORE than we will send
    const sock = await server.udsConnectRaw();
    sock.on("error", () => {});
    sock.write(Buffer.concat([header, payload.subarray(0, 40)])); // send a sliver of the body
    await sleep(60);
    sock.destroy();
    await sleep(200);

    const { text } = await udsCompactBody(sid);
    assert.ok(!text.includes("TRUNCATED_POISON"), "truncated frame was NOT recorded — no silent truncation");
  });
});

// ── AC-08 / R-12: no-SubagentStop full lifecycle ───────────────────────────

describe("AC-08 / R-12 — full lifecycle with SubagentStop NEVER sent", () => {
  it("test_no_subagentstop_full_lifecycle", async () => {
    const sid = "r12-lifecycle";
    const logBefore = server.serverLog();

    // SessionRegister → deltas → SessionClose, all over UDS; SubagentStop omitted.
    const reg = await server.udsPost(
      { type: "SessionRegister", session_id: sid, cwd: "/x", agent_role: null, feature: null },
      { sync: false }
    );
    assert.ok(reg.ok, "session opens");

    let bytes = "";
    for (let i = 0; i < 3; i += 1) bytes += exchange("LIFE", i);
    const d = await server.udsPost(deltaFrame(sid, 0, bytes), { sync: false });
    assert.ok(d.ok, "deltas merge without any SubagentStop dependency");
    await sleep(150);

    // Buffer finalized & servable before close — no SubagentStop was needed.
    const { text } = await udsCompactBody(sid);
    assert.ok(text.includes("LIFE user 2"), "buffer finalizes/serves with SubagentStop absent");

    const close = await server.udsPost(
      { type: "SessionClose", session_id: sid, outcome: "success", duration_secs: 0 },
      { sync: false }
    );
    assert.ok(close.ok, "SessionClose accepted with SubagentStop never sent (lifecycle independent — R-12)");

    await sleep(100);
    const errs = newWarnErrors(logBefore, server.serverLog());
    assert.deepStrictEqual(errs, [], "no server ERROR/panic across a SubagentStop-less lifecycle\n" + errs.join("\n"));
  });
});

// ── AC-04 / R-10: cross-transport replay, both directions ──────────────────

describe("AC-04 / R-10 — cross-transport replay (both directions), session-id split", () => {
  it("test_replay_uds_origin_frames_over_http", async () => {
    // A queue staged with transport-agnostic frames (their UDS origin is
    // immaterial — frames carry no transport state, no `accept`) replays over
    // HTTP on the next spawn that resolves HTTP config. Ingest accepts; the HTTP
    // path keys the session http-{sid}.
    const { home, proj } = freshHomeProject("httpreplay");
    const sd = stateDirFor(home, proj);
    const sid = "xrep-http";
    let bytes = "";
    for (let i = 0; i < 3; i += 1) bytes += exchange("XHTTP", i);
    seedQueue(sd, [
      { type: "SessionRegister", session_id: sid, cwd: "/x", agent_role: null, feature: null },
      deltaFrame(sid, 0, bytes),
    ]);
    assert.strictEqual(queueDepth(sd), 2, "two frames staged");

    // Spawn over HTTP (env wins) → replay-before-send fires over the HTTP transport.
    const stdin = JSON.stringify({ hook_event_name: "Stop", session_id: "xrep-http-carry", cwd: proj });
    const res = await spawnClient("Stop", stdin, {
      HOME: home,
      USERPROFILE: home,
      UNIMATRIX_REMOTE_URL: server.url,
      UNIMATRIX_REMOTE_TOKEN: server.token,
    });
    assert.strictEqual(res.status, 0);
    assert.strictEqual(queueDepth(sd), 0, "both queued frames replayed and deleted (HTTP ingest accepted)");
    await sleep(150);

    // Session-id split: HTTP ingest keyed http-{sid}; read it back over HTTP.
    const pc = await server.precompact(sid, { token_limit: 4000 });
    assert.strictEqual(pc.status, 200);
    assert.ok(pc.text.includes("XHTTP user 2"), "replayed delta content recorded at the HTTP ingest (http-{sid})");
    rmAll(home, proj);
  });

  it("test_replay_http_origin_frames_over_uds", async () => {
    // The mirror direction: frames staged in the live server's stateDir replay
    // over UDS to the live listener on a spawn that resolves local (no remote
    // config). UDS ingest keys the raw {sid} (the pinned split).
    const sid = "xrep-uds";
    const sd = path.join(server.home, ".unimatrix", projectHash(server.projectDir), "hook-client");
    let bytes = "";
    for (let i = 0; i < 3; i += 1) bytes += exchange("XUDS", i);
    seedQueue(sd, [
      { type: "SessionRegister", session_id: sid, cwd: "/x", agent_role: null, feature: null },
      deltaFrame(sid, 0, bytes),
    ]);
    assert.strictEqual(queueDepth(sd), 2, "two frames staged in the live stateDir");

    const stdin = JSON.stringify({ hook_event_name: "Stop", session_id: "xrep-uds-carry", cwd: server.projectDir });
    const res = await spawnClient("Stop", stdin); // no remote env → UDS transport
    assert.strictEqual(res.status, 0);
    assert.strictEqual(queueDepth(sd), 0, "queued frames replayed and deleted (UDS ingest accepted)");
    await sleep(200);

    // Raw {sid} over UDS shows the content...
    const { text } = await udsCompactBody(sid);
    assert.ok(text.includes("XUDS user 2"), "replayed delta recorded at the UDS ingest (raw {sid})");

    // ...and the SAME id read over HTTP (which mints http-{sid}) does NOT —
    // pinning the documented session-id split so a future change is deliberate.
    const pcHttp = await server.precompact(sid, { token_limit: 4000 });
    assert.ok(!pcHttp.text.includes("XUDS user 2"), "HTTP read of the same id is empty — session-id split pinned");
  });

  it("test_replay_poison_pill_does_not_abort_subsequent", async () => {
    // A malformed queue file is deleted and the remaining frame still replays
    // (poison-pill immunity, R-10 s4) — exercised over the live UDS ingest.
    const sid = "xrep-poison";
    const sd = path.join(server.home, ".unimatrix", projectHash(server.projectDir), "hook-client");
    const dir = path.join(sd, "queue");
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    const base = Date.now();
    fs.writeFileSync(path.join(dir, String(base).padStart(13, "0") + "-00000-0000.json"), "{not valid json", { mode: 0o600 });
    fs.writeFileSync(
      path.join(dir, String(base + 1).padStart(13, "0") + "-00000-0001.json"),
      JSON.stringify({ type: "SessionRegister", session_id: sid, cwd: "/x", agent_role: null, feature: null }),
      { mode: 0o600 }
    );
    assert.strictEqual(queueDepth(sd), 2);

    const stdin = JSON.stringify({ hook_event_name: "Stop", session_id: "xrep-poison-carry", cwd: server.projectDir });
    const res = await spawnClient("Stop", stdin);
    assert.strictEqual(res.status, 0);
    assert.strictEqual(queueDepth(sd), 0, "poison pill deleted AND the valid frame replayed");
  });
});

// ── AC-05 / R-15: latency over UDS ─────────────────────────────────────────

describe("AC-05 / R-15 — latency over UDS (p95 < 20 ms), separate FNF/sync", () => {
  it("test_timeout_constant_is_40ms", () => {
    assert.strictEqual(transportUds.TIMEOUT_MS, 40, "40 ms parity timeout constant (not load-bearing for p95)");
  });

  it("test_uds_fnf_and_sync_p95_under_20ms", async () => {
    const N = 60;
    const WARM = 10;
    function p95(samples) {
      const s = samples.slice().sort((a, b) => a - b);
      return s[Math.min(s.length - 1, Math.floor(s.length * 0.95))];
    }
    // Warm the socket + handler paths first.
    for (let i = 0; i < WARM; i += 1) {
      await server.udsPost({ type: "Ping" }, { sync: true });
      await server.udsPost({ type: "SessionRegister", session_id: "warm-" + i, cwd: "/x", agent_role: null, feature: null }, { sync: false });
    }

    const sync = [];
    const fnf = [];
    for (let i = 0; i < N; i += 1) {
      let t = process.hrtime.bigint();
      await server.udsPost({ type: "Ping" }, { sync: true });
      sync.push(Number(process.hrtime.bigint() - t) / 1e6);

      t = process.hrtime.bigint();
      await server.udsPost({ type: "SessionRegister", session_id: "lat-" + i, cwd: "/x", agent_role: null, feature: null }, { sync: false });
      fnf.push(Number(process.hrtime.bigint() - t) / 1e6);
    }
    const sp = p95(sync);
    const fp = p95(fnf);
    // eslint-disable-next-line no-console
    console.error("    [AC-05] UDS p95 sync=" + sp.toFixed(2) + "ms fnf=" + fp.toFixed(2) + "ms (n=" + N + ")");
    assert.ok(sp < 20, "sync p95 < 20 ms (got " + sp.toFixed(2) + ")");
    assert.ok(fp < 20, "FNF p95 < 20 ms (got " + fp.toFixed(2) + ")");
  });
});
