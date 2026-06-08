"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const state = require("../../lib/hook-client/state");

const IS_WINDOWS = process.platform === "win32";
const IS_ROOT = typeof process.getuid === "function" && process.getuid() === 0;

/** Fresh temp state dir per test (stands in for ~/.unimatrix/{hash}/hook-client). */
function tempStateDir() {
  return path.join(
    fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-state-test-")),
    "hook-client"
  );
}

function readHealth(stateDir) {
  return JSON.parse(fs.readFileSync(path.join(stateDir, "health.json"), "utf8"));
}

function okResult() {
  return { ok: true, status: 200, failureClass: null };
}

function failResult(failureClass, status) {
  return { ok: false, status: status || 0, failureClass: failureClass };
}

// ── Breadcrumb accuracy (R-10) ─────────────────────────────────────

describe("state breadcrumb (ADR-005, R-10)", function () {
  it("test_failure_class_matrix", function () {
    const rows = [
      ["connect", 0], // ECONNREFUSED
      ["timeout", 0],
      ["auth", 401],
      ["auth", 403],
      ["http_4xx", 404],
      ["http_4xx", 413],
      ["http_5xx", 500],
    ];
    for (const [cls, status] of rows) {
      const dir = tempStateDir();
      const ok = state.recordSendOutcomes(dir, "host.example", [failResult(cls, status)], 0);
      assert.strictEqual(ok, true);
      assert.strictEqual(readHealth(dir).failure_class, cls, cls + "/" + status);
    }
  });

  it("test_consecutive_failures_counter", function () {
    const dir = tempStateDir();
    for (let i = 1; i <= 3; i++) {
      state.recordSendOutcomes(dir, "h", [failResult("connect")], 0);
      const h = readHealth(dir);
      assert.strictEqual(h.consecutive_failures, i);
      assert.strictEqual(typeof h.last_failure, "number");
      assert.strictEqual(h.last_success, null, "no success yet");
    }
    const before = readHealth(dir);
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    const h = readHealth(dir);
    assert.strictEqual(h.consecutive_failures, 0);
    assert.strictEqual(typeof h.last_success, "number");
    assert.strictEqual(h.last_failure, before.last_failure, "last_failure preserved on success");
  });

  it("test_queue_depth_truthful", function () {
    for (const n of [0, 3, 500]) {
      const dir = tempStateDir();
      const qdir = path.join(dir, "queue");
      fs.mkdirSync(qdir, { recursive: true });
      for (let i = 0; i < n; i++) {
        fs.writeFileSync(path.join(qdir, String(1000000 + i) + "-1-" + i + ".json"), "{}");
      }
      const depth = fs.readdirSync(qdir).length; // caller-computed, as index.js does
      state.recordSendOutcomes(dir, "h", [failResult("http_5xx", 500)], depth);
      assert.strictEqual(readHealth(dir).queue_depth, n);
    }
  });

  it("test_sync_failures_update_breadcrumb", function () {
    // Sync-trio spawns attempt exactly one send; a failure must still land.
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [failResult("timeout")], 0);
    const h = readHealth(dir);
    assert.strictEqual(h.failure_class, "timeout");
    assert.strictEqual(h.consecutive_failures, 1);
  });

  it("test_w4_transition_sequence", function () {
    // W4: healthy → outage (2 failing spawns) → recovery.
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    let h = readHealth(dir);
    assert.strictEqual(h.consecutive_failures, 0);
    assert.strictEqual(h.last_failure, null);
    const firstSuccess = h.last_success;

    state.recordSendOutcomes(dir, "h", [failResult("connect")], 1);
    state.recordSendOutcomes(dir, "h", [failResult("connect")], 2);
    h = readHealth(dir);
    assert.strictEqual(h.consecutive_failures, 2);
    assert.strictEqual(h.failure_class, "connect");
    assert.strictEqual(h.queue_depth, 2);
    assert.strictEqual(h.last_success, firstSuccess, "success timestamp survives outage");

    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    h = readHealth(dir);
    assert.strictEqual(h.consecutive_failures, 0);
    assert.strictEqual(h.queue_depth, 0);
    assert.strictEqual(typeof h.last_success, "number");
    assert.strictEqual(typeof h.last_failure, "number", "failure history retained");
  });

  it("test_carrying_event_class_wins_over_delta", function () {
    // index.js passes carrying-event result first; its class wins when both fail.
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [failResult("http_5xx", 500), failResult("timeout")], 0);
    assert.strictEqual(readHealth(dir).failure_class, "http_5xx");
  });

  it("test_partial_failure_counts_as_failure", function () {
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    const prevSuccess = readHealth(dir).last_success;
    state.recordSendOutcomes(dir, "h", [okResult(), failResult("timeout")], 0);
    const h = readHealth(dir);
    assert.strictEqual(h.consecutive_failures, 1);
    assert.strictEqual(h.failure_class, "timeout");
    assert.strictEqual(h.last_success, prevSuccess, "partial success does not bump last_success");
  });

  it("test_no_attempted_sends_no_write", function () {
    const dir = tempStateDir();
    assert.strictEqual(state.recordSendOutcomes(dir, "h", [], 0), false);
    assert.strictEqual(state.recordSendOutcomes(dir, "h", [null, undefined], 0), false);
    assert.strictEqual(fs.existsSync(path.join(dir, "health.json")), false);
  });

  it("test_content_free", function () {
    // Across the full failure matrix the breadcrumb must never contain the
    // token, payload fragments, transcript bytes, or a full URL (R-16).
    const token = "unit-test-placeholder-token-1";
    const fullUrl = "https://unimatrix.example.com:8443/observe";
    const payloadFragment = "tool_input_secret_value";
    for (const cls of ["auth", "connect", "timeout", "http_4xx", "http_5xx"]) {
      const dir = tempStateDir();
      state.recordSendOutcomes(dir, "unimatrix.example.com:8443", [failResult(cls)], 1);
      const raw = fs.readFileSync(path.join(dir, "health.json"), "utf8");
      assert.ok(!raw.includes(token));
      assert.ok(!raw.includes(fullUrl));
      assert.ok(!raw.includes(payloadFragment));
      assert.ok(!raw.includes("://"), "no URL scheme — host only");
      const h = JSON.parse(raw);
      assert.strictEqual(h.url_host, "unimatrix.example.com:8443");
      assert.deepStrictEqual(
        Object.keys(h).sort(),
        ["consecutive_failures", "failure_class", "last_failure", "last_success", "queue_depth", "stamp_miss", "url_host"]
      );
    }
  });

  it("test_breadcrumb_write_failure_nonfatal", { skip: IS_WINDOWS || IS_ROOT }, function () {
    const dir = tempStateDir();
    fs.mkdirSync(path.join(dir, "offsets"), { recursive: true });
    fs.chmodSync(dir, 0o500); // read-only state dir
    try {
      assert.strictEqual(
        state.recordSendOutcomes(dir, "h", [failResult("connect")], 0),
        false
      );
    } finally {
      fs.chmodSync(dir, 0o700);
    }
  });

  it("test_config_miss_variant_increments_and_classifies", function () {
    const dir = tempStateDir();
    state.writeBreadcrumb(dir, { failureClass: "connect" }); // missing/malformed config
    let h = readHealth(dir);
    assert.strictEqual(h.failure_class, "connect");
    assert.strictEqual(h.consecutive_failures, 1);
    assert.strictEqual(h.last_success, null);
    assert.strictEqual(h.url_host, "", "url_host previous value or empty");
    state.writeBreadcrumb(dir, { failureClass: "auth" }); // partial_env
    h = readHealth(dir);
    assert.strictEqual(h.failure_class, "auth");
    assert.strictEqual(h.consecutive_failures, 2, "perpetual misconfig shows a growing counter");
  });

  it("test_corrupt_breadcrumb_degrades_to_default", function () {
    const dir = tempStateDir();
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, "health.json"), "{not json");
    assert.deepStrictEqual(state.readBreadcrumb(dir), {
      last_success: null,
      last_failure: null,
      failure_class: null,
      consecutive_failures: 0,
      queue_depth: 0,
      url_host: "",
      stamp_miss: 0,
    });
    // Mistyped fields degrade field-by-field.
    fs.writeFileSync(
      path.join(dir, "health.json"),
      JSON.stringify({ consecutive_failures: "9", queue_depth: -1, last_success: 5 })
    );
    const h = state.readBreadcrumb(dir);
    assert.strictEqual(h.consecutive_failures, 0);
    assert.strictEqual(h.queue_depth, 0);
    assert.strictEqual(h.last_success, 5);
  });
});

