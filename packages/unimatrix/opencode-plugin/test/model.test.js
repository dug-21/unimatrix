// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveModel, isValidModelId } from "../lib/model.js";
import { makeHarness, lastEmit } from "./helpers.js";

// Model resolution + untrusted-input validation at the shim boundary (R-15, AC-06).

test("resolveModel composes providerID/modelID", () => {
  assert.equal(resolveModel({ providerID: "ollama", modelID: "qwen3-coder" }), "ollama/qwen3-coder");
});

test("resolveModel returns undefined for absent/partial model", () => {
  assert.equal(resolveModel(undefined), undefined);
  assert.equal(resolveModel({}), undefined);
  assert.equal(resolveModel({ providerID: "ollama" }), undefined);
});

test("isValidModelId enforces the C4 carrier charset (^[a-z0-9._/-]{1,128}$)", () => {
  assert.equal(isValidModelId("ollama/qwen3-coder"), true);
  assert.equal(isValidModelId("Ollama/Qwen"), false); // uppercase rejected
  assert.equal(isValidModelId("has space"), false);
  assert.equal(isValidModelId("a".repeat(129)), false); // over length
  assert.equal(isValidModelId(""), false);
});

test("over-length / bad-char model is rejected at the shim: --model omitted, never passed raw", async () => {
  const { hooks, calls } = makeHarness();
  await hooks["chat.message"](
    { sessionID: "s1", model: { providerID: "evil provider", modelID: "x".repeat(200) } },
    { parts: [{ type: "text", text: "hi" }] },
  );
  const { argv } = lastEmit(calls);
  assert.deepEqual(argv, ["hook", "UserPromptSubmit", "--provider", "opencode"]);
  assert.ok(!argv.includes("--model"));
});
