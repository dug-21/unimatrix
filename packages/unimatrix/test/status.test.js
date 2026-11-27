"use strict";
// C15 status (1036 FR-34..FR-38, ADR-007): resolver-sourced paths, claudeMcp, stale/alternate findings.
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const path = require("path");
const status = require("../lib/status.js");
const { resolveConfigDirs } = require("../lib/config-dirs.js");
const { fixture, run, w } = require("./helpers/launcher-fixture.js");

const posix = process.platform === "win32" ? { skip: "posix only" } : {};
const OWNED = { command: "unimatrix", args: ["serve"] };
const OC_OWNED = { type: "local", command: ["unimatrix", "serve"] };
const FOREIGN = { command: "other-tool", args: [] };

const ctx = (f, env) => ({ env: Object.assign({ HOME: f.home }, env || {}), home: f.home, platform: process.platform, cwd: f.proj });
const text = (f, env) => status.renderText(status.collect(ctx(f, env)));
const claudeJson = (file, root, e) => w(file, JSON.stringify({ projects: { [root]: { mcpServers: { unimatrix: e } } } }));

function snapshot(dir) {
  const out = {};
  (function walk(d) {
    for (const n of fs.readdirSync(d)) {
      const p = path.join(d, n);
      const st = fs.lstatSync(p);
      if (st.isDirectory()) walk(p); else out[p] = st.mtimeMs + ":" + st.size;
    }
  })(dir);
  return out;
}

test("SR-09 parity: configDirs equals resolver paths for every override set", posix, () => {
  const f = fixture();
  const D = path.join(f.base, "cfg");
  const sets = [{}, { CLAUDE_CONFIG_DIR: D }, { OPENCODE_CONFIG_DIR: D }, { XDG_CONFIG_HOME: D }, { CODEX_HOME: D },
    { CLAUDE_CONFIG_DIR: D, OPENCODE_CONFIG_DIR: D, XDG_CONFIG_HOME: D + "x", CODEX_HOME: D }];
  for (const env of sets) {
    const c = ctx(f, env);
    const r = resolveConfigDirs({ env: c.env, home: f.home, platform: c.platform });
    const cd = status.collect(c).configDirs;
    assert.equal(cd.claude.claudeJson, r.claude.claudeJson);
    assert.equal(cd.claude.userSettings, r.claude.userSettings);
    assert.equal(cd.opencode.configJson, r.opencode.configJson);
    assert.equal(cd.codex.hooksJson, r.codex.hooksJson);
    assert.equal(cd.codex.configToml, r.codex.configToml);
    assert.deepEqual(cd.warnings, r.warnings);
  }
});

test("R-10: --json carries exact §6a field sets; no _dirs / value leak", posix, () => {
  const f = fixture();
  const r = run(f, ["status", "--json"], { CLAUDE_CONFIG_DIR: path.join(f.base, "cfg"), CODEX_HOME: path.join(f.base, "cx") });
  assert.equal(r.status, 0, r.stderr);
  const j = JSON.parse(r.stdout);
  assert.deepEqual(Object.keys(j.configDirs).sort(), ["claude", "codex", "opencode", "warnings"]);
  assert.deepEqual(Object.keys(j.configDirs.claude).sort(), ["claudeJson", "source", "userSettings"]);
  assert.deepEqual(Object.keys(j.configDirs.opencode).sort(), ["configJson", "source"]);
  assert.deepEqual(Object.keys(j.configDirs.codex).sort(), ["configToml", "hooksJson", "source"]);
  assert.equal(j.configDirs.claude.source, "CLAUDE_CONFIG_DIR");
  assert.deepEqual(Object.keys(j.claudeMcp).sort(), ["file", "key", "status"]);
  for (const k of ["opencodeConfig", "staleLegacy", "alternateProfiles"]) assert.ok(k in j, k);
  assert.ok(!r.stdout.includes('"_dirs"') && !r.stdout.includes('"value"'));
});

test("FR-34: spawned status with CLAUDE_CONFIG_DIR prints D/.claude.json and VAR=value source", posix, () => {
  const f = fixture();
  const D = path.join(f.base, "cfg");
  const r = run(f, ["status"], { CLAUDE_CONFIG_DIR: D });
  assert.equal(r.status, 0);
  assert.ok(r.stdout.includes(`config claude-code mcp: ${path.join(D, ".claude.json")} (CLAUDE_CONFIG_DIR=${D})\n`));
  assert.ok(r.stdout.includes(`config claude-code user hooks: ${path.join(D, "settings.json")} (CLAUDE_CONFIG_DIR=${D})\n`));
  assert.match(r.stdout, /^config opencode mcp: .*opencode\.json \(default\)$/m);
});

