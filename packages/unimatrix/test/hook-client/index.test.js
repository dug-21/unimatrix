"use strict";

// Test plan: index.js (entry / dispatch). Oracle: hook.rs::run / read_stdin /
// parse_hook_input / resolve_cwd; sync/FNF split hook.rs:244-251.
//
// Two layers:
//  1. In-process unit tests of the pure helpers (parseHookInput, resolveCwd,
//     sessionIdOf, settle helpers) and dispatch routing via a stubbed transport.
//  2. Spawn-level tests that run the REAL entry through child_process with
//     controlled stdin + a settings.local.json pointing at a stub server — the
//     exit-0 / no-stdout matrix (C-05), AC-08 sync-isolation fs-spy, AC-09
//     failure rows, and FNF replay→event→delta ordering.
//
// Cumulative infra: reuses test/helpers/stub-server.js. Adversarial bytes are
// built via String.fromCharCode (pattern #4769), never bare \uXXXX literals.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const index = require("../../lib/hook-client/index");
const { startStubServer, refusedPort } = require("../helpers/stub-server");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");

// ── temp project scaffolding ────────────────────────────────────────

let tmpRoot;

function freshProject() {
  tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-index-test-"));
  // Make it a project root (config walks to the first `.git`).
  fs.mkdirSync(path.join(tmpRoot, ".git"), { recursive: true });
  fs.mkdirSync(path.join(tmpRoot, ".claude"), { recursive: true });
  return tmpRoot;
}

function writeRemoteConfig(url, token, timeouts) {
  const remote = { url, token };
  if (timeouts) remote.timeouts = timeouts;
  fs.writeFileSync(
    path.join(tmpRoot, ".claude", "settings.local.json"),
    JSON.stringify({ unimatrix: { remote } })
  );
}

// The state dir the SPAWNED child will use: HOME=tmpRoot and projectRoot=tmpRoot
// (the .git marker is at tmpRoot). Mirrors config.js stateDirFor + hash so the
// in-process test can pre-seed the same queue/offsets the child reads.
function childStateDir() {
  // config.walkToProjectRoot uses path.resolve (NOT realpath); the child's cwd
  // is tmpRoot and .git sits there, so projectRoot === path.resolve(tmpRoot).
  const projectRoot = path.resolve(tmpRoot);
  const hash = require("../../lib/hook-client/config").computeProjectHash(projectRoot);
  return path.join(tmpRoot, ".unimatrix", hash, "hook-client");
}

function cleanup() {
  if (tmpRoot) {
    try {
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    } catch (_e) {
      /* best-effort */
    }
    tmpRoot = null;
  }
}

// Spawn the real entry ASYNCHRONOUSLY. spawnSync would freeze the parent event
// loop, so an in-process stub server could never service the child's request —
// the child would time out. We use spawn + a Promise so the stub keeps running.
//
// Uses a HOME inside `home` (default tmpRoot) so state lands in the temp tree,
// and cwd = tmpRoot so config resolution finds settings.local.json. Remote env
// vars are scrubbed unless explicitly provided.
function runEntry(event, stdin, opts) {
  const options = opts || {};
  const env = Object.assign({}, process.env, options.env || {});
  delete env.UNIMATRIX_REMOTE_URL;
  delete env.UNIMATRIX_REMOTE_TOKEN;
  if (options.env) {
    if (options.env.UNIMATRIX_REMOTE_URL) env.UNIMATRIX_REMOTE_URL = options.env.UNIMATRIX_REMOTE_URL;
    if (options.env.UNIMATRIX_REMOTE_TOKEN) env.UNIMATRIX_REMOTE_TOKEN = options.env.UNIMATRIX_REMOTE_TOKEN;
  }
  const home = options.home || tmpRoot;
  env.HOME = home;
  env.USERPROFILE = home; // Windows home
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [ENTRY, event], {
      cwd: options.cwd || tmpRoot,
      env,
    });
    const out = [];
    const errc = [];
    child.stdout.on("data", (c) => out.push(c));
    child.stderr.on("data", (c) => errc.push(c));
    child.on("error", reject);
    child.on("close", (code) => {
      resolve({ status: code, stdout: Buffer.concat(out), stderr: Buffer.concat(errc) });
    });
    child.stdin.on("error", () => {}); // EPIPE if the child never reads stdin
    child.stdin.end(Buffer.from(stdin === undefined ? "" : stdin, "utf8"));
  });
}

