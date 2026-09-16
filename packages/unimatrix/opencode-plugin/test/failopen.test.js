// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { makeHarness, shellSpy } from "./helpers.js";

// Fail-open transport (NFR-02, R-13): an emit failure must never throw into or
// block the OpenCode session.

test("shell rejecting (non-zero exit / socket down) does not throw; session continues", async () => {
  const { hooks, warnings } = makeHarness({ shell: shellSpy("reject") });
  await assert.doesNotReject(
    hooks.event({ event: { type: "session.created", properties: { info: { id: "s1", directory: "/p" } } } }),
  );
  assert.ok(warnings.some((w) => w.includes("emit failed")), "fail-open warned content-free");
});

test("shell throwing synchronously (ENOENT / binary absent) does not throw", async () => {
  const { hooks } = makeHarness({ shell: shellSpy("throw") });
  await assert.doesNotReject(
    hooks["chat.message"]({ sessionID: "s1" }, { parts: [{ type: "text", text: "hi" }] }),
  );
});

test("missing shell (ctx.sh not a function) is inert, never throws", async () => {
  const { hooks, ctx, calls } = makeHarness();
  ctx.sh = undefined; // simulate a PluginInput with no Bun shell
  await assert.doesNotReject(
    hooks["tool.execute.before"]({ tool: "bash", sessionID: "s1", callID: "c1" }, { args: {} }),
  );
  assert.equal(calls.length, 0);
});

test("fail-open warnings are content-free (no session id / path / model / frame)", async () => {
  const { hooks, warnings } = makeHarness({ shell: shellSpy("reject") });
  await hooks["chat.message"](
    { sessionID: "secret-session-123", model: { providerID: "ollama", modelID: "qwen3-coder" } },
    { parts: [{ type: "text", text: "/home/user/secret/path" }] },
  );
  for (const w of warnings) {
    assert.ok(!w.includes("secret-session-123"));
    assert.ok(!w.includes("/home/user/secret/path"));
    assert.ok(!w.includes("qwen3-coder"));
  }
});
