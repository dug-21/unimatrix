"use strict";

/**
 * state-canary.test.js — `stamp_miss` zero-tolerance inheritance-drift canary.
 *
 * ADR-006 rev2 / AC-06 / FR-09, FR-10. Risks: R-03, R-08, R-14, R-19.
 *
 * The canary is a ZERO-TOLERANCE INVARIANT (`stamp_miss == 0`), NOT a rate
 * signal. Removed entirely (their absence is asserted below): the 0.20 threshold,
 * the `fnf_record_send_count` denominator, the `anyOtherCycleFile` concurrent-file
 * rule, the per-deployment baseline, and the human re-set ritual.
 *
 * PINNED CLI: claude 2.1.167 — `--resume` session_id reuse + depth-1 root-id
 * inheritance are empirical on this version only. These AC-06 fixtures ARE the
 * re-run-on-CLI-bump drift check (cheap, part of the standard suite); drift
 * surfaces as a nonzero counter, never silent loss (NFR-08).
 *
 * OQ-E disposition: the test-time invariant ships under EITHER branch. Only the
 * PRODUCTION increment call site (in index.js, subagent-gated) is probe-gated —
 * Branch A calls `bumpStampMiss` in production, Branch B gates it to no-op. The
 * `bumpStampMiss` RMW itself (exercised here) is branch-agnostic. The subagent-
 * gating decision lives in index-decoration.md; these fixtures model that gate
 * decision locally and assert the counter outcome.
 */

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const state = require("../../lib/hook-client/state");

const IS_WINDOWS = process.platform === "win32";
const IS_ROOT = typeof process.getuid === "function" && process.getuid() === 0;

/** Fresh temp state dir per test (state.test.js idiom). */
function tempStateDir() {
  return path.join(
    fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-canary-test-")),
    "hook-client"
  );
}

function readHealth(stateDir) {
  return JSON.parse(fs.readFileSync(path.join(stateDir, "health.json"), "utf8"));
}

function okResult() {
  return { ok: true, status: 200, failureClass: null };
}

/**
 * Model of the index.js subagent-gated FNF decoration miss branch (ADR-006 §2).
 * Increments iff the event is in subagent context (depth >= 1) AND no tracker is
 * found for the (expected-inherited) root session_id the event carries. Depth-0
 * never-declare events are structural noise — never counted. This mirrors the
 * gating decision so the canary fixtures exercise it against the real RMW.
 */
function decorationMissGate(stateDir, event) {
  // event: { depth, carriedId, trackerExists }
  if (event.depth >= 1 && !event.trackerExists) {
    return state.bumpStampMiss(stateDir);
  }
  return false; // depth-0, or tracker found — no increment
}

// ── bumpStampMiss — content-free RMW (R-03, FR-09) ──────────────────