// ── Atomic writes ──────────────────────────────────────────────────

describe("state atomic writes (ADR-003)", function () {
  let spyWrites, spyRenames, origWrite, origRename;

  beforeEach(function () {
    spyWrites = [];
    spyRenames = [];
    origWrite = fs.writeFileSync;
    origRename = fs.renameSync;
    fs.writeFileSync = function (fp, data, opts) {
      spyWrites.push(String(fp));
      return origWrite.call(fs, fp, data, opts);
    };
    fs.renameSync = function (from, to) {
      spyRenames.push([String(from), String(to)]);
      return origRename.call(fs, from, to);
    };
  });

  afterEach(function () {
    fs.writeFileSync = origWrite;
    fs.renameSync = origRename;
  });

  it("test_offset_write_temp_plus_rename", function () {
    const dir = tempStateDir();
    assert.strictEqual(state.writeOffset(dir, "sess-1", 42), true);
    const finalPath = state.offsetPath(dir, "sess-1");
    assert.strictEqual(spyWrites.length, 1);
    assert.notStrictEqual(spyWrites[0], finalPath, "never writes the final path directly");
    assert.ok(spyWrites[0].includes(".tmp-"), "temp name carries .tmp- marker");
    assert.strictEqual(path.dirname(spyWrites[0]), path.dirname(finalPath), "same dir for rename atomicity");
    assert.deepStrictEqual(spyRenames, [[spyWrites[0], finalPath]]);
    // Hammered writes: a reader between any two sync ops only ever sees full JSON.
    for (let i = 0; i < 50; i++) {
      state.writeOffset(dir, "sess-1", i);
      const parsed = JSON.parse(fs.readFileSync(finalPath, "utf8"));
      assert.strictEqual(parsed.offset, i);
    }
    // No tmp remnants.
    const leftovers = fs.readdirSync(path.dirname(finalPath)).filter((n) => n.includes(".tmp-"));
    assert.deepStrictEqual(leftovers, []);
  });

  it("test_breadcrumb_atomic", function () {
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    const finalPath = state.healthPath(dir);
    assert.strictEqual(spyWrites.length, 1);
    assert.ok(spyWrites[0].includes(".tmp-"));
    assert.strictEqual(spyRenames[0][1], finalPath);
    assert.ok(!spyWrites.includes(finalPath));
  });

  it("test_atomic_write_failure_cleans_tmp", function () {
    const dir = tempStateDir();
    fs.mkdirSync(dir, { recursive: true });
    fs.renameSync = function () {
      const err = new Error("EXDEV: cross-device link");
      err.code = "EXDEV";
      throw err;
    };
    assert.strictEqual(state.atomicWrite(path.join(dir, "x.json"), "{}"), false);
    const leftovers = fs.readdirSync(dir).filter((n) => n.includes(".tmp-"));
    assert.deepStrictEqual(leftovers, [], "tmp unlinked on failure");
  });
});

