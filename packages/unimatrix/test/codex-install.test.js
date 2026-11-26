"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it } = require("node:test");

// Component tests — Codex installer Wave B (nan-023, ADR-001/002/003/006). These
// drive the real fs-touching writers against on-disk `.git` fixtures. The
// BYTE-PRESERVATION (R-04), COMMAND-STRING (R-05/AC-08), TOML-ESCAPE (R-10) and
// WireLeg (SR-09) assertions live here. Behavioral fire/return (AC-09/AC-10) is
// the C14 verifier's concern (c14-verifier.md), NOT exercised here.

const {
  maybeWireCodex,
  writeCodexMcpToml,
  writeCodexHooks,
  removeOwnedCodexHooks,
  removeOwnedHooksToml,
  removeOwnedCodexProjectMcp,
  requireProviderFlag,
  codexHome,
  detectsCodex,
  codexApprovalStep,
  CODEX_EVENTS,
} = require("../lib/codex-install.js");
const { PRETOOLUSE_CYCLE_MATCHER, buildShimCommand } = require("../lib/merge-settings.js");

const PLAT = "linux";

/** Isolated fixture: temp HOME + project root; PATH stripped of codex. */
function fixture(extraEnv) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-codex-test-"));
  const home = path.join(base, "home");
  const root = path.join(base, "proj");
  const bin = path.join(base, "bin");
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(path.join(root, ".git"), { recursive: true });
  fs.mkdirSync(bin, { recursive: true });
  const env = Object.assign({ PATH: bin }, extraEnv || {});
  const o = { home, env, platform: PLAT, node: "/usr/bin/node", projectRoot: root, dryRun: false };
  return { base, home, root, bin, env, o };
}
const hooksOf = (dir) => path.join(dir, "hooks.json");
const cfgOf = (dir) => path.join(dir, "config.toml");
const rd = (p) => fs.readFileSync(p, "utf8");
function snapshot(dir) {
  const out = [];
  (function walk(d) {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { out.push("D " + p); walk(p); } else out.push("F " + p + " " + rd(p));
    }
  })(dir);
  return out.sort();
}
function quiet(fn) {
  const orig = process.stderr.write;
  const lines = [];
  process.stderr.write = (s) => { lines.push(String(s)); return true; };
  try { return { result: fn(), stderr: lines }; } finally { process.stderr.write = orig; }
}
function seedOwnedHooks(file, home) {
  const hooks = {};
  for (const ev of CODEX_EVENTS) {
    hooks[ev] = [{ matcher: "", hooks: [
      { type: "command", command: "unimatrix hook " + ev },
      { type: "command", command: "/usr/bin/foreign-" + ev },
    ] }];
  }
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify({ hooks }, null, 2) + "\n");
}

// ---------------------------------------------------------------------------
// codexHome / detectsCodex (D-081, D-085)
// ---------------------------------------------------------------------------

describe("codexHome", () => {
  it("defaults to ~/.codex; no XDG", () => {
    assert.deepStrictEqual(codexHome({ XDG_CONFIG_HOME: "/x" }, "/h"), { dir: path.join("/h", ".codex") });
  });
  it("honours an absolute CODEX_HOME", () => {
    assert.deepStrictEqual(codexHome({ CODEX_HOME: "/abs/ch" }, "/h"), { dir: "/abs/ch" });
  });
  it("relative / NUL CODEX_HOME -> ~/.codex + warning (D-085)", () => {
    for (const bad of ["./x", "rel/dir", "/a\0b"]) {
      const r = codexHome({ CODEX_HOME: bad }, "/h");
      assert.strictEqual(r.dir, path.join("/h", ".codex"));
      assert.ok(/not an absolute path/.test(r.warning));
    }
  });
  it("empty CODEX_HOME is treated as unset (no warning)", () => {
    assert.deepStrictEqual(codexHome({ CODEX_HOME: "" }, "/h"), { dir: path.join("/h", ".codex") });
  });
});

