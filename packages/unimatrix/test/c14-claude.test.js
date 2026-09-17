"use strict";

// C14 verifier — claude-code leg (nan-023, ADR-006). Retrieval-RETURNS from the
// wired `.mcp.json` entry (local Rust binary + cloud token-free bridge) and the
// shared hook-client runtime FIRES. Executed from the WireLeg manifest — never a
// config-presence proxy. The SR-07 byte-identical golden lives in wire.test.js.

const { describe, it, before, after } = require("node:test");
const assert = require("assert");
const path = require("path");

const { wire } = require("../lib/wire.js");
const V = require("./c14-verifier.js");

const CLIENT = path.resolve(__dirname, "../lib/hook-client/index.js");

function claudeManifest(opts) {
  const root = V.makeTempProject([".claude"]);
  const res = wire(root, Object.assign({ clientPath: CLIENT }, opts));
  return { root, manifest: res.manifest };
}
function claudeLeg(manifest, surface) {
  return manifest.find((l) => l.harness === "claude-code" && l.surface === surface);
}

describe("C14 claude retrieval-returns (AC-10, required local + cloud)", () => {
  it("test_verify_claude_retrieval_returns_local", async () => {
    const { root, manifest } = claudeManifest({ binaryPath: V.STDIO_FIXTURE });
    const leg = claudeLeg(manifest, "mcp");
    assert.ok(leg.entry, "claude mcp leg carries the exact wired entry");
    await V.assertRetrievalReturns(leg, {}); // spawns entry.command VERBATIM
    V.cleanupDir(root);
  });

  it("test_verify_claude_retrieval_returns_cloud (token-free bridge)", async () => {
    const { root, manifest } = claudeManifest({
      mcp: { url: "https://cloud.example", token: "secret-token" },
      bridgePath: V.STDIO_FIXTURE,
    });
    const leg = claudeLeg(manifest, "mcp");
    assert.strictEqual(leg.entry.command, "node", "cloud claude entry is the node bridge");
    assert.strictEqual(JSON.stringify(leg.entry).indexOf("secret-token"), -1, "no token in the entry");
    await V.assertRetrievalReturns(leg, {});
    V.cleanupDir(root);
  });
});

describe("C14 claude hook-fires (shared-runtime regression baseline)", () => {
  let ingress;
  let ctx;
  before(async () => {
    // The claude JS-hook-client command shape is the cloud/bridge form
    // (`node <client> <EVENT>`); the local form invokes the legacy Rust binary
    // directly, which is not the JS client this verifier exercises.
    ctx = claudeManifest({ mcp: { url: "https://cloud.example", token: "t" }, bridgePath: V.STDIO_FIXTURE });
    ingress = await V.makeUdsIngress();
  });
  after(async () => {
    if (ingress) await ingress.close();
    if (ctx) V.cleanupDir(ctx.root);
  });

  it("test_verify_claude_hook_fires — ingress observed", async () => {
    const legs = ctx.manifest.filter((l) => l.harness === "claude-code" && l.surface === "hooks");
    assert.ok(legs.length > 0, "claude hooks legs present");
    const leg = legs.find((l) => V.eventOf(l.command) === "UserPromptSubmit") || legs[0];
    const out = await V.observeHookFire(leg, ingress, {});
    assert.strictEqual(out.fired, true, "claude hook event reached the JS hook client");
    // claude commands carry no --provider → inferred claude-code (never codex-cli).
    if (out.provider !== undefined) {
      assert.notStrictEqual(out.provider, "codex-cli", "claude leg is not mislabeled codex-cli");
    }
  });
});
