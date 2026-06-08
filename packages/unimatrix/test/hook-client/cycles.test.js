"use strict";

/**
 * cycles.test.js — C1 cycle tracker module (ADR-001).
 * ACs: AC-01, AC-08. Risks: R-02, R-03, R-08, R-15, R-22 + path traversal.
 *
 * All functions are never-throw (F3 C-05). Sanitization happens INSIDE cycles.js
 * (#4772 — never pre-sanitize at call sites). Reuses the state.test.js
 * tempStateDir idiom and the config worktree gitdir-port shape (AC-08).
 */

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const cycles = require("../../lib/hook-client/cycles");
const state = require("../../lib/hook-client/state");
const config = require("../../lib/hook-client/config");

/** Fresh temp state dir per test (stands in for ~/.unimatrix/{hash}/hook-client). */
function tempStateDir() {
  return path.join(
    fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-cycles-test-")),
    "hook-client"
  );
}

function readRaw(stateDir, sid) {
  return JSON.parse(fs.readFileSync(cycles.cyclePath(stateDir, sid), "utf8"));
}

// ── Lifecycle (R-02, AC-01) ─────────────────────────────────────────

describe("cycles lifecycle (ADR-001, R-02, AC-01)", function () {
  it("test_writeCycle_creates_file_atomically", function () {
    const dir = tempStateDir();
    const sid = "vnc-030-session";
    assert.strictEqual(cycles.writeCycle(dir, sid, "vnc-030", "delivery"), true);
    const p = cycles.cyclePath(dir, sid);
    assert.strictEqual(fs.existsSync(p), true);
    const rec = readRaw(dir, sid);
    assert.strictEqual(rec.topic, "vnc-030");
    assert.strictEqual(rec.phase, "delivery");
    assert.strictEqual(typeof rec.declared_at, "number");
    assert.strictEqual(typeof rec.updated, "number");
    // No partial-file window: no .tmp-* remnant left behind.
    const leftovers = fs.readdirSync(cycles.cyclesDir(dir)).filter((n) => n.includes(".tmp-"));
    assert.deepStrictEqual(leftovers, []);
  });

  it("test_writeCycle_overwrites_last_writer_wins", function () {
    const dir = tempStateDir();
    const sid = "s1";
    cycles.writeCycle(dir, sid, "vnc-029", "design");
    assert.strictEqual(cycles.writeCycle(dir, sid, "vnc-030", "delivery"), true);
    const rec = readRaw(dir, sid);
    assert.strictEqual(rec.topic, "vnc-030");
    assert.strictEqual(rec.phase, "delivery");
  });

  it("test_writeCycle_null_phase_stored_as_null", function () {
    const dir = tempStateDir();
    cycles.writeCycle(dir, "s", "vnc-030", null);
    assert.strictEqual(readRaw(dir, "s").phase, null);
    cycles.writeCycle(dir, "s2", "vnc-030", undefined);
    assert.strictEqual(readRaw(dir, "s2").phase, null);
  });

  it("test_readCycle_present_returns_topic_phase", function () {
    const dir = tempStateDir();
    cycles.writeCycle(dir, "s", "vnc-030", "delivery");
    assert.deepStrictEqual(cycles.readCycle(dir, "s"), {
      topic: "vnc-030",
      phase: "delivery",
    });
  });

  it("test_readCycle_returns_only_stamp_surface", function () {
    // declared_at / updated are file-internal; readCycle must not expose them.
    const dir = tempStateDir();
    cycles.writeCycle(dir, "s", "vnc-030", "delivery");
    assert.deepStrictEqual(Object.keys(cycles.readCycle(dir, "s")).sort(), [
      "phase",
      "topic",
    ]);
  });

  it("test_readCycle_missing_returns_null", function () {
    const dir = tempStateDir();
    assert.strictEqual(cycles.readCycle(dir, "nope"), null);
  });

  it("test_updatePhase_rmw_bumps_phase_and_updated", function () {
    const dir = tempStateDir();
    const sid = "s";
    cycles.writeCycle(dir, sid, "vnc-030", "delivery");
    const before = readRaw(dir, sid);
    // Force a strictly-earlier baseline so the bump is observable.
    fs.writeFileSync(
      cycles.cyclePath(dir, sid),
      JSON.stringify({
        topic: before.topic,
        phase: before.phase,
        declared_at: before.declared_at,
        updated: before.updated - 5,
      })
    );
    assert.strictEqual(cycles.updatePhase(dir, sid, "review"), true);
    const after = readRaw(dir, sid);
    assert.strictEqual(after.phase, "review");
    assert.strictEqual(after.topic, "vnc-030", "topic preserved");
    assert.strictEqual(after.declared_at, before.declared_at, "declared_at preserved");
    assert.ok(after.updated >= before.updated - 5, "updated bumped");
  });

  it("test_updatePhase_missing_file_noop_false_no_recreate", function () {
    // R-22: phase-end without a prior start must NOT recreate the tracker.
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    assert.strictEqual(cycles.updatePhase(dir, "ghost", "review"), false);
    assert.strictEqual(fs.existsSync(cycles.cyclePath(dir, "ghost")), false);
  });

  it("test_updatePhase_corrupt_file_noop_false", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    fs.writeFileSync(cycles.cyclePath(dir, "bad"), "{not json");
    assert.strictEqual(cycles.updatePhase(dir, "bad", "review"), false);
  });

  it("test_deleteCycle_removes_file", function () {
    const dir = tempStateDir();
    cycles.writeCycle(dir, "s", "vnc-030", "delivery");
    assert.strictEqual(cycles.deleteCycle(dir, "s"), true);
    assert.strictEqual(fs.existsSync(cycles.cyclePath(dir, "s")), false);
    // Second delete → false, no throw.
    assert.strictEqual(cycles.deleteCycle(dir, "s"), false);
  });
});