describe("detectsCodex", () => {
  it("row 5: no trigger -> false", () => {
    const f = fixture();
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home }), false);
  });
  it("row 1: project .codex/", () => {
    const f = fixture();
    fs.mkdirSync(path.join(f.root, ".codex"));
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home }), true);
  });
  it("row 2: existing $CODEX_HOME dir; row 6: absent dir -> false", () => {
    const f = fixture({ CODEX_HOME: null });
    const ch = path.join(f.base, "ch");
    f.env.CODEX_HOME = ch;
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home }), false);
    fs.mkdirSync(ch);
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home }), true);
  });
  it("$CODEX_HOME that is a regular file counts as absent", () => {
    const f = fixture();
    const ch = path.join(f.base, "chfile");
    fs.writeFileSync(ch, "x");
    f.env.CODEX_HOME = ch;
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home }), false);
  });
  it("existing default ~/.codex counts", () => {
    const f = fixture();
    fs.mkdirSync(path.join(f.home, ".codex"));
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home }), true);
  });
  it("row 3: codex on PATH (stat only)", () => {
    const f = fixture();
    fs.writeFileSync(path.join(f.bin, "codex"), "#!/bin/sh\n", { mode: 0o755 });
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home, platform: "linux" }), true);
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home, platform: "win32" }), false);
  });
  it("row 4: --harness codex-cli", () => {
    const f = fixture();
    assert.strictEqual(detectsCodex({ root: f.root, env: f.env, home: f.home, harnessSel: "codex-cli" }), true);
  });
});

// ---------------------------------------------------------------------------
// writeCodexHooks
// ---------------------------------------------------------------------------

describe("writeCodexHooks", () => {
  it("writes the seven-event set to <codexHome>/hooks.json with shim commands", () => {
    const f = fixture();
    const legs = writeCodexHooks(f.o);
    assert.strictEqual(legs.length, 7);
    const hp = hooksOf(path.join(f.home, ".codex"));
    const disk = JSON.parse(rd(hp));
    assert.deepStrictEqual(Object.keys(disk.hooks).sort(), CODEX_EVENTS.slice().sort());
    assert.strictEqual(disk.hooks.PreToolUse[0].matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(disk.hooks.SessionStart[0].matcher, "");
    for (const ev of CODEX_EVENTS) {
      const cmd = disk.hooks[ev][0].hooks[0].command;
      assert.strictEqual(cmd, buildShimCommand(ev, "codex-cli", PLAT, f.home));
      assert.ok(cmd.includes("--provider codex-cli"));
      assert.ok(!cmd.includes("hook-client"));
    }
    for (const l of legs) assert.strictEqual(l.path, hp);
  });

  it("custom absolute CODEX_HOME honoured; ~/.codex untouched", () => {
    const f = fixture();
    const ch = path.join(f.base, "ch");
    f.o.env.CODEX_HOME = ch;
    writeCodexHooks(f.o);
    assert.ok(fs.existsSync(hooksOf(ch)));
    assert.ok(!fs.existsSync(path.join(f.home, ".codex")));
  });

  it("byte-identical across re-init and 'upgrade' (D-028); re-run does not rewrite", () => {
    const f = fixture();
    const ch = path.join(f.base, "ch");
    f.o.env.CODEX_HOME = ch;
    writeCodexHooks(f.o);
    const first = rd(hooksOf(ch));
    const m1 = fs.statSync(hooksOf(ch)).mtimeMs;
    fs.mkdirSync(path.join(f.home, ".unimatrix", "bin"), { recursive: true });
    fs.writeFileSync(path.join(f.home, ".unimatrix", "bin", "trust.js"), "// refreshed\n");
    const legs2 = writeCodexHooks(f.o);
    assert.ok(legs2.every((l) => l.action === "unchanged"));
    assert.strictEqual(rd(hooksOf(ch)), first);
    assert.strictEqual(fs.statSync(hooksOf(ch)).mtimeMs, m1);
  });

  it("preserves foreign hooks; owned deduped to one per event", () => {
    const f = fixture();
    const hp = hooksOf(path.join(f.home, ".codex"));
    seedOwnedHooks(hp, f.home);
    writeCodexHooks(f.o);
    const disk = JSON.parse(rd(hp));
    for (const ev of CODEX_EVENTS) {
      const hs = disk.hooks[ev].flatMap((g) => g.hooks);
      assert.strictEqual(hs.filter((h) => /\.unimatrix\/bin\/unimatrix/.test(h.command)).length, 1);
      assert.strictEqual(hs.filter((h) => h.command === "/usr/bin/foreign-" + ev).length, 1);
      assert.ok(!hs.some((h) => h.command === "unimatrix hook " + ev), "legacy owned replaced");
    }
  });

  it("provider flag is fail-loud", () => {
    assert.throws(() => requireProviderFlag("x PreToolUse"), /missing --provider codex-cli/);
    assert.doesNotThrow(() => requireProviderFlag("x PreToolUse --provider codex-cli"));
  });

  it("malformed hooks.json -> skipped-malformed, untouched, no throw", () => {
    const f = fixture();
    const hp = hooksOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(hp), { recursive: true });
    fs.writeFileSync(hp, "{ nope ");
    const legs = writeCodexHooks(f.o);
    assert.strictEqual(legs.length, 1);
    assert.strictEqual(legs[0].action, "skipped-malformed");
    assert.strictEqual(rd(hp), "{ nope ");
  });

  it("non-object `hooks` key -> skipped-malformed", () => {
    const f = fixture();
    const hp = hooksOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(hp), { recursive: true });
    fs.writeFileSync(hp, '{"hooks":"nope"}');
    assert.strictEqual(writeCodexHooks(f.o)[0].action, "skipped-malformed");
  });

  it("dry-run lists every event, writes nothing", () => {
    const f = fixture();
    f.o.dryRun = true;
    const legs = writeCodexHooks(f.o);
    assert.strictEqual(legs.length, 7);
    assert.strictEqual(legs[0].action, "created");
    assert.ok(!fs.existsSync(path.join(f.home, ".codex")));
  });
});