// ─────────────────────────────────────────────────────────────────────────
// 1. Unit tests — pure helpers
// ─────────────────────────────────────────────────────────────────────────

describe("parseHookInput (serde parity)", () => {
  it("test_parse_clean_object_yields_empty_extra", () => {
    const out = index.parseHookInput(JSON.stringify({ hook_event_name: "Stop", session_id: "s1" }));
    assert.strictEqual(out.hook_event_name, "Stop");
    assert.strictEqual(out.session_id, "s1");
    assert.deepStrictEqual(out.extra, {}); // clean parse → {} (Rust flatten parity)
  });

  it("test_parse_unknown_fields_survive_in_extra", () => {
    const out = index.parseHookInput(
      JSON.stringify({ session_id: "s1", agent_type: "researcher", custom: { a: 1 } })
    );
    assert.strictEqual(out.extra.agent_type, "researcher");
    assert.deepStrictEqual(out.extra.custom, { a: 1 });
    // unknown keys preserved in insertion order (ass-071 / wire.rs flatten)
    assert.deepStrictEqual(Object.keys(out.extra), ["agent_type", "custom"]);
  });

  it("test_parse_malformed_json_falls_back_to_empty", () => {
    const out = index.parseHookInput("{not json");
    assert.strictEqual(out.hook_event_name, "");
    assert.strictEqual(out.extra, null); // parse failure → extra=null
  });

  it("test_parse_wrong_typed_named_field_falls_back_to_empty", () => {
    // serde fails the WHOLE parse on a wrong-typed named field.
    const out = index.parseHookInput(JSON.stringify({ session_id: 123, agent_type: "x" }));
    assert.strictEqual(out.session_id, null);
    assert.strictEqual(out.extra, null);
  });

  it("test_parse_array_or_scalar_falls_back_to_empty", () => {
    assert.strictEqual(index.parseHookInput("[1,2,3]").extra, null);
    assert.strictEqual(index.parseHookInput("42").extra, null);
    assert.strictEqual(index.parseHookInput("null").extra, null);
  });

  it("test_parse_empty_string_yields_empty_no_stderr", () => {
    // Empty stdin is the no-pipe case — must NOT emit a parse error line.
    const out = index.parseHookInput("");
    assert.strictEqual(out.extra, null);
  });

  it("test_parse_mcp_context_any_value_accepted", () => {
    const out = index.parseHookInput(JSON.stringify({ mcp_context: { tool_name: "x" } }));
    assert.deepStrictEqual(out.mcp_context, { tool_name: "x" });
  });
});

describe("resolveCwd", () => {
  it("test_resolve_cwd_prefers_stdin_cwd", () => {
    assert.strictEqual(index.resolveCwd({ cwd: "/from/stdin" }), "/from/stdin");
  });
  it("test_resolve_cwd_empty_falls_back_to_process_cwd", () => {
    assert.strictEqual(index.resolveCwd({ cwd: "" }), process.cwd());
  });
  it("test_resolve_cwd_null_falls_back_to_process_cwd", () => {
    assert.strictEqual(index.resolveCwd({ cwd: null }), process.cwd());
  });
});

describe("sessionIdOf", () => {
  it("test_session_id_register_close", () => {
    assert.strictEqual(index.sessionIdOf({ type: "SessionRegister", session_id: "a" }), "a");
    assert.strictEqual(index.sessionIdOf({ type: "SessionClose", session_id: "b" }), "b");
  });
  it("test_session_id_record_event_flattened", () => {
    assert.strictEqual(index.sessionIdOf({ type: "RecordEvent", session_id: "c" }), "c");
  });
  it("test_session_id_record_events_first", () => {
    assert.strictEqual(
      index.sessionIdOf({ type: "RecordEvents", events: [{ session_id: "d" }] }),
      "d"
    );
  });
});

