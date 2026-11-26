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
  assert.ok(["store", "mcp", "retrieval", "hooks", "codex"].includes(leg.surface), "surface: " + leg.surface);
  assert.ok(
    ["written", "created", "updated", "unchanged", "removed", "skipped-undetected", "skipped-not-detected",
      "skipped-malformed", "skipped-intent", "skipped-foreign", "skipped"].includes(leg.action),
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
    // one action line per leg, plus warnings (gitignore) appended after
    assert.ok(res.actions.length >= res.manifest.length);
    res.manifest.forEach((leg, i) => assert.ok(res.actions[i].includes(leg.harness + " " + leg.surface + ": " + leg.action)));
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

  it("test_wire_gitignore_warning_only_and_gitignore_untouched", () => {
    const dir = makeTempProject();
    gitInit(dir);
    const home = makeHome();
    writeFile(dir, ".gitignore", "node_modules\n");
    const before = read(dir, ".gitignore");
    const res = hermeticGit(() => wire(dir, local(home, { dryRun: true })));
    assert.ok(res.actions.some((a) => a.includes(".claude/settings.local.json is not git-ignored")));
    assert.ok(read(dir, ".gitignore").equals(before));
    const res2 = hermeticGit(() => wire(dir, local(home)));
    assert.ok(res2.actions.some((a) => a.includes("not git-ignored")));
    assert.ok(read(dir, ".gitignore").equals(before), ".gitignore never edited");
  });

  it("test_wire_no_gitignore_warning_when_ignored", () => {
    const dir = makeTempProject();
    gitInit(dir);
    const home = makeHome();
    writeFile(dir, ".gitignore", ".claude/settings.local.json\n");
    const res = wire(dir, local(home));
    assert.ok(!res.actions.some((a) => a.includes("not git-ignored")));
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