// ── Offset persistence lifecycle ───────────────────────────────────

describe("state offset lifecycle (ADR-003, FR-16)", function () {
  it("test_offset_file_shape", function () {
    const dir = tempStateDir();
    const before = Math.floor(Date.now() / 1000);
    assert.strictEqual(state.writeOffset(dir, "sess-shape", 1234), true);
    const parsed = JSON.parse(
      fs.readFileSync(path.join(dir, "offsets", "sess-shape.json"), "utf8")
    );
    assert.deepStrictEqual(Object.keys(parsed).sort(), ["offset", "updated"]);
    assert.strictEqual(parsed.offset, 1234);
    assert.ok(parsed.updated >= before && parsed.updated <= Math.floor(Date.now() / 1000) + 1);
    assert.strictEqual(state.readOffset(dir, "sess-shape"), 1234);
  });

  it("test_offset_prune_7days", function () {
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const now = Math.floor(Date.now() / 1000);
    const stale = now - state.OFFSET_PRUNE_SECS - 60;
    const fresh = now - 3600;
    fs.writeFileSync(
      path.join(dir, "offsets", "old-sess.json"),
      JSON.stringify({ offset: 10, updated: stale })
    );
    fs.writeFileSync(
      path.join(dir, "offsets", "new-sess.json"),
      JSON.stringify({ offset: 20, updated: fresh })
    );
    state.pruneOffsets(dir);
    assert.strictEqual(fs.existsSync(path.join(dir, "offsets", "old-sess.json")), false);
    assert.strictEqual(fs.existsSync(path.join(dir, "offsets", "new-sess.json")), true);
    // Pruned-mid-session file → offset 0: safe re-ship (F2 idempotent merge).
    assert.strictEqual(state.readOffset(dir, "old-sess"), 0);
    assert.strictEqual(state.readOffset(dir, "new-sess"), 20);
  });

  it("test_pruneoffsets_deletes_only_files_older_than_7_days", function () {
    // Age-prune is the SOLE effective offset-cleanup mechanism (ADR-006 §2).
    // Boundary: file exactly at the 7-day cutoff is kept (strict `< cutoff`).
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const now = Math.floor(Date.now() / 1000);
    const cases = [
      ["older.json", now - state.OFFSET_PRUNE_SECS - 1, false], // pruned
      ["at-cutoff.json", now - state.OFFSET_PRUNE_SECS, true], // kept (not strictly older)
      ["fresh.json", now - 60, true], // kept
    ];
    for (const [name, updated] of cases) {
      fs.writeFileSync(
        path.join(dir, "offsets", name),
        JSON.stringify({ offset: 1, updated })
      );
    }
    state.pruneOffsets(dir);
    for (const [name, , kept] of cases) {
      assert.strictEqual(
        fs.existsSync(path.join(dir, "offsets", name)),
        kept,
        name
      );
    }
  });

  it("test_pruneoffsets_mtime_fallback_for_unreadable_json", function () {
    // updated unreadable (corrupt/missing field) → mtime decides (state-offset-rekey.md).
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const stalePath = path.join(dir, "offsets", "corrupt-stale.json");
    const freshPath = path.join(dir, "offsets", "corrupt-fresh.json");
    fs.writeFileSync(stalePath, "{not valid json");
    fs.writeFileSync(freshPath, "{not valid json");
    const stale = (Date.now() - (state.OFFSET_PRUNE_SECS + 60) * 1000) / 1000;
    fs.utimesSync(stalePath, stale, stale);
    state.pruneOffsets(dir);
    assert.strictEqual(fs.existsSync(stalePath), false, "stale mtime → pruned");
    assert.strictEqual(fs.existsSync(freshPath), true, "fresh mtime → kept");
  });

  it("test_pruneoffsets_skips_tmp_remnants", function () {
    // Only *.json is considered; .tmp-* atomic-write remnants are skipped.
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const stale = Math.floor(Date.now() / 1000) - state.OFFSET_PRUNE_SECS - 60;
    const tmpPath = path.join(dir, "offsets", "x.json.tmp-1-deadbeef");
    fs.writeFileSync(tmpPath, JSON.stringify({ offset: 1, updated: stale }));
    state.pruneOffsets(dir);
    assert.strictEqual(fs.existsSync(tmpPath), true, ".tmp- remnant untouched by prune");
  });

  it("test_pruneoffsets_mid_session_degrades_to_one_restream", function () {
    // A pruned mid-session offset → next readOffset returns 0 → one full re-stream
    // (idempotent server-side merge, R-04 s4). No error path.
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const stale = Math.floor(Date.now() / 1000) - state.OFFSET_PRUNE_SECS - 60;
    fs.writeFileSync(
      path.join(dir, "offsets", "mid.json"),
      JSON.stringify({ offset: 4242, updated: stale })
    );
    assert.strictEqual(state.readOffset(dir, "mid"), 4242);
    state.pruneOffsets(dir);
    assert.strictEqual(state.readOffset(dir, "mid"), 0, "degrades to 0, safe re-ship");
  });

  it("test_pruneoffsets_fail_open", function () {
    // Unreadable/missing offsets dir → no-op, no throw (fail-open, R-14 s2 / R-04 s4).
    // ENOENT: offsets dir never created.
    const dir = tempStateDir();
    assert.strictEqual(fs.existsSync(path.join(dir, "offsets")), false);
    assert.doesNotThrow(() => state.pruneOffsets(dir));
    // Empty offsets dir → no-op, no throw.
    state.ensureStateDir(dir);
    assert.doesNotThrow(() => state.pruneOffsets(dir));
    assert.deepStrictEqual(fs.readdirSync(path.join(dir, "offsets")), []);
    // readdir throws (EACCES) → swallowed best-effort.
    const origReaddir = fs.readdirSync;
    fs.readdirSync = function () {
      const err = new Error("EACCES: permission denied");
      err.code = "EACCES";
      throw err;
    };
    try {
      assert.doesNotThrow(() => state.pruneOffsets(dir));
    } finally {
      fs.readdirSync = origReaddir;
    }
  });

  it("test_pruneoffsets_unlink_error_best_effort", function () {
    // A stale file whose unlink fails must not throw; prune continues over siblings.
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const stale = Math.floor(Date.now() / 1000) - state.OFFSET_PRUNE_SECS - 60;
    fs.writeFileSync(
      path.join(dir, "offsets", "a.json"),
      JSON.stringify({ offset: 1, updated: stale })
    );
    fs.writeFileSync(
      path.join(dir, "offsets", "b.json"),
      JSON.stringify({ offset: 2, updated: stale })
    );
    const origUnlink = fs.unlinkSync;
    let calls = 0;
    fs.unlinkSync = function (p) {
      calls += 1;
      if (calls === 1) {
        const err = new Error("EBUSY: resource busy");
        err.code = "EBUSY";
        throw err;
      }
      return origUnlink.call(fs, p);
    };
    try {
      assert.doesNotThrow(() => state.pruneOffsets(dir));
    } finally {
      fs.unlinkSync = origUnlink;
    }
    assert.strictEqual(calls, 2, "prune attempted both stale files despite first failure");
  });

  it("test_delete_offset_unlinks_fail_open", function () {
    // ADR-006 (amended): deleteOffset itself is event-agnostic — it just unlinks
    // fail-open. The TaskCompleted-vs-Stop keying lives in index.js (Wave 5), NOT
    // here; see index.test.js for the keying-discrimination assertions. This test
    // pins only the file-level contract this module owns.
    const dir = tempStateDir();
    state.writeOffset(dir, "sess-del", 99);
    const fp = state.offsetPath(dir, "sess-del");
    assert.ok(fs.existsSync(fp));
    assert.strictEqual(state.deleteOffset(dir, "sess-del"), true);
    assert.strictEqual(fs.existsSync(fp), false);
    // Repeat delete (missing file) is a safe no-op returning false; never throws.
    assert.strictEqual(state.deleteOffset(dir, "sess-del"), false);
    assert.doesNotThrow(() => state.deleteOffset(dir, "never-existed"));
  });

  it("test_offset_corruption_reads_zero", function () {
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const cases = [
      ['{"offset":"x","updated":1}', "str"],
      ['{"offset":-5,"updated":1}', "neg"],
      ['{"offset":1.5,"updated":1}', "float"],
      ['{"offset": 12', "truncated"],
      ['{"offset":9007199254740993,"updated":1}', "unsafe"],
      ["[]", "array"],
      ["", "empty"],
    ];
    for (const [content, key] of cases) {
      fs.writeFileSync(path.join(dir, "offsets", key + ".json"), content);
      assert.strictEqual(state.readOffset(dir, key), 0, key);
    }
    assert.strictEqual(state.readOffset(dir, "missing-file"), 0);
  });
});

