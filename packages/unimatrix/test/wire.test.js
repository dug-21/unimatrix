"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");
const { describe, it } = require("node:test");

// Wire orchestrator (nan-024): one flow for local init, cloud attach and the wire verb.
// Proves the manifest contract (every dispatch yields a well-formed leg), per-machine
// destinations, dry-run == real, idempotence, the D-082 preflight, and warnings that
// never change the outcome. Behavioral fire/return is the C14 verifier's concern.

const { wire, detectHarnesses } = require("../lib/wire.js");
const { buildMcpEntry } = require("../lib/mcp-entry.js");
const { makeProject, makeHome, wireOpts, listTree, readJson } = require("./helpers/wire-fixture.js");

const FIXED_BIN = "/opt/unimatrix/bin/unimatrix";
const GOLDEN_DIR = path.join(__dirname, "fixtures", "wire", "golden");

const makeTempProject = () => makeProject("unimatrix-wire-test-");
const touchDir = (dir, rel) => fs.mkdirSync(path.join(dir, rel), { recursive: true });
function writeFile(dir, rel, content) {
  const p = path.join(dir, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, content, "utf8");
}
const read = (dir, rel) => fs.readFileSync(path.join(dir, rel));

const local = (home, extra) => wireOpts(home, Object.assign({ binaryPath: FIXED_BIN, platform: "linux" }, extra || {}));
const cloud = (home, extra) => wireOpts(home, Object.assign({ mcp: true, platform: "linux" }, extra || {}));

const SKIP_ACTIONS = /^skipped/;

/** Every leg is a well-formed WireLeg. */
function assertWellFormedLeg(leg) {
  assert.ok(leg && typeof leg === "object", "leg is an object");
  assert.ok(["unimatrix", "claude-code", "opencode", "codex-cli"].includes(leg.harness), "harness: " + leg.harness);
  assert.ok(["store", "mcp", "retrieval", "hooks", "codex", "git-exclude"].includes(leg.surface), "surface: " + leg.surface);
  assert.ok(
    ["written", "created", "updated", "unchanged", "removed", "skipped-undetected", "skipped-not-detected",
      "skipped-malformed", "skipped-intent", "skipped-foreign", "skipped", "skipped-no-git", "skipped-not-repo",
      "skipped-git-error", "skipped-tracked", "skipped-dest-unwired", "appended-not-effective"].includes(leg.action),
    "action: " + leg.action
  );
  assert.strictEqual(typeof leg.path, "string", "path is a string");
  if (SKIP_ACTIONS.test(leg.action)) assert.ok(leg.reason, "skipped legs carry a reason: " + JSON.stringify(leg));
  if (leg.surface === "hooks" && /^(created|updated|unchanged)$/.test(leg.action)) {
    assert.strictEqual(typeof leg.command, "string", "hooks leg carries command");
  }
}

describe("detectHarnesses", () => {
  it("test_detectHarnesses_all_markers", () => {
    const dir = makeTempProject();
    writeFile(dir, ".mcp.json", "{}");
    writeFile(dir, "opencode.json", "{}");
    assert.deepStrictEqual(detectHarnesses(dir), { "claude-code": true, opencode: true });
  });

  it("test_detectHarnesses_none", () => {
    assert.deepStrictEqual(detectHarnesses(makeTempProject()), { "claude-code": false, opencode: false });
  });

  it("test_detectHarnesses_partial", () => {
    const dir = makeTempProject();
    touchDir(dir, ".opencode");
    assert.deepStrictEqual(detectHarnesses(dir), { "claude-code": false, opencode: true });
  });

  it("test_detectHarnesses_ignores_gemini_and_codex", () => {
    const dir = makeTempProject();
    touchDir(dir, ".gemini");
    touchDir(dir, ".codex");
    const d = detectHarnesses(dir);
    assert.ok(!("gemini-cli" in d) && !("gemini" in d), "gemini not detected");
    assert.ok(!("codex-cli" in d), "codex is detected by detectsCodex, not here (D-081)");
  });
});

describe("claude per-machine golden", () => {
  it("test_wire_claude_mcp_settings_byte_identical_to_golden", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const res = wire(dir, local(home));
    const sub = (b) => Buffer.from(b.toString().split("@@HOME@@").join(home).split("@@ROOT@@").join(dir));
    assert.ok(read(dir, ".claude/settings.local.json").equals(sub(fs.readFileSync(path.join(GOLDEN_DIR, "settings.json")))),
      "settings.local.json byte-identical to golden");
    const cj = readJson(path.join(home, ".claude.json"));
    assert.deepStrictEqual(cj, JSON.parse(sub(fs.readFileSync(path.join(GOLDEN_DIR, "mcp.json")))));
    assert.ok(!fs.existsSync(path.join(dir, ".mcp.json")), "no project .mcp.json");
    assert.ok(!fs.existsSync(path.join(dir, ".claude", "settings.json")), "no committed settings.json");
    const claudeMcp = res.manifest.find((l) => l.harness === "claude-code" && l.surface === "mcp");
    assert.ok(claudeMcp && claudeMcp.action === "created", "claude mcp created");
    assert.strictEqual(claudeMcp.path, path.join(home, ".claude.json"));
  });

  it("test_wire_local_and_cloud_write_identical_claude_config", () => {
    const d1 = makeTempProject();
    const h1 = makeHome();
    wire(d1, local(h1));
    const s1 = read(d1, ".claude/settings.local.json").toString().split(h1).join("@H@");
    const c1 = fs.readFileSync(path.join(h1, ".claude.json"), "utf8").split(h1).join("@H@").split(d1).join("@R@");
    const d2 = makeTempProject();
    const h2 = makeHome();
    wire(d2, cloud(h2));
    const s2 = read(d2, ".claude/settings.local.json").toString().split(h2).join("@H@");
    const c2 = fs.readFileSync(path.join(h2, ".claude.json"), "utf8").split(h2).join("@H@").split(d2).join("@R@");
    assert.strictEqual(s1, s2);
    assert.strictEqual(c1, c2);
  });
});

