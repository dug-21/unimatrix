"use strict";

// bridge-cycle-driver.test.js (nan-022 C2') — OFF-Docker driver-shape tests for the
// manifest phase VIEWS the driver replays. Requiring the driver module here is safe:
// main() runs only under `require.main === module`, so no bridge spawns. The LIVE
// drive (the real tools/call envelopes over the bridge) is Stage 3c / Docker.
//
// Run: node --test scripts/bridge-cycle-driver.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const driver = require("./bridge-cycle-driver.js");

// A manifest mirroring the augmented C4' workload: seed (context_store) + cycle
// (Read/Bash/Grep) + query (search/lookup/get + briefing), one tool_calls list.
const MANIFEST_TOOL_CALLS = [
  { name: "context_store", args: { content: "c0", topic: "t", category: "pattern" } },
  { name: "context_store", args: { content: "c1", topic: "t", category: "pattern" } },
  { name: "Read", args: { file_path: "x" }, observe: true },
  { name: "Bash", args: { command: "git log" }, observe: true, response_snippet: "feat tok" },
  { name: "Grep", args: { pattern: "p" }, observe: true },
  { name: "context_search", args: { query: "t parity", k: 5 } },
  { name: "context_lookup", args: { topic: "t", category: "pattern" } },
  { name: "context_briefing", args: { task: "work on t" } },
  { name: "context_briefing", args: { task: "rank t" } },
];

test("seedCalls: only context_store, in manifest order", () => {
  const seeds = driver.seedCalls(MANIFEST_TOOL_CALLS);
  assert.equal(seeds.length, 2);
  assert.ok(seeds.every((c) => c.name === "context_store"));
});

test("retrievalCalls: search/lookup/get only (no briefing, no cycle, no store)", () => {
  const r = driver.retrievalCalls(MANIFEST_TOOL_CALLS);
  assert.deepEqual(r.map((c) => c.name), ["context_search", "context_lookup"]);
});

test("briefingCalls: context_briefing only", () => {
  const b = driver.briefingCalls(MANIFEST_TOOL_CALLS);
  assert.equal(b.length, 2);
  assert.ok(b.every((c) => c.name === "context_briefing"));
});

test("views partition cleanly — no cycle/observe call leaks into a query view", () => {
  const all = new Set(MANIFEST_TOOL_CALLS);
  const seed = driver.seedCalls(MANIFEST_TOOL_CALLS);
  const retr = driver.retrievalCalls(MANIFEST_TOOL_CALLS);
  const brief = driver.briefingCalls(MANIFEST_TOOL_CALLS);
  for (const c of [...seed, ...retr, ...brief]) assert.ok(all.has(c));
  // The observe-driven cycle calls (Read/Bash/Grep) are NOT driven on the MCP
  // bridge surface here — they ride the shell's /observe route (single source).
  for (const c of [...seed, ...retr, ...brief]) {
    assert.ok(!["Read", "Bash", "Grep"].includes(c.name), "no cycle call in a bridge-surface view");
  }
});

test("parseArgs: positional + --bridge/--witness options", () => {
  const argv = [
    "node", "driver.js", "HASH", "/m.json",
    "--bridge", "/b.js", "--witness", "/w.js",
  ];
  const { projectHash, manifestPath, opt } = driver.parseArgs(argv);
  assert.equal(projectHash, "HASH");
  assert.equal(manifestPath, "/m.json");
  assert.equal(opt.bridge, "/b.js");
  assert.equal(opt.witness, "/w.js");
});

test("toolCall: well-formed JSON-RPC tools/call envelope", () => {
  const env = driver.toolCall(7, "context_search", { query: "q" });
  assert.deepEqual(env, {
    jsonrpc: "2.0",
    id: 7,
    method: "tools/call",
    params: { name: "context_search", arguments: { query: "q" } },
  });
});