// ── Session-key sanitization (R-19 / security table) ───────────────

describe("state session-key sanitization (ADR-003, R-19)", function () {
  it("test_key_passthrough", function () {
    for (const id of ["abc", "A-b_9", "ppid-12345", "x".repeat(64)]) {
      assert.strictEqual(state.sanitizeSessionKey(id), id);
    }
    // ppid-collision keys (R-19): same key → same offset file (Rust-parity, documented).
    const dir = tempStateDir();
    state.writeOffset(dir, "ppid-777", 5);
    assert.strictEqual(state.readOffset(dir, "ppid-777"), 5);
    assert.strictEqual(
      state.offsetPath(dir, "ppid-777"),
      state.offsetPath(dir, "ppid-777")
    );
  });

  it("test_key_hashed_otherwise", function () {
    const dir = tempStateDir();
    const offsetsRoot = path.resolve(dir, "offsets") + path.sep;
    const corpus = [
      "../../etc/passwd",
      "/etc/passwd",
      "a/b",
      "..",
      "x\0y",
      "x".repeat(65),
      "héllo-wörld",
      "sess id with spaces",
      "",
    ];
    for (const id of corpus) {
      const key = state.sanitizeSessionKey(id);
      assert.match(key, /^[a-f0-9]{16}$/, JSON.stringify(id));
      const expected = crypto.createHash("sha256").update(id, "utf8").digest("hex").slice(0, 16);
      assert.strictEqual(key, expected);
      // Resolved path stays inside offsets/ (path-traversal closure).
      const resolved = path.resolve(state.offsetPath(dir, id));
      assert.ok(resolved.startsWith(offsetsRoot), resolved);
    }
    // Idempotent: a hashed key passes through unchanged.
    const once = state.sanitizeSessionKey("../../etc/passwd");
    assert.strictEqual(state.sanitizeSessionKey(once), once);
  });
});

