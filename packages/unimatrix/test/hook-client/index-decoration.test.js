"use strict";

// Test plan: test-plan/index-decoration.md (C2 — index.js FNF-path stamp
// decoration, ADR-002). Covers AC-01 (lifecycle dispatch), AC-02 (stamp attach +
// batch), AC-03 (suppression strip), AC-06 dispatch side (subagent-gated canary),
// the never-declare floor, sync-path isolation, and the R-07 seam-survival
// gate (cross-referenced from seam-and-roundtrip.md §1).
//
// Pinned CLI: claude 2.1.167 (--resume id-reuse, depth-1 root-id inheritance).
//
// OQ-E disposition (ADR-006 §7): BRANCH A (signals independent). The subagent
// "I am a subagent" marker (`input.extra.agent_type`) rides a structurally
// distinct stdin channel from the named top-level `session_id` field, so a CLI
// regression that breaks root-id inheritance does NOT strip the subagent marker
// — the production canary fires under drift and ships ACTIVE. The test-time
// zero-tolerance invariant (`stamp_miss == 0`) ships either branch.
//
// Two layers (the index.test.js idiom):
//   1. In-process unit tests of `decorateCycleStamp` against a real temp stateDir
//      (cycles/state are the never-throw helpers; deterministic, no socket).
//   2. Spawn-level seam-survival tests through the REAL entry + stub server,
//      asserting the rebased pipeline keeps FR-01..03 hanging on the seam.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const index = require("../../lib/hook-client/index");
const cycles = require("../../lib/hook-client/cycles");
const state = require("../../lib/hook-client/state");
const buildRequest = require("../../lib/hook-client/build-request");
const { startStubServer } = require("../helpers/stub-server");

const ENTRY = path.resolve(__dirname, "../../lib/hook-client/index.js");
const {
  CYCLE_START_EVENT,
  CYCLE_PHASE_END_EVENT,
  CYCLE_STOP_EVENT,
} = buildRequest;

// ── temp state-dir scaffolding (in-process layer) ───────────────────

let tmpDir;

function freshStateDir() {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-decoration-"));
  return tmpDir;
}

function cleanupStateDir() {
  if (tmpDir) {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch (_e) {
      /* best-effort */
    }
    tmpDir = null;
  }
}

function readHealth(stateDir) {
  return state.readBreadcrumb(stateDir);
}

// A plain RecordEvent frame (build-request-tools.recordEventFrame shape).
function recordEvent(eventType, sessionId, payload, topicSignal) {
  const f = {
    type: "RecordEvent",
    event_type: eventType,
    session_id: sessionId,
    timestamp: 123,
    payload: payload || {},
  };
  if (topicSignal !== undefined) f.topic_signal = topicSignal;
  return f;
}

function cycleFrame(eventType, sessionId, topic, nextPhase) {
  const payload = { feature_cycle: topic };
  if (nextPhase !== undefined) payload.next_phase = nextPhase;
  // CYCLE_* frames carry topic_signal = topic (the declaration).
  return recordEvent(eventType, sessionId, payload, topic);
}

// HookInput stub (only the fields decoration reads: extra.agent_type).
function input(extra) {
  return { extra: extra || null, transcript_path: null, provider: null };
}

function config(stateDir) {
  return { stateDir };
}

// ─────────────────────────────────────────────────────────────────────────
// 1. Lifecycle dispatch keyed on CYCLE_* frames (R-02, AC-01)
// ─────────────────────────────────────────────────────────────────────────

