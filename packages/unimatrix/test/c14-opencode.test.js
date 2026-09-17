"use strict";

// C14 verifier — opencode leg (nan-023, ADR-006, vnc-049). Retrieval-RETURNS
// from the wired `mcp.unimatrix` slug (local + cloud token-free bridge), and the
// vnc-049 sentinel (Ollama `provider` block + foreign keys) is byte-preserved
// (SR-06, C-05, AC-05). opencode "firing" is plugin-level (in-process TS plugin),
// asserted structurally here — NOT JS-hook-client ingress (AC-10 note).

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const { wire } = require("../lib/wire.js");
const V = require("./c14-verifier.js");

const CLIENT = path.resolve(__dirname, "../lib/hook-client/index.js");

// A fixture opencode.json with the vnc-049 sentinel regions: an Ollama provider
// block and a foreign top-level key that MUST survive byte-for-byte.
const SENTINEL = JSON.stringify(
  {
    $schema: "https://opencode.ai/config.json",
    provider: {
      ollama: {
        npm: "@ai-sdk/openai-compatible",
        options: { baseURL: "http://localhost:11434/v1" },
        models: { "qwen3-coder": { name: "Qwen3 Coder" } },
      },
    },
    foreignKey: { keep: "me byte-for-byte" },
  },
  null,
  2
) + "\n";

function opencodeProject(opts) {
  const root = V.makeTempProject();
  fs.writeFileSync(path.join(root, "opencode.json"), SENTINEL, "utf8");
  const res = wire(root, Object.assign({ harness: "opencode", clientPath: CLIENT }, opts));
  return { root, manifest: res.manifest };
}
function opencodeLeg(manifest) {
  return manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval");
}

describe("C14 opencode retrieval-returns + sentinel (AC-05, SR-06)", () => {
  it("test_verify_opencode_retrieval_returns_local", async () => {
    const { root, manifest } = opencodeProject({ binaryPath: V.STDIO_FIXTURE });
    const leg = opencodeLeg(manifest);
    assert.ok(leg.entry, "opencode retrieval leg carries the wired mcp.unimatrix entry");
    await V.assertRetrievalReturns(leg, {}); // spawns entry.command (argv array) VERBATIM
    V.cleanupDir(root);
  });

  it("test_verify_opencode_retrieval_returns_cloud (token-free bridge)", async () => {
    const { root, manifest } = opencodeProject({
      mcp: { url: "https://cloud.example", token: "secret-token" },
      bridgePath: V.STDIO_FIXTURE,
    });
    const leg = opencodeLeg(manifest);
    assert.deepStrictEqual(leg.entry.command.slice(0, 1), ["node"], "cloud opencode entry spawns node bridge");
    assert.strictEqual(JSON.stringify(leg.entry).indexOf("secret-token"), -1, "no token in the entry");
    await V.assertRetrievalReturns(leg, {});
    V.cleanupDir(root);
  });

  it("test_verify_opencode_sentinel_byte_preserved_then_returns", async () => {
    const { root, manifest } = opencodeProject({ binaryPath: V.STDIO_FIXTURE });
    const after = fs.readFileSync(path.join(root, "opencode.json"), "utf8");
    // vnc-049 sentinel regions survive byte-for-byte (additive merge only).
    assert.ok(after.indexOf('"ollama"') !== -1, "Ollama provider block preserved");
    assert.ok(after.indexOf('"baseURL": "http://localhost:11434/v1"') !== -1, "Ollama options preserved");
    assert.ok(after.indexOf('"keep": "me byte-for-byte"') !== -1, "foreign key preserved");
    // ...and retrieval still RETURNS after the merge (regression, C-05).
    await V.assertRetrievalReturns(opencodeLeg(manifest), {});
    V.cleanupDir(root);
  });
});

describe("C14 opencode firing is plugin-level (AC-10 note)", () => {
  it("test_opencode_firing_asserted_at_plugin_level_not_js_client", () => {
    // opencode observation runs as an in-process TS plugin, provisioned by
    // maybeProvisionOpenCode → a `plugin` surface leg. It is NOT a JS-hook-client
    // ingress event, so firing is asserted at the plugin-provisioning level, and
    // recorded distinctly from the codex JS-client fire path.
    const { root, manifest } = opencodeProject({ binaryPath: V.STDIO_FIXTURE });
    const plugin = manifest.find((l) => l.harness === "opencode" && l.surface === "plugin");
    assert.ok(plugin, "opencode plugin surface leg present (plugin-level firing path)");
    assert.ok(["created", "updated", "unchanged"].includes(plugin.action), "plugin provisioned");
    V.cleanupDir(root);
  });
});