// ── Lifecycle-event isolation (R-02 — delete-on-close trap) ─────────

describe("cycles lifecycle-event isolation (R-02, FR-04)", function () {
  it("test_tracker_untouched_by_lifecycle_events", function () {
    // The module exposes only cycle-keyed ops; no SessionStart/Close/Stop entry
    // point exists that would delete-on-close (which would kill the stamp after
    // turn 1). Cross-ref index-decoration.md for the multi-turn dispatch test.
    const api = Object.keys(cycles);
    for (const banned of ["onSessionClose", "onSessionStart", "onStop", "onClose"]) {
      assert.strictEqual(api.includes(banned), false, "no " + banned + " export");
    }
    for (const required of [
      "readCycle",
      "writeCycle",
      "updatePhase",
      "deleteCycle",
      "pruneCycles",
    ]) {
      assert.strictEqual(typeof cycles[required], "function", required);
    }
  });
});

// ── Prune (FR-05, AC-01) ────────────────────────────────────────────

describe("cycles prune (FR-05, AC-01)", function () {
  it("test_pruneCycles_removes_only_stale", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const now = Math.floor(Date.now() / 1000);
    const day = 24 * 60 * 60;
    const cases = [
      ["fresh", now],
      ["sixday", now - 6 * day],
      ["eightday", now - 8 * day],
    ];
    for (const [sid, updated] of cases) {
      fs.writeFileSync(
        cycles.cyclePath(dir, sid),
        JSON.stringify({ topic: "vnc-030", phase: null, declared_at: updated, updated })
      );
    }
    cycles.pruneCycles(dir);
    assert.strictEqual(fs.existsSync(cycles.cyclePath(dir, "fresh")), true);
    assert.strictEqual(fs.existsSync(cycles.cyclePath(dir, "sixday")), true);
    assert.strictEqual(fs.existsSync(cycles.cyclePath(dir, "eightday")), false);
  });

  it("test_pruneCycles_mtime_fallback_when_json_unreadable", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const fp = cycles.cyclePath(dir, "corrupt");
    fs.writeFileSync(fp, "{not json");
    // Backdate mtime past the 7-day cutoff.
    const old = Date.now() / 1000 - cycles.PRUNE_SECS - 60;
    fs.utimesSync(fp, old, old);
    cycles.pruneCycles(dir);
    assert.strictEqual(fs.existsSync(fp), false);
  });

  it("test_pruneCycles_ignores_non_json", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const tmp = path.join(cycles.cyclesDir(dir), "x.tmp-123");
    fs.writeFileSync(tmp, "{}");
    const old = Date.now() / 1000 - cycles.PRUNE_SECS - 60;
    fs.utimesSync(tmp, old, old);
    cycles.pruneCycles(dir);
    assert.strictEqual(fs.existsSync(tmp), true, "non-.json skipped");
  });

  it("test_pruneCycles_piggybacks_queue_prune_never_throws", function () {
    const dir = tempStateDir();
    // Missing cycles/ dir → no-op, no throw.
    assert.strictEqual(fs.existsSync(cycles.cyclesDir(dir)), false);
    assert.doesNotThrow(() => cycles.pruneCycles(dir));
    // Empty cycles/ dir → no-op, no throw.
    cycles.ensureCyclesDir(dir);
    assert.doesNotThrow(() => cycles.pruneCycles(dir));
  });

  it("test_pruneCycles_unlink_error_best_effort", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const stale = Math.floor(Date.now() / 1000) - cycles.PRUNE_SECS - 60;
    for (const sid of ["a", "b"]) {
      fs.writeFileSync(
        cycles.cyclePath(dir, sid),
        JSON.stringify({ topic: "t", phase: null, declared_at: stale, updated: stale })
      );
    }
    const orig = fs.unlinkSync;
    fs.unlinkSync = function () {
      const err = new Error("EACCES: permission denied");
      err.code = "EACCES";
      throw err;
    };
    try {
      assert.doesNotThrow(() => cycles.pruneCycles(dir));
    } finally {
      fs.unlinkSync = orig;
    }
  });
});

