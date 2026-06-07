"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const queue = require("../../lib/hook-client/queue");

const IS_WINDOWS = process.platform === "win32";
const IS_ROOT = typeof process.getuid === "function" && process.getuid() === 0;

/** Fresh temp state dir per test (stands in for ~/.unimatrix/{hash}/hook-client). */
function tempStateDir() {
  return path.join(
    fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-queue-test-")),
    "hook-client"
  );
}

function qdir(stateDir) {
  return path.join(stateDir, "queue");
}

function listFiles(stateDir) {
  try {
    return fs
      .readdirSync(qdir(stateDir))
      .filter((n) => n.endsWith(".json"))
      .sort();
  } catch (_err) {
    return [];
  }
}

/** A bare non-delta FNF frame (RecordEvent). */
function recordEventFrame(i) {
  return {
    type: "RecordEvent",
    event_type: "PostToolUse",
    session_id: "sess-" + (i || 0),
    timestamp: 1700000000,
    payload: { tool_input: { n: i || 0 } },
  };
}

function deltaFrame() {
  return {
    type: "RecordEvent",
    event_type: "transcript_delta",
    session_id: "sess-d",
    timestamp: 1700000000,
    payload: { offset: 0, bytes: "secret conversation bytes" },
  };
}

/** A `post` stub recording calls; configurable per-call behavior. */
function makePost(behavior) {
  const calls = [];
  const fn = async (config, frame, opts) => {
    calls.push({ frame, opts });
    const r = behavior(frame, calls.length - 1);
    return r;
  };
  fn.calls = calls;
  return fn;
}

function ok() {
  return { ok: true, status: 200, failureClass: null };
}
function fail() {
  return { ok: false, status: 0, failureClass: "connect" };
}

/** Write a frame file directly with a chosen ts prefix (bypasses enqueue). */
function writeFrameAt(stateDir, ts, seq, frame) {
  const dir = qdir(stateDir);
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const pad = (n, w) => String(n).padStart(w, "0");
  const name = pad(ts, 13) + "-" + process.pid + "-" + pad(seq, 4) + ".json";
  fs.writeFileSync(path.join(dir, name), JSON.stringify(frame), {
    mode: 0o600,
  });
  return name;
}

// ── Lifecycle (AC-15 amended letter) ───────────────────────────────

describe("queue lifecycle (ADR-003, AC-15)", function () {
  it("test_lifecycle_fail_enqueue_recover_replay_drain", async function () {
    const dir = tempStateDir();
    // Outage: enqueue three non-delta frames (as index.js would on send failure).
    queue.enqueue(dir, recordEventFrame(1));
    queue.enqueue(dir, recordEventFrame(2));
    queue.enqueue(dir, recordEventFrame(3));
    const files = listFiles(dir);
    assert.strictEqual(files.length, 3, "three frames persisted");
    // One frame per file, O_EXCL name shape.
    for (const f of files) {
      assert.match(f, /^\d{13}-\d+-\d{4}\.json$/, "ts_ms-pid-seq.json");
    }
    if (!IS_WINDOWS && !IS_ROOT) {
      const mode = fs.statSync(path.join(qdir(dir), files[0])).mode & 0o777;
      assert.strictEqual(mode, 0o600, "frame file is 0600");
    }

    // Recover: replay in lexicographic (age) order, each deleted after 2xx.
    const post = makePost(() => ok());
    const r = await queue.replay({ stateDir: dir }, post);
    assert.strictEqual(r.sent, 3);
    assert.strictEqual(r.stoppedOnFailure, false);
    assert.strictEqual(listFiles(dir).length, 0, "queue drained");
    // Order preserved: session ids 1,2,3.
    assert.deepStrictEqual(
      post.calls.map((c) => c.frame.session_id),
      ["sess-1", "sess-2", "sess-3"]
    );
    // Replay uses non-sync transport.
    assert.strictEqual(post.calls[0].opts.sync, false);
  });

  it("test_delta_never_queued", function () {
    const dir = tempStateDir();
    queue.enqueue(dir, deltaFrame());
    assert.strictEqual(listFiles(dir).length, 0, "delta frame never written");
    // Full directory scan: no file content contains transcript_delta.
    queue.enqueue(dir, recordEventFrame(1));
    queue.enqueue(dir, deltaFrame());
    queue.enqueue(dir, recordEventFrame(2));
    for (const f of listFiles(dir)) {
      const body = fs.readFileSync(path.join(qdir(dir), f), "utf8");
      assert.ok(
        !body.includes("transcript_delta"),
        "no transcript_delta at rest in " + f
      );
    }
    assert.strictEqual(listFiles(dir).length, 2, "only the two record events");
  });

  it("test_replay_empty_queue_noop", async function () {
    const dir = tempStateDir();
    const post = makePost(() => ok());
    const r = await queue.replay({ stateDir: dir }, post);
    assert.strictEqual(r.sent, 0);
    assert.strictEqual(post.calls.length, 0, "no posts when queue empty");
  });
});

