"use strict";

// AC-13 benchmark as a node:test gate. Runs the harness once and asserts the
// CLIENT-WORK budget (parse + build + config hash/root-walk + transform +
// health.json breadcrumb) -- the path the ~12 ms AC-13 target governs. Full
// child-spawn wall time is recorded for the artifact but NOT hard-asserted here:
// it is dominated by the host env's Node interpreter cold-start (see
// node_startup_ms), which varies by machine and is outside client control
// (C-03: "~12 ms spawn floor" is the interpreter spawn cost).
//
// The committed artifact (product/features/vnc-026/testing/) is produced by the
// standalone script: `node test/hook-client/benchmark-spawn.js --write`.

const { test } = require("node:test");
const assert = require("assert");

const {
  runBenchmark,
  formatSummary,
  TARGET_P50_MS,
  TARGET_P95_MS,
  ITERATIONS,
} = require("./benchmark-spawn");

test("AC-13 spawn benchmark: client work within the p50/p95 budget", async (t) => {
  const r = await runBenchmark();
  t.diagnostic(formatSummary(r));

  assert.strictEqual(r.iterations >= 50, true, "must measure >= 50 iterations (R-13)");
  assert.strictEqual(r.breadcrumb_written, true, "measured path must write health.json (R-13)");

  const w = r.client_work_ms;
  assert.ok(
    w.p50 <= TARGET_P50_MS,
    "client-work p50 " + w.p50.toFixed(3) + "ms exceeds " + TARGET_P50_MS + "ms"
  );
  assert.ok(
    w.p95 <= TARGET_P95_MS,
    "client-work p95 " + w.p95.toFixed(3) + "ms exceeds " + TARGET_P95_MS + "ms"
  );

  // Soft visibility on full spawn vs target -- diagnostic only (env-dependent).
  if (r.results_ms.p50 > TARGET_P50_MS) {
    t.diagnostic(
      "note: full-spawn p50 " +
        r.results_ms.p50.toFixed(1) +
        "ms exceeds the " +
        TARGET_P50_MS +
        "ms target due to this env's Node cold-start (" +
        r.node_startup_ms.p50.toFixed(1) +
        "ms); client work is " +
        w.p50.toFixed(3) +
        "ms"
    );
  }

  void ITERATIONS;
});