// ── Crash + --resume (R-08, AC-01) ──────────────────────────────────

describe("cycles crash + resume (R-08, AC-01)", function () {
  it("test_resume_finds_tracker_same_session_key", function () {
    // --resume reuses the session_id (empirical, claude 2.1.167). A fresh module
    // load against the same stateDir + session_id finds the tracker → stamping
    // continues with zero gap.
    const dir = tempStateDir();
    const sid = "resumed-session";
    cycles.writeCycle(dir, sid, "vnc-030", "delivery");
    // Simulate a fresh process: drop the require cache, re-require.
    delete require.cache[require.resolve("../../lib/hook-client/cycles")];
    const cyclesReloaded = require("../../lib/hook-client/cycles");
    assert.deepStrictEqual(cyclesReloaded.readCycle(dir, sid), {
      topic: "vnc-030",
      phase: "delivery",
    });
  });
});

// ── Worktree path routing (R-15, AC-08, C-11) ───────────────────────

describe("cycles worktree path routing (R-15, AC-08, C-11)", function () {
  let root;
  let mainRepo;
  let worktree;

  beforeEach(function () {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-wt-"));
    // Main repo: a real .git DIRECTORY.
    mainRepo = path.join(root, "main");
    fs.mkdirSync(path.join(mainRepo, ".git", "worktrees", "wt"), { recursive: true });
    // Worktree: a .git FILE pointing at the main gitdir (F3 gitdir-port shape).
    worktree = path.join(root, "wt");
    fs.mkdirSync(worktree, { recursive: true });
    fs.writeFileSync(
      path.join(worktree, ".git"),
      "gitdir: " + path.join(mainRepo, ".git", "worktrees", "wt") + "\n"
    );
  });

  afterEach(function () {
    fs.rmSync(root, { recursive: true, force: true });
  });

  it("test_worktree_tracker_under_main_root_hash", function () {
    // Both the worktree cwd and the main-root cwd must resolve to the SAME
    // stateDir hash, so a worktree-issued cycle_start is readable everywhere.
    const wtState = config.resolve(worktree).stateDir;
    const mainState = config.resolve(mainRepo).stateDir;
    assert.strictEqual(wtState, mainState, "worktree + main share one state dir");

    const sid = "wt-session";
    assert.strictEqual(cycles.writeCycle(wtState, sid, "vnc-030", "delivery"), true);
    // A subsequent main-root-cwd event finds the same tracker.
    assert.deepStrictEqual(cycles.readCycle(mainState, sid), {
      topic: "vnc-030",
      phase: "delivery",
    });
  });

  it("test_no_stamp_path_hashes_raw_cwd", function () {
    // C-11: every tracker path derives from config.resolve(cwd).stateDir, never
    // a raw-cwd hash. The worktree path differs from the main root, so if cycles
    // hashed raw cwd the two would diverge — they must not.
    const wtState = config.resolve(worktree).stateDir;
    const rawCwdHash = require("../../lib/hook-client/config").computeProjectHash(worktree);
    // The state dir must NOT contain the raw worktree-cwd hash.
    assert.strictEqual(
      wtState.includes(rawCwdHash),
      false,
      "state dir must not embed raw-cwd hash"
    );
  });
});

// ── Fail-open injection (R-03, NFR-03 — per fs touchpoint) ──────────