// ── Bounds + Eviction (FR-14) ──────────────────────────────────────

describe("queue bounds + eviction (FR-14)", function () {
  it("test_enqueue_501st_file_drops_oldest", function () {
    const dir = tempStateDir();
    // Seed 500 files with ascending ts so order is deterministic.
    const base = Date.now() - 1000;
    for (let i = 0; i < 500; i++) {
      writeFrameAt(dir, base + i, 0, recordEventFrame(i));
    }
    assert.strictEqual(listFiles(dir).length, 500);
    const oldestBefore = listFiles(dir)[0];
    // 501st enqueue → drop oldest, count stays 500.
    queue.enqueue(dir, recordEventFrame(999));
    const after = listFiles(dir);
    assert.strictEqual(after.length, 500, "count capped at 500");
    assert.ok(!after.includes(oldestBefore), "oldest dropped");
  });

  it("test_enqueue_over_5mib_drops_oldest", function () {
    const dir = tempStateDir();
    // Three ~2 MiB frames: total 6 MiB > 5 MiB cap. Seed two, enqueue a third.
    const big = "x".repeat(2 * 1024 * 1024);
    const base = Date.now() - 1000;
    const f0 = writeFrameAt(dir, base, 0, { type: "RecordEvent", event_type: "E", session_id: "a", timestamp: 1, payload: big });
    writeFrameAt(dir, base + 1, 0, { type: "RecordEvent", event_type: "E", session_id: "b", timestamp: 1, payload: big });
    assert.strictEqual(listFiles(dir).length, 2);
    queue.enqueue(dir, { type: "RecordEvent", event_type: "E", session_id: "c", timestamp: 1, payload: big });
    const after = listFiles(dir);
    assert.ok(!after.includes(f0), "oldest dropped on size bound");
    // Total bytes under 5 MiB after eviction.
    let total = 0;
    for (const f of after) total += fs.statSync(path.join(qdir(dir), f)).size;
    assert.ok(total <= 5 * 1024 * 1024, "total under 5 MiB: " + total);
  });

  it("test_age_prune_24h", async function () {
    const dir = tempStateDir();
    const now = Date.now();
    const oldName = writeFrameAt(dir, now - (25 * 3600 * 1000), 0, recordEventFrame(1));
    const freshName = writeFrameAt(dir, now - 1000, 0, recordEventFrame(2));
    // prune() removes the >24h file, NOT replayed.
    queue.prune(dir);
    const after = listFiles(dir);
    assert.ok(!after.includes(oldName), ">24h file pruned");
    assert.ok(after.includes(freshName), "fresh file kept");
    // Confirm the stub never receives the pruned frame on a subsequent replay.
    const post = makePost(() => ok());
    const r = await queue.replay({ stateDir: dir }, post);
    assert.strictEqual(r.sent, 1);
    assert.deepStrictEqual(post.calls.map((c) => c.frame.session_id), ["sess-2"]);
  });

  it("test_age_prune_at_enqueue", function () {
    const dir = tempStateDir();
    const now = Date.now();
    const oldName = writeFrameAt(dir, now - (25 * 3600 * 1000), 0, recordEventFrame(1));
    // enqueue runs enforceBounds → age prune.
    queue.enqueue(dir, recordEventFrame(2));
    assert.ok(!listFiles(dir).includes(oldName), ">24h file pruned at enqueue");
    assert.strictEqual(listFiles(dir).length, 1, "only the fresh enqueue remains");
  });

  it("test_same_ms_same_pid_seq_bump", function () {
    const dir = tempStateDir();
    // Force collisions by stubbing Date.now to a constant.
    const realNow = Date.now;
    const fixed = 1700000000000;
    Date.now = () => fixed;
    try {
      queue.enqueue(dir, recordEventFrame(1));
      queue.enqueue(dir, recordEventFrame(2));
      queue.enqueue(dir, recordEventFrame(3));
    } finally {
      Date.now = realNow;
    }
    const files = listFiles(dir);
    assert.strictEqual(files.length, 3, "all three persist via seq bump");
    // seq increments preserve order.
    const seqs = files.map((f) => f.match(/-(\d{4})\.json$/)[1]);
    assert.deepStrictEqual(seqs, ["0000", "0001", "0002"]);
  });
});

// ── Replay Budget (FR-15) ──────────────────────────────────────────