test("FR-34: invalid override warning printed with 'warning: ' prefix", posix, () => {
  const f = fixture();
  const t = text(f, { CLAUDE_CONFIG_DIR: "relative/dir" });
  assert.match(t, /^warning: /m);
});

test("FR-35: claudeMcp wired / missing / foreign / unreadable / n/a", posix, () => {
  const f = fixture();
  const cjf = path.join(f.home, ".claude.json");
  claudeJson(cjf, f.proj, OWNED);
  let i = status.collect(ctx(f));
  assert.deepEqual(i.claudeMcp, { status: "wired", file: cjf, key: f.proj });
  assert.match(status.renderText(i), new RegExp(`^mcp claude-code: wired \\(${cjf.replace(/[.\\/]/g, "\\$&")}\\)$`, "m"));

  w(cjf, JSON.stringify({ projects: {} }));
  const t = text(f);
  assert.ok(t.includes(`mcp claude-code: MISSING projects["${f.proj}"].mcpServers.unimatrix in ${cjf}; run 'npx unimatrix init'\n`));

  fs.unlinkSync(cjf);
  assert.equal(status.collect(ctx(f)).claudeMcp.status, "missing");

  claudeJson(cjf, f.proj, FOREIGN);
  assert.ok(text(f).includes(`mcp claude-code: foreign entry at projects["${f.proj}"].mcpServers.unimatrix in ${cjf}; not managed by Unimatrix\n`));

  w(cjf, "{not json");
  assert.ok(text(f).includes(`mcp claude-code: unreadable (${cjf} is not valid JSON)\n`));

  const g = fixture({ noStore: true, noInstall: true });
  i = status.collect(ctx(g));
  assert.deepEqual(i.claudeMcp, { status: "n/a", file: null, key: null });
  assert.match(status.renderText(i), /^mcp claude-code: n\/a$/m);
});

test("SR-16/D-028: owned entry only in default ~/.claude.json under CLAUDE_CONFIG_DIR -> missing + other profile, not stale", posix, () => {
  const f = fixture();
  const D = path.join(f.base, "cfg");
  const def = path.join(f.home, ".claude.json");
  claudeJson(def, f.proj, OWNED);
  const i = status.collect(ctx(f, { CLAUDE_CONFIG_DIR: D }));
  assert.equal(i.claudeMcp.status, "missing");
  assert.equal(i.claudeMcp.file, path.join(D, ".claude.json"));
  assert.deepEqual(i.alternateProfiles, [{ harness: "claude-code", file: def, key: `projects["${f.proj}"].mcpServers.unimatrix`, var: "CLAUDE_CONFIG_DIR" }]);
  assert.deepEqual(i.staleLegacy, []);
  assert.ok(status.renderText(i).includes(`other profile: ${def} projects["${f.proj}"].mcpServers.unimatrix (CLAUDE_CONFIG_DIR points elsewhere; separate claude-code config profile, init leaves it in place)\n`));
  const r = run(f, ["status"], { CLAUDE_CONFIG_DIR: D });
  assert.equal(r.status, 0);
  assert.match(r.stdout, /^other profile: /m);
});

test("FR-38: top-level mcpServers in default file listed; same-file override lists nothing", posix, () => {
  const f = fixture();
  const def = path.join(f.home, ".claude.json");
  w(def, JSON.stringify({ mcpServers: { unimatrix: OWNED } }));
  const i = status.collect(ctx(f, { CLAUDE_CONFIG_DIR: path.join(f.base, "cfg") }));
  assert.deepEqual(i.alternateProfiles.map((a) => a.key), ["mcpServers.unimatrix"]);
  assert.deepEqual(status.collect(ctx(f, { CLAUDE_CONFIG_DIR: f.home })).alternateProfiles, []);
});