// ---------------------------------------------------------------------------
// removeOwnedCodexHooks
// ---------------------------------------------------------------------------

describe("removeOwnedCodexHooks", () => {
  it("removes owned from project hooks.json; foreign kept; emptied events dropped", () => {
    const f = fixture();
    const ph = path.join(f.root, ".codex", "hooks.json");
    seedOwnedHooks(ph, f.home);
    const r = removeOwnedCodexHooks(f.o);
    const disk = JSON.parse(rd(ph));
    for (const ev of CODEX_EVENTS) {
      assert.deepStrictEqual(disk.hooks[ev][0].hooks, [{ type: "command", command: "/usr/bin/foreign-" + ev }]);
    }
    assert.ok(r.legs.some((l) => l.action === "removed" && l.path === ph));
  });

  it("only-owned project file loses every event; foreign-only file not rewritten", () => {
    const f = fixture();
    const ph = path.join(f.root, ".codex", "hooks.json");
    fs.mkdirSync(path.dirname(ph), { recursive: true });
    const foreign = JSON.stringify({ hooks: { Stop: [{ matcher: "", hooks: [{ type: "command", command: "beep" }] }] } }) + "\n";
    fs.writeFileSync(ph, foreign);
    const m = fs.statSync(ph).mtimeMs;
    const r = removeOwnedCodexHooks(f.o);
    assert.strictEqual(rd(ph), foreign);
    assert.strictEqual(fs.statSync(ph).mtimeMs, m);
    assert.deepStrictEqual(r.legs, []);
  });

  it("dry-run lists removal and leaves files untouched", () => {
    const f = fixture();
    const ph = path.join(f.root, ".codex", "hooks.json");
    seedOwnedHooks(ph, f.home);
    const before = rd(ph);
    f.o.dryRun = true;
    const r = removeOwnedCodexHooks(f.o);
    assert.ok(r.actions.length > 0 && r.actions.every((a) => a.startsWith("[dry-run] ")));
    assert.strictEqual(rd(ph), before);
  });

  it("malformed project hooks.json -> skipped-malformed leg, untouched", () => {
    const f = fixture();
    const ph = path.join(f.root, ".codex", "hooks.json");
    fs.mkdirSync(path.dirname(ph), { recursive: true });
    fs.writeFileSync(ph, "{ bad");
    const r = removeOwnedCodexHooks(f.o);
    assert.strictEqual(r.legs[0].action, "skipped-malformed");
    assert.strictEqual(rd(ph), "{ bad");
  });

  it("project root == home: the user-level hooks file is not stripped", () => {
    const f = fixture();
    f.o.projectRoot = f.home;
    writeCodexHooks(f.o);
    const before = rd(hooksOf(path.join(f.home, ".codex")));
    removeOwnedCodexHooks(f.o);
    assert.strictEqual(rd(hooksOf(path.join(f.home, ".codex"))), before);
  });

  it("removes owned [[hooks.X]] tables from <codexHome>/config.toml; foreign + comments preserved", () => {
    const f = fixture();
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    const raw = [
      "# top comment",
      "[tools]",
      "a = 1",
      "",
      "[[hooks.Stop]]",
      'matcher = ""',
      "[[hooks.Stop.hooks]]",
      'type = "command"',
      "command = \"'/home/x/.unimatrix/bin/unimatrix' hook Stop --provider codex-cli\"",
      "",
      "[[hooks.PreToolUse]]",
      'matcher = "*"',
      "[[hooks.PreToolUse.hooks]]",
      'type = "command"',
      'command = "/usr/bin/foreign"',
      "",
      "[mcp_servers.other]",
      'command = "keep"',
      "",
    ].join("\n");
    fs.writeFileSync(cfg, raw);
    const r = removeOwnedCodexHooks(f.o);
    assert.ok(r.legs.some((l) => l.action === "removed" && l.path === cfg));
    const disk = rd(cfg);
    assert.ok(!/unimatrix\/bin\/unimatrix/.test(disk));
    assert.ok(!disk.includes("[[hooks.Stop]]"), "emptied parent group removed");
    assert.ok(disk.startsWith("# top comment\n[tools]\na = 1\n\n"));
    assert.ok(disk.includes('[[hooks.PreToolUse.hooks]]\ntype = "command"\ncommand = "/usr/bin/foreign"\n'));
    assert.ok(disk.endsWith('[mcp_servers.other]\ncommand = "keep"\n'));
  });

  it("config.toml with no owned hooks is not rewritten; CRLF preserved; malformed skipped", () => {
    assert.strictEqual(removeOwnedHooksToml('[[hooks.A.hooks]]\r\ncommand = "foreign"\r\n').changed, false);
    const crlf = '[x]\r\na=1\r\n\r\n[[hooks.A]]\r\n[[hooks.A.hooks]]\r\ncommand = "unimatrix hook Stop"\r\n\r\n[y]\r\nb=2\r\n';
    const r = removeOwnedHooksToml(crlf);
    assert.strictEqual(r.text, "[x]\r\na=1\r\n\r\n[y]\r\nb=2\r\n");
    assert.strictEqual(removeOwnedHooksToml('[a\nx=1\n').malformed, true);
  });
});

