"use strict";

// C14 verifier — claude-code leg (nan-023 ADR-006, re-based on nan-024). Retrieval-
// RETURNS from the wired per-machine `~/.claude.json` entry and the hook client
// FIRES through the per-machine shim. Executed from the WireLeg manifest against an
// isolated machine (HOME + project) — never a config-presence proxy. The byte-
// identical golden lives in wire.test.js.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const V = require("./c14-verifier.js");

function claudeLeg(manifest, surface) {
  return manifest.find((l) => l.harness === "claude-code" && l.surface === surface);
}

describe("C14 claude retrieval-returns (AC-10, required local + cloud)", () => {
  it("test_verify_claude_retrieval_returns_local", async () => {
    const m = V.makeMachine([".claude"]);
    const manifest = V.wireMachine(m, { binaryPath: V.STDIO_FIXTURE });
    const leg = claudeLeg(manifest, "mcp");
    assert.ok(leg.entry, "claude mcp leg carries the exact wired entry");
    // spawns entry.command VERBATIM (sh -c exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio)
    await V.assertRetrievalReturns(leg, { cwd: m.root, env: V.machineEnv(m) });
    V.cleanupMachine(m);
  });

  it("test_verify_claude_retrieval_returns_cloud (token-free entry)", async () => {
    const m = V.makeMachine([".claude"]);
    const manifest = V.wireMachine(m, { mcp: true });
    V.writeCloudCredential(m, "https://cloud.example/v1/p/observe");
    const leg = claudeLeg(manifest, "mcp");
    assert.strictEqual(JSON.stringify(leg.entry).indexOf("c14-token"), -1, "no token in the entry");
    assert.strictEqual(JSON.stringify(leg.entry).indexOf("cloud.example"), -1, "no url in the entry");
    // The cloud route loads the bridge from the client root; stub it through a redirect
    // so the entry is executed end to end without a remote server.
    V.installStubBridge(m);
    await V.assertRetrievalReturns(leg, { cwd: m.root, env: V.machineEnv(m) });
    V.cleanupMachine(m);
  });
});

describe("C14 claude hook-fires through the shim", () => {
  let ingress;
  let ctx;
  before(async () => {
    ctx = V.makeMachine([".claude"]);
    ctx.manifest = V.wireMachine(ctx, { binaryPath: V.STDIO_FIXTURE });
    ingress = await V.makeUdsIngress(ctx);
  });
  after(async () => {
    if (ingress) await ingress.close();
  });

  it("test_verify_claude_hook_fires — ingress observed", async () => {
    const legs = ctx.manifest.filter((l) => l.harness === "claude-code" && l.surface === "hooks");
    assert.ok(legs.length > 0, "claude hooks legs present");
    const leg = legs.find((l) => V.eventOf(l.command) === "UserPromptSubmit") || legs[0];
    const out = await V.observeHookFire(leg, ingress, {});
    assert.strictEqual(out.fired, true, "claude hook event reached the JS hook client via the shim");
    // claude commands carry no --provider → inferred claude-code (never codex-cli).
    if (out.provider !== undefined) {
      assert.notStrictEqual(out.provider, "codex-cli", "claude leg is not mislabeled codex-cli");
    }
  });
});
