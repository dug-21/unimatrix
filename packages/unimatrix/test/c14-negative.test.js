"use strict";

// C14 verifier — mutation / negative controls (nan-023, ADR-006, R-02, SR-09).
//
// THE anti-tautology proof. Each control breaks the WIRED ARTIFACT while leaving
// its manifest string INTACT (an entry object is still present, a command string
// is still there), then asserts the verifier FAILS. A presence/string proxy
// would still pass on these inputs — the executor must not. If any control were
// to PASS, the corresponding assertion is a presence proxy and must be rewritten.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const path = require("path");

const { wire } = require("../lib/wire.js");
const V = require("./c14-verifier.js");

const CLIENT = path.resolve(__dirname, "../lib/hook-client/index.js");

function clone(obj) {
  return JSON.parse(JSON.stringify(obj));
}

function realManifest(harness, opts, extraDirs, prep) {
  const root = V.makeTempProject(extraDirs);
  if (prep) prep(root);
  const res = wire(root, Object.assign({ harness, clientPath: CLIENT, binaryPath: V.STDIO_FIXTURE }, opts));
  return { root, manifest: res.manifest };
}

// ── AC-10 retrieval mutation controls (one per behavioral leg) ────────────

describe("AC-10 retrieval mutation controls (R-02)", () => {
  async function assertFailsToReturn(leg) {
    // Manifest string is INTACT (entry present); only the wired TARGET is broken.
    assert.ok(leg.entry, "leg still carries an entry (presence check would pass)");
    let threw = false;
    try {
      await V.assertRetrievalReturns(leg, { timeoutMs: 2500 });
    } catch (_e) {
      threw = true;
    }
    assert.strictEqual(threw, true, "verifier FAILS on a broken target (not a presence proxy)");
  }

  it("test_verify_retrieval_mutation_broken_entry_fails_claude", async () => {
    const { root, manifest } = realManifest("claude-code", {}, [".claude"]);
    const leg = clone(manifest.find((l) => l.harness === "claude-code" && l.surface === "mcp"));
    leg.entry.command = "/nonexistent/unimatrix-binary"; // broken target, entry shape intact
    await assertFailsToReturn(leg);
    V.cleanupDir(root);
  });

  it("test_verify_retrieval_mutation_broken_entry_fails_codex", async () => {
    const { root, manifest } = realManifest("codex-cli", {}, [".codex"]);
    const leg = clone(manifest.find((l) => l.harness === "codex-cli" && l.surface === "mcp"));
    leg.entry.command = "/nonexistent/unimatrix-binary";
    await assertFailsToReturn(leg);
    V.cleanupDir(root);
  });

  it("test_verify_retrieval_mutation_broken_entry_fails_opencode", async () => {
    const fs = require("fs");
    const { root, manifest } = realManifest("opencode", {}, [], (r) => {
      fs.writeFileSync(path.join(r, "opencode.json"), "{}\n", "utf8");
    });
    const leg = clone(manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval"));
    leg.entry.command = ["/nonexistent/unimatrix-binary"]; // opencode argv-array shape, still present
    await assertFailsToReturn(leg);
    V.cleanupDir(root);
  });

  it("test_verify_retrieval_fails_when_entry_target_blanked (negative control)", async () => {
    const { root, manifest } = realManifest("codex-cli", {}, [".codex"]);
    const leg = clone(manifest.find((l) => l.harness === "codex-cli" && l.surface === "mcp"));
    leg.entry.command = ""; // target gone; manifest still has an `entry` object
    await assertFailsToReturn(leg);
    V.cleanupDir(root);
  });
});

// ── AC-09 hook mutation control ───────────────────────────────────────────

describe("AC-09 hook mutation control (R-02)", () => {
  let ingress;
  before(async () => { ingress = await V.makeHttpIngress(); });
  after(async () => { if (ingress) await ingress.close(); });

  it("test_verify_codex_hook_fire_mutation_broken_client_fails", async () => {
    const { root, manifest } = realManifest("codex-cli", {}, [".codex"]);
    const good = manifest.find(
      (l) => l.harness === "codex-cli" && l.surface === "hooks" && V.eventOf(l.command) === "UserPromptSubmit"
    );
    // Sanity: the intact wired command DOES fire (baseline for the control).
    const okOut = await V.observeHookFire(good, ingress, {});
    assert.strictEqual(okOut.fired, true, "intact wired command fires (control baseline)");

    // Break the client path; the command string STILL looks like a hook command.
    const broken = { command: good.command.replace(CLIENT, "/nonexistent/hook-client/index.js") };
    assert.ok(broken.command.indexOf("--provider codex-cli") !== -1, "string still intact (presence would pass)");
    const badOut = await V.observeHookFire(broken, ingress, {});
    assert.strictEqual(badOut.fired, false, "no ingress → verifier FAILS (executes, not string-matches)");
    V.cleanupDir(root);
  });
});

// ── Tautology guard (documents the presence-vs-behavior gap) ──────────────

describe("Tautology guard: presence passes but execution must fail (SR-09)", () => {
  it("test_broken_artifact_would_pass_a_presence_check_yet_fails_execution", async () => {
    const { root, manifest } = realManifest("codex-cli", {}, [".codex"]);
    const leg = clone(manifest.find((l) => l.harness === "codex-cli" && l.surface === "mcp"));
    leg.entry.command = "/nonexistent/unimatrix-binary";

    // A config/manifest PRESENCE check — the forbidden discharge — still passes:
    const presencePasses = !!(leg.entry && typeof leg.entry.command === "string");
    assert.strictEqual(presencePasses, true, "presence proxy would (wrongly) pass on the broken artifact");

    // The behavioral executor does NOT:
    let executionFailed = false;
    try {
      await V.assertRetrievalReturns(leg, { timeoutMs: 2500 });
    } catch (_e) {
      executionFailed = true;
    }
    assert.strictEqual(executionFailed, true, "behavioral verifier fails where the presence proxy passes");
    V.cleanupDir(root);
  });
});
