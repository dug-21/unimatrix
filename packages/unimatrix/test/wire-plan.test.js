"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const { buildWirePlan, runWirePlan, OK_ACTIONS } = require("../lib/wire-plan.js");
const { resolveConfigDirs } = require("../lib/config-dirs.js");
const { SafeWriteError } = require("../lib/safe-write.js");
const { makeProject, makeHome, listTree } = require("./helpers/wire-fixture.js");

const noGit = () => ({ status: 128, stdout: "" });
const noCodex = () => null;

function ctxFor(env, extra) {
  const root = makeProject("unimatrix-wire-plan-");
  const home = makeHome();
  const e = {};
  for (const [k, v] of Object.entries(env || {})) e[k] = v.replace("<home>", home);
  const dirs = resolveConfigDirs({ env: e, home, platform: process.platform });
  return Object.assign({ root, home, platform: process.platform, env: e, node: process.execPath, dirs,
    harnessSel: null, writeMcp: true, hooksOnly: false, gitRun: noGit, pathLookup: noCodex }, extra || {});
}
const ids = (c) => buildWirePlan(c).map((s) => s.id);
const key = (m) => m.map((l) => [l.harness, l.surface, l.action, l.path, l.reason, JSON.stringify(l.entry), l.command]);

test("OK_ACTIONS", () => {
  assert.deepStrictEqual([...OK_ACTIONS].sort(), ["created", "unchanged", "updated"]);
});

test("no overrides: ids in §3.3 order, no R4/R10/R5", () => {
  const c = ctxFor({});
  fs.mkdirSync(path.join(c.root, ".opencode"));
  assert.deepStrictEqual(ids(c), ["W1", "W2", "R2", "R3", "R6", "CX", "W5", "R11", "X1"]);
});

test("CLAUDE_CONFIG_DIR=<d>: R4 targets legacy settings.json; no step on ~/.claude.json", () => {
  const c = ctxFor({ CLAUDE_CONFIG_DIR: "<home>/alt" });
  const steps = buildWirePlan(c);
  assert.deepStrictEqual(steps.map((s) => s.id), ["W1", "W2", "R2", "R3", "R4", "R6", "CX", "R11", "X1"]);
  assert.strictEqual(steps.find((s) => s.id === "R4").file, path.join(c.home, ".claude", "settings.json"));
  assert.ok(!steps.some((s) => s.file === path.join(c.home, ".claude.json")));
});

test("CLAUDE_CONFIG_DIR=<home>: R4 present (different file)", () => {
  assert.ok(ids(ctxFor({ CLAUDE_CONFIG_DIR: "<home>" })).includes("R4"));
});

test("CLAUDE_CONFIG_DIR=<home>/.claude (same real file): no R4", () => {
  assert.ok(!ids(ctxFor({ CLAUDE_CONFIG_DIR: "<home>/.claude/" })).includes("R4"));
});

for (const env of [
  { XDG_CONFIG_HOME: "<home>/.config" },
  { OPENCODE_CONFIG_DIR: "<home>/.config/opencode" },
  { OPENCODE_CONFIG_DIR: "<home>/.config/opencode/" },
]) {
  test("same-file opencode override → no R10: " + JSON.stringify(env), () => {
    const s = ids(ctxFor(env, { harnessSel: "opencode" }));
    assert.ok(s.includes("W5") && !s.includes("R10"));
  });
}

test("symlinked OPENCODE_CONFIG_DIR to legacy dir → no R10", () => {
  const c = ctxFor({});
  fs.mkdirSync(path.join(c.home, ".config", "opencode"), { recursive: true });
  fs.symlinkSync(path.join(c.home, ".config", "opencode"), path.join(c.home, "oc"));
  const dirs = resolveConfigDirs({ env: { OPENCODE_CONFIG_DIR: path.join(c.home, "oc") }, home: c.home, platform: c.platform });
  assert.ok(!ids({ ...c, dirs, harnessSel: "opencode" }).includes("R10"));
});