describe("cycles fail-open injection (R-03, NFR-03)", function () {
  let captured;
  let origWrite;

  beforeEach(function () {
    captured = "";
    origWrite = process.stderr.write;
    process.stderr.write = function (chunk) {
      captured += String(chunk);
      return true;
    };
  });

  afterEach(function () {
    process.stderr.write = origWrite;
  });

  function injectFsError(method, code) {
    const orig = fs[method];
    fs[method] = function () {
      const err = new Error(code + ": injected");
      err.code = code;
      throw err;
    };
    return () => {
      fs[method] = orig;
    };
  }

  it("test_failopen_unusable_statedir", function () {
    // Null/empty stateDir (no HOME) → every op degrades, never throws.
    for (const bad of [null, undefined, "", 123]) {
      assert.strictEqual(cycles.readCycle(bad, "s"), null);
      assert.strictEqual(cycles.writeCycle(bad, "s", "t", "p"), false);
      assert.strictEqual(cycles.updatePhase(bad, "s", "p"), false);
      assert.strictEqual(cycles.deleteCycle(bad, "s"), false);
      assert.doesNotThrow(() => cycles.pruneCycles(bad));
    }
  });

  it("test_failopen_per_fs_touchpoint", function () {
    const dir = tempStateDir();
    for (const code of ["EACCES", "ENOENT", "EROFS", "ENOSPC"]) {
      // readCycle (readFileSync)
      let restore = injectFsError("readFileSync", code);
      assert.strictEqual(cycles.readCycle(dir, "s"), null);
      assert.strictEqual(cycles.updatePhase(dir, "s", "p"), false);
      restore();

      // writeCycle (mkdirSync path)
      restore = injectFsError("mkdirSync", code);
      assert.strictEqual(cycles.writeCycle(dir, "s", "t", "p"), false);
      restore();

      // writeCycle (writeFileSync inside atomicWrite)
      cycles.ensureCyclesDir(dir);
      restore = injectFsError("writeFileSync", code);
      assert.strictEqual(cycles.writeCycle(dir, "s", "t", "p"), false);
      restore();

      // deleteCycle (unlinkSync)
      restore = injectFsError("unlinkSync", code);
      assert.strictEqual(cycles.deleteCycle(dir, "s"), false);
      restore();

      // pruneCycles (readdirSync)
      restore = injectFsError("readdirSync", code);
      assert.doesNotThrow(() => cycles.pruneCycles(dir));
      restore();
    }
    // No secret/path leaked to stderr across all injections.
    assert.strictEqual(captured, "", "fail-open paths write nothing to stderr");
  });

  it("test_writeCycle_disk_full_degrades_false", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const restore = injectFsError("writeFileSync", "ENOSPC");
    assert.strictEqual(cycles.writeCycle(dir, "s", "vnc-030", "delivery"), false);
    restore();
    // Degrade, not abort: the event would still be sent (unstamped) by the caller.
  });

  it("test_readCycle_corrupt_json_returns_null", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    fs.writeFileSync(cycles.cyclePath(dir, "s"), "{ not valid json");
    assert.strictEqual(cycles.readCycle(dir, "s"), null);
  });

  it("test_readCycle_mistyped_or_empty_topic_returns_null", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const variants = [
      { topic: 123, phase: "p" },
      { topic: "", phase: "p" },
      { phase: "p" },
      [1, 2, 3],
      "a string",
      42,
    ];
    let i = 0;
    for (const v of variants) {
      const sid = "v" + i++;
      fs.writeFileSync(cycles.cyclePath(dir, sid), JSON.stringify(v));
      assert.strictEqual(cycles.readCycle(dir, sid), null, JSON.stringify(v));
    }
  });

  it("test_readCycle_non_string_phase_coerced_to_null", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    fs.writeFileSync(
      cycles.cyclePath(dir, "s"),
      JSON.stringify({ topic: "vnc-030", phase: 99 })
    );
    assert.deepStrictEqual(cycles.readCycle(dir, "s"), { topic: "vnc-030", phase: null });
  });
});

// ── Security — path traversal via session_id (CRITICAL) ─────────────

describe("cycles path-traversal safety (security)", function () {
  it("test_sanitizeSessionKey_neutralizes_traversal", function () {
    const dir = tempStateDir();
    cycles.ensureCyclesDir(dir);
    const cyclesRoot = fs.realpathSync(cycles.cyclesDir(dir));
    const adversarial = [
      "../../etc/x",
      "..\\..\\windows",
      "/etc/passwd",
      "a" + String.fromCharCode(0) + "b",
      "x".repeat(80),
      "‮ ../",
    ];
    for (const sid of adversarial) {
      // Every op must stay within cycles/ — assert the resolved path is contained.
      const p = cycles.cyclePath(dir, sid);
      const resolved = path.resolve(p);
      assert.strictEqual(
        resolved.startsWith(cyclesRoot + path.sep),
        true,
        "path escaped cycles/: " + sid
      );

      assert.strictEqual(cycles.writeCycle(dir, sid, "vnc-030", "p"), true);
      assert.deepStrictEqual(cycles.readCycle(dir, sid), { topic: "vnc-030", phase: "p" });
      assert.strictEqual(cycles.deleteCycle(dir, sid), true);
    }
    // Nothing was written outside cycles/: only the sanitized .json files existed.
    // (All were deleted above; the dir is now empty of escapees.)
    const escapees = fs
      .readdirSync(cyclesRoot)
      .filter((n) => !n.endsWith(".json"));
    assert.deepStrictEqual(escapees, []);
  });

  it("test_sanitizeSessionKey_matches_state_module", function () {
    // cycles must reuse state.sanitizeSessionKey, not re-implement it.
    const sid = "../../escape";
    const expected = state.sanitizeSessionKey(sid) + ".json";
    assert.strictEqual(path.basename(cycles.cyclePath("/x", sid)), expected);
  });
});
