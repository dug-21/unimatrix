// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { makeHarness, lastEmit, shellSpy } from "./helpers.js";

// PreCompact experimental dependency (FR-12, R-12, C-6, OQ-7).

test("PreCompact present-path lands (default enabled)", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["experimental.session.compacting"]({ sessionID: "s1" }, { context: [] });
  const { argv, frame } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "PreCompact", "--provider", "opencode"]);
  assert.equal(frame.hook_event_name, "PreCompact");
  assert.equal(frame.experimental, true);
});

test("PreCompact disabled via plugin option → gated no-op, no throw", async () => {
  const { hooks, calls } = makeHarness({ options: { disablePreCompact: true } });
  await assert.doesNotReject(hooks["experimental.session.compacting"]({ sessionID: "s1" }, { context: [] }));
  assert.equal(calls.length, 0);
});

test("PreCompact handler fails safe on a hostile payload; other legs still function", async () => {
  const { hooks, calls } = makeHarness();
  // A malformed input (getter throws) must not escape or crash the session.
  const boom = Object.defineProperty({}, "sessionID", {
    get() {
      throw new Error("experimental API drift");
    },
  });
  await assert.doesNotReject(hooks["experimental.session.compacting"](boom, {}));
  // Other legs still work afterwards.
  await hooks["chat.message"]({ sessionID: "s1" }, { parts: [{ type: "text", text: "hi" }] });
  assert.ok(calls.some((c) => c.subs[0][1] === "UserPromptSubmit"));
});

test("PreCompact emit failure is fail-open (session survives)", async () => {
  const { hooks } = makeHarness({ shell: shellSpy("reject") });
  await assert.doesNotReject(hooks["experimental.session.compacting"]({ sessionID: "s1" }, { context: [] }));
});