describe("decorateCycleStamp: lifecycle dispatch (AC-01, R-02)", () => {
  beforeEach(() => freshStateDir());
  afterEach(() => cleanupStateDir());

  it("test_cycle_start_frame_writes_tracker", () => {
    const sd = tmpDir;
    const f = cycleFrame(CYCLE_START_EVENT, "s1", "vnc-030", "delivery");
    index.decorateCycleStamp(f, input(), config(sd));
    const tracker = cycles.readCycle(sd, "s1");
    assert.deepStrictEqual(tracker, { topic: "vnc-030", phase: "delivery" });
  });

  it("test_cycle_phase_end_frame_updates_phase", () => {
    const sd = tmpDir;
    cycles.writeCycle(sd, "s1", "vnc-030", "design");
    const f = cycleFrame(CYCLE_PHASE_END_EVENT, "s1", "vnc-030", "delivery");
    index.decorateCycleStamp(f, input(), config(sd));
    assert.strictEqual(cycles.readCycle(sd, "s1").phase, "delivery");
  });

  it("test_cycle_stop_frame_deletes_tracker", () => {
    const sd = tmpDir;
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const f = cycleFrame(CYCLE_STOP_EVENT, "s1", "vnc-030");
    index.decorateCycleStamp(f, input(), config(sd));
    assert.strictEqual(cycles.readCycle(sd, "s1"), null);
  });

  it("test_lifecycle_events_never_touch_tracker", () => {
    // SessionRegister / SessionClose frames carry no ImplantEvents → skipped
    // entirely: tracker byte-unchanged, no stamp, no canary (FR-04, R-02).
    const sd = tmpDir;
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const before = fs.readFileSync(cycles.cyclePath(sd, "s1"));
    const reg = { type: "SessionRegister", session_id: "s1", cwd: "/p" };
    const close = { type: "SessionClose", session_id: "s1", outcome: "success" };
    index.decorateCycleStamp(reg, input(), config(sd));
    index.decorateCycleStamp(close, input(), config(sd));
    const after = fs.readFileSync(cycles.cyclePath(sd, "s1"));
    assert.ok(before.equals(after), "tracker must be byte-unchanged");
    assert.strictEqual(reg.cycle_stamp, undefined);
    assert.strictEqual(close.cycle_stamp, undefined);
  });

  it("test_multiturn_stop_does_not_kill_stamp", () => {
    // cycle_start → 3×(Stop SessionClose + RecordEvent). Stop builds SessionClose
    // (no events → lifecycle no-op); every post-Stop RecordEvent still stamps.
    const sd = tmpDir;
    index.decorateCycleStamp(
      cycleFrame(CYCLE_START_EVENT, "s1", "vnc-030", "delivery"),
      input(),
      config(sd)
    );
    const trackerBytes = fs.readFileSync(cycles.cyclePath(sd, "s1"));
    for (let turn = 0; turn < 3; turn++) {
      const close = { type: "SessionClose", session_id: "s1", outcome: "success" };
      index.decorateCycleStamp(close, input(), config(sd));
      assert.ok(
        fs.readFileSync(cycles.cyclePath(sd, "s1")).equals(trackerBytes),
        "Stop must leave the tracker byte-unchanged on every turn"
      );
      const rec = recordEvent("post_tool_use", "s1", { tool: "x" }, "extracted");
      index.decorateCycleStamp(rec, input(), config(sd));
      assert.strictEqual(rec.cycle_stamp.topic, "vnc-030");
    }
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 2. Stamp attach (FR-06, AC-02 client side)
// ─────────────────────────────────────────────────────────────────────────

describe("decorateCycleStamp: stamp attach (AC-02)", () => {
  beforeEach(() => freshStateDir());
  afterEach(() => cleanupStateDir());

  it("test_recordevent_present_tracker_attaches_cycle_stamp", () => {
    const sd = tmpDir;
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const f = recordEvent("post_tool_use", "s1", {}, "extracted");
    index.decorateCycleStamp(f, input(), config(sd));
    assert.deepStrictEqual(f.cycle_stamp, { topic: "vnc-030", phase: "delivery" });
  });

  it("test_recordevent_present_tracker_phase_null_omits_phase_key", () => {
    const sd = tmpDir;
    cycles.writeCycle(sd, "s1", "vnc-030", null);
    const f = recordEvent("post_tool_use", "s1", {});
    index.decorateCycleStamp(f, input(), config(sd));
    assert.deepStrictEqual(f.cycle_stamp, { topic: "vnc-030" });
    assert.ok(!("phase" in f.cycle_stamp), "phase key omitted when null");
  });

  it("test_recordevent_missing_tracker_no_stamp", () => {
    const sd = tmpDir;
    const f = recordEvent("post_tool_use", "s1", {}, "extracted");
    index.decorateCycleStamp(f, input(), config(sd));
    assert.strictEqual(f.cycle_stamp, undefined);
  });

  it("test_corrupt_tracker_sends_unstamped_no_throw", () => {
    const sd = tmpDir;
    cycles.ensureCyclesDir(sd);
    fs.writeFileSync(cycles.cyclePath(sd, "s1"), "{not json");
    const f = recordEvent("post_tool_use", "s1", {}, "extracted");
    assert.doesNotThrow(() =>
      index.decorateCycleStamp(f, input(), config(sd))
    );
    assert.strictEqual(f.cycle_stamp, undefined);
    assert.strictEqual(f.topic_signal, "extracted"); // unstamped → kept
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 3. Extraction suppression strip (R-05, AC-03 — both directions)
// ─────────────────────────────────────────────────────────────────────────

describe("decorateCycleStamp: suppression strip (AC-03, R-05)", () => {
  beforeEach(() => freshStateDir());
  afterEach(() => cleanupStateDir());

  it("test_suppression_strip_on_non_cycle_frame", () => {
    const sd = tmpDir;
    // tracker present → stamp + topic_signal stripped.
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const withTracker = recordEvent("post_tool_use", "s1", {}, "extracted");
    index.decorateCycleStamp(withTracker, input(), config(sd));
    assert.deepStrictEqual(withTracker.cycle_stamp, {
      topic: "vnc-030",
      phase: "delivery",
    });
    assert.strictEqual(withTracker.topic_signal, undefined, "topic_signal stripped");

    // tracker absent → topic_signal kept, no stamp.
    cycles.deleteCycle(sd, "s1");
    const withoutTracker = recordEvent("post_tool_use", "s1", {}, "extracted");
    index.decorateCycleStamp(withoutTracker, input(), config(sd));
    assert.strictEqual(withoutTracker.cycle_stamp, undefined);
    assert.strictEqual(withoutTracker.topic_signal, "extracted");
  });

  it("test_cycle_frame_keeps_topic_signal", () => {
    // A CYCLE_* frame with a tracker present KEEPS topic_signal = topic.
    const sd = tmpDir;
    const f = cycleFrame(CYCLE_START_EVENT, "s1", "vnc-030", "delivery");
    index.decorateCycleStamp(f, input(), config(sd));
    assert.strictEqual(f.topic_signal, "vnc-030", "CYCLE_* keeps its declaration");
    assert.strictEqual(f.cycle_stamp.topic, "vnc-030");
  });

  it("test_unstamped_session_extraction_byte_unchanged", () => {
    // Never-declare session: the frame is untouched except the (skipped) stamp.
    const sd = tmpDir;
    const f = recordEvent("post_tool_use", "s1", { a: 1 }, "extracted");
    const snapshot = JSON.parse(JSON.stringify(f));
    index.decorateCycleStamp(f, input(), config(sd));
    assert.deepStrictEqual(f, snapshot, "unstamped frame byte-unchanged");
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 4. Batch / replay decoration (R-06, AC-02 batch)
// ─────────────────────────────────────────────────────────────────────────

describe("decorateCycleStamp: batch (AC-02, R-06)", () => {
  beforeEach(() => freshStateDir());
  afterEach(() => cleanupStateDir());

  it("test_recordevents_batch_every_event_stamped", () => {
    const sd = tmpDir;
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const events = [
      // CYCLE_* member keeps topic_signal; non-cycle members get stripped.
      {
        event_type: CYCLE_PHASE_END_EVENT,
        session_id: "s1",
        timestamp: 1,
        payload: { feature_cycle: "vnc-030", next_phase: "delivery" },
        topic_signal: "vnc-030",
      },
      {
        event_type: "post_tool_use",
        session_id: "s1",
        timestamp: 2,
        payload: {},
        topic_signal: "extracted-a",
      },
      {
        event_type: "user_prompt",
        session_id: "s1",
        timestamp: 3,
        payload: {},
        topic_signal: "extracted-b",
      },
    ];
    const batch = { type: "RecordEvents", events };
    index.decorateCycleStamp(batch, input(), config(sd));
    for (const ev of events) {
      assert.deepStrictEqual(ev.cycle_stamp, { topic: "vnc-030", phase: "delivery" });
    }
    assert.strictEqual(events[0].topic_signal, "vnc-030", "CYCLE_* member kept");
    assert.strictEqual(events[1].topic_signal, undefined, "non-cycle stripped");
    assert.strictEqual(events[2].topic_signal, undefined, "non-cycle stripped");
  });

  it("test_send_failure_enqueue_replay_carries_stamp", async () => {
    // The decorated `request` is what runFireAndForget enqueues on send-failure;
    // assert the enqueued (post-decoration) frame carries the stamp. A spy
    // transport that always fails forces the enqueue path.
    const sd = tmpDir;
    state.ensureStateDir(sd);
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const f = recordEvent("post_tool_use", "s1", {}, "extracted");
    index.decorateCycleStamp(f, input(), config(sd));
    assert.deepStrictEqual(f.cycle_stamp, { topic: "vnc-030", phase: "delivery" });

    const queue = require("../../lib/hook-client/queue");
    const failingPost = {
      post: () =>
        Promise.resolve({
          ok: false,
          status: 0,
          contentType: null,
          body: null,
          failureClass: "connect",
        }),
    };
    const cfg = { stateDir: sd, urlHost: "h", mode: "uds" };
    await index.runFireAndForget(f, input(), cfg, failingPost, "PostToolUse");
    const depth = queue.queueDepth(sd);
    assert.ok(depth >= 1, "send failure must enqueue the frame");
    // Read back the queued frame and confirm the stamp survived enqueue.
    const replayed = [];
    await queue.replay(
      { stateDir: sd },
      (_c, frame) => {
        replayed.push(frame);
        return Promise.resolve({ ok: true, status: 0, failureClass: null });
      }
    );
    assert.ok(replayed.length >= 1, "replay drained the queued frame");
    assert.deepStrictEqual(replayed[0].cycle_stamp, {
      topic: "vnc-030",
      phase: "delivery",
    });
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 5. Canary dispatch (subagent-gated miss branch) (AC-06, R-19)
// ─────────────────────────────────────────────────────────────────────────

describe("decorateCycleStamp: subagent-gated canary (AC-06, R-19)", () => {
  beforeEach(() => freshStateDir());
  afterEach(() => cleanupStateDir());

  it("test_depth0_never_declare_no_increment", () => {
    // Top-level (no agent_type), no tracker → structural noise → no increment.
    const sd = tmpDir;
    const f = recordEvent("post_tool_use", "s1", {}, "extracted");
    index.decorateCycleStamp(f, input(/* no extra */), config(sd));
    assert.strictEqual(readHealth(sd).stamp_miss, 0);
  });

  it("test_depth1_subagent_inherited_tracker_present_no_increment", () => {
    // Subagent event finds its inherited root tracker → stamps, no increment.
    const sd = tmpDir;
    cycles.writeCycle(sd, "root", "vnc-030", "delivery");
    const f = recordEvent("post_tool_use", "root", {}, "extracted");
    index.decorateCycleStamp(f, input({ agent_type: "uni-rust-dev" }), config(sd));
    assert.deepStrictEqual(f.cycle_stamp, { topic: "vnc-030", phase: "delivery" });
    assert.strictEqual(readHealth(sd).stamp_miss, 0);
  });

  it("test_depth1_subagent_noninherited_id_root_tracker_exists_one_increment", () => {
    // Inheritance drift: subagent carries a NON-inherited id; no tracker for it,
    // while a root tracker exists → exactly one increment.
    const sd = tmpDir;
    cycles.writeCycle(sd, "root", "vnc-030", "delivery");
    const f = recordEvent("post_tool_use", "drifted-id", {}, "extracted");
    index.decorateCycleStamp(f, input({ agent_type: "uni-rust-dev" }), config(sd));
    assert.strictEqual(f.cycle_stamp, undefined, "no tracker for the carried id");
    assert.strictEqual(readHealth(sd).stamp_miss, 1);
  });

  it("test_depthgt1_grandchild_no_tracker_lands_in_stamp_miss", () => {
    // depth>1 forward-compat: grandchild id with no tracker while root exists →
    // lands in stamp_miss (silent loss impossible). Same client shape as depth-1.
    const sd = tmpDir;
    cycles.writeCycle(sd, "root", "vnc-030", "delivery");
    const f = recordEvent("post_tool_use", "grandchild-id", {}, "extracted");
    index.decorateCycleStamp(f, input({ agent_type: "uni-tester" }), config(sd));
    assert.strictEqual(readHealth(sd).stamp_miss, 1);
  });

  it("test_healthy_single_declared_session_with_subagent_stamp_miss_zero", () => {
    // End-to-end zero-tolerance invariant (ships either OQ-E branch): one declared
    // root + one depth-1 subagent inheriting the root id → stamp_miss == 0.
    const sd = tmpDir;
    index.decorateCycleStamp(
      cycleFrame(CYCLE_START_EVENT, "root", "vnc-030", "delivery"),
      input(),
      config(sd)
    );
    const sub = recordEvent("post_tool_use", "root", {}, "extracted");
    index.decorateCycleStamp(sub, input({ agent_type: "uni-rust-dev" }), config(sd));
    assert.deepStrictEqual(sub.cycle_stamp, { topic: "vnc-030", phase: "delivery" });
    assert.strictEqual(readHealth(sd).stamp_miss, 0);
  });

  it("test_subagent_context_signal_independent_of_session_id (OQ-E Branch A)", () => {
    // The subagent marker is on input.extra.agent_type — a channel structurally
    // distinct from the named session_id field. subagentContext reports isSubagent
    // from the marker alone, so it survives a root-id inheritance break.
    const ctxA = index.subagentContext(input({ agent_type: "uni-rust-dev" }), "id1");
    assert.strictEqual(ctxA.isSubagent, true);
    assert.strictEqual(ctxA.rootSessionId, "id1");
    const ctxB = index.subagentContext(input(/* no marker */), "id1");
    assert.strictEqual(ctxB.isSubagent, false, "top-level event is not a subagent");
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 6. frameEvents / isCycleEvent unit coverage
// ─────────────────────────────────────────────────────────────────────────

describe("frameEvents / isCycleEvent helpers", () => {
  it("test_frameEvents_single_returns_frame_itself", () => {
    const f = recordEvent("post_tool_use", "s1", {});
    assert.deepStrictEqual(index.frameEvents(f), [f]);
  });

  it("test_frameEvents_batch_returns_events", () => {
    const events = [{ event_type: "a" }, { event_type: "b" }];
    assert.deepStrictEqual(index.frameEvents({ type: "RecordEvents", events }), events);
  });

  it("test_frameEvents_session_frames_return_empty", () => {
    assert.deepStrictEqual(index.frameEvents({ type: "SessionRegister" }), []);
    assert.deepStrictEqual(index.frameEvents({ type: "SessionClose" }), []);
  });

  it("test_frameEvents_missing_events_array_returns_empty", () => {
    assert.deepStrictEqual(index.frameEvents({ type: "RecordEvents" }), []);
  });

  it("test_isCycleEvent_recognizes_all_three", () => {
    assert.ok(index.isCycleEvent({ event_type: CYCLE_START_EVENT }));
    assert.ok(index.isCycleEvent({ event_type: CYCLE_PHASE_END_EVENT }));
    assert.ok(index.isCycleEvent({ event_type: CYCLE_STOP_EVENT }));
    assert.ok(!index.isCycleEvent({ event_type: "post_tool_use" }));
  });
});

// ─────────────────────────────────────────────────────────────────────────
// 7. Fail-open
// ─────────────────────────────────────────────────────────────────────────

describe("decorateCycleStamp: fail-open (C-04)", () => {
  beforeEach(() => freshStateDir());
  afterEach(() => cleanupStateDir());

  it("test_unusable_statedir_sends_unstamped_no_throw", () => {
    const f = recordEvent("post_tool_use", "s1", {}, "extracted");
    assert.doesNotThrow(() => index.decorateCycleStamp(f, input(), config(null)));
    assert.strictEqual(f.cycle_stamp, undefined);
  });

  it("test_null_sessionid_returns_early_no_throw", () => {
    const sd = tmpDir;
    const f = recordEvent("post_tool_use", null, {}, "extracted");
    assert.doesNotThrow(() => index.decorateCycleStamp(f, input(), config(sd)));
    assert.strictEqual(f.cycle_stamp, undefined);
  });
});

// ═════════════════════════════════════════════════════════════════════════
// SPAWN-LEVEL: seam survival (R-07 / FR-28 / ADR-007 §1) — GATE 1
// (cross-referenced from seam-and-roundtrip.md §1; driven through the rebased
// index.js pipeline, not the cycles module in isolation.)
// ═════════════════════════════════════════════════════════════════════════

let tmpRoot;

function freshProject() {
  tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-seam-"));
  fs.mkdirSync(path.join(tmpRoot, ".git"), { recursive: true });
  fs.mkdirSync(path.join(tmpRoot, ".claude"), { recursive: true });
  return tmpRoot;
}

function writeRemoteConfig(url, token) {
  fs.writeFileSync(
    path.join(tmpRoot, ".claude", "settings.local.json"),
    JSON.stringify({ unimatrix: { remote: { url, token } } })
  );
}

function childStateDir(root) {
  const base = root || tmpRoot;
  const config = require("../../lib/hook-client/config");
  const hash = config.computeProjectHash(config.walkToProjectRoot(base));
  return path.join(base, ".unimatrix", hash, "hook-client");
}

function cleanupProject() {
  if (tmpRoot) {
    try {
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    } catch (_e) {
      /* best-effort */
    }
    tmpRoot = null;
  }
}

function runEntry(event, stdin, opts) {
  const options = opts || {};
  const env = Object.assign({}, process.env, options.env || {});
  delete env.UNIMATRIX_REMOTE_URL;
  delete env.UNIMATRIX_REMOTE_TOKEN;
  const home = options.home || tmpRoot;
  env.HOME = home;
  env.USERPROFILE = home;
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
    child.stdin.on("error", () => {});
    child.stdin.end(Buffer.from(stdin === undefined ? "" : stdin, "utf8"));
  });
}

function trackerExists(root, sid) {
  const sd = childStateDir(root);
  return fs.existsSync(cycles.cyclePath(sd, sid));
}

describe("spawn seam-survival: stamp rides the rebased pipeline (R-07, FR-28)", () => {
  let stub;

  beforeEach(() => freshProject());
  afterEach(async () => {
    if (stub) {
      await stub.close();
      stub = null;
    }
    cleanupProject();
  });

  it("test_seam_cycle_start_yields_stamped_frame_and_writes_tracker", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const stdin = JSON.stringify({
      session_id: "s1",
      tool_name: "mcp__unimatrix__context_cycle",
      tool_input: { type: "start", topic: "vnc-030" },
    });
    const r = await runEntry("PreToolUse", stdin);
    assert.strictEqual(r.status, 0);
    // Tracker created via cycles.writeCycle (reached the seam, not the sentinel).
    assert.ok(trackerExists(tmpRoot, "s1"), "tracker file created");
    // A CYCLE_START RecordEvent frame was sent, carrying cycle_stamp.
    assert.strictEqual(stub.requests.length, 1, "one FNF POST (not the null sentinel)");
    const body = JSON.parse(stub.requests[0].body.toString("utf8"));
    assert.strictEqual(body.type, "RecordEvent");
    assert.strictEqual(body.event_type, CYCLE_START_EVENT);
    assert.ok(body.cycle_stamp, "frame carries cycle_stamp");
    assert.strictEqual(body.cycle_stamp.topic, "vnc-030");
    // CYCLE_* frame keeps its topic_signal (the declaration).
    assert.strictEqual(body.topic_signal, "vnc-030");
  });

  it("test_seam_noncycle_pretooluse_yields_sentinel_no_side_effects", async () => {
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const stdin = JSON.stringify({ session_id: "s1", tool_name: "Bash" });
    const r = await runEntry("PreToolUse", stdin);
    assert.strictEqual(r.status, 0);
    // Null sentinel: no network, no tracker, no stamp_miss bump.
    assert.strictEqual(stub.requests.length, 0, "null sentinel makes no network call");
    const sd = childStateDir(tmpRoot);
    assert.ok(!fs.existsSync(cycles.cyclesDir(sd)), "no tracker dir touched");
    assert.ok(
      !fs.existsSync(state.healthPath(sd)),
      "no stamp_miss bump (health.json absent)"
    );
  });

  it("test_seam_cli_validation_gate_rejects_invalid_params_no_tracker", async () => {
    // Invalid validateCycleParams (missing topic) → null sentinel, no tracker.
    stub = await startStubServer();
    stub.respondWith({ status: 204 });
    writeRemoteConfig(stub.url, "tok");
    const stdin = JSON.stringify({
      session_id: "s1",
      tool_name: "context_cycle",
      tool_input: { type: "start" }, // no topic → validation fails
    });
    const r = await runEntry("PreToolUse", stdin);
    assert.strictEqual(r.status, 0);
    assert.strictEqual(stub.requests.length, 0, "invalid params → no frame");
    assert.ok(!trackerExists(tmpRoot, "s1"), "no tracker on invalid params");
  });

  it("test_stamped_recordevent_over_uds_and_http_identical_cycle_stamp", async () => {
    // AC-10/FR-29 (offline, UNGUARDED): a stamped RecordEvent serialized for UDS
    // (transport-uds.encodeFrame) carries cycle_stamp byte-equivalent to the HTTP
    // body. Decoration is upstream of selectTransport → both stringify the same
    // object. No socket opened here.
    const transportUds = require("../../lib/hook-client/transport-uds");
    const sd = freshStateDir();
    cycles.writeCycle(sd, "s1", "vnc-030", "delivery");
    const f = recordEvent("post_tool_use", "s1", { tool: "x" }, "extracted");
    index.decorateCycleStamp(f, input(), config(sd));
    // HTTP body bytes = JSON.stringify(frame); UDS = encodeFrame header+JSON.
    const httpBody = Buffer.from(JSON.stringify(f), "utf8");
    const udsFrame = transportUds.encodeFrame(f, { sync: false });
    assert.ok(udsFrame, "encodeFrame produced bytes");
    const udsJson = udsFrame.subarray(4); // strip the 4-byte BE length prefix
    assert.ok(udsJson.equals(httpBody), "UDS payload byte-equivalent to HTTP body");
    const decoded = JSON.parse(udsJson.toString("utf8"));
    assert.deepStrictEqual(decoded.cycle_stamp, { topic: "vnc-030", phase: "delivery" });
    cleanupStateDir();
  });
});