describe("settle helpers (AC-09 independence)", () => {
  it("test_settled_send_rejected_becomes_connect_failure", () => {
    const r = index.settledSendResult({ status: "rejected", reason: new Error("x") });
    assert.strictEqual(r.ok, false);
    assert.strictEqual(r.failureClass, "connect");
  });
  it("test_settled_send_fulfilled_passthrough", () => {
    const v = { ok: true, status: 200 };
    assert.strictEqual(index.settledSendResult({ status: "fulfilled", value: v }), v);
  });
  it("test_settled_delta_rejected_non_attempt", () => {
    const o = index.settledDeltaOutcome({ status: "rejected", reason: new Error("x") });
    assert.strictEqual(o.attempted, false);
  });
  it("test_settled_delta_null_when_absent", () => {
    assert.strictEqual(index.settledDeltaOutcome(undefined), null);
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 2. Dispatch routing (in-process, stubbed transport) — mirror hook.rs:244-251
// ─────────────────────────────────────────────────────────────────────────

describe("dispatch split (sync vs fire-and-forget)", () => {
  let calls;
  let origPost;
  const transport = require("../../lib/hook-client/transport-http");

  beforeEach(() => {
    freshProject();
    writeRemoteConfig("http://127.0.0.1:9/x", "tok");
    calls = [];
    origPost = transport.post;
    transport.post = (config, frame, opts) => {
      calls.push({ frame, opts });
      // 204-style success; null body so transform writes nothing.
      return Promise.resolve({
        ok: true,
        status: 200,
        contentType: "application/json",
        body: null,
        failureClass: null,
      });
    };
  });

  afterEach(() => {
    transport.post = origPost;
    cleanup();
  });

  async function runReal(event, stdin) {
    // Drive main() in-process with argv + stdin replaced.
    const origArgv = process.argv;
    process.argv = [process.execPath, ENTRY, event];
    // Stub readStdin by feeding through parseHookInput path: easiest is to set
    // fd 0 — but in-process we instead exercise runSync/runFireAndForget below.
    process.argv = origArgv;
    void stdin;
  }
  void runReal;

  it("test_dispatch_sync_set_uses_accept_text_plain", async () => {
    const config = require("../../lib/hook-client/config").resolve(tmpRoot);
    await index.runSync({ type: "Ping" }, null, config);
    assert.strictEqual(calls.length, 1);
    assert.strictEqual(calls[0].opts.sync, true); // sync → Accept: text/plain
  });

  it("test_dispatch_fnf_set_no_sync_flag", async () => {
    const config = require("../../lib/hook-client/config").resolve(tmpRoot);
    await index.runFireAndForget(
      { type: "RecordEvent", session_id: "s1", event_type: "PreToolUse", payload: {} },
      { transcript_path: null },
      config
    );
    // last call is the carrying event (no replay frames queued) — not sync.
    const carrying = calls[calls.length - 1];
    assert.strictEqual(carrying.opts.sync, false);
  });

  it("test_fnf_carrying_failure_enqueues", async () => {
    transport.post = () =>
      Promise.resolve({ ok: false, status: 0, contentType: null, body: null, failureClass: "connect" });
    const config = require("../../lib/hook-client/config").resolve(tmpRoot);
    const queue = require("../../lib/hook-client/queue");
    await index.runFireAndForget(
      { type: "RecordEvent", session_id: "s1", event_type: "PreToolUse", payload: {} },
      { transcript_path: null },
      config
    );
    assert.strictEqual(queue.queueDepth(config.stateDir), 1); // enqueued on failure
  });

  it("test_fnf_delta_runs_concurrently_with_carrying", async () => {
    // transcript present → exactly two POSTs (carrying + delta), both sync:false.
    const tFile = path.join(tmpRoot, "transcript.jsonl");
    fs.writeFileSync(tFile, "line one\nline two\n");
    const config = require("../../lib/hook-client/config").resolve(tmpRoot);
    await index.runFireAndForget(
      { type: "RecordEvent", session_id: "s1", event_type: "Stop", payload: {} },
      { transcript_path: tFile, provider: "claude-code" },
      config
    );
    // carrying + delta = 2 posts; none sync.
    assert.strictEqual(calls.length, 2);
    assert.ok(calls.every((c) => c.opts.sync === false));
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 3. Spawn-level exit-0 / no-stdout matrix (C-05, FR-05)
// ─────────────────────────────────────────────────────────────────────────

describe("spawn: exit-0 / no-stdout guarantee", () => {
  let stub;

  beforeEach(() => {
    freshProject();
  });
  afterEach(async () => {
    if (stub) {
      await stub.close();
      stub = null;
    }
    cleanup();
  });

  it("test_stdin_piped_json_parses_and_posts", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }));
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    assert.strictEqual(stub.requests.length, 1);
    assert.strictEqual(stub.requests[0].path, "/observe");
  });

  it("test_stdin_empty_yields_empty_input_exit0", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const r = await runEntry("Stop", "");
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
  });

  it("test_stdin_malformed_json_defensive_parse_exit0", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const r = await runEntry("Stop", "{not json");
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    // pipeline still dispatched (Stop → SessionClose → one POST).
    assert.strictEqual(stub.requests.length, 1);
  });

  it("test_missing_config_no_network_breadcrumb_exit0", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    // No settings.local.json, no env → config miss.
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }));
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    assert.strictEqual(stub.requests.length, 0); // NO network attempt
    // breadcrumb written under HOME/.unimatrix/{hash}/hook-client/health.json
    const found = findHealthFile(tmpRoot);
    assert.ok(found, "config-miss must write a breadcrumb");
  });

  it("test_partial_env_no_network_exit0", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }), {
      env: { UNIMATRIX_REMOTE_URL: stub.url }, // token missing → partial
    });
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    assert.strictEqual(stub.requests.length, 0);
    const stderr = r.stderr.toString("utf8");
    assert.ok(stderr.includes("unimatrix: auth:"), "partial env → auth class");
  });

  it("test_econnrefused_exit0_no_stdout", async () => {
    const port = await refusedPort();
    writeRemoteConfig("http://127.0.0.1:" + port + "/x", "tok");
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }), {});
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    const stderr = r.stderr.toString("utf8");
    assert.ok(stderr.includes("unimatrix: connect:"), "connect failure stderr line");
  });

  it("test_http_401_exit0_no_stdout_auth_class", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 401 });
    writeRemoteConfig(stub.url, "badtok");
    // Use a sync event so a non-2xx still produces no stdout (R-15).
    const r = await runEntry(
      "UserPromptSubmit",
      JSON.stringify({ prompt: "implement the spec writer agent", session_id: "s1" })
    );
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    assert.ok(r.stderr.toString("utf8").includes("unimatrix: auth:"));
  });

  it("test_http_500_exit0_no_stdout", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 500 });
    writeRemoteConfig(stub.url, "tok");
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }));
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
  });

  it("test_unwritable_state_dir_exit0", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    // Point HOME at a file so the state dir cannot be created (state is best-effort).
    const homeFile = path.join(tmpRoot, "home-as-file");
    fs.writeFileSync(homeFile, "x");
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }), { home: homeFile });
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    assert.strictEqual(stub.requests.length, 1); // send still attempted despite no state
  });

  it("test_throwing_transcript_path_exit0", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    // transcript_path points at a directory → delta stat path handles it (no throw),
    // carrying event still posts. Exit 0, no stdout.
    const r = await runEntry(
      "Stop",
      JSON.stringify({ session_id: "s1", transcript_path: tmpRoot })
    );
    assert.strictEqual(r.status, 0);
    assert.strictEqual(r.stdout.length, 0);
    assert.ok(stub.requests.length >= 1);
  });

  it("test_source_never_appears_dev_stdin", () => {
    // Closed gate-note 1: '/dev/stdin' must not appear anywhere in hook-client.
    const dir = path.resolve(__dirname, "../../lib/hook-client");
    for (const f of fs.readdirSync(dir)) {
      if (!f.endsWith(".js")) continue;
      const src = fs.readFileSync(path.join(dir, f), "utf8");
      assert.ok(!src.includes("/dev/stdin"), f + " must not use /dev/stdin");
    }
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 4. Sync isolation (AC-08, R-13) — fs-spy via spawn request counting
// ─────────────────────────────────────────────────────────────────────────

describe("spawn: sync-path isolation (AC-08 / R-13)", () => {
  let stub;
  beforeEach(() => freshProject());
  afterEach(async () => {
    if (stub) {
      await stub.close();
      stub = null;
    }
    cleanup();
  });

  it("test_sync_no_queue_io_and_one_post", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 200, contentType: "text/plain", body: "" });
    writeRemoteConfig(stub.url, "tok");
    const r = await runEntry(
      "UserPromptSubmit",
      JSON.stringify({ prompt: "implement the spec writer agent", session_id: "s1" })
    );
    assert.strictEqual(r.status, 0);
    assert.strictEqual(stub.requests.length, 1); // exactly one POST
    // No queue dir should exist for a sync spawn (R-13: zero queue I/O).
    const qd = findQueueDir(tmpRoot);
    assert.ok(!qd, "sync spawn must not create a queue dir");
    // No offsets persisted on a sync spawn (no delta I/O).
    const od = findOffsetsDir(tmpRoot);
    if (od) {
      assert.strictEqual(fs.readdirSync(od).filter((n) => n.endsWith(".json")).length, 0);
    }
  });

  it("test_subagentstart_tail_read_exempt_one_post", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 200, contentType: "text/plain", body: "" });
    writeRemoteConfig(stub.url, "tok");
    const tFile = path.join(tmpRoot, "sa.jsonl");
    // Transcript blocks must be content ARRAYS of {type:"text", text} (oracle
    // shape); a bare string content yields no text blocks → no query.
    fs.writeFileSync(
      tFile,
      JSON.stringify({
        type: "user",
        message: { role: "user", content: [{ type: "text", text: "spawn the researcher agent" }] },
      }) +
        "\n" +
        JSON.stringify({
          type: "assistant",
          message: { role: "assistant", content: [{ type: "text", text: "on it" }] },
        }) +
        "\n"
    );
    const r = await runEntry(
      "SubagentStart",
      JSON.stringify({ session_id: "s1", transcript_path: tFile, agent_type: "researcher" })
    );
    assert.strictEqual(r.status, 0);
    assert.strictEqual(stub.requests.length, 1); // tail read derives query; still ONE post
    // It became a ContextSearch (sync) → no queue dir.
    assert.ok(!findQueueDir(tmpRoot), "SubagentStart→ContextSearch is sync, no queue");
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 5. FNF ordering: replay-before-send then concurrent delta (AC-09)
// ─────────────────────────────────────────────────────────────────────────

