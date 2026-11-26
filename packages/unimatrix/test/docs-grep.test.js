"use strict";
// AC-11 docs grep (D-083): no Rust-form `unimatrix hook` in README / .gemini reference config.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "..", "..", "..");
const RUST_HOOK = /(?<!\.unimatrix\/bin\/)(?<![\w.-])unimatrix(-server)?["']?\s+hook\b/;
const cases = JSON.parse(
  fs.readFileSync(path.join(__dirname, "fixtures", "docs-grep", "cases.json"), "utf8")
);

function violates(line) {
  return RUST_HOOK.test(line) || (line.includes("LD_LIBRARY_PATH=") && /\bhook\b/.test(line));
}
function violations(text) {
  return text.split("\n").map((l, i) => (violates(l) ? i + 1 : 0)).filter(Boolean);
}
const read = (p) => fs.readFileSync(path.join(ROOT, p), "utf8");

test("rule self-test: negative fixtures fail", () => {
  for (const f of cases.mustFail) assert.ok(violates(f), "should fail: " + f);
});

test("rule self-test: positive fixtures pass", () => {
  for (const f of cases.mustPass) assert.ok(!violates(f), "should pass: " + f);
});

test("README.md and .gemini/settings.json have no Rust hook form", () => {
  for (const p of ["README.md", ".gemini/settings.json"]) {
    assert.deepStrictEqual(violations(read(p)), [], p);
  }
});

test(".gemini/settings.json: four D-083 commands, no _comment", () => {
  const j = JSON.parse(read(".gemini/settings.json"));
  assert.ok(!("_comment" in j) && !("_comment" in j.hooks));
  assert.deepStrictEqual(Object.keys(j.hooks).sort(), ["AfterTool", "BeforeTool", "SessionEnd", "SessionStart"]);
  for (const ev of Object.keys(j.hooks)) {
    const cmds = j.hooks[ev].flatMap((g) => g.hooks.map((h) => h.command));
    assert.deepStrictEqual(cmds, ['"$HOME/.unimatrix/bin/unimatrix" hook ' + ev + " --provider gemini-cli"]);
  }
});

test("README carries D-083 wording, PowerShell and Windows statements", () => {
  const r = read("README.md");
  assert.ok((r.match(/launcher's `hook` verb/g) || []).length >= 3);
  assert.ok(r.split("\n").some((l) => /PowerShell/.test(l) && /unsupported/i.test(l)));
  assert.ok(/absolute path of `unimatrix\.cmd`/.test(r));
  assert.ok(/retrieval only/.test(r));
});

test("guard: Rust-form arms of the plain AC-11 check absent in those files", () => {
  for (const p of ["README.md", ".gemini/settings.json"]) {
    const t = read(p);
    assert.ok(!/(^|[\s"'`])unimatrix(-server)?["']?\s+hook\s+[A-Z]/m.test(t.replace(/\.unimatrix\/bin\/unimatrix"?\s+hook/g, "")), p);
  }
});

test("release notes state OpenCode retrieval-only and headless root requirement", () => {
  const c = read("CHANGELOG.md");
  assert.ok(/OpenCode is retrieval only/.test(c));
  assert.ok(/UNIMATRIX_PROJECT_ROOT/.test(c));
});

test("docs/client-setup.md lists UNIMATRIX_PROJECT_ROOT", () => {
  assert.ok(/UNIMATRIX_PROJECT_ROOT/.test(fs.readFileSync(path.join(ROOT, "docs", "client-setup.md"), "utf8")));
});