describe("queue replay budget (FR-15)", function () {
  it("test_replay_caps_32_frames", async function () {
    const dir = tempStateDir();
    const base = Date.now() - 5000;
    for (let i = 0; i < 40; i++) writeFrameAt(dir, base + i, 0, recordEventFrame(i));
    const post = makePost(() => ok());
    const r = await queue.replay({ stateDir: dir }, post);
    assert.strictEqual(r.sent, 32, "exactly 32 replayed");
    assert.strictEqual(post.calls.length, 32);
    assert.strictEqual(listFiles(dir).length, 8, "8 remain for next spawn");
  });

  it("test_replay_caps_256kib", async function () {
    const dir = tempStateDir();
    // Frames ~100 KiB each → 256 KiB cap hits before 32 frames.
    const big = "y".repeat(100 * 1024);
    const base = Date.now() - 5000;
    for (let i = 0; i < 10; i++) {
      writeFrameAt(dir, base + i, 0, { type: "RecordEvent", event_type: "E", session_id: "s" + i, timestamp: 1, payload: big });
    }
    const post = makePost(() => ok());
    const r = await queue.replay({ stateDir: dir }, post);
    // Each file > 100 KiB; cap 256 KiB → 3 frames sent (after 3rd, sentBytes >= cap).
    assert.ok(r.sent <= 3, "byte cap stops early: sent=" + r.sent);
    assert.ok(r.sent >= 2, "at least 2 sent before cap: sent=" + r.sent);
    assert.ok(listFiles(dir).length > 0, "remainder left after byte cap");
  });

  it("test_stop_at_first_failure", async function () {
    const dir = tempStateDir();
    const base = Date.now() - 5000;
    const names = [];
    for (let i = 0; i < 6; i++) names.push(writeFrameAt(dir, base + i, 0, recordEventFrame(i)));
    // succeed 3, then fail.
    const post = makePost((frame, idx) => (idx < 3 ? ok() : fail()));
    const r = await queue.replay({ stateDir: dir }, post);
    assert.strictEqual(r.sent, 3);
    assert.strictEqual(r.stoppedOnFailure, true);
    const after = listFiles(dir);
    assert.strictEqual(after.length, 3, "frames 1-3 deleted, 4-6 remain");
    // The failed file (index 3) is NOT deleted.
    assert.ok(after.includes(names[3]), "failed frame left on disk");
    assert.ok(after.includes(names[4]) && after.includes(names[5]), "remainder left");
  });

  it("test_poison_pill", async function () {
    const dir = tempStateDir();
    const base = Date.now() - 5000;
    writeFrameAt(dir, base, 0, recordEventFrame(1));
    // Corrupt middle frame.
    const dirp = qdir(dir);
    const corruptName = String(base + 1).padStart(13, "0") + "-" + process.pid + "-0000.json";
    fs.writeFileSync(path.join(dirp, corruptName), "{not valid json", { mode: 0o600 });
    writeFrameAt(dir, base + 2, 0, recordEventFrame(3));

    const post = makePost(() => ok());
    const r = await queue.replay({ stateDir: dir }, post);
    assert.strictEqual(r.sent, 2, "two valid frames sent");
    assert.strictEqual(r.stoppedOnFailure, false);
    assert.strictEqual(listFiles(dir).length, 0, "poison pill deleted, queue drained");
    assert.deepStrictEqual(post.calls.map((c) => c.frame.session_id), ["sess-1", "sess-3"]);
  });
});

// ── Concurrency (R-08) ─────────────────────────────────────────────

describe("queue concurrency (R-08)", function () {
  it("test_concurrent_recovering_spawns_double_send", async function () {
    const dir = tempStateDir();
    const base = Date.now() - 5000;
    writeFrameAt(dir, base, 0, recordEventFrame(1));
    // Two spawns read the same frame before either deletes it.
    const post1 = makePost(() => ok());
    const post2 = makePost(() => ok());
    const [r1, r2] = await Promise.all([
      queue.replay({ stateDir: dir }, post1),
      queue.replay({ stateDir: dir }, post2),
    ]);
    // At-most-duplicate delivery; both may report sent=1. Server tolerates dup (Layer 2).
    assert.ok(r1.sent + r2.sent >= 1, "frame delivered at least once");
    assert.strictEqual(listFiles(dir).length, 0, "file removed");
  });
});

// ── Failure Isolation (FR-15 / C-05) ───────────────────────────────