describe("manifest contract", () => {
  function multiHarnessFixture() {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(dir, "opencode.json", "{}");
    touchDir(dir, ".codex");
    // user-level opencode entry already owned -> additive, not intent-gated
    writeFile(home, ".config/opencode/opencode.json", JSON.stringify({ mcp: { unimatrix: { type: "local", command: ["old"] } } }));
    return { dir, home };
  }

  it("test_wire_every_dispatch_returns_wellformed_leg", () => {
    const { dir, home } = multiHarnessFixture();
    const res = wire(dir, local(home));
    for (const leg of res.manifest) assertWellFormedLeg(leg);
    const surfaces = res.manifest.map((l) => l.harness + "/" + l.surface);
    for (const s of ["unimatrix/store", "claude-code/mcp", "claude-code/hooks", "opencode/retrieval", "codex-cli/hooks"]) {
      assert.ok(surfaces.includes(s), s + " leg present: " + surfaces.join(","));
    }
  });

  it("test_wire_actions_and_manifest_consistent", () => {
    const { dir, home } = multiHarnessFixture();
    const res = wire(dir, local(home));
    // resolved:/note: header lines, then one action line per leg, then notice + warnings
    const legLines = res.actions.filter((a) => !/^(resolved:|note:|If Claude Code)/.test(a));
    assert.ok(legLines.length >= res.manifest.length);
    res.manifest.forEach((leg, i) => assert.ok(legLines[i].includes(leg.harness + " " + leg.surface + ": " + leg.action)));
  });

  it("test_wire_codex_harness_writes_user_level_hooks_and_mcp", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const res = wire(dir, local(home, { harness: "codex-cli" }));
    assert.ok(res.manifest.filter((l) => l.harness === "codex-cli").length > 0, "codex legs present");
    assert.ok(fs.existsSync(path.join(home, ".codex", "hooks.json")), "hooks at $CODEX_HOME/hooks.json");
    assert.ok(fs.existsSync(path.join(home, ".codex", "config.toml")), "mcp at $CODEX_HOME/config.toml");
    assert.ok(!fs.existsSync(path.join(dir, ".codex")), "no project .codex written");
    assert.ok(res.manifest.every((l) => l.harness === "codex-cli" || l.harness === "unimatrix"), "only codex + store legs");
  });

  it("test_wire_codex_not_detected_yields_skipped_leg", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const res = wire(dir, local(home));
    const l = res.manifest.find((x) => x.harness === "codex-cli");
    assert.strictEqual(l.action, "skipped-not-detected");
    assert.ok(!fs.existsSync(path.join(home, ".codex")), "nothing written for Codex");
  });

  it("test_wire_skip_reasons_are_distinct", () => {
    const home = makeHome();
    const d1 = makeTempProject();
    const noTransport = wire(d1, wireOpts(home, { platform: "linux" })).manifest[0];
    const d2 = makeTempProject();
    writeFile(home, ".config/opencode/opencode.json", "{ not json");
    const malformed = wire(d2, local(home, { harness: "opencode" })).manifest.find((l) => l.surface === "retrieval");
    const d3 = makeTempProject();
    const home3 = makeHome();
    writeFile(d3, "opencode.json", "{}");
    const intent = wire(d3, local(home3)).manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval");
    assert.strictEqual(noTransport.action, "skipped-undetected");
    assert.strictEqual(malformed.action, "skipped-malformed");
    assert.strictEqual(intent.action, "skipped-intent");
  });

  it("test_wire_no_transport_writes_nothing", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const res = wire(dir, wireOpts(home, { platform: "linux" }));
    assert.ok(res.manifest.every((l) => SKIP_ACTIONS.test(l.action)));
    assert.deepStrictEqual(listTree(home), []);
  });

  it("test_wire_unknown_harness_no_writes", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const res = wire(dir, local(home, { harness: "nope" }));
    assert.deepStrictEqual(res.actions, ["unknown --harness: nope"]);
    assert.deepStrictEqual(listTree(home), []);
  });
});

describe("intent gate", () => {
  it("test_wire_no_harness_gates_new_opencode_entry", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(dir, "opencode.json", "{}");
    const res = wire(dir, local(home));
    const oc = res.manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval");
    assert.strictEqual(oc.action, "skipped-intent");
    assert.ok(/--harness opencode/.test(oc.reason));
    assert.ok(!fs.existsSync(path.join(home, ".config", "opencode")), "no user opencode config created");
  });

  it("test_wire_harness_opencode_opts_in_new_entry", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(dir, "opencode.json", "{}");
    const res = wire(dir, local(home, { harness: "opencode" }));
    const oc = res.manifest.find((l) => l.surface === "retrieval");
    assert.strictEqual(oc.action, "created");
    assert.strictEqual(oc.path, path.join(home, ".config", "opencode", "opencode.json"));
    assert.deepStrictEqual(
      readJson(oc.path).mcp.unimatrix.command,
      [buildMcpEntry("linux", home, process.execPath).command].concat(buildMcpEntry("linux", home, process.execPath).args)
    );
    assert.ok(res.manifest.every((l) => l.harness === "opencode" || l.harness === "unimatrix"), "only opencode + store");
    assert.ok(!fs.existsSync(path.join(dir, ".claude")), "claude not wired by --harness opencode");
  });

  it("test_wire_claude_common_path_not_gated", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const res = wire(dir, local(home));
    const mcp = res.manifest.find((l) => l.harness === "claude-code" && l.surface === "mcp");
    assert.ok(mcp && mcp.action === "created");
    assert.ok(fs.existsSync(path.join(home, ".claude.json")));
  });
});

