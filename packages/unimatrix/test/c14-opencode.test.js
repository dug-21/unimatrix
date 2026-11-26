"use strict";

// C14 verifier — opencode leg (nan-023, ADR-006, vnc-049). Retrieval-RETURNS
// from the wired per-machine `mcp.unimatrix` (~/.config/opencode/opencode.json;
// local + cloud token-free shim entry), and the vnc-049 sentinel (Ollama `provider`
// block + foreign keys) in that user config is byte-preserved (SR-06, C-05, AC-05).
// nan-024 ADR-006: init/wire write no OpenCode plugin provisioning, asserted below.

const { describe, it } = require("node:test");
const assert = require("assert");
const { linuxLocal } = require("./helpers/linux-local.js");
const fs = require("fs");
const path = require("path");

const V = require("./c14-verifier.js");

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

// Seeds the machine's USER opencode.json with the sentinel, then wires `--harness opencode`.
function opencodeProject(opts) {
  const m = V.makeMachine();
  const cfg = path.join(m.home, ".config", "opencode", "opencode.json");
  fs.mkdirSync(path.dirname(cfg), { recursive: true });
  fs.writeFileSync(cfg, SENTINEL, "utf8");
  m.cfg = cfg;
  m.manifest = V.wireMachine(m, Object.assign({ harness: "opencode" }, opts));
  return m;
}
function opencodeLeg(manifest) {
  return manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval");
}

describe("C14 opencode retrieval-returns + sentinel (AC-05, SR-06)", () => {
  it("test_verify_opencode_retrieval_returns_local", linuxLocal, async () => {
    const m = opencodeProject({ binaryPath: V.STDIO_FIXTURE });
    const leg = opencodeLeg(m.manifest);
    assert.ok(leg.entry, "opencode retrieval leg carries the wired mcp.unimatrix entry");
    await V.assertRetrievalReturns(leg, { cwd: m.root, env: V.machineEnv(m) }); // entry.command argv VERBATIM
    V.cleanupMachine(m);
  });

  it("test_verify_opencode_retrieval_returns_cloud (token-free entry)", async () => {
    const m = opencodeProject({ mcp: true });
    V.writeCloudCredential(m, "https://cloud.example/v1/p/observe");
    V.installStubBridge(m);
    const leg = opencodeLeg(m.manifest);
    assert.strictEqual(JSON.stringify(leg.entry).indexOf("secret-token"), -1, "no token in the entry");
    assert.ok(JSON.stringify(leg.entry.command).includes("mcp-stdio"), "entry runs mcp-stdio");
    await V.assertRetrievalReturns(leg, { cwd: m.root, env: V.machineEnv(m) });
    V.cleanupMachine(m);
  });

  it("test_verify_opencode_sentinel_byte_preserved_then_returns", async () => {
    // D-032: local entry on Linux; token-free cloud entry (stub bridge) on macOS/Windows.
    const m = process.platform === "linux" ? opencodeProject({ binaryPath: V.STDIO_FIXTURE }) : opencodeProject({ mcp: true });
    if (process.platform !== "linux") {
      V.writeCloudCredential(m, "https://cloud.example/v1/p/observe");
      V.installStubBridge(m);
    }
    const after = fs.readFileSync(m.cfg, "utf8");
    // vnc-049 sentinel regions survive byte-for-byte (additive merge only).
    assert.ok(after.indexOf('"ollama"') !== -1, "Ollama provider block preserved");
    assert.ok(after.indexOf('"baseURL": "http://localhost:11434/v1"') !== -1, "Ollama options preserved");
    assert.ok(after.indexOf('"keep": "me byte-for-byte"') !== -1, "foreign key preserved");
    // ...and retrieval still RETURNS after the merge (regression, C-05).
    await V.assertRetrievalReturns(opencodeLeg(m.manifest), { cwd: m.root, env: V.machineEnv(m) });
    V.cleanupMachine(m);
  });
});

describe("C14 opencode provisioning (ADR-006)", () => {
  it("test_opencode_init_writes_no_plugin_provisioning", () => {
    // init/wire write no OpenCode plugin provisioning: no plugin leg, no project
    // .opencode/ tree, and only the user-level mcp.unimatrix key changes.
    const m = opencodeProject({ binaryPath: V.STDIO_FIXTURE });
    assert.ok(!m.manifest.some((l) => l.harness === "opencode" && l.surface === "plugin"), "no plugin leg");
    assert.ok(!fs.existsSync(path.join(m.root, ".opencode")), "no project .opencode/ written");
    const cfg = JSON.parse(fs.readFileSync(m.cfg, "utf8"));
    const orig = JSON.parse(SENTINEL);
    assert.deepStrictEqual(cfg.provider, orig.provider);
    assert.deepStrictEqual(cfg.foreignKey, orig.foreignKey);
    assert.ok(cfg.mcp && cfg.mcp.unimatrix, "mcp.unimatrix written");
    assert.strictEqual(cfg.plugin, undefined, "no plugin[] entry added");
    V.cleanupMachine(m);
  });
});
