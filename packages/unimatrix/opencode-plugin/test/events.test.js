// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { makeHarness, lastEmit } from "./helpers.js";

// 7-event mapping (FR-01, AC-01, R-16) + bus-derived field synthesis (R-14).

test("session.created maps to SessionStart, cwd from session directory, no transcript_path", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({
    event: { type: "session.created", properties: { info: { id: "s1", directory: "/proj/here" } } },
  });
  assert.equal(calls.length, 1);
  const { argv, frame } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "SessionStart", "--provider", "opencode"]);
  assert.equal(frame.hook_event_name, "SessionStart");
  assert.equal(frame.session_id, "s1");
  assert.equal(frame.cwd, "/proj/here");
  assert.ok(!("transcript_path" in frame), "bus-derived frame omits transcript_path");
  assert.equal(frame.bus_derived, true);
});

test("session.created without directory derives cwd from PluginInput.worktree", async () => {
  const { hooks, calls } = makeHarness({ worktree: "/proj/wt", directory: "/proj" });
  await hooks.event({ event: { type: "session.created", properties: { info: { id: "s2" } } } });
  const { frame } = lastEmit(calls);
  assert.equal(frame.cwd, "/proj/wt");
});

test("chat.message maps to UserPromptSubmit with prompt and --model", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["chat.message"](
    { sessionID: "s1", model: { providerID: "ollama", modelID: "qwen3-coder" } },
    { message: { content: "" }, parts: [{ type: "text", text: "hello " }, { type: "text", text: "world" }] },
  );
  const { argv, frame } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "UserPromptSubmit", "--provider", "opencode", "--model", "ollama/qwen3-coder"]);
  assert.equal(frame.hook_event_name, "UserPromptSubmit");
  assert.equal(frame.prompt, "hello world");
  // provider/model_id are CLI-only, never frame fields (wire.rs).
  assert.ok(!("provider" in frame) && !("model_id" in frame));
});

test("tool.execute.before maps to PreToolUse with tool_name and tool_input", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["tool.execute.before"]({ tool: "bash", sessionID: "s1", callID: "c1" }, { args: { command: "ls" } });
  const { argv, frame } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "PreToolUse", "--provider", "opencode"]);
  assert.equal(frame.hook_event_name, "PreToolUse");
  assert.equal(frame.tool_name, "bash");
  assert.deepEqual(frame.tool_input, { command: "ls" });
});

test("tool.execute.after maps to PostToolUse; failure synthesizes non-zero exit_code", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["tool.execute.after"](
    { tool: "bash", sessionID: "s1", callID: "c1", args: { command: "false" } },
    { title: "t", output: "err text", metadata: { error: "boom" } },
  );
  const { frame } = lastEmit(calls);
  assert.equal(frame.hook_event_name, "PostToolUse");
  assert.equal(frame.tool_name, "bash");
  assert.equal(frame.tool_response, "err text");
  assert.equal(frame.exit_code, 1);
});

test("tool.execute.after success path yields exit_code 0", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["tool.execute.after"](
    { tool: "read", sessionID: "s1", callID: "c1", args: {} },
    { title: "t", output: "ok", metadata: {} },
  );
  const { frame } = lastEmit(calls);
  assert.equal(frame.exit_code, 0);
});

test("model cache: tool events after chat.message carry the session model", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["chat.message"](
    { sessionID: "s1", model: { providerID: "ollama", modelID: "qwen3-coder" } },
    { parts: [{ type: "text", text: "hi" }] },
  );
  await hooks["tool.execute.before"]({ tool: "bash", sessionID: "s1", callID: "c1" }, { args: {} });
  const { argv } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "PreToolUse", "--provider", "opencode", "--model", "ollama/qwen3-coder"]);
});

test("event with no backend model omits --model (frame still valid)", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["chat.message"]({ sessionID: "s1" }, { parts: [{ type: "text", text: "hi" }] });
  const { argv } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "UserPromptSubmit", "--provider", "opencode"]);
});

test("degraded legs (SessionStart/Stop) carry a bus-derived marker, not fabricated fidelity", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1", directory: "/p" } } } });
  const start = lastEmit(calls).frame;
  await hooks.event({ event: { type: "session.idle", properties: { sessionID: "s1" } } });
  const stop = lastEmit(calls).frame;
  assert.equal(start.bus_derived, true);
  assert.equal(stop.bus_derived, true);
  assert.equal(stop.degraded, true);
});

test("malformed bus payload degrades honestly (no frame emitted)", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({ event: { type: "session.created", properties: { info: {} } } }); // no id
  await hooks.event({ event: { type: "session.created", properties: {} } }); // no info
  await hooks.event({ event: null });
  assert.equal(calls.length, 0);
});

test("unknown bus event types are ignored", async () => {
  const { hooks, calls } = makeHarness();
  await hooks.event({ event: { type: "file.edited", properties: { file: "x" } } });
  assert.equal(calls.length, 0);
});