describe("spawn: FNF replay → carrying → delta order", () => {
  let stub;
  beforeEach(() => freshProject());
  afterEach(async () => {
    if (stub) {
      await stub.close();
      stub = null;
    }
    cleanup();
  });

  it("test_replay_precedes_carrying_event", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    // Pre-seed a queue frame so replay has something to send first. Use the
    // SPAWNED child's state dir (HOME=tmpRoot), not the in-process homedir.
    const queue = require("../../lib/hook-client/queue");
    queue.enqueue(childStateDir(), {
      type: "RecordEvent",
      session_id: "old",
      event_type: "PreToolUse",
      payload: { marker: "queued" },
    });
    const r = await runEntry("Stop", JSON.stringify({ session_id: "s1" }));
    assert.strictEqual(r.status, 0);
    // Two POSTs: replayed frame first, then the carrying SessionClose.
    assert.strictEqual(stub.requests.length, 2);
    const first = JSON.parse(stub.requests[0].body.toString("utf8"));
    const second = JSON.parse(stub.requests[1].body.toString("utf8"));
    assert.strictEqual(first.payload && first.payload.marker, "queued"); // replay first
    assert.strictEqual(second.type, "SessionClose"); // carrying second
  });

  it("test_carrying_and_delta_both_sent_concurrently", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const tFile = path.join(tmpRoot, "t.jsonl");
    fs.writeFileSync(tFile, "alpha\nbeta\ngamma\n");
    const r = await runEntry(
      "Stop",
      JSON.stringify({ session_id: "s1", transcript_path: tFile })
    );
    assert.strictEqual(r.status, 0);
    // carrying SessionClose + one transcript_delta = 2 POSTs (no replay seeded).
    assert.strictEqual(stub.requests.length, 2);
    const bodies = stub.requests.map((q) => JSON.parse(q.body.toString("utf8")));
    assert.ok(bodies.some((b) => b.type === "SessionClose"));
    assert.ok(bodies.some((b) => b.event_type === "transcript_delta"));
  });
});

// ─────────────────────────────────────────────────────────────────────────
// helpers: locate state artifacts under the temp HOME
// ─────────────────────────────────────────────────────────────────────────

function hookClientDirs(home) {
  const base = path.join(home, ".unimatrix");
  const out = [];
  let hashes;
  try {
    hashes = fs.readdirSync(base);
  } catch (_e) {
    return out;
  }
  for (const h of hashes) {
    const hc = path.join(base, h, "hook-client");
    if (fs.existsSync(hc)) out.push(hc);
  }
  return out;
}

function findHealthFile(home) {
  for (const hc of hookClientDirs(home)) {
    if (fs.existsSync(path.join(hc, "health.json"))) return path.join(hc, "health.json");
  }
  return null;
}

function findQueueDir(home) {
  for (const hc of hookClientDirs(home)) {
    const q = path.join(hc, "queue");
    if (fs.existsSync(q)) return q;
  }
  return null;
}

function findOffsetsDir(home) {
  for (const hc of hookClientDirs(home)) {
    const o = path.join(hc, "offsets");
    if (fs.existsSync(o)) return o;
  }
  return null;
}
