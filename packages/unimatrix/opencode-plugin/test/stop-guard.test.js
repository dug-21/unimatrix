// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { makeHarness, lastEmit } from "./helpers.js";

// Stop over-count guard (FR-02, R-11, ADR-009).

async function created(hooks, id) {
  await hooks.event({ event: { type: "session.created", properties: { info: { id, directory: "/p" } } } });
}
async function idle(hooks, id) {
  await hooks.event({ event: { type: "session.idle", properties: { sessionID: id } } });
}

test("repeated session.idle produces exactly one Stop per active window", async () => {
  const { hooks, calls } = makeHarness();
  await created(hooks, "s1"); // 1 emit (SessionStart)
  await idle(hooks, "s1"); // 2 emit (Stop)
  await idle(hooks, "s1"); // no-op
  await idle(hooks, "s1"); // no-op
  const stops = calls.filter((c) => c.subs[0][1] === "Stop");
  assert.equal(stops.length, 1);
});

test("Stop re-arms on the next chat.message (new active window → new Stop)", async () => {
  const { hooks, calls } = makeHarness();
  await created(hooks, "s1");
  await idle(hooks, "s1"); // Stop #1
  await hooks["chat.message"]({ sessionID: "s1" }, { parts: [{ type: "text", text: "again" }] }); // re-arm
  await idle(hooks, "s1"); // Stop #2
  const stops = calls.filter((c) => c.subs[0][1] === "Stop");
  assert.equal(stops.length, 2);
});

test("idle for an unknown/never-active session emits no Stop", async () => {
  const { hooks, calls } = makeHarness();
  await idle(hooks, "ghost");
  assert.equal(calls.length, 0);
});

test("Stop computes duration and outcome plugin-side", async () => {
  const { hooks, calls } = makeHarness();
  await created(hooks, "s1");
  await idle(hooks, "s1");
  const { frame } = lastEmit(calls);
  assert.equal(frame.hook_event_name, "Stop");
  assert.equal(frame.outcome, "idle");
  assert.equal(typeof frame.duration, "number");
  assert.ok(frame.duration >= 0);
});
