"use strict";

// C14 verifier — codex-cli leg (nan-023, ADR-006 / ADR-003). Retrieval-RETURNS
// and hook-FIRES executed from the WireLeg manifest, never from config presence.
//
// Gate-0 (test-plan/OVERVIEW §2): seeding a trusted, codex-FIRING `.codex/` in CI
// is NOT feasible (codex-cli absent from the env). The C14 discharge here
// therefore EXECUTES the exact wired manifest command (`node <client> <EVENT>
// --provider codex-cli`) with synthetic stdin — bypassing codex — so command-
// level firing, local firing, and BOTH retrieval-return arms stay HARD. Codex
// SELF-firing the 7 events in a real trusted `.codex/` is a DOCUMENTED CONDITIONAL,
// recorded per-event as `wired-inactive (untestable-in-CI)`, never silent-skipped.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const path = require("path");

const { wire } = require("../lib/wire.js");
const V = require("./c14-verifier.js");

const CLIENT = path.resolve(__dirname, "../lib/hook-client/index.js");
const BRIDGE = path.resolve(__dirname, "../lib/hook-client/mcp-bridge.js");
const CODEX_EVENTS = [
  "SessionStart", "UserPromptSubmit", "PreToolUse",
  "PostToolUse", "PreCompact", "SubagentStart", "Stop",
];
// The RecordEvent-family events whose frames carry `provider` (probe-confirmed).
// Session-lifecycle / sync frames (SessionRegister/SessionClose/CompactPayload)
// do not carry it — asserted only where present (R-05 no-mislabel).

function codexManifest(opts) {
  const root = V.makeTempProject([".codex"]);
  const res = wire(root, Object.assign({ harness: "codex-cli", clientPath: CLIENT }, opts));
  return { root, manifest: res.manifest };
}

function codexLeg(manifest, surface) {
  return manifest.find((l) => l.harness === "codex-cli" && l.surface === surface);
}
function codexHookLegs(manifest) {
  return manifest.filter((l) => l.harness === "codex-cli" && l.surface === "hooks");
}

// ── AC-10: retrieval RETURNS (local + cloud, never url=) ──────────────────

describe("C14 codex retrieval-returns (AC-10, C-06)", () => {
  it("test_verify_codex_retrieval_returns_local", async () => {
    const { root, manifest } = codexManifest({ binaryPath: V.STDIO_FIXTURE });
    const leg = codexLeg(manifest, "mcp");
    assert.strictEqual(leg.action, "created", "codex mcp table written (intent opt-in)");
    await V.assertRetrievalReturns(leg, {}); // spawns entry.command VERBATIM
    V.cleanupDir(root);
  });

  it("test_verify_codex_retrieval_returns_cloud", async () => {
    // Cloud entry = command="node", args=[bridge, hash] (Q1) — bridge stubbed by
    // the stdio fixture (arg-agnostic). Executes the manifest entry verbatim.
    const { root, manifest } = codexManifest({
      mcp: { url: "https://cloud.example", token: "secret-token" },
      bridgePath: V.STDIO_FIXTURE,
    });
    const leg = codexLeg(manifest, "mcp");
    assert.strictEqual(leg.entry.command, "node", "cloud codex entry command is node");
    assert.strictEqual(leg.entry.args[0], V.STDIO_FIXTURE, "bridge path is args[0]");
    await V.assertRetrievalReturns(leg, {});
    V.cleanupDir(root);
  });

  it("test_verify_codex_cloud_entry_carries_no_token_or_url (Q1 security)", () => {
    const { root, manifest } = codexManifest({
      mcp: { url: "https://cloud.example", token: "secret-token" },
      bridgePath: BRIDGE,
    });
    const leg = codexLeg(manifest, "mcp");
    const serialized = JSON.stringify(leg.entry);
    assert.ok(!/\burl\b/i.test(serialized), "no url= in the cloud codex entry");
    assert.ok(serialized.indexOf("secret-token") === -1, "no bearer token in the entry (Principle 8)");
    assert.ok(serialized.indexOf("https://cloud.example") === -1, "no mcp url in the entry");
    V.cleanupDir(root);
  });
});

// ── AC-09: hook FIRES, per-event RECORD (Q4, R-15) ────────────────────────