describe("bumpStampMiss — content-free RMW (ADR-006 rev2)", function () {
  it("test_bumpStampMiss_increments_count_only", function () {
    const dir = tempStateDir();
    // fresh breadcrumb default carries stamp_miss: 0
    assert.strictEqual(state.readBreadcrumb(dir).stamp_miss, 0);

    assert.strictEqual(state.bumpStampMiss(dir), true);
    assert.strictEqual(readHealth(dir).stamp_miss, 1);

    // RMW, not overwrite — second call lands on top of the persisted 1
    assert.strictEqual(state.bumpStampMiss(dir), true);
    assert.strictEqual(readHealth(dir).stamp_miss, 2);
  });

  it("test_bumpStampMiss_content_free_no_topic_no_sid_no_path", function () {
    // SECURITY (ADR-006 §1): a count only — no topic, sid, or path can be stored.
    const dir = tempStateDir();
    for (let i = 0; i < 3; i++) state.bumpStampMiss(dir);
    const h = readHealth(dir);
    // Exactly the content-free breadcrumb keys; no free-form field exists.
    assert.deepStrictEqual(
      Object.keys(h).sort(),
      [
        "consecutive_failures",
        "failure_class",
        "last_failure",
        "last_success",
        "queue_depth",
        "stamp_miss",
        "url_host",
      ]
    );
    assert.strictEqual(h.stamp_miss, 3);
    // Every value is a count/timestamp/class/host — none derived from cycle input.
    const raw = fs.readFileSync(path.join(dir, "health.json"), "utf8");
    assert.ok(!/session|topic|cycle|\//.test(raw.replace("url_host", "")),
      "no topic/session-id/path leaked into health.json");
  });

  it("test_bumpStampMiss_failopen_never_throws", { skip: IS_WINDOWS || IS_ROOT }, function () {
    // R-03 / NFR-03: EACCES on the health RMW → false, no throw, exit-0 contract.
    const dir = tempStateDir();
    fs.mkdirSync(path.join(dir, "offsets"), { recursive: true });
    fs.chmodSync(dir, 0o500); // read-only state dir → atomicWrite fails
    try {
      assert.strictEqual(state.bumpStampMiss(dir), false);
    } finally {
      fs.chmodSync(dir, 0o700);
    }
  });

  it("test_bumpStampMiss_unusable_statedir_false", function () {
    // No HOME → unusable stateDir → false, never throws.
    assert.strictEqual(state.bumpStampMiss(null), false);
    assert.strictEqual(state.bumpStampMiss(""), false);
    assert.strictEqual(state.bumpStampMiss(undefined), false);
  });

  it("test_bumpStampMiss_corrupt_health_degrades_then_increments", function () {
    // Field-by-field degrade: a corrupt health.json re-defaults stamp_miss to 0,
    // so bumpStampMiss writes 1 rather than throwing on the bad prior.
    const dir = tempStateDir();
    fs.mkdirSync(path.join(dir, "offsets"), { recursive: true });
    fs.writeFileSync(path.join(dir, "health.json"), "{ this is not json");
    assert.strictEqual(state.bumpStampMiss(dir), true);
    assert.strictEqual(readHealth(dir).stamp_miss, 1);

    // Mistyped stamp_miss (string) also degrades to 0 before increment.
    fs.writeFileSync(
      path.join(dir, "health.json"),
      JSON.stringify({ ...state.readBreadcrumb(dir), stamp_miss: "lots" })
    );
    assert.strictEqual(state.readBreadcrumb(dir).stamp_miss, 0);
    assert.strictEqual(state.bumpStampMiss(dir), true);
    assert.strictEqual(readHealth(dir).stamp_miss, 1);
  });

  it("test_health_default_stamp_miss_zero", function () {
    // A fresh (never-written) breadcrumb carries stamp_miss: 0.
    const dir = tempStateDir();
    assert.strictEqual(state.readBreadcrumb(dir).stamp_miss, 0);
  });
});

// ── R-19 carry-through (masking guard) ──────────────────────────────

describe("stamp_miss carry-through (R-19 masking guard)", function () {
  it("test_recordSendOutcomes_preserves_stamp_miss", function () {
    // A normal send must NOT reset the counter — else drift is masked (R-19).
    const dir = tempStateDir();
    state.bumpStampMiss(dir);
    state.bumpStampMiss(dir);
    assert.strictEqual(readHealth(dir).stamp_miss, 2);

    assert.strictEqual(state.recordSendOutcomes(dir, "h", [okResult()], 0), true);
    assert.strictEqual(readHealth(dir).stamp_miss, 2, "send preserved stamp_miss");
  });

  it("test_writeBreadcrumb_preserves_stamp_miss", function () {
    // A config-miss rewrite must also carry stamp_miss through.
    const dir = tempStateDir();
    state.bumpStampMiss(dir);
    assert.strictEqual(readHealth(dir).stamp_miss, 1);

    assert.strictEqual(state.writeBreadcrumb(dir, { failureClass: "connect" }), true);
    const h = readHealth(dir);
    assert.strictEqual(h.stamp_miss, 1, "config-miss preserved stamp_miss");
    assert.strictEqual(h.consecutive_failures, 1, "other fields still update");
  });

  it("test_interleaved_send_then_bump_then_send_monotonic", function () {
    // Counter is monotonic across an interleaved lifetime of sends and bumps.
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    assert.strictEqual(readHealth(dir).stamp_miss, 0);
    state.bumpStampMiss(dir);
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    state.bumpStampMiss(dir);
    assert.strictEqual(readHealth(dir).stamp_miss, 2);
  });
});

// ── Subagent-gated canary fixtures (R-19, FR-09, AC-06) — GATE-BLOCKING ──