describe("fail-safe posture", () => {
  it("test_wire_opencode_malformed_never_throws", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(home, ".config/opencode/opencode.json", "{ not valid json");
    let res;
    assert.doesNotThrow(() => { res = wire(dir, local(home, { harness: "opencode" })); });
    assert.strictEqual(res.manifest.find((l) => l.surface === "retrieval").action, "skipped-malformed");
    assert.strictEqual(read(home, ".config/opencode/opencode.json").toString(), "{ not valid json");
  });

  it("test_wire_claude_malformed_throws", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(home, ".claude.json", "{ broken");
    assert.throws(() => wire(dir, local(home)), /not valid JSON/);
    assert.strictEqual(read(home, ".claude.json").toString(), "{ broken", "byte-unchanged");
  });

  it("test_wire_malformed_project_mcp_json_throws_untouched", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(dir, ".mcp.json", "{ broken");
    assert.throws(() => wire(dir, local(home)), /\.mcp\.json/);
    assert.strictEqual(read(dir, ".mcp.json").toString(), "{ broken");
  });

  it("test_wire_foreign_unimatrix_entry_kept_with_warning", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const foreign = { projects: { [dir]: { mcpServers: { unimatrix: { command: "/usr/bin/other-thing", args: [] } } } } };
    writeFile(home, ".claude.json", JSON.stringify(foreign));
    const res = wire(dir, local(home));
    const mcp = res.manifest.find((l) => l.harness === "claude-code" && l.surface === "mcp");
    assert.strictEqual(mcp.action, "skipped-foreign");
    assert.ok(res.warnings.some((w) => w.includes("non-Unimatrix")));
    assert.deepStrictEqual(readJson(path.join(home, ".claude.json")), foreign, "foreign entry unchanged");
  });
});

describe("idempotence + dry-run", () => {
  function seed(dir, home) {
    writeFile(dir, ".mcp.json", JSON.stringify({ mcpServers: { unimatrix: { command: "/old/unimatrix", args: [] }, keep: { command: "k" } } }, null, 2));
    writeFile(dir, "opencode.json", JSON.stringify({ mcp: { unimatrix: { type: "local", command: ["unimatrix", "serve"] } } }, null, 2));
    writeFile(home, ".claude/settings.json", JSON.stringify({
      hooks: { Stop: [{ matcher: "", hooks: [{ type: "command", command: "unimatrix hook Stop" }] }] },
    }, null, 2));
    touchDir(dir, ".codex");
  }

  it("test_wire_run_twice_files_byte_identical", () => {
    const dir = makeTempProject();
    const home = makeHome();
    seed(dir, home);
    wire(dir, local(home));
    const snap = (b) => [...listTree(b)].map((f) => f + "=" + fs.readFileSync(path.join(b, f)).toString("base64"));
    const s1 = snap(dir).concat(snap(home));
    wire(dir, local(home));
    assert.deepStrictEqual(snap(dir).concat(snap(home)), s1);
  });

  it("test_wire_second_run_reports_unchanged_for_writer_surfaces", () => {
    const dir = makeTempProject();
    const home = makeHome();
    seed(dir, home);
    wire(dir, local(home));
    const res = wire(dir, local(home));
    for (const surface of ["mcp", "hooks"]) {
      const l = res.manifest.find((x) => x.harness === "claude-code" && x.surface === surface);
      assert.strictEqual(l.action, "unchanged", surface);
    }
    assert.ok(res.manifest.filter((l) => l.harness === "codex-cli" && l.surface === "hooks").every((l) => l.action === "unchanged"));
    assert.ok(!res.manifest.some((l) => l.action === "removed"), "nothing left to remove");
  });

  it("test_wire_migrates_owned_entries_across_scopes", () => {
    const dir = makeTempProject();
    const home = makeHome();
    seed(dir, home);
    wire(dir, local(home));
    assert.deepStrictEqual(readJson(path.join(dir, ".mcp.json")).mcpServers, { keep: { command: "k" } });
    assert.strictEqual(readJson(path.join(dir, "opencode.json")).mcp.unimatrix, undefined);
    const homeSettings = readJson(path.join(home, ".claude", "settings.json"));
    assert.strictEqual(homeSettings.hooks.Stop, undefined, "owned user-scope hook removed");
  });

  it("test_wire_dryRun_equals_real_and_writes_nothing", () => {
    const rd = makeTempProject();
    const rh = makeHome();
    const dd = makeTempProject();
    const dh = makeHome();
    seed(rd, rh);
    seed(dd, dh);
    const before = listTree(dd).concat(listTree(dh)).map((f) => f);
    const snapBefore = [dd, dh].map((b) => listTree(b).map((f) => f + fs.readFileSync(path.join(b, f)).toString("base64")));
    const real = wire(rd, local(rh));
    const dry = wire(dd, local(dh, { dryRun: true }));
    const norm = (res, d, h) =>
      res.manifest.map((l) => l.harness + "/" + l.surface + "/" + l.action + "/" +
        l.path.split(h).join("@H").split(d).join("@R").replace(/(pkg\/)[^/]+/, "$1V").replace(/\.unimatrix\/[0-9a-f]{16}\//, ".unimatrix/HASH/")).sort();
    assert.deepStrictEqual(norm(dry, dd, dh), norm(real, rd, rh), "dry-run leg list == real leg list");
    assert.ok(dry.actions.filter((a) => !a.startsWith("warning")).every((a) => a.startsWith("[dry-run] ")));
    const snapAfter = [dd, dh].map((b) => listTree(b).map((f) => f + fs.readFileSync(path.join(b, f)).toString("base64")));
    assert.deepStrictEqual(snapAfter, snapBefore, "dry-run changed no file");
    assert.deepStrictEqual(before, listTree(dd).concat(listTree(dh)));
    // every file the real run created or changed is covered by a leg path (R-08)
    const covered = real.manifest.map((l) => l.path);
    for (const f of listTree(rh)) {
      const abs = path.join(rh, f);
      assert.ok(covered.some((c) => abs === c || abs.startsWith(c + path.sep)), "file covered by a leg: " + f);
    }
  });
});

describe("warnings and preflight", () => {
  const gitInit = (dir) => require("child_process").execFileSync("git", ["init", "-q"], { cwd: dir });
  // Hermetic git: ignore the developer's global excludes (may already ignore settings.local.json).
  const hermeticGit = (fn) => {
    const keep = { g: process.env.GIT_CONFIG_GLOBAL, x: process.env.XDG_CONFIG_HOME };
    process.env.GIT_CONFIG_GLOBAL = "/dev/null";
    process.env.XDG_CONFIG_HOME = makeHome();
    try { return fn(); } finally {
      for (const [k, v] of [["GIT_CONFIG_GLOBAL", keep.g], ["XDG_CONFIG_HOME", keep.x]]) {
        if (v === undefined) delete process.env[k]; else process.env[k] = v;
      }
    }
  };

  // AC-8 (D-011): the exclude file replaces the old warning; .gitignore is never edited.
  it("test_wire_ac8_not_ignored_appends_exclude_and_gitignore_untouched", () => {
    const dir = makeTempProject();
    gitInit(dir);
    const home = makeHome();
    writeFile(dir, ".gitignore", "node_modules\n");
    const before = read(dir, ".gitignore");
    const ex = path.join(dir, ".git", "info", "exclude");
    const exBefore = fs.existsSync(ex) ? fs.readFileSync(ex, "utf8") : null;
    const dry = hermeticGit(() => wire(dir, local(home, { dryRun: true })));
    const dl = dry.manifest.find((l) => l.surface === "git-exclude");
    assert.ok(["created", "updated"].includes(dl.action), dl.action);
    assert.strictEqual(fs.existsSync(ex) ? fs.readFileSync(ex, "utf8") : null, exBefore, "dry-run writes no exclude");
    const res = hermeticGit(() => wire(dir, local(home)));
    assert.strictEqual(res.manifest.find((l) => l.surface === "git-exclude").action, dl.action);
    assert.ok(fs.readFileSync(ex, "utf8").split("\n").includes("/.claude/settings.local.json"));
    assert.ok(!res.actions.some((a) => a.includes("not git-ignored")), "no legacy warning");
    assert.ok(read(dir, ".gitignore").equals(before), ".gitignore never edited");
    const again = hermeticGit(() => wire(dir, local(home)));
    assert.strictEqual(again.manifest.find((l) => l.surface === "git-exclude").action, "unchanged");
  });

  it("test_wire_no_gitignore_warning_when_ignored", () => {
    const dir = makeTempProject();
    gitInit(dir);
    const home = makeHome();
    writeFile(dir, ".gitignore", ".claude/settings.local.json\n");
    const res = hermeticGit(() => wire(dir, local(home)));
    assert.ok(!res.actions.some((a) => a.includes("not git-ignored")));
    assert.strictEqual(res.manifest.find((l) => l.surface === "git-exclude").action, "unchanged");
  });

  it("test_wire_preflight_missing_recorded_node_no_path_node_exits", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(home, ".unimatrix/install.json", JSON.stringify({ version: "1", packageRoot: "/x", node: "/nonexistent/node" }));
    const emptyPath = makeHome();
    const before = listTree(home);
    assert.throws(
      () => wire(dir, local(home, { env: { PATH: emptyPath } })),
      /recorded node \/nonexistent\/node missing; run 'npx unimatrix init'/
    );
    assert.deepStrictEqual(listTree(home), before);
    assert.ok(!fs.existsSync(path.join(dir, ".claude")));
  });

  it("test_wire_preflight_missing_recorded_node_with_path_node_warns", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(home, ".unimatrix/install.json", JSON.stringify({ version: "1", packageRoot: "/x", node: "/nonexistent/node" }));
    const bin = makeHome();
    fs.copyFileSync(process.execPath, path.join(bin, "node"));
    const res = wire(dir, local(home, { env: { PATH: bin } }));
    assert.ok(res.warnings.some((w) => w.includes("/nonexistent/node")));
    assert.ok(fs.existsSync(path.join(dir, ".claude", "settings.local.json")), "files written normally");
  });

  it("test_wire_preflight_win32_missing_recorded_node_exits", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(home, ".unimatrix/install.json", JSON.stringify({ version: "1", packageRoot: "/x", node: "C:\\gone\\node.exe" }));
    assert.throws(() => wire(dir, local(home, { platform: "win32" })), /recorded node .* missing/);
  });

  it("test_wire_rerecord_skips_preflight_and_rerecords_node", () => {
    const dir = makeTempProject();
    const home = makeHome();
    writeFile(home, ".unimatrix/install.json", JSON.stringify({ version: "1", packageRoot: "/x", node: "/nonexistent/node" }));
    wire(dir, local(home, { rerecord: true }));
    assert.strictEqual(readJson(path.join(home, ".unimatrix", "install.json")).node, process.execPath);
  });

  it("test_wire_unsafe_shim_path_refused_before_any_write", () => {
    const dir = makeTempProject();
    const home = path.join(makeHome(), "it's");
    fs.mkdirSync(home);
    assert.throws(() => wire(dir, local(home)), (e) => e.name === "UnsafeShimPathError");
    assert.deepStrictEqual(listTree(home), []);
    assert.ok(!fs.existsSync(path.join(dir, ".claude")));
  });
});