// ---------------------------------------------------------------------------
// MCP (user scope + project removal)
// ---------------------------------------------------------------------------

describe("writeCodexMcpToml", () => {
  it("creates the owned table at <codexHome>/config.toml (D-084, no opt-in gate)", () => {
    const f = fixture();
    const l = writeCodexMcpToml(f.o);
    const cfg = cfgOf(path.join(f.home, ".codex"));
    assert.strictEqual(l.action, "created");
    assert.strictEqual(l.path, cfg);
    assert.deepStrictEqual(l.entry, { command: "sh", args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'] });
    const disk = rd(cfg);
    assert.ok(disk.includes("[mcp_servers.unimatrix]"));
    assert.ok(disk.includes('command = "sh"'));
    assert.ok(disk.includes('args = ["-c", "exec \\"$HOME/.unimatrix/bin/unimatrix\\" mcp-stdio"]'));
    assert.ok(!/^\s*(url|env)/m.test(disk));
  });

  it("win32 entry records node + shim-entry.js; stale owned entry updated in place", () => {
    const f = fixture();
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    const old = '[tools]\na = 1\n\n[mcp_servers.unimatrix]\ncommand = "node"\nargs = ["/old/home/.unimatrix/bin/shim-entry.js", "mcp-stdio"]\n\n[mcp_servers.other]\ncommand = "keep"\n';
    fs.writeFileSync(cfg, old);
    f.o.platform = "win32";
    f.o.node = "C:/node/node.exe";
    const l = writeCodexMcpToml(f.o);
    assert.strictEqual(l.action, "updated");
    const disk = rd(cfg);
    assert.ok(disk.includes('command = "C:/node/node.exe"'));
    assert.ok(disk.startsWith("[tools]\na = 1\n\n"));
    assert.ok(disk.endsWith('[mcp_servers.other]\ncommand = "keep"\n'));
  });

  it("idempotent: second run writes nothing", () => {
    const f = fixture();
    writeCodexMcpToml(f.o);
    const cfg = cfgOf(path.join(f.home, ".codex"));
    const first = rd(cfg);
    const m = fs.statSync(cfg).mtimeMs;
    assert.strictEqual(writeCodexMcpToml(f.o).action, "unchanged");
    assert.strictEqual(rd(cfg), first);
    assert.strictEqual(fs.statSync(cfg).mtimeMs, m);
  });

  it("foreign unimatrix table untouched + D-065 warning in leg.reason", () => {
    const f = fixture();
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    const raw = '# mine\n[mcp_servers.unimatrix]\ncommand = "/opt/custom-server"\n';
    fs.writeFileSync(cfg, raw);
    const l = writeCodexMcpToml(f.o);
    assert.strictEqual(l.action, "skipped-foreign");
    assert.ok(/non-Unimatrix entry; left unchanged/.test(l.reason));
    assert.strictEqual(rd(cfg), raw);
  });

  it("preserves foreign tables, comments, CRLF when appending", () => {
    const f = fixture();
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    const raw = "# c\r\n[mcp_servers.other]\r\ncommand = \"o\"\r\n";
    fs.writeFileSync(cfg, raw);
    writeCodexMcpToml(f.o);
    const disk = rd(cfg);
    assert.ok(disk.startsWith(raw));
    assert.ok(disk.includes("[mcp_servers.unimatrix]\r\n"));
  });

  it("malformed TOML -> skipped-malformed, byte-preserved", () => {
    const f = fixture();
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    const raw = '[tools]\nnote = """\nunclosed\n';
    fs.writeFileSync(cfg, raw);
    assert.strictEqual(writeCodexMcpToml(f.o).action, "skipped-malformed");
    assert.strictEqual(rd(cfg), raw);
  });

  it("dry-run writes nothing", () => {
    const f = fixture();
    f.o.dryRun = true;
    assert.strictEqual(writeCodexMcpToml(f.o).action, "created");
    assert.ok(!fs.existsSync(path.join(f.home, ".codex")));
  });
});

describe("removeOwnedCodexProjectMcp", () => {
  const cfgIn = (f) => path.join(f.root, ".codex", "config.toml");
  function seed(f, raw) {
    fs.mkdirSync(path.dirname(cfgIn(f)), { recursive: true });
    fs.writeFileSync(cfgIn(f), raw);
  }
  const OWNED = '[mcp_servers.unimatrix]\ncommand = "sh"\nargs = ["-c", "exec \\"$HOME/.unimatrix/bin/unimatrix\\" mcp-stdio"]\n';

  it("removes owned; foreign [mcp_servers.other] byte-preserved; file kept", () => {
    const f = fixture();
    const other = '[mcp_servers.other]\ncommand = "keep"\n';
    seed(f, "# c\n" + other + "\n" + OWNED);
    const r = removeOwnedCodexProjectMcp(f.root, false);
    assert.strictEqual(r.actions.length, 1);
    assert.strictEqual(rd(cfgIn(f)), "# c\n" + other);
  });

  it("owned legacy bridge form removed", () => {
    const f = fixture();
    seed(f, '[mcp_servers.unimatrix]\ncommand = "node"\nargs = ["/x/mcp-bridge.js", "abc"]\n');
    assert.strictEqual(removeOwnedCodexProjectMcp(f.root, false).actions.length, 1);
    assert.strictEqual(rd(cfgIn(f)), "");
  });

  it("foreign unimatrix: warning, untouched; only-foreign file not rewritten", () => {
    const f = fixture();
    const raw = '[mcp_servers.unimatrix]\ncommand = "/opt/mine"\n';
    seed(f, raw);
    const r = removeOwnedCodexProjectMcp(f.root, false);
    assert.strictEqual(r.actions.length, 0);
    assert.ok(/non-Unimatrix/.test(r.warnings[0]));
    assert.strictEqual(rd(cfgIn(f)), raw);
  });

  it("dry-run lists, does not write; absent file is a no-op", () => {
    const f = fixture();
    assert.deepStrictEqual(removeOwnedCodexProjectMcp(f.root, false), { actions: [], warnings: [] });
    seed(f, OWNED);
    const r = removeOwnedCodexProjectMcp(f.root, true);
    assert.ok(r.actions[0].startsWith("[dry-run] "));
    assert.strictEqual(rd(cfgIn(f)), OWNED);
  });
});

// ---------------------------------------------------------------------------
// maybeWireCodex — detection matrix, orchestration, fail-safe
// ---------------------------------------------------------------------------

describe("maybeWireCodex", () => {
  const written = (f, dir) => fs.existsSync(hooksOf(dir)) && fs.existsSync(cfgOf(dir));

  it("row 1: project .codex/ -> hooks + MCP in ~/.codex", () => {
    const f = fixture();
    fs.mkdirSync(path.join(f.root, ".codex"));
    const legs = maybeWireCodex(f.o);
    assert.ok(written(f, path.join(f.home, ".codex")));
    assert.strictEqual(legs.filter((l) => l.surface === "hooks").length, 7);
    assert.strictEqual(legs.filter((l) => l.surface === "mcp").length, 1);
  });

  it("row 2: existing $CODEX_HOME only -> written there, ~/.codex NOT created", () => {
    const f = fixture();
    const ch = path.join(f.base, "ch");
    fs.mkdirSync(ch);
    f.o.env.CODEX_HOME = ch;
    maybeWireCodex(f.o);
    assert.ok(written(f, ch));
    assert.ok(!fs.existsSync(path.join(f.home, ".codex")));
  });

  it("row 3: codex on PATH only", () => {
    const f = fixture();
    fs.writeFileSync(path.join(f.bin, "codex"), "#!/bin/sh\n", { mode: 0o755 });
    maybeWireCodex(f.o);
    assert.ok(written(f, path.join(f.home, ".codex")));
  });

  it("row 4: --harness codex-cli only -> ~/.codex created", () => {
    const f = fixture();
    f.o.harnessSel = "codex-cli";
    maybeWireCodex(f.o);
    assert.ok(written(f, path.join(f.home, ".codex")));
  });

  it("rows 5/6: no trigger (or absent $CODEX_HOME) -> nothing written, one skipped-not-detected leg", () => {
    const f = fixture({ XDG_CONFIG_HOME: null });
    f.env.XDG_CONFIG_HOME = path.join(f.base, "xdg");
    fs.mkdirSync(f.env.XDG_CONFIG_HOME);
    for (const ch of [undefined, path.join(f.base, "absent")]) {
      if (ch) f.o.env.CODEX_HOME = ch;
      const before = snapshot(f.base);
      const legs = maybeWireCodex(f.o);
      assert.deepStrictEqual(snapshot(f.base), before);
      assert.strictEqual(legs.length, 1);
      assert.strictEqual(legs[0].action, "skipped-not-detected");
    }
  });

  it("row 7: multiple triggers -> single write, one owned entry per event", () => {
    const f = fixture();
    const ch = path.join(f.base, "ch");
    fs.mkdirSync(ch);
    fs.mkdirSync(path.join(f.root, ".codex"));
    f.o.env.CODEX_HOME = ch;
    maybeWireCodex(f.o);
    const disk = JSON.parse(rd(hooksOf(ch)));
    for (const ev of CODEX_EVENTS) assert.strictEqual(disk.hooks[ev].flatMap((g) => g.hooks).length, 1);
    assert.ok(!fs.existsSync(path.join(f.home, ".codex")));
  });

  it("no XDG write; relative CODEX_HOME -> ~/.codex + one stderr warning, cwd untouched", () => {
    const f = fixture();
    f.o.harnessSel = "codex-cli";
    f.o.env.CODEX_HOME = "./x";
    f.o.env.XDG_CONFIG_HOME = path.join(f.base, "xdg");
    const cwdBefore = snapshot(process.cwd() === "/" ? f.base : f.base);
    const { result: legs, stderr } = quiet(() => maybeWireCodex(f.o));
    assert.ok(written(f, path.join(f.home, ".codex")));
    assert.ok(!fs.existsSync(f.o.env.XDG_CONFIG_HOME));
    assert.ok(!fs.existsSync(path.join(process.cwd(), "x", "hooks.json")));
    assert.strictEqual(stderr.filter((s) => /not an absolute path/.test(s)).length, 1);
    assert.ok(legs.every((l) => /not an absolute path/.test(l.reason || "")));
    assert.ok(cwdBefore.length >= 0);
  });

  it("removes owned legacy entries across scopes; hooks and MCP always agree", () => {
    const f = fixture();
    fs.mkdirSync(path.join(f.root, ".codex"));
    seedOwnedHooks(path.join(f.root, ".codex", "hooks.json"), f.home);
    fs.writeFileSync(path.join(f.root, ".codex", "config.toml"),
      '[mcp_servers.unimatrix]\ncommand = "node"\nargs = ["/x/mcp-bridge.js", "h"]\n');
    const legs = maybeWireCodex(f.o);
    const user = JSON.parse(rd(hooksOf(path.join(f.home, ".codex"))));
    const proj = JSON.parse(rd(path.join(f.root, ".codex", "hooks.json")));
    for (const ev of CODEX_EVENTS) {
      assert.strictEqual(user.hooks[ev].flatMap((g) => g.hooks).length, 1);
      assert.deepStrictEqual(proj.hooks[ev][0].hooks.map((h) => h.command), ["/usr/bin/foreign-" + ev]);
    }
    assert.strictEqual(rd(path.join(f.root, ".codex", "config.toml")), "");
    assert.ok(legs.some((l) => l.surface === "mcp" && l.action === "removed"));
  });

  it("dry-run lists user-level writes; set of paths equals the real run (AC-13)", () => {
    const f = fixture();
    f.o.harnessSel = "codex-cli";
    fs.mkdirSync(path.join(f.root, ".codex"));
    seedOwnedHooks(path.join(f.root, ".codex", "hooks.json"), f.home);
    f.o.dryRun = true;
    const before = snapshot(f.base);
    const dry = maybeWireCodex(f.o);
    assert.deepStrictEqual(snapshot(f.base), before);
    f.o.dryRun = false;
    const real = maybeWireCodex(f.o);
    const paths = (ls) => Array.from(new Set(ls.filter((l) => l.action !== "unchanged").map((l) => l.path))).sort();
    assert.deepStrictEqual(paths(dry), paths(real));
  });

  it("foreign unimatrix MCP: leg skipped-foreign and warning on stderr; hooks still written", () => {
    const f = fixture();
    f.o.harnessSel = "codex-cli";
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    fs.writeFileSync(cfg, '[mcp_servers.unimatrix]\ncommand = "/opt/mine"\n');
    const { result: legs, stderr } = quiet(() => maybeWireCodex(f.o));
    assert.ok(legs.some((l) => l.action === "skipped-foreign"));
    assert.ok(stderr.some((s) => /non-Unimatrix entry/.test(s)));
    assert.ok(fs.existsSync(hooksOf(path.join(f.home, ".codex"))));
  });

  it("never throws: unsafe shim path becomes a skipped leg", () => {
    const f = fixture();
    f.o.harnessSel = "codex-cli";
    f.o.home = path.join(f.base, "it's");
    fs.mkdirSync(f.o.home);
    let legs;
    assert.doesNotThrow(() => { legs = maybeWireCodex(f.o); });
    assert.ok(legs.some((l) => l.action === "skipped-malformed" && /codex wiring error/.test(l.reason)));
  });

  it("malformed config.toml: MCP leg skipped-malformed, file preserved", () => {
    const f = fixture();
    f.o.harnessSel = "codex-cli";
    const cfg = cfgOf(path.join(f.home, ".codex"));
    fs.mkdirSync(path.dirname(cfg), { recursive: true });
    fs.writeFileSync(cfg, '[tools]\nnote = """\nunclosed\n');
    const legs = maybeWireCodex(f.o);
    assert.ok(legs.some((l) => l.surface === "mcp" && l.action === "skipped-malformed"));
    assert.strictEqual(rd(cfg), '[tools]\nnote = """\nunclosed\n');
  });
});

describe("codexApprovalStep", () => {
  it("is constant text naming /hooks", () => {
    assert.strictEqual(codexApprovalStep(), codexApprovalStep());
    assert.ok(codexApprovalStep().includes("/hooks"));
  });
});

// ---------------------------------------------------------------------------
// F1: user-level file modes preserved / new files 0600 (POSIX only)
// ---------------------------------------------------------------------------

describe("codex user-level file modes", { skip: process.platform === "win32" }, () => {
  const modeOf = (p) => fs.statSync(p).mode & 0o777;
  it("0600 config.toml stays 0600 after writeCodexMcpToml; new file is 0600", () => {
    const f = fixture();
    const dir = path.join(f.home, ".codex");
    fs.mkdirSync(dir);
    fs.writeFileSync(cfgOf(dir), 'model = "x"\n', { mode: 0o600 });
    fs.chmodSync(cfgOf(dir), 0o600);
    assert.strictEqual(writeCodexMcpToml(f.o).action, "created");
    assert.strictEqual(modeOf(cfgOf(dir)), 0o600);
    const g = fixture();
    assert.strictEqual(writeCodexMcpToml(g.o).action, "created");
    assert.strictEqual(modeOf(cfgOf(path.join(g.home, ".codex"))), 0o600);
  });
  it("0600 hooks.json stays 0600; 0640 preserved; new hooks.json is 0600", () => {
    const f = fixture();
    const dir = path.join(f.home, ".codex");
    fs.mkdirSync(dir);
    fs.writeFileSync(hooksOf(dir), "{}\n");
    fs.chmodSync(hooksOf(dir), 0o640);
    writeCodexHooks(f.o);
    assert.strictEqual(modeOf(hooksOf(dir)), 0o640);
    fs.chmodSync(hooksOf(dir), 0o600);
    f.o.dryRun = false;
    fs.writeFileSync(hooksOf(dir), "{}\n");
    fs.chmodSync(hooksOf(dir), 0o600);
    writeCodexHooks(f.o);
    assert.strictEqual(modeOf(hooksOf(dir)), 0o600);
    const g = fixture();
    writeCodexHooks(g.o);
    assert.strictEqual(modeOf(hooksOf(path.join(g.home, ".codex"))), 0o600);
  });
});