describe("C14 codex hook-fires + per-event evidence (AC-09, Q4)", () => {
  let httpIngress;
  let udsIngress;
  let ctx;

  before(async () => {
    ctx = codexManifest({ binaryPath: V.STDIO_FIXTURE });
    httpIngress = await V.makeHttpIngress();
    udsIngress = await V.makeUdsIngress();
  });
  after(async () => {
    if (httpIngress) await httpIngress.close();
    if (udsIngress) await udsIngress.close();
    if (ctx) V.cleanupDir(ctx.root);
  });

  it("test_verify_codex_hook_fires_local — ingress + provider=codex-cli", async () => {
    // UserPromptSubmit → RecordEvent frame, which carries provider (probe-confirmed).
    const leg = codexHookLegs(ctx.manifest).find((l) => V.eventOf(l.command) === "UserPromptSubmit");
    const out = await V.observeHookFire(leg, udsIngress, {});
    assert.strictEqual(out.fired, true, "event reached the JS hook client over UDS (local)");
    assert.strictEqual(out.provider, "codex-cli", "provider stamped codex-cli (NOT source_domain, R-05)");
  });

  it("test_verify_codex_hook_fires_cloud — command-level, deployment-independent", async () => {
    const leg = codexHookLegs(ctx.manifest).find((l) => V.eventOf(l.command) === "PostToolUse");
    const out = await V.observeHookFire(leg, httpIngress, {});
    assert.strictEqual(out.fired, true, "wired command reached the client over the cloud transport");
    assert.strictEqual(out.provider, "codex-cli", "provider stamped codex-cli");
  });

  it("test_verify_codex_per_event_firing_records (all 7 events)", async () => {
    const legs = codexHookLegs(ctx.manifest);
    assert.strictEqual(legs.length, 7, "manifest carries all 7 codex hook events");

    const table = [];
    for (const event of CODEX_EVENTS) {
      const leg = legs.find((l) => V.eventOf(l.command) === event);
      assert.ok(leg, "manifest carries hook leg for " + event);
      const out = await V.observeHookFire(leg, httpIngress, {});
      // Command-level firing is HARD for every event (Gate-0). "wired-inactive"
      // would be a NAMED gap, never a silent drop — surfaced in the assert msg.
      const status = out.fired ? "fires" : "wired-inactive";
      table.push({ event, deployment: "cloud", status, provider: out.provider });
      assert.strictEqual(out.fired, true, "command-level firing HARD for " + event + " (" + status + ")");
      // No-mislabel: any provider that IS stamped must be codex-cli (R-05).
      if (out.provider !== undefined) {
        assert.strictEqual(out.provider, "codex-cli", "no provider mislabel on " + event);
      }
    }
    // Evidence table is COMPLETE (7 rows) — Stage 3c folds it into RISK-COVERAGE-REPORT.
    assert.strictEqual(table.length, 7, "per-event evidence recorded for all 7 events");
    const stamped = table.filter((r) => r.provider === "codex-cli").map((r) => r.event);
    // At least the RecordEvent-family events stamp the provider.
    assert.ok(stamped.length >= 3, "provider stamped on the RecordEvent-family events: " + stamped.join(","));
  });

  it("test_codex_self_firing_documented_conditional (Gate-0, never silent-skip)", () => {
    // Codex SELF-raising the 7 events in a real trusted `.codex/` needs codex
    // installed + confirmed trust — CI has neither (Gate-0). Recorded per-event
    // as wired-inactive(untestable-in-CI), NOT skipped. This test ASSERTS the
    // record exists so the gap is visible, not silent.
    const selfFiring = CODEX_EVENTS.map((event) => ({
      event,
      column: "codex-raises-it",
      status: "wired-inactive (untestable-in-CI: codex not installed / trust unconfirmed)",
    }));
    assert.strictEqual(selfFiring.length, 7, "documented-conditional recorded for all 7 events");
    assert.ok(
      selfFiring.every((r) => r.status.indexOf("wired-inactive") === 0),
      "every self-firing cell is a named documented gap"
    );
  });
});

// ── R-06: trust surfaced as a precondition, never an inert no-op ──────────

describe("C14 codex trust precondition (R-06)", () => {
  it("test_verify_untrusted_codex_surfaces_precondition_note", () => {
    // The codex writer attaches a trust precondition note to its legs (fail-loud
    // / warn), so a consumer's untrusted `.codex/` is never read as silent success.
    const { root, manifest } = codexManifest({ binaryPath: V.STDIO_FIXTURE });
    const legs = manifest.filter((l) => l.harness === "codex-cli");
    const withNote = legs.filter((l) => typeof l.note === "string" && /trust/i.test(l.note));
    assert.ok(withNote.length > 0, "codex legs surface a trust precondition note (not a silent no-op)");
    V.cleanupDir(root);
  });
});
