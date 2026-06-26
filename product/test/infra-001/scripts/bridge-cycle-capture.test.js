"use strict";

// bridge-cycle-capture.test.js (nan-022 C2') — OFF-Docker driver-shape / contract
// tests for the MCP-bridge-surface capture helpers. Pure-logic checks that need NO
// live bridge: parser PARITY with the Python oracle (id/score ordering, score-presence
// semantics), MCP-arguments parity, double-capture shape, and the emitted bundle
// fragment contract. The LIVE end-to-end drive is Stage 3c / Docker (the matrix
// orchestrator + cross-language bundle contract).
//
// Run: node --test scripts/bridge-cycle-capture.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const cap = require("./bridge-cycle-capture.js");

// A tiny fake `drive` + `resultText` so driveRetrieval/driveBriefing run off-Docker:
// drive(name, args) -> a fake bridge response whose text the scripted map provides,
// keyed by the JSON of (name, args). resultText extracts our fake text field.
function makeDrive(responsesByCall) {
  const calls = [];
  const drive = async (name, args) => {
    calls.push({ name, args });
    const key = JSON.stringify({ name, args });
    const text = responsesByCall[key] !== undefined ? responsesByCall[key] : "{}";
    return { _text: text };
  };
  const resultText = (resp) => (resp && resp._text) || "";
  return { drive, resultText, calls };
}

// ===========================================================================
// parseRankedResult — RANKED id order + score-presence semantics (oracle parity)
// ===========================================================================

test("parseRankedResult: ids in RANKED (response) order, scores aligned", () => {
  const text = JSON.stringify({
    entries: [
      { id: 7, score: 0.9 },
      { id: 3, score: 0.8 },
      { id: 5, score: 0.7 },
    ],
  });
  const { ids, scores } = cap.parseRankedResult(text);
  assert.deepEqual(ids, [7, 3, 5], "ids preserve server RANKED order, not sorted");
  assert.deepEqual(scores, [0.9, 0.8, 0.7]);
});

test("parseRankedResult: scores null when ANY entry lacks a score (membership-only)", () => {
  const text = JSON.stringify({ entries: [{ id: 1, score: 0.9 }, { id: 2 }] });
  const { ids, scores } = cap.parseRankedResult(text);
  assert.deepEqual(ids, [1, 2]);
  assert.equal(scores, null, "one missing score -> degrade to membership-only (K3)");
});

test("parseRankedResult: accepts top-level results[] and bare [...]", () => {
  const r = cap.parseRankedResult(JSON.stringify({ results: [{ id: 9, similarity: 0.5 }] }));
  assert.deepEqual(r.ids, [9]);
  assert.deepEqual(r.scores, [0.5], "similarity is an accepted score key");
  const bare = cap.parseRankedResult(JSON.stringify([{ id: 4, score: 0.1 }]));
  assert.deepEqual(bare.ids, [4]);
  assert.deepEqual(bare.scores, [0.1]);
});

test("parseRankedResult: unparseable/empty -> empty ranking (Python guard names it INFRA)", () => {
  assert.deepEqual(cap.parseRankedResult("not json"), { ids: [], scores: null });
  assert.deepEqual(cap.parseRankedResult(JSON.stringify({ entries: [] })), { ids: [], scores: null });
});

// ===========================================================================
// parseBriefingResult — ranked ids + injection set mapping (oracle parity)
// ===========================================================================

test("parseBriefingResult: ranked ids, scores, injection_set mapped to ids", () => {
  const text = JSON.stringify({
    entries: [{ id: 11, score: 0.6 }, { id: 12, score: 0.4 }],
    injection_set: [{ id: 11 }, 12],
  });
  const r = cap.parseBriefingResult(text);
  assert.deepEqual(r.ids, [11, 12]);
  assert.deepEqual(r.scores, [0.6, 0.4]);
  assert.deepEqual(r.injection_set, [11, 12], "dict entries -> id, scalars pass through");
});

test("parseBriefingResult: 'injected' alias + empty on garbage", () => {
  const r = cap.parseBriefingResult(JSON.stringify({ results: [{ id: 1 }], injected: [{ id: 1 }] }));
  assert.deepEqual(r.injection_set, [1]);
  assert.deepEqual(cap.parseBriefingResult("garbage"), { ids: [], scores: null, injection_set: [] });
});