test("alternates never produce a step", () => {
  const c = ctxFor({ CLAUDE_CONFIG_DIR: "<home>/a", OPENCODE_CONFIG_DIR: "<home>/o", XDG_CONFIG_HOME: "<home>/x", CODEX_HOME: "<home>/c" },
    { harnessSel: null });
  fs.mkdirSync(path.join(c.home, "c"));
  fs.mkdirSync(path.join(c.root, ".opencode"));
  const files = new Set(buildWirePlan(c).map((s) => s.file));
  assert.ok(c.dirs.alternates.length > 0);
  for (const a of c.dirs.alternates) assert.ok(!files.has(a.file), a.file);
});

test("harnessSel / hooksOnly / writeMcp:false filter", () => {
  assert.deepStrictEqual(ids(ctxFor({}, { harnessSel: "claude-code" })), ["W1", "W2", "R2", "R3", "R6", "R11", "X1"]);
  assert.deepStrictEqual(ids(ctxFor({}, { harnessSel: "codex-cli" })), ["CX", "R11"]);
  assert.deepStrictEqual(ids(ctxFor({}, { hooksOnly: true, writeMcp: false })), ["W1", "R2", "R3", "X1"]);
  assert.ok(!ids(ctxFor({}, { writeMcp: false })).includes("W2"));
  assert.ok(ids(ctxFor({}, { harnessSel: "opencode" })).includes("W5"));
});

test("dry-run manifest equals real manifest; dry-run writes nothing", () => {
  for (const env of [{}, { CLAUDE_CONFIG_DIR: "<home>/alt" }, { OPENCODE_CONFIG_DIR: "<home>/o" }]) {
    const c = ctxFor(env, { harnessSel: null });
    fs.mkdirSync(path.join(c.root, ".opencode"));
    const before = [listTree(c.root), listTree(c.home)];
    const dry = runWirePlan(buildWirePlan(c), { dryRun: true });
    assert.deepStrictEqual([listTree(c.root), listTree(c.home)], before);
    const real = runWirePlan(buildWirePlan(c), { dryRun: false });
    assert.deepStrictEqual(key(dry.manifest), key(real.manifest), JSON.stringify(env));
    assert.deepStrictEqual(dry.warnings, real.warnings);
    // Fixpoint: second real run is all unchanged / no removals.
    const again = runWirePlan(buildWirePlan(c), { dryRun: false }).manifest;
    for (const l of again) assert.ok(!["created", "updated", "removed"].includes(l.action), JSON.stringify(l));
  }
});

test("override source stamped on resolved legs only", () => {
  const c = ctxFor({ CLAUDE_CONFIG_DIR: "<home>/alt" }, { harnessSel: "claude-code" });
  const m = runWirePlan(buildWirePlan(c), { dryRun: true }).manifest;
  assert.strictEqual(m.find((l) => l.surface === "mcp" && l.path === c.dirs.claude.claudeJson).source, "CLAUDE_CONFIG_DIR");
  assert.strictEqual(m.find((l) => l.surface === "hooks").source, undefined);
});

test("R4 removes owned legacy hooks after W1 ok; foreign hooks kept", () => {
  const c = ctxFor({ CLAUDE_CONFIG_DIR: "<home>/alt" }, { harnessSel: "claude-code" });
  runWirePlan(buildWirePlan(c), { dryRun: false });
  const owned = JSON.parse(fs.readFileSync(path.join(c.root, ".claude", "settings.local.json"), "utf8"));
  owned.hooks.Stop.push({ hooks: [{ type: "command", command: "echo foreign" }] });
  const legacy = c.dirs.legacy.userSettings;
  fs.mkdirSync(path.dirname(legacy), { recursive: true });
  fs.writeFileSync(legacy, JSON.stringify(owned, null, 2));
  const m = runWirePlan(buildWirePlan(c), { dryRun: false }).manifest;
  assert.ok(m.some((l) => l.path === legacy && l.action === "removed" && /legacy/.test(l.reason)));
  assert.match(fs.readFileSync(legacy, "utf8"), /echo foreign/);
  assert.doesNotMatch(fs.readFileSync(legacy, "utf8"), /unimatrix/);
});