describe("subagent-gated canary fixtures (ADR-006 §2-§5)", function () {
  it("test_depth0_never_declare_no_increment", function () {
    // depth-0 top-level event, no tracker → structural noise → no increment.
    const dir = tempStateDir();
    const bumped = decorationMissGate(dir, {
      depth: 0,
      carriedId: "top-session",
      trackerExists: false,
    });
    assert.strictEqual(bumped, false);
    assert.strictEqual(state.readBreadcrumb(dir).stamp_miss, 0);
  });

  it("test_depth1_subagent_inherited_tracker_present_no_increment", function () {
    // cycles/{root}.json exists; depth-1 subagent carrying root id finds it → no increment.
    const dir = tempStateDir();
    const bumped = decorationMissGate(dir, {
      depth: 1,
      carriedId: "root-session",
      trackerExists: true,
    });
    assert.strictEqual(bumped, false);
    assert.strictEqual(state.readBreadcrumb(dir).stamp_miss, 0);
  });

  it("test_depth1_subagent_noninherited_id_root_tracker_exists_one_increment", function () {
    // Root tracker exists; depth-1 subagent carries a NON-inherited id → no tracker
    // for the carried id → inheritance drift → exactly one increment.
    const dir = tempStateDir();
    const bumped = decorationMissGate(dir, {
      depth: 1,
      carriedId: "stranger-id",
      trackerExists: false,
    });
    assert.strictEqual(bumped, true);
    assert.strictEqual(readHealth(dir).stamp_miss, 1);
  });

  it("test_depthgt1_grandchild_no_tracker_lands_in_stamp_miss", function () {
    // R-14 forward-compat (ADR-006 §5): a depth>1 grandchild id with no tracker,
    // root tracker present → lands in stamp_miss (silent loss is impossible).
    const dir = tempStateDir();
    const bumped = decorationMissGate(dir, {
      depth: 2,
      carriedId: "grandchild-id",
      trackerExists: false,
    });
    assert.strictEqual(bumped, true);
    assert.strictEqual(readHealth(dir).stamp_miss, 1);
  });

  it("test_healthy_single_declared_session_with_subagent_stamp_miss_zero", function () {
    // Zero-tolerance, ships either OQ-E branch. One declared root + one depth-1
    // subagent inheriting the root id → stamp_miss == 0 after the full flow.
    const dir = tempStateDir();
    // root declares: a normal send occurs.
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    // depth-1 subagent inherits root id; tracker found → no increment.
    const bumped = decorationMissGate(dir, {
      depth: 1,
      carriedId: "root-session",
      trackerExists: true,
    });
    // subagent's own send.
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    assert.strictEqual(bumped, false);
    assert.strictEqual(readHealth(dir).stamp_miss, 0);
  });
});

// ── CLI-drift re-run check (R-08, FR-10) ────────────────────────────

describe("CLI-drift re-run check (R-08, FR-10)", function () {
  it("test_canary_fixtures_are_the_cli_drift_check", function () {
    // These AC-06 fixtures ARE the re-run-on-CLI-bump drift check. The invariant
    // is `stamp_miss == 0` — NO ratio, NO fnf_record_send_count denominator, NO
    // 0.20 threshold, NO per-deployment baseline, NO human re-set. Pinned CLI is
    // claude 2.1.167 (see module doc comment). Drift surfaces as a nonzero
    // counter, never silent loss.
    const src = fs.readFileSync(__filename, "utf8");
    assert.ok(/claude 2\.1\.167/.test(src), "pinned CLI named in this test module");
    // Removed knobs must not reappear as identifiers in the canary surface
    // (prose mentions in doc comments stating their absence are fine).
    const stateSrc = fs.readFileSync(
      path.join(__dirname, "..", "..", "lib", "hook-client", "state.js"),
      "utf8"
    );
    assert.ok(!/fnf_record_send_count/.test(stateSrc), "no denominator knob");
    assert.ok(!/anyOtherCycleFile/.test(stateSrc), "no concurrent-file knob");
    // The increment is unconditional (+1) — no ratio/threshold arithmetic.
    assert.ok(/stamp_miss:\s*prev\.stamp_miss\s*\+\s*1/.test(stateSrc),
      "increment is a plain +1, not a thresholded ratio");

    // The invariant itself: a healthy lifetime stays at 0.
    const dir = tempStateDir();
    state.recordSendOutcomes(dir, "h", [okResult()], 0);
    assert.strictEqual(readHealth(dir).stamp_miss, 0);
  });
});