// ── 1036 acceptance (assembled wire(): real plan + real writer, injected env/home) ──
describe("1036 acceptance via wire()", () => {
  const { mock } = require("node:test");
  const os = require("os");
  const { HOOK_EVENTS } = require("../lib/merge-settings.js");
  const { isUnimatrixCommand, isUnimatrixMcpEntry } = require("../lib/hook-ownership.js");
  const { seedLegacy, seedSymlinked, writeJsonAt, ownedHooks } = require("./helpers/wire-fixture.js");
  const tmpDir = (p) => fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), p || "unimatrix-cfg-")));
  const envOf = (home, extra) => Object.assign({ PATH: process.env.PATH, HOME: home }, extra || {});
  const lw = (home, env, extra) => local(home, Object.assign({ env: envOf(home, env) }, extra || {}));
  const entryFor = (home) => buildMcpEntry("linux", home, process.execPath);
  const stat = (p) => { const s = fs.statSync(p); return { m: s.mtimeMs, b: fs.readFileSync(p).toString("hex") }; };
  const project = (proj, extra) => Object.assign({}, ...[proj].map((l) => {
    const o = { harness: l.harness, surface: l.surface, action: l.action, path: l.path, reason: l.reason };
    if (l.entry) o.entry = l.entry;
    if (l.command) o.command = l.command;
    return o;
  }), extra || {});
  const quiet = (fn) => {
    const m = mock.method(process.stderr, "write", () => true);
    try { return fn(); } finally { m.mock.restore(); }
  };
  const noTemps = (dir) => listTree(dir).filter((f) => /\.tmp-[0-9a-f]{16}$/.test(f));

  /** Hash every file (bytes + mtime + mode) under each root, links reported as links. */
  function treeState(roots) {
    const out = {};
    const walk = (d) => {
      let es = [];
      try { es = fs.readdirSync(d, { withFileTypes: true }); } catch (_) { return; }
      for (const e of es) {
        const p = path.join(d, e.name);
        if (e.isSymbolicLink()) out[p] = "link:" + fs.readlinkSync(p);
        else if (e.isDirectory()) { out[p + "/"] = "dir"; walk(p); }
        else { const s = fs.statSync(p); out[p] = s.mtimeMs + ":" + s.mode + ":" + fs.readFileSync(p).toString("base64"); }
      }
    };
    for (const r of roots) walk(r);
    return out;
  }

  /** AC-9 parity: dry-run writes nothing, equals real manifest + lines, predicts the changed-file set. */
  function parity(dir, home, opts, roots) {
    const all = [dir, home].concat(roots || []);
    const before = treeState(all);
    const dry = quiet(() => wire(dir, Object.assign({}, opts, { dryRun: true })));
    assert.deepStrictEqual(treeState(all), before, "dry-run writes nothing (FR-62)");
    const real = quiet(() => wire(dir, Object.assign({}, opts, { dryRun: false })));
    assert.deepStrictEqual(real.manifest.map((l) => project(l)), dry.manifest.map((l) => project(l)), "FR-61 manifest");
    assert.deepStrictEqual(dry.actions, real.actions.map((a) => (a.startsWith("warning") ? a : "[dry-run] " + a)).map((a, i) =>
      dry.actions[i] && !dry.actions[i].startsWith("[dry-run] ") ? dry.actions[i] : a), "action lines equal apart from prefix");
    const after = treeState(all);
    const changed = new Set(Object.keys(after).filter((k) => after[k] !== before[k] && !k.endsWith("/")).map((k) => {
      try { return fs.realpathSync(k); } catch (_) { return k; }
    }));
    const predicted = new Set();
    for (const l of dry.manifest) {
      if (l.action === "unchanged" || /^skipped/.test(l.action)) continue;
      try {
        const st = fs.statSync(l.path);
        if (st.isDirectory()) {
          for (const k of changed) if (k.startsWith(fs.realpathSync(l.path) + path.sep)) predicted.add(k);
          continue;
        }
      } catch (_) { /* removed */ }
      try { predicted.add(fs.realpathSync(l.path)); } catch (_) { predicted.add(l.path); }
    }
    assert.deepStrictEqual([...changed].sort(), [...predicted].sort(), "FR-63 changed-file set");
    return { dry, real };
  }

  it("test_ac1a_claude_config_dir_writes_only_resolved_and_keeps_default_profile", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const D = tmpDir();
    const entry = entryFor(home);
    const def = path.join(home, ".claude.json");
    writeJsonAt(def, { mcpServers: { unimatrix: entry }, projects: { [dir]: { mcpServers: { unimatrix: entry } } } });
    const before = stat(def);
    const res = quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D })));
    const cj = readJson(path.join(D, ".claude.json"));
    assert.deepStrictEqual(cj.projects[dir].mcpServers.unimatrix, entry);
    assert.deepStrictEqual(stat(def), before, "default ~/.claude.json bytes + mtime unchanged (D-028)");
    assert.ok(!res.manifest.some((l) => l.path === def), "no leg names the default ~/.claude.json");
    assert.ok(res.actions.includes("resolved: claude-code mcp -> " + path.join(D, ".claude.json") + " (CLAUDE_CONFIG_DIR=" + D + ")"));
    assert.strictEqual(res.actions.filter((a) => a.startsWith("note: start each harness with the same CLAUDE_CONFIG_DIR")).length, 1);
  });

  for (const [name, mk] of [
    ["exact", (h) => h],
    ["trailing-slash", (h) => h + "/"],
    ["symlink", (h) => { const l = path.join(tmpDir(), "link"); fs.symlinkSync(h, l); return l; }],
  ]) {
    it("test_ac1b_same_file_claude_config_dir_" + name + "_one_entry", () => {
      const dir = makeTempProject();
      const home = makeHome();
      const res = quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: mk(home) })));
      const cj = readJson(path.join(home, ".claude.json"));
      assert.deepStrictEqual(cj.projects[dir].mcpServers.unimatrix, entryFor(home));
      assert.ok(!cj.mcpServers || !cj.mcpServers.unimatrix, "no user-scope duplicate");
      assert.ok(!res.manifest.some((l) => l.action === "removed" && /\.claude\.json$/.test(l.path)), "no wrongful removal");
    });
  }

  for (const [name, env, resolvedDir] of [
    ["opencode_config_dir", (t) => ({ OPENCODE_CONFIG_DIR: t }), (t) => t],
    ["xdg_only", (t) => ({ XDG_CONFIG_HOME: t }), (t) => path.join(t, "opencode")],
  ]) {
    it("test_ac2a_" + name + "_writes_resolved_and_removes_owned_twin", () => {
      const dir = makeTempProject();
      touchDir(dir, ".opencode");
      const home = makeHome();
      const t = tmpDir();
      const twin = path.join(home, ".config", "opencode", "opencode.json");
      writeJsonAt(twin, { theme: "x", mcp: { unimatrix: { type: "local", command: require("../lib/mcp-entry.js").buildMcpArgv("linux", home) }, other: { type: "local", command: ["o"] } } });
      quiet(() => wire(dir, lw(home, env(t))));
      const got = readJson(path.join(resolvedDir(t), "opencode.json"));
      assert.ok(got.mcp && got.mcp.unimatrix, "resolved file holds the entry (FR-13 migration, no --harness)");
      const tw = readJson(twin);
      assert.ok(!tw.mcp.unimatrix, "owned twin removed");
      assert.deepStrictEqual(tw.mcp.other, { type: "local", command: ["o"] });
      assert.strictEqual(tw.theme, "x");
    });
  }

  it("test_ac2a_both_set_opencode_config_dir_wins_xdg_untouched", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const O = tmpDir();
    const X = tmpDir();
    const xf = path.join(X, "opencode", "opencode.json");
    writeJsonAt(xf, { mcp: { unimatrix: { type: "local", command: ["keep"] } } });
    const before = stat(xf);
    quiet(() => wire(dir, lw(home, { OPENCODE_CONFIG_DIR: O, XDG_CONFIG_HOME: X }, { harness: "opencode" })));
    assert.ok(readJson(path.join(O, "opencode.json")).mcp.unimatrix);
    assert.deepStrictEqual(stat(xf), before, "AP-1 untouched");
  });

  for (const kind of ["absolute", "relative", "chain"]) {
    it("test_ac3_user_global_symlinks_kept_target_changed_" + kind, () => {
      const dir = makeTempProject();
      const home = makeHome();
      const C = tmpDir();
      const files = [path.join(home, ".claude.json"), path.join(C, "hooks.json"), path.join(C, "config.toml"),
        path.join(home, ".config", "opencode", "opencode.json")];
      const targets = files.map((f) => {
        if (/\.toml$/.test(f)) fs.writeFileSync(f, "");
        return seedSymlinked(f, kind);
      });
      const links = files.map((f) => fs.readlinkSync(f));
      for (const h of ["claude-code", "codex-cli", "opencode"]) quiet(() => wire(dir, lw(home, { CODEX_HOME: C }, { harness: h })));
      files.forEach((f, i) => {
        assert.ok(fs.lstatSync(f).isSymbolicLink(), f + " still a link");
        assert.strictEqual(fs.readlinkSync(f), links[i]);
        assert.ok(/unimatrix/.test(fs.readFileSync(targets[i], "utf8")), "target holds change: " + f);
        assert.deepStrictEqual(noTemps(path.dirname(f)).concat(noTemps(path.dirname(targets[i]))), []);
      });
    });
  }

  function rtFixture() {
    const dir = makeTempProject();
    touchDir(dir, ".opencode");
    const home = makeHome();
    const D = tmpDir();
    const seeded = seedLegacy(home, dir, { claudeConfigDir: D });
    return { dir, home, D, seeded };
  }

  it("test_ac4_rt_table_owned_removed_foreign_kept_byte_equal", () => {
    const { dir, home, D, seeded } = rtFixture();
    quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D })));
    // RT-01/02/03: owned hooks gone, foreign hook + siblings identical, key order kept.
    for (const f of [path.join(dir, ".claude", "settings.json"), path.join(D, "settings.json"), path.join(home, ".claude", "settings.json")]) {
      const got = readJson(f);
      assert.deepStrictEqual(got, { theme: "dark", hooks: { Notification: seeded[f].hooks.Notification } }, f);
      assert.deepStrictEqual(Object.keys(got), ["theme", "hooks"]);
      assert.ok(fs.readFileSync(f, "utf8").includes('\n  "theme"'), "2-space indent reused");
    }
    // RT-04: user-scope removed from resolved .claude.json, siblings kept; local-scope written.
    const cj = readJson(path.join(D, ".claude.json"));
    assert.strictEqual(cj.numStartups, 3);
    assert.deepStrictEqual(cj.mcpServers, { other: { command: "y" } });
    assert.ok(cj.projects[dir].mcpServers.unimatrix);
    // RT-06
    assert.deepStrictEqual(readJson(path.join(dir, ".mcp.json")), { mcpServers: { other: { command: "x" } } });
    // RT-08/RT-09
    const oc = readJson(path.join(dir, "opencode.json"));
    assert.ok(!oc.mcp || !oc.mcp.unimatrix, "RT-08 owned project mcp removed");
    assert.deepStrictEqual(oc.plugin, ["foreign-plugin"], "RT-09 legacy plugin removed, foreign kept");
    assert.strictEqual(oc.$schema, "https://opencode.ai/config.json");
  });

  it("test_ac4_foreign_value_at_unimatrix_key_kept_with_warning", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const D = tmpDir();
    const f = path.join(D, ".claude.json");
    writeJsonAt(f, { mcpServers: { unimatrix: { command: "mine" } } });
    writeJsonAt(path.join(dir, ".mcp.json"), { mcpServers: { unimatrix: { command: "mine2" } } });
    const res = quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D })));
    assert.deepStrictEqual(readJson(f).mcpServers.unimatrix, { command: "mine" });
    assert.deepStrictEqual(readJson(path.join(dir, ".mcp.json")).mcpServers.unimatrix, { command: "mine2" });
    assert.ok(res.warnings.some((w) => w.includes(f) && w.includes("unimatrix")), "warning names file and key: " + res.warnings.join("|"));
    for (const w of res.warnings) assert.ok(res.actions.includes(w), "warning in actions (FR-53): " + w);
  });

  it("test_ac4_tab_indent_reused_on_removal", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const f = path.join(dir, ".claude", "settings.json");
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, JSON.stringify({ a: 1, hooks: ownedHooks(home, "linux", ["Stop"]) }, null, "\t") + "\n");
    quiet(() => wire(dir, lw(home)));
    assert.strictEqual(fs.readFileSync(f, "utf8"), JSON.stringify({ a: 1, hooks: {} }, null, "\t") + "\n");
  });

  it("test_ac6_at_most_one_owned_entry_per_event_and_key", () => {
    const { dir, home, D } = rtFixture();
    // duplicate owned hooks pre-seeded in the destination too
    const sl = path.join(dir, ".claude", "settings.local.json");
    const dup = ownedHooks(home, "linux");
    for (const e of Object.keys(dup)) dup[e] = dup[e].concat(dup[e]);
    writeJsonAt(sl, { hooks: dup });
    quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D })));
    const hookFiles = [sl, path.join(dir, ".claude", "settings.json"), path.join(D, "settings.json"), path.join(home, ".claude", "settings.json")];
    for (const e of HOOK_EVENTS) {
      let n = 0;
      for (const f of hookFiles) {
        const h = (readJson(f).hooks || {})[e] || [];
        for (const g of h) for (const c of g.hooks || []) if (isUnimatrixCommand(c.command)) n++;
      }
      assert.ok(n <= 1, e + " owned count " + n);
    }
    let mcp = 0;
    const cj = readJson(path.join(D, ".claude.json"));
    for (const m of [cj.mcpServers || {}, (cj.projects[dir] || {}).mcpServers || {}, readJson(path.join(dir, ".mcp.json")).mcpServers || {}]) {
      for (const [k, v] of Object.entries(m)) if (isUnimatrixMcpEntry(k, v)) mcp++;
    }
    assert.strictEqual(mcp, 1, "exactly one Claude MCP entry (local scope)");
  });

  it("test_ac5_second_wire_all_unchanged_no_mtime_change_no_notice", () => {
    for (const env of [{}, { CLAUDE_CONFIG_DIR: tmpDir() }]) {
      const { dir, home } = rtFixture();
      const D = env.CLAUDE_CONFIG_DIR;
      const opts = lw(home, env);
      const first = quiet(() => wire(dir, opts));
      assert.strictEqual(first.actions.filter((a) => a.startsWith("If Claude Code was running")).length, 1, "D-013 notice once");
      const roots = [dir, home].concat(D ? [D] : []);
      const before = treeState(roots);
      const second = quiet(() => wire(dir, opts));
      for (const l of second.manifest) assert.ok(l.action === "unchanged" || /^skipped/.test(l.action), JSON.stringify(l));
      assert.deepStrictEqual(treeState(roots), before, "no file touched on run 2");
      assert.ok(!second.actions.some((a) => a.startsWith("If Claude Code was running")), "no notice on run 2");
    }
  });

  it("test_ac7_conflict_once_retries_and_keeps_concurrent_change", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const cjf = path.join(home, ".claude.json");
    writeJsonAt(cjf, { a: 1 });
    const orig = fs.openSync;
    let fired = false;
    const m = mock.method(fs, "openSync", function (p, flags, mode) {
      if (!fired && flags === "wx" && String(p).includes(".claude.json.tmp-")) {
        fired = true;
        fs.writeFileSync(cjf, JSON.stringify({ a: 1, concurrent: true }));
      }
      return orig.call(fs, p, flags, mode);
    });
    let res;
    try { res = quiet(() => wire(dir, lw(home))); } finally { m.mock.restore(); }
    assert.ok(fired);
    const leg = res.manifest.find((l) => l.path === cjf && l.surface === "mcp");
    assert.ok(["created", "updated"].includes(leg.action), leg.action);
    const cj = readJson(cjf);
    assert.strictEqual(cj.concurrent, true);
    assert.ok(cj.projects[dir].mcpServers.unimatrix);
  });

  it("test_ac7_conflict_every_attempt_throws_conflict_naming_file", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const cjf = path.join(home, ".claude.json");
    writeJsonAt(cjf, { a: 1 });
    const orig = fs.openSync;
    let n = 0;
    const m = mock.method(fs, "openSync", function (p, flags, mode) {
      if (flags === "wx" && String(p).includes(".claude.json.tmp-")) fs.writeFileSync(cjf, JSON.stringify({ a: ++n }));
      return orig.call(fs, p, flags, mode);
    });
    try {
      assert.throws(() => quiet(() => wire(dir, lw(home))), (e) => e.name === "SafeWriteError" && e.code === "conflict" && e.message.includes(cjf));
    } finally { m.mock.restore(); }
    assert.deepStrictEqual(noTemps(home), []);
  });

  it("test_ac7_verify_failed_on_w1_aborts_before_r4", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const D = tmpDir();
    const legacy = path.join(home, ".claude", "settings.json");
    writeJsonAt(legacy, { hooks: ownedHooks(home, "linux") });
    const legacyBefore = stat(legacy);
    const sl = path.join(dir, ".claude", "settings.local.json");
    const orig = fs.renameSync;
    const m = mock.method(fs, "renameSync", function (from, to) {
      const r = orig.call(fs, from, to);
      if (to === sl) fs.writeFileSync(sl, "{}\n");
      return r;
    });
    try {
      assert.throws(() => quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D }))),
        (e) => e.name === "SafeWriteError" && e.code === "verify-failed" && e.message.includes(sl));
    } finally { m.mock.restore(); }
    assert.deepStrictEqual(stat(legacy), legacyBefore, "R4 not run: legacy owned hooks still present");
    assert.deepStrictEqual(noTemps(path.dirname(sl)), []);
  });

  it("test_ac7_verify_failed_on_w2_keeps_default_claude_json", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const D = tmpDir();
    const def = path.join(home, ".claude.json");
    writeJsonAt(def, { projects: { [dir]: { mcpServers: { unimatrix: entryFor(home) } } } });
    const before = stat(def);
    const target = path.join(D, ".claude.json");
    const orig = fs.renameSync;
    const m = mock.method(fs, "renameSync", function (from, to) {
      const r = orig.call(fs, from, to);
      if (to === target) fs.writeFileSync(target, "{}\n");
      return r;
    });
    try {
      assert.throws(() => quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D }))), (e) => e.code === "verify-failed" && e.message.includes(target));
    } finally { m.mock.restore(); }
    assert.deepStrictEqual(stat(def), before);
  });

  it("test_r11_w2_skipped_foreign_leaves_user_scope_untouched", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const cjf = path.join(home, ".claude.json");
    const entry = entryFor(home);
    writeJsonAt(cjf, { mcpServers: { unimatrix: entry }, projects: { [dir]: { mcpServers: { unimatrix: { command: "foreign" } } } } });
    const res = quiet(() => wire(dir, lw(home)));
    assert.ok(res.manifest.some((l) => l.path === cjf && l.action === "skipped-foreign"));
    assert.deepStrictEqual(readJson(cjf).mcpServers.unimatrix, entry, "R1 not run");
  });

  it("test_r11_w5_skipped_malformed_keeps_twin", () => {
    const dir = makeTempProject();
    touchDir(dir, ".opencode");
    const home = makeHome();
    const O = tmpDir();
    fs.writeFileSync(path.join(O, "opencode.json"), "{ not json");
    const twin = path.join(home, ".config", "opencode", "opencode.json");
    writeJsonAt(twin, { mcp: { unimatrix: { type: "local", command: require("../lib/mcp-entry.js").buildMcpArgv("linux", home) } } });
    const before = stat(twin);
    const res = quiet(() => wire(dir, lw(home, { OPENCODE_CONFIG_DIR: O })));
    assert.ok(res.manifest.some((l) => l.action === "skipped-malformed"));
    assert.deepStrictEqual(stat(twin), before, "R10 not run");
  });

  for (const [name, harness, file] of [
    ["codex_hooks", "codex-cli", (h, C) => path.join(C, "hooks.json")],
    ["codex_config", "codex-cli", (h, C) => path.join(C, "config.toml")],
    ["opencode", "opencode", (h) => path.join(h, ".config", "opencode", "opencode.json")],
  ]) {
    it("test_r12_rename_failure_" + name + "_throws_naming_file_never_skipped", () => {
      const dir = makeTempProject();
      const home = makeHome();
      const C = tmpDir();
      const f = file(home, C);
      const orig = fs.renameSync;
      const m = mock.method(fs, "renameSync", function (from, to) {
        if (to === f) { const e = new Error("EIO: injected"); e.code = "EIO"; throw e; }
        return orig.call(fs, from, to);
      });
      try {
        assert.throws(() => quiet(() => wire(dir, lw(home, { CODEX_HOME: C }, { harness }))), (e) => e.name === "SafeWriteError" && e.message.includes(f));
      } finally { m.mock.restore(); }
      assert.deepStrictEqual(noTemps(path.dirname(f)), []);
    });
  }

  it("test_fr52_notice_once_and_dry_run_prefixed", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const dry = quiet(() => wire(dir, lw(home, {}, { dryRun: true })));
    const cj = path.join(home, ".claude.json");
    const text = "If Claude Code was running during this install, it may overwrite " + cj + "; re-run 'npx unimatrix init' or check 'npx unimatrix status'";
    assert.deepStrictEqual(dry.actions.filter((a) => a.includes("If Claude Code")), ["[dry-run] " + text]);
    const real = quiet(() => wire(dir, lw(home)));
    assert.deepStrictEqual(real.actions.filter((a) => a.includes("If Claude Code")), [text]);
    assert.ok(real.actions.includes("resolved: claude-code mcp -> " + cj + " (default)"));
    assert.ok(!real.actions.some((a) => a.startsWith("note:")), "no note without overrides");
  });

  it("test_no_transport_leg_uses_resolved_claude_json", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const D = tmpDir();
    const res = wire(dir, wireOpts(home, { platform: "linux", env: envOf(home, { CLAUDE_CONFIG_DIR: D }) }));
    assert.strictEqual(res.manifest[0].path, path.join(D, ".claude.json"));
  });

  it("test_ac9_parity_default_override_and_twins", () => {
    { const dir = makeTempProject(); const home = makeHome(); parity(dir, home, lw(home)); }
    { const { dir, home, D } = rtFixture(); parity(dir, home, lw(home, { CLAUDE_CONFIG_DIR: D }), [D]); }
    {
      const dir = makeTempProject(); touchDir(dir, ".opencode"); const home = makeHome(); const O = tmpDir(); const C = tmpDir();
      writeJsonAt(path.join(home, ".config", "opencode", "opencode.json"), { mcp: { unimatrix: { type: "local", command: require("../lib/mcp-entry.js").buildMcpArgv("linux", home) } } });
      parity(dir, home, lw(home, { OPENCODE_CONFIG_DIR: O, CODEX_HOME: C }, { harness: undefined }), [O, C]);
    }
  });

  // S-2: invalid overrides warn once (naming the var) and resolve exactly as no override.
  const norm = (res, home, dir) => res.manifest.map((l) => JSON.stringify(project(l)).split(home).join("<H>").split(dir).join("<R>").replace(/\/[0-9a-f]{16}\//g, "/<hash>/"));
  for (const v of ["CLAUDE_CONFIG_DIR", "OPENCODE_CONFIG_DIR", "XDG_CONFIG_HOME", "CODEX_HOME"]) {
    for (const [label, val] of [["empty", ""], ["blank", "  "], ["tilde", "~/x"], ["relative", "cfg"], ["nul", "/tmp/a\u0000b"]]) {
      it("test_s2_" + v.toLowerCase() + "_" + label + "_falls_back_with_one_warning", () => {
        const run = (env) => {
          const dir = makeTempProject();
          touchDir(dir, ".opencode");
          const home = makeHome();
          const lines = [];
          const m = mock.method(process.stderr, "write", (s) => { lines.push(String(s)); return true; });
          let res;
          try { res = wire(dir, lw(home, env, { dryRun: true })); } finally { m.mock.restore(); }
          return { res, lines, home, dir };
        };
        const base = run({});
        const bad = run({ [v]: val });
        assert.deepStrictEqual(norm(bad.res, bad.home, bad.dir), norm(base.res, base.home, base.dir), "manifest == no-override");
        const named = bad.lines.filter((l) => l.includes(v));
        assert.strictEqual(named.length, val === "" ? 0 : 1, "warnings: " + bad.lines.join("|"));
        if (val !== "") assert.ok(bad.res.actions.some((a) => a.includes(v)), "warning also in actions");
      });
    }
  }

  // S-6 / D-030: R4 removes owned hooks from the default settings.json under an override.
  it("test_s6_r4_removes_owned_legacy_hooks_keeps_foreign", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const D = tmpDir();
    const legacy = path.join(home, ".claude", "settings.json");
    const foreign = { hooks: [{ type: "command", command: "echo mine" }] };
    writeJsonAt(legacy, { hooks: Object.assign(ownedHooks(home, "linux", ["Stop"]), { PreCompact: [foreign] }) });
    const res = quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: D })));
    assert.deepStrictEqual(readJson(legacy), { hooks: { PreCompact: [foreign] } });
    assert.ok(res.manifest.some((l) => l.path === legacy && l.action === "removed" && /legacy location/.test(l.reason)));
  });

  it("test_s6_same_file_override_no_legacy_step", () => {
    const dir = makeTempProject();
    const home = makeHome();
    const legacy = path.join(home, ".claude", "settings.json");
    writeJsonAt(legacy, { hooks: ownedHooks(home, "linux", ["Stop"]) });
    const res = quiet(() => wire(dir, lw(home, { CLAUDE_CONFIG_DIR: path.join(home, ".claude") })));
    assert.ok(!res.manifest.some((l) => /legacy location/.test(l.reason || "")), "R4 dropped for same file");
    assert.strictEqual(res.manifest.filter((l) => l.path === legacy && l.action === "removed").length, 1);
  });
});