test("FR-38: AP-1 opencode and AP-2 codex alternates", posix, () => {
  const f = fixture();
  const X = path.join(f.base, "xdg");
  w(path.join(X, "opencode", "opencode.json"), JSON.stringify({ mcp: { unimatrix: OC_OWNED } }));
  w(path.join(f.home, ".codex", "config.toml"), '[mcp_servers.unimatrix]\ncommand = "unimatrix"\nargs = ["serve"]\n');
  w(path.join(f.home, ".codex", "hooks.json"), JSON.stringify({ hooks: { Stop: [{ hooks: [{ type: "command", command: "unimatrix hook Stop" }] }] } }));
  const i = status.collect(ctx(f, { OPENCODE_CONFIG_DIR: path.join(f.base, "oc"), XDG_CONFIG_HOME: X, CODEX_HOME: path.join(f.base, "cx") }));
  const got = i.alternateProfiles.map((a) => `${a.harness}:${a.key}:${a.var}`).sort();
  assert.ok(got.includes("opencode:mcp.unimatrix:OPENCODE_CONFIG_DIR"), got);
  assert.ok(got.includes("codex-cli:mcp_servers.unimatrix:CODEX_HOME"), got);
});

test("FR-38: stale legacy twins listed under override; unreadable twin; absent when same file", posix, () => {
  const f = fixture();
  const legacyS = path.join(f.home, ".claude", "settings.json");
  w(legacyS, JSON.stringify({ hooks: { Stop: [{ hooks: [{ type: "command", command: "unimatrix hook Stop" }] }] } }));
  const legacyO = path.join(f.home, ".config", "opencode", "opencode.json");
  w(legacyO, "{ // jsonc");
  const env = { CLAUDE_CONFIG_DIR: path.join(f.base, "cfg"), OPENCODE_CONFIG_DIR: path.join(f.base, "oc") };
  const i = status.collect(ctx(f, env));
  assert.deepEqual(i.staleLegacy, [
    { harness: "claude-code", file: legacyS, key: "hooks" },
    { harness: "opencode", file: legacyO, key: null, unreadable: true },
  ]);
  const t = status.renderText(i);
  assert.ok(t.includes(`stale: ${legacyS} hooks holds an owned Unimatrix entry the harness no longer reads; re-run 'npx unimatrix init' to remove it\n`));
  assert.ok(t.includes(`stale: ${legacyO} unreadable (not valid JSON)\n`));
  assert.deepEqual(status.collect(ctx(f, { CLAUDE_CONFIG_DIR: path.join(f.home, ".claude") })).staleLegacy, []);
  assert.deepEqual(status.collect(ctx(f)).staleLegacy, []);
});

test("D-024: opencodeConfig ok / absent / unreadable; unreadable named as leaving OpenCode unwired, exit 0", posix, () => {
  const f = fixture();
  const oc = path.join(f.home, ".config", "opencode", "opencode.json");
  assert.deepEqual(status.collect(ctx(f)).opencodeConfig, { status: "absent", file: oc });
  w(oc, "{}");
  assert.equal(status.collect(ctx(f)).opencodeConfig.status, "ok");
  w(oc, "{ // comment\n}");
  assert.equal(status.collect(ctx(f)).opencodeConfig.status, "unreadable");
  const r = run(f, ["status"]);
  assert.equal(r.status, 0);
  assert.ok(r.stdout.includes(`opencode mcp: ${oc} is not valid JSON; OpenCode is left without Unimatrix wiring until it is fixed\n`));
});

test("read-only: status never writes; exit 0 on broken inputs", posix, () => {
  const f = fixture();
  w(path.join(f.home, ".claude.json"), "{bad");
  w(path.join(f.home, ".codex", "hooks.json"), "{bad");
  const before = snapshot(f.base);
  const r = run(f, ["status"], { CLAUDE_CONFIG_DIR: path.join(f.base, "cfg"), CODEX_HOME: path.join(f.base, "cx") });
  assert.equal(r.status, 0);
  assert.deepEqual(snapshot(f.base), before);
});

test("FR-01 grep: no private codexHome or home/.claude.json path in status.js", () => {
  const src = fs.readFileSync(path.join(__dirname, "..", "lib", "status.js"), "utf8");
  assert.ok(!/function codexHome/.test(src));
  assert.ok(!/\.claude\.json/.test(src));
  assert.ok(!/CODEX_HOME|CLAUDE_CONFIG_DIR|OPENCODE_CONFIG_DIR|XDG_CONFIG_HOME/.test(src));
});
