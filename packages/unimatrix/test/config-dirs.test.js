"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { resolveConfigDirs, sameRealFile, codexHome, realPathForm, VARS } = require("../lib/config-dirs.js");

const H = "/home/u";
const R = (env) => resolveConfigDirs({ env, home: H, platform: "linux" });

test("defaults", () => {
  const r = R({});
  assert.strictEqual(r.claude.dir, "/home/u/.claude");
  assert.strictEqual(r.claude.claudeJson, "/home/u/.claude.json");
  assert.strictEqual(r.claude.userSettings, "/home/u/.claude/settings.json");
  assert.strictEqual(r.opencode.configJson, "/home/u/.config/opencode/opencode.json");
  assert.strictEqual(r.codex.dir, "/home/u/.codex");
  assert.strictEqual(r.codex.hooksJson, "/home/u/.codex/hooks.json");
  assert.strictEqual(r.codex.configToml, "/home/u/.codex/config.toml");
  for (const h of ["claude", "opencode", "codex"]) {
    assert.strictEqual(r[h].source, "default");
    assert.strictEqual(r[h].value, null);
  }
  assert.deepStrictEqual(r.warnings, []);
  assert.deepStrictEqual(r.alternates, []);
  assert.deepStrictEqual(VARS, ["CLAUDE_CONFIG_DIR", "OPENCODE_CONFIG_DIR", "XDG_CONFIG_HOME", "CODEX_HOME"]);
});

test("CLAUDE_CONFIG_DIR override + alternates (D-028)", () => {
  const r = R({ CLAUDE_CONFIG_DIR: "/d" });
  assert.strictEqual(r.claude.claudeJson, "/d/.claude.json");
  assert.strictEqual(r.claude.userSettings, "/d/settings.json");
  assert.strictEqual(r.claude.source, "CLAUDE_CONFIG_DIR");
  assert.strictEqual(r.claude.value, "/d");
  assert.strictEqual(r.legacy.userSettings, "/home/u/.claude/settings.json");
  const alt = r.alternates.filter((a) => a.harness === "claude-code");
  assert.strictEqual(alt.length, 2);
  for (const a of alt) {
    assert.strictEqual(a.file, "/home/u/.claude.json");
    assert.strictEqual(a.var, "CLAUDE_CONFIG_DIR");
  }
});

test("CLAUDE_CONFIG_DIR=home -> claudeJson equals default path", () => {
  assert.strictEqual(R({ CLAUDE_CONFIG_DIR: H + "/" }).claude.claudeJson, R({}).claude.claudeJson);
});

test("OpenCode precedence", () => {
  let r = R({ XDG_CONFIG_HOME: "/x" });
  assert.strictEqual(r.opencode.configJson, "/x/opencode/opencode.json");
  assert.strictEqual(r.opencode.source, "XDG_CONFIG_HOME");
  assert.deepStrictEqual(r.alternates, []);
  r = R({ OPENCODE_CONFIG_DIR: "/o", XDG_CONFIG_HOME: "/x" });
  assert.strictEqual(r.opencode.configJson, "/o/opencode.json");
  assert.strictEqual(r.opencode.source, "OPENCODE_CONFIG_DIR");
  assert.deepStrictEqual(r.alternates, [
    { harness: "opencode", file: "/x/opencode/opencode.json", key: "mcp.unimatrix", var: "OPENCODE_CONFIG_DIR" },
  ]);
  assert.strictEqual(r.legacy.opencodeJson, "/home/u/.config/opencode/opencode.json");
  assert.strictEqual(R({ OPENCODE_CONFIG_DIR: "/o" }).alternates.length, 0);
});

