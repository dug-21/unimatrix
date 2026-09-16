// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { makeHarness, lastEmit } from "./helpers.js";

// Subagent observe-channel provenance (FR-07, R-06, R-07, AC-04) + parity-gap honesty.

test("child session.created with parentID + validated agent derives SubagentStart", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({
    event: {
      type: "session.created",
      properties: { info: { id: "child1", parentID: "parent1", directory: "/p", agent: "reviewer" } },
    },
  });
  const { argv, frame } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "SubagentStart", "--provider", "opencode"]);
  assert.equal(frame.hook_event_name, "SubagentStart");
  assert.equal(frame.session_id, "child1");
});

test("validated session.agent rides extra.agent_type (observe channel)", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({
    event: { type: "session.created", properties: { info: { id: "c", parentID: "p", agent: "reviewer" } } },
  });
  const { frame } = lastEmit(calls);
  assert.equal(frame.agent_type, "reviewer");
});

test("SubagentStart frame carries parentID for downstream cycle correlation", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({
    event: { type: "session.created", properties: { info: { id: "c", parentID: "p42", agent: "reviewer" } } },
  });
  const { frame } = lastEmit(calls);
  assert.equal(frame.parent_id, "p42");
});

test("spoof rejection: agent value inside tool args does NOT populate extra.agent_type", async () => {
  const { hooks, calls } = makeHarness();
  // A crafted PreToolUse whose args try to inject an agent identity.
  await hooks["tool.execute.before"](
    { tool: "bash", sessionID: "s1", callID: "c1" },
    { args: { agent_type: "admin", agent: "root", command: "ls" } },
  );
  const { frame } = lastEmit(calls);
  // agent_type is never lifted from tool args; the spoof rides only inside tool_input.
  assert.ok(!("agent_type" in frame), "agent_type must not come from tool args");
  assert.equal(frame.tool_input.agent_type, "admin"); // preserved verbatim as tool data, harmless
});

test("conflicting tool-args agent does not override validated session.agent", async () => {
  const { hooks, calls } = makeHarness();
  // Subagent established with validated agent...
  await hooks.event({
    event: { type: "session.created", properties: { info: { id: "s1", parentID: "p", agent: "reviewer" } } },
  });
  const subagentFrame = lastEmit(calls).frame;
  assert.equal(subagentFrame.agent_type, "reviewer");
  // ...a later tool call in that session with a spoofed agent does not change it.
  await hooks["tool.execute.before"]({ tool: "bash", sessionID: "s1", callID: "c1" }, { args: { agent_type: "admin" } });
  const toolFrame = lastEmit(calls).frame;
  assert.ok(!("agent_type" in toolFrame));
});

test("agent absent → agent_type omitted (honest), parent_id still carried (R-06 link preserved)", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({
    event: { type: "session.created", properties: { info: { id: "c", parentID: "p" } } }, // no agent
  });
  const { frame } = lastEmit(calls);
  assert.ok(!("agent_type" in frame), "no fabricated agent");
  assert.equal(frame.parent_id, "p");
});

test("subagent injection is not attempted; only the observe frame is emitted", async () => {
  const { hooks, calls, ctx } = makeHarness();
  await hooks.event({
    event: { type: "session.created", properties: { info: { id: "c", parentID: "p", agent: "reviewer" } } },
  });
  // Exactly one shell-out (the observe emit); the plugin exposes no injection path.
  assert.equal(calls.length, 1);
  const { argv } = lastEmit(calls);
  assert.equal(argv[1], "SubagentStart");
  // The plugin never reads back / injects context — no client retrieval call is wired.
  assert.deepEqual(Object.keys(ctx.client), []);
});