// ── State dir creation + degraded modes (R-14) ─────────────────────

describe("state dir creation and degradation (R-14)", function () {
  it("test_dir_modes", { skip: IS_WINDOWS }, function () {
    const dir = tempStateDir();
    assert.strictEqual(state.ensureStateDir(dir), true);
    assert.strictEqual(fs.statSync(dir).mode & 0o777, 0o700);
    assert.strictEqual(fs.statSync(path.join(dir, "offsets")).mode & 0o777, 0o700);
    state.writeOffset(dir, "sess-mode", 1);
    assert.strictEqual(
      fs.statSync(state.offsetPath(dir, "sess-mode")).mode & 0o777,
      0o600
    );
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    assert.strictEqual(fs.statSync(state.healthPath(dir)).mode & 0o777, 0o600);
  });

  it("test_no_home_env", function () {
    // config.js yields stateDir === null when HOME/USERPROFILE are unset;
    // every state function must degrade to a no-op without throwing.
    for (const nullDir of [null, undefined, ""]) {
      assert.strictEqual(state.ensureStateDir(nullDir), false);
      assert.strictEqual(state.readOffset(nullDir, "s"), 0);
      assert.strictEqual(state.writeOffset(nullDir, "s", 1), false);
      assert.strictEqual(state.deleteOffset(nullDir, "s"), false);
      assert.doesNotThrow(() => state.pruneOffsets(nullDir));
      assert.strictEqual(state.recordSendOutcomes(nullDir, "h", [okResult()], 0), false);
      assert.strictEqual(state.writeBreadcrumb(nullDir, { failureClass: "connect" }), false);
      assert.deepStrictEqual(state.readBreadcrumb(nullDir).consecutive_failures, 0);
    }
  });

  it("test_full_disk_all_writes_fail", function () {
    const dir = tempStateDir();
    state.ensureStateDir(dir);
    const origWrite = fs.writeFileSync;
    fs.writeFileSync = function () {
      const err = new Error("ENOSPC: no space left on device");
      err.code = "ENOSPC";
      throw err;
    };
    try {
      assert.strictEqual(state.writeOffset(dir, "s", 1), false);
      assert.strictEqual(state.recordSendOutcomes(dir, "h", [failResult("connect")], 0), false);
      assert.strictEqual(state.writeBreadcrumb(dir, { failureClass: "connect" }), false);
      assert.strictEqual(state.atomicWrite(path.join(dir, "x.json"), "{}"), false);
    } finally {
      fs.writeFileSync = origWrite;
    }
  });

  it("test_ensure_state_dir_failure_returns_false", { skip: IS_WINDOWS || IS_ROOT }, function () {
    const parent = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-state-ro-"));
    fs.chmodSync(parent, 0o500);
    try {
      assert.strictEqual(state.ensureStateDir(path.join(parent, "hook-client")), false);
    } finally {
      fs.chmodSync(parent, 0o700);
    }
  });
});