test("gate: W5 skipped-intent → R10 skipped-dest-unwired, legacy bytes unchanged (both modes)", () => {
  const c = ctxFor({ OPENCODE_CONFIG_DIR: "<home>/o" });
  fs.mkdirSync(path.join(c.root, ".opencode"));
  const legacy = c.dirs.legacy.opencodeJson;
  fs.mkdirSync(path.dirname(legacy), { recursive: true });
  const bytes = '{\n  "mcp": {\n    "other": {"type": "local", "command": ["x"]}\n  }\n}\n';
  fs.writeFileSync(legacy, bytes);
  for (const dryRun of [true, false]) {
    const m = runWirePlan(buildWirePlan(c), { dryRun }).manifest;
    const w5 = m.find((l) => l.path === c.dirs.opencode.configJson);
    assert.ok(!OK_ACTIONS.has(w5.action), w5.action);
    const r10 = m.find((l) => l.path === legacy);
    assert.strictEqual(r10.action, "skipped-dest-unwired");
    assert.strictEqual(r10.harness, "opencode");
    assert.match(r10.reason, /not wired/);
    assert.strictEqual(fs.readFileSync(legacy, "utf8"), bytes);
  }
});

test("gate: destination skipped-foreign / skipped-malformed → skipped-dest-unwired", () => {
  for (const action of ["skipped-foreign", "skipped-malformed"]) {
    for (const dryRun of [true, false]) {
      let ran = false;
      const steps = [
        { id: "W", harness: "h", surface: "s", file: "/d", run: () => [{ harness: "h", surface: "s", action, path: "/d" }] },
        { id: "R", harness: "h", surface: "s", file: "/l", gate: "W", run: () => { ran = true; return []; } },
      ];
      const m = runWirePlan(steps, { dryRun }).manifest;
      assert.strictEqual(ran, false);
      assert.deepStrictEqual(m[1], { harness: "h", surface: "s", action: "skipped-dest-unwired", path: "/l", reason: "destination /d not wired" });
    }
  }
});

test("SafeWriteError from a step rethrown; no later step runs", () => {
  const calls = [];
  const steps = [
    { id: "W1", harness: "h", surface: "s", file: "/a", run: () => { calls.push("W1"); throw new SafeWriteError("conflict", "/a", "boom"); } },
    { id: "R3", harness: "h", surface: "s", file: "/b", run: () => { calls.push("R3"); return []; } },
  ];
  assert.throws(() => runWirePlan(steps, { dryRun: false }), (e) => e instanceof SafeWriteError);
  assert.deepStrictEqual(calls, ["W1"]);
});

test("real plan: W1 throw (malformed settings.local.json) leaves R3 file untouched", () => {
  const c = ctxFor({}, { harnessSel: "claude-code" });
  fs.mkdirSync(path.join(c.root, ".claude"));
  fs.writeFileSync(path.join(c.root, ".claude", "settings.local.json"), "{ nope");
  fs.mkdirSync(path.dirname(c.dirs.claude.userSettings), { recursive: true });
  const r3 = '{"hooks":{}}';
  fs.writeFileSync(c.dirs.claude.userSettings, r3);
  assert.throws(() => runWirePlan(buildWirePlan(c), { dryRun: false }));
  assert.strictEqual(fs.readFileSync(c.dirs.claude.userSettings, "utf8"), r3);
});

test("legs.warnings (no-leg warnings) collected", () => {
  const legs = Object.assign([], { warnings: ["w1"] });
  const out = runWirePlan([{ id: "X", harness: "h", surface: "s", file: "/x", run: () => legs }], {});
  assert.deepStrictEqual(out.warnings, ["w1"]);
});

test("static: module <= 300 code lines", () => {
  const src = fs.readFileSync(path.join(__dirname, "..", "lib", "wire-plan.js"), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "").split("\n").filter((l) => l.trim() && !l.trim().startsWith("//"));
  assert.ok(src.length <= 300, String(src.length));
});