test("CODEX_HOME matches codexHome and adds alternates", () => {
  const env = { CODEX_HOME: "/c/" };
  const r = R(env);
  assert.strictEqual(r.codex.dir, codexHome(env, H, "linux").dir);
  assert.strictEqual(r.codex.dir, "/c");
  assert.strictEqual(r.codex.source, "CODEX_HOME");
  assert.deepStrictEqual(r.alternates.map((a) => a.file), ["/home/u/.codex/hooks.json", "/home/u/.codex/config.toml"]);
  const bad = codexHome({ CODEX_HOME: "rel" }, H, "linux");
  assert.strictEqual(bad.warning, "unimatrix: CODEX_HOME is not an absolute path; ignoring it and using ~/.codex");
  assert.strictEqual(bad.dir, "/home/u/.codex");
});

test("edge values per var", () => {
  const base = R({});
  for (const v of VARS) {
    assert.deepStrictEqual(R({ [v]: "" }), base, v + " empty");
    for (const bad of ["  ", "~/x", "cfg", "/a\0b"]) {
      const r = R({ [v]: bad });
      assert.strictEqual(r.warnings.length, 1, `${v}=${JSON.stringify(bad)}`);
      assert.ok(r.warnings[0].includes(v));
      assert.deepStrictEqual({ ...r, warnings: [] }, base);
    }
    const ok = R({ [v]: "/abs/" });
    assert.deepStrictEqual(ok.warnings, []);
    assert.ok(!JSON.stringify(ok).includes("/abs/\""), v);
  }
});

test("generic warning text", () => {
  assert.strictEqual(R({ CLAUDE_CONFIG_DIR: "cfg" }).warnings[0],
    'unimatrix: CLAUDE_CONFIG_DIR is not an absolute path ("cfg"); ignoring it and using ~/.claude.json and ~/.claude/settings.json');
  assert.ok(R({ OPENCODE_CONFIG_DIR: "o", XDG_CONFIG_HOME: "/x" }).warnings[0].endsWith("using $XDG_CONFIG_HOME/opencode"));
});

test("purity: frozen env, process.env ignored, never throws", () => {
  const env = Object.freeze({ CLAUDE_CONFIG_DIR: "/d" });
  process.env.CLAUDE_CONFIG_DIR = "/sentinel";
  try {
    assert.strictEqual(R(env).claude.value, "/d");
    assert.strictEqual(R({}).claude.source, "default");
  } finally {
    delete process.env.CLAUDE_CONFIG_DIR;
  }
  assert.doesNotThrow(() => resolveConfigDirs({ env: { CODEX_HOME: 5, XDG_CONFIG_HOME: {} }, home: "/nonexistent/x", platform: "linux" }));
  assert.doesNotThrow(() => resolveConfigDirs());
});

test("win32 platform honoured", () => {
  const r = resolveConfigDirs({ env: { CLAUDE_CONFIG_DIR: "C:\\cfg\\" }, home: "C:\\Users\\u", platform: "win32" });
  assert.strictEqual(r.claude.claudeJson, "C:\\cfg\\.claude.json");
  assert.strictEqual(resolveConfigDirs({ env: { CLAUDE_CONFIG_DIR: "cfg" }, home: "C:\\Users\\u", platform: "win32" }).warnings.length, 1);
});

test("sameRealFile / realPathForm", () => {
  const t = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "cfgdirs-")));
  try {
    const real = path.join(t, "real");
    fs.mkdirSync(real);
    fs.writeFileSync(path.join(real, "f.json"), "{}");
    const link = path.join(t, "link");
    fs.symlinkSync(real, link);
    assert.ok(sameRealFile(path.join(real, "f.json"), path.join(real, "f.json")));
    assert.ok(sameRealFile(real, real + "/"));
    assert.ok(sameRealFile(path.join(link, "f.json"), path.join(real, "f.json")));
    assert.ok(sameRealFile(path.join(link, "none.json"), path.join(real, "none.json")));
    assert.strictEqual(realPathForm(path.join(link, "a", "b")), path.join(real, "a", "b"));
    fs.writeFileSync(path.join(real, "g.json"), "{}");
    assert.ok(!sameRealFile(path.join(real, "f.json"), path.join(real, "g.json")));
    assert.ok(sameRealFile("C:\\X\\f.json", "c:\\x\\f.json", "win32"));
  } finally {
    fs.rmSync(t, { recursive: true, force: true });
  }
});