test("parseBriefingResult: text-table fallback (the shape context_briefing emits)", () => {
  // context_briefing does NOT honour format=json — it returns this ranked table.
  const text = [
    "Use context_get with the entry ID for full content when relevant.",
    "",
    " #      id  topic                 cat               conf  snippet",
    "--  ------  --------------------  --------------  ------  --------",
    " 1       3  nan-022-parity-corpu  pattern           0.67  entry 02",
    " 2       1  nan-022-parity-corpu  pattern           0.63  entry 00",
    " 3       2  nan-022-parity-corpu  pattern           0.61  entry 01",
  ].join("\n");
  const r = cap.parseBriefingResult(text);
  assert.deepEqual(r.ids, [3, 1, 2], "ranked id column in printed order");
  assert.deepEqual(r.scores, [0.67, 0.63, 0.61], "conf column -> aligned scores");
  assert.deepEqual(r.injection_set, [], "table carries no injection set");
});

test("parseBriefingTable: header/rule rows ignored; ids-only when conf absent", () => {
  const text = [" #  id  topic", "--  --  -----", " 1  9  t", " 2  8  t"].join("\n");
  const r = cap.parseBriefingTable(text);
  assert.deepEqual(r.ids, [9, 8]);
  assert.equal(r.scores, null, "no conf column -> membership-only (scores null)");
});

// ===========================================================================
// MCP-arguments parity — byte-identical to the UDS leg's MCP arguments
// ===========================================================================

test("retrievalArgs: search defaults format=json, keeps query + whitelisted k", () => {
  const args = cap.retrievalArgs({ name: "context_search", args: { query: "q", k: 5 } });
  assert.deepEqual(args, { query: "q", k: 5, format: "json" });
});

test("retrievalArgs: lookup whitelist drops unknown keys (mirrors _clean)", () => {
  const args = cap.retrievalArgs({
    name: "context_lookup",
    args: { topic: "t", category: "c", bogus: "x" },
  });
  assert.deepEqual(args, { topic: "t", category: "c", format: "json" });
  assert.ok(!("bogus" in args), "non-whitelisted key dropped");
});

test("retrievalArgs: get coerces id to int (mirrors int(args.pop('id')))", () => {
  const a1 = cap.retrievalArgs({ name: "context_get", args: { id: "42" } });
  assert.equal(a1.id, 42);
  const a2 = cap.retrievalArgs({ name: "context_get", args: { entry_id: 7 } });
  assert.equal(a2.id, 7, "entry_id fallback");
});

test("briefingArgs: role defaults to 'tester', task rides through, format=json", () => {
  const args = cap.briefingArgs({ name: "context_briefing", args: { task: "do x" } });
  assert.deepEqual(args, { role: "tester", task: "do x", format: "json" });
});

test("briefingArgs: explicit role/format preserved", () => {
  const args = cap.briefingArgs({
    name: "context_briefing",
    args: { role: "dev", task: "t", format: "markdown" },
  });
  assert.deepEqual(args, { role: "dev", task: "t", format: "markdown" });
});

// ===========================================================================
// Analytics secondary captures — informs_edges + phase_signal (oracle parity)
// ===========================================================================

test("informsEdgesFromReport: informs_edges dicts -> ids; edges[] alias; absent -> []", () => {
  assert.deepEqual(
    cap.informsEdgesFromReport({ informs_edges: [{ id: 1 }, { edge_id: 2 }, 3] }),
    [1, 2, 3]
  );
  assert.deepEqual(cap.informsEdgesFromReport({ edges: [9] }), [9], "edges[] alias");
  assert.deepEqual(cap.informsEdgesFromReport({}), [], "absent -> [] (comparator surfaces diff)");
  assert.deepEqual(cap.informsEdgesFromReport(null), []);
});

test("phaseSignalFromMetricVector: returns phases mapping or {}", () => {
  assert.deepEqual(
    cap.phaseSignalFromMetricVector({ phases: { delivery: { n: 3 } } }),
    { delivery: { n: 3 } }
  );
  assert.deepEqual(cap.phaseSignalFromMetricVector({}), {}, "absent phases -> {}");
  assert.deepEqual(cap.phaseSignalFromMetricVector({ phases: [1, 2] }), {}, "non-dict phases -> {}");
  assert.deepEqual(cap.phaseSignalFromMetricVector(null), {});
});

// ===========================================================================
// driveRetrieval / driveBriefing — capture shape + bridge-in-path discipline
// ===========================================================================