describe("queue failure isolation (FR-15, C-05)", function () {
  it("test_full_disk_swallowed_enqueue", function () {
    // Unwritable queue dir: enqueue must not throw, frame not written.
    if (IS_WINDOWS || IS_ROOT) return; // chmod advisory on Windows; root bypasses
    const dir = tempStateDir();
    const dirp = queue.ensureQueueDir(dir);
    assert.ok(dirp, "queue dir created");
    fs.chmodSync(dirp, 0o500); // read+execute, no write
    try {
      assert.doesNotThrow(() => queue.enqueue(dir, recordEventFrame(1)));
      assert.strictEqual(listFiles(dir).length, 0, "frame not written, no throw");
    } finally {
      fs.chmodSync(dirp, 0o700);
    }
  });

  it("test_replay_read_failure_swallowed", async function () {
    const dir = tempStateDir();
    writeFrameAt(dir, Date.now() - 1000, 0, recordEventFrame(1));
    // post that throws — replay must not propagate.
    const post = async () => {
      throw new Error("transport blew up");
    };
    let r;
    await assert.doesNotReject(async () => {
      r = await queue.replay({ stateDir: dir }, post);
    });
    assert.strictEqual(r.stoppedOnFailure, true, "throw treated as failure");
    assert.strictEqual(listFiles(dir).length, 1, "frame left on disk");
  });

  it("test_null_statedir_noop", async function () {
    assert.doesNotThrow(() => queue.enqueue(null, recordEventFrame(1)));
    assert.doesNotThrow(() => queue.prune(null));
    assert.strictEqual(queue.queueDepth(null), 0);
    const r = await queue.replay({ stateDir: null }, makePost(() => ok()));
    assert.strictEqual(r.sent, 0);
  });

  it("test_queue_dir_missing_recreated", function () {
    const dir = tempStateDir();
    queue.enqueue(dir, recordEventFrame(1));
    assert.strictEqual(listFiles(dir).length, 1);
    // Remove the queue dir between spawns.
    fs.rmSync(qdir(dir), { recursive: true, force: true });
    queue.enqueue(dir, recordEventFrame(2));
    assert.strictEqual(listFiles(dir).length, 1, "queue/ recreated and written");
    if (!IS_WINDOWS && !IS_ROOT) {
      const mode = fs.statSync(qdir(dir)).mode & 0o777;
      assert.strictEqual(mode, 0o700, "recreated dir is 0700");
    }
  });
});

// ── Security Posture (R-16 / FR-16) ────────────────────────────────

describe("queue security posture (R-16, FR-16)", function () {
  it("test_modes", function () {
    if (IS_WINDOWS || IS_ROOT) return;
    const dir = tempStateDir();
    queue.enqueue(dir, recordEventFrame(1));
    const dmode = fs.statSync(qdir(dir)).mode & 0o777;
    assert.strictEqual(dmode, 0o700, "queue dir 0700");
    const f = listFiles(dir)[0];
    const fmode = fs.statSync(path.join(qdir(dir), f)).mode & 0o777;
    assert.strictEqual(fmode, 0o600, "frame file 0600");
  });

  it("test_no_auth_header_in_frames", function () {
    const dir = tempStateDir();
    queue.enqueue(dir, recordEventFrame(1));
    const body = fs.readFileSync(path.join(qdir(dir), listFiles(dir)[0]), "utf8");
    assert.ok(!/authorization/i.test(body), "no Authorization in frame");
    assert.ok(!/bearer/i.test(body), "no Bearer token in frame");
  });

  it("test_distinct_dirs_from_rust_queue", function () {
    const dir = tempStateDir();
    const dirp = queue.queueDir(dir);
    assert.ok(dirp.endsWith(path.join("hook-client", "queue")), "path is hook-client/queue");
    assert.ok(!dirp.includes("event-queue"), "never the Rust event-queue dir");
  });
});

// ── Concrete Assertions ────────────────────────────────────────────

describe("queue concrete assertions", function () {
  it("test_queue_depth_counts_json", function () {
    const dir = tempStateDir();
    assert.strictEqual(queue.queueDepth(dir), 0);
    queue.enqueue(dir, recordEventFrame(1));
    queue.enqueue(dir, recordEventFrame(2));
    assert.strictEqual(queue.queueDepth(dir), 2);
  });

  it("test_enqueue_never_throws_outward", function () {
    assert.doesNotThrow(() => queue.enqueue(undefined, recordEventFrame(1)));
    assert.doesNotThrow(() => queue.enqueue("", recordEventFrame(1)));
    // Circular frame → JSON.stringify throws internally, must be swallowed.
    const circular = { type: "RecordEvent", event_type: "E" };
    circular.self = circular;
    const dir = tempStateDir();
    assert.doesNotThrow(() => queue.enqueue(dir, circular));
    assert.strictEqual(listFiles(dir).length, 0, "unserializable frame not written");
  });
});