test("driveRetrieval: emits {tool,args,result_ids,scores}; original args echoed", async () => {
  const call = { name: "context_search", args: { query: "q", k: 5 } };
  const { drive, resultText, calls } = makeDrive({
    [JSON.stringify({ name: "context_search", args: cap.retrievalArgs(call) })]: JSON.stringify({
      entries: [{ id: 1, score: 0.9 }, { id: 2, score: 0.8 }],
    }),
  });
  const out = await cap.driveRetrieval([call], drive, resultText);
  assert.equal(out.length, 1);
  assert.deepEqual(out[0], {
    tool: "context_search",
    args: { query: "q", k: 5 },
    result_ids: [1, 2],
    scores: [0.9, 0.8],
  });
  // bridge-in-path: the call rode `drive` (the rpc/toolCall over the SAME bridge).
  assert.equal(calls.length, 1);
  assert.equal(calls[0].name, "context_search");
});

test("driveRetrieval: short/empty result emitted AS-IS (never padded -> INFRA upstream)", async () => {
  const call = { name: "context_search", args: { query: "q" } };
  const { drive, resultText } = makeDrive({}); // no response -> "{}" -> empty ranking
  const out = await cap.driveRetrieval([call], drive, resultText);
  assert.deepEqual(out[0].result_ids, [], "empty emitted, not hidden");
  assert.equal(out[0].scores, null);
});

test("driveBriefing: aggregates ids + injection; scores null if ANY call lacked scores", async () => {
  const c1 = { name: "context_briefing", args: { task: "a" } };
  const c2 = { name: "context_briefing", args: { task: "b" } };
  const { drive, resultText } = makeDrive({
    [JSON.stringify({ name: "context_briefing", args: cap.briefingArgs(c1) })]: JSON.stringify({
      entries: [{ id: 1, score: 0.5 }],
      injection_set: [{ id: 1 }],
    }),
    [JSON.stringify({ name: "context_briefing", args: cap.briefingArgs(c2) })]: JSON.stringify({
      entries: [{ id: 2 }], // no score -> overall null
      injection_set: [2],
    }),
  });
  const r = await cap.driveBriefing([c1, c2], drive, resultText);
  assert.deepEqual(r.ids, [1, 2]);
  assert.equal(r.scores, null, "any score-less call degrades the aggregate to membership-only");
  assert.deepEqual(r.injection_set, [1, 2]);
});

// ===========================================================================
// Bundle fragment contract — the documented retrieval/proactive shapes (R-09)
// These mirror exactly what the driver emits and what the shell/Python ingest.
// ===========================================================================

test("retrieval fragment carries queries + capture_2 (intra double-capture)", async () => {
  const calls = [{ name: "context_search", args: { query: "q", k: 5 } }];
  const { drive, resultText } = makeDrive({
    [JSON.stringify({ name: "context_search", args: cap.retrievalArgs(calls[0]) })]: JSON.stringify({
      entries: [{ id: 1, score: 0.9 }],
    }),
  });
  const r1 = await cap.driveRetrieval(calls, drive, resultText);
  const r2 = await cap.driveRetrieval(calls, drive, resultText);
  const fragment = { queries: r1, capture_2: r2 };
  assert.ok(Array.isArray(fragment.queries) && fragment.queries.length === 1);
  assert.ok(Array.isArray(fragment.capture_2), "capture_2 present for the intra-stability check");
  for (const q of fragment.queries) {
    assert.deepEqual(Object.keys(q).sort(), ["args", "result_ids", "scores", "tool"]);
  }
});

test("proactive fragment carries briefing_ids/scores/injection_set + capture_2", async () => {
  const calls = [{ name: "context_briefing", args: { task: "a" } }];
  const { drive, resultText } = makeDrive({
    [JSON.stringify({ name: "context_briefing", args: cap.briefingArgs(calls[0]) })]: JSON.stringify({
      entries: [{ id: 1, score: 0.5 }],
      injection_set: [{ id: 1 }],
    }),
  });
  const b1 = await cap.driveBriefing(calls, drive, resultText);
  const b2 = await cap.driveBriefing(calls, drive, resultText);
  const fragment = {
    briefing_ids: b1.ids,
    briefing_scores: b1.scores,
    injection_set: b1.injection_set,
    capture_2: { briefing_ids: b2.ids, briefing_scores: b2.scores },
  };
  assert.deepEqual(fragment.briefing_ids, [1]);
  assert.deepEqual(fragment.injection_set, [1]);
  assert.ok("capture_2" in fragment, "capture_2 present for the intra-stability check");
  assert.deepEqual(Object.keys(fragment.capture_2).sort(), ["briefing_ids", "briefing_scores"]);
});
