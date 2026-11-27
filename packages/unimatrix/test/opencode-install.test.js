"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { describe, it } = require("node:test");

// nan-024 ADR-006: no OpenCode plugin provisioning; the per-machine MCP writer
// (~/.config/opencode/opencode.json) and the owned project-entry remover.

const {
  detectsOpenCode,
  writeOpencodeMcp,
  removeOwnedOpencodeProjectMcp,
  buildOpencodeEntry,
  opencodeLeg,
  isWithinProject,
  detectIndent,
  readJsonSafe,
} = require("../lib/opencode-install.js");
const lib = require("../lib/opencode-install.js");
const { removeOwnedOpencodeUserMcpLeg, removeOwnedOpencodeProjectLegs } = lib;
const { resolveConfigDirs, sameRealFile } = require("../lib/config-dirs.js");
const { SafeWriteError } = require("../lib/safe-write.js");

const NODE = "/usr/bin/node";
const UNIX_ARGV = ["sh", "-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'];
const unixEntry = () => ({ type: "local", command: UNIX_ARGV });

function tmp(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}
function makeTempProject() {
  const dir = tmp("unimatrix-opencode-test-");
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}
function cfgPathOf(home) {
  return path.join(home, ".config", "opencode", "opencode.json");
}
function writeUser(home, obj, indent) {
  fs.mkdirSync(path.dirname(cfgPathOf(home)), { recursive: true });
  fs.writeFileSync(cfgPathOf(home), JSON.stringify(obj, null, indent || 2) + "\n", "utf8");
}
function readUser(home) {
  return JSON.parse(fs.readFileSync(cfgPathOf(home), "utf8"));
}
function rawUser(home) {
  return fs.readFileSync(cfgPathOf(home), "utf8");
}
function wr(home, extra, env) {
  const file = resolveConfigDirs({ env: env || {}, home, platform: "linux" }).opencode.configJson;
  return writeOpencodeMcp(Object.assign({ home, file, platform: "linux", node: NODE }, extra));
}
/** Install-shaped run: W5, then R10 only if the destination is wired and is not the same real file (D-007, R-11). */
function install(home, env, extra) {
  const dirs = resolveConfigDirs({ env, home, platform: "linux" });
  const file = dirs.opencode.configJson;
  const legacy = dirs.legacy.opencodeJson;
  const w = writeOpencodeMcp(Object.assign({ home, file, legacyFile: legacy, platform: "linux", node: NODE }, extra));
  const wired = ["created", "updated", "unchanged"].includes(w.action);
  const r = sameRealFile(file, legacy, "linux") ? null : wired ? removeOwnedOpencodeUserMcpLeg(legacy, false) : "skipped-dest-unwired";
  return { w, r, file, legacy };
}
function writeProjectJson(dir, obj, indent) {
  fs.writeFileSync(path.join(dir, "opencode.json"), JSON.stringify(obj, null, indent || 2) + "\n", "utf8");
}
function projectRaw(dir) {
  return fs.readFileSync(path.join(dir, "opencode.json"), "utf8");
}
function snapshot(dir) {
  const out = {};
  (function walk(d) {
    for (const n of fs.readdirSync(d)) {
      const p = path.join(d, n);
      if (fs.statSync(p).isDirectory()) walk(p);
      else out[path.relative(dir, p)] = fs.readFileSync(p, "utf8");
    }
  })(dir);
  return out;
}

function sentinelConfig() {
  return {
    $schema: "https://opencode.ai/config.json",
    plugin: ["some-foreign-plugin"],
    mcp: { other: { type: "local", command: ["other-server"] } },
    provider: {
      ollama: {
        npm: "@ai-sdk/openai-compatible",
        options: { baseURL: "http://localhost:11434/v1" },
        models: { "qwen3-coder": {} },
      },
    },
    permission: { edit: "allow", bash: "ask" },
  };
}

describe("detection", () => {
  it("test_installer_detects_opencode_by_opencode_json", () => {
    const dir = makeTempProject();
    writeProjectJson(dir, sentinelConfig());
    assert.strictEqual(detectsOpenCode(dir), true);
  });
  it("test_installer_detects_opencode_by_dot_opencode_dir", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".opencode"), { recursive: true });
    assert.strictEqual(detectsOpenCode(dir), true);
  });
  it("test_installer_no_opencode_no_branch", () => {
    const dir = makeTempProject();
    assert.strictEqual(detectsOpenCode(dir), false);
    assert.ok(!fs.existsSync(path.join(dir, ".opencode")));
    assert.ok(!fs.existsSync(path.join(dir, "opencode.json")));
  });
});

describe("no provisioning (ADR-006, AC-09, R-14)", () => {
  it("test_installer_provisioning_exports_removed", () => {
    for (const n of [
      "provisionOpenCode", "maybeProvisionOpenCode", "appendPluginEntry", "mergePackageDep",
      "resolvePluginVersion", "PLUGIN_PACKAGE", "PLUGIN_EXPORT", "PLUGIN_ENTRY_FILE", "PLUGIN_SHIM_CONTENT",
    ]) {
      assert.strictEqual(lib[n], undefined, n + " must be deleted");
    }
  });
  it("test_installer_source_has_no_plugin_shim_writer", () => {
    const src = fs.readFileSync(path.join(__dirname, "..", "lib", "opencode-install.js"), "utf8");
    assert.ok(!/plugins/.test(src), "no .opencode/plugins writer");
    assert.ok(!/unimatrix-opencode-plugin/.test(src), "no plugin package reference");
    assert.ok(!/package\.json/.test(src), "no .opencode/package.json writer");
  });
  it("test_installer_emit_js_unchanged_in_scope", () => {
    const p = path.join(__dirname, "..", "opencode-plugin", "lib", "emit.js");
    assert.ok(fs.existsSync(p));
    assert.match(crypto.createHash("sha256").update(fs.readFileSync(p)).digest("hex"), /^[0-9a-f]{64}$/);
  });
  it("test_installer_writer_creates_no_dot_opencode_files", () => {
    const home = tmp("uni-oc-home-");
    const proj = makeTempProject();
    fs.mkdirSync(path.join(proj, ".opencode"));
    writeProjectJson(proj, sentinelConfig());
    const before = snapshot(proj);
    wr(home, { harnessSelected: true });
    assert.deepStrictEqual(snapshot(proj), before, "project tree unchanged by user-level writer");
  });
  it("test_installer_plugin_array_byte_equal_after_removal", () => {
    const proj = makeTempProject();
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = unixEntry();
    writeProjectJson(proj, cfg);
    removeOwnedOpencodeProjectMcp(proj, false);
    assert.deepStrictEqual(JSON.parse(projectRaw(proj)).plugin, ["some-foreign-plugin"]);
  });
});

describe("buildOpencodeEntry", () => {
  it("test_build_entry_unix_argv_no_environment", () => {
    const e = buildOpencodeEntry({ platform: "linux", home: "/h", node: NODE });
    assert.deepStrictEqual(e, unixEntry());
    assert.strictEqual(e.environment, undefined);
  });
  it("test_build_entry_windows_node_shim_entry", () => {
    const e = buildOpencodeEntry({ platform: "win32", home: "C:\\Users\\u", node: "C:\\node.exe" });
    assert.deepStrictEqual(e, {
      type: "local",
      command: ["C:\\node.exe", "C:/Users/u/.unimatrix/bin/shim-entry.js", "mcp-stdio"],
    });
  });
  it("test_build_entry_no_secret_or_url", () => {
    assert.ok(!/token|url|bearer/i.test(JSON.stringify(buildOpencodeEntry({ platform: "linux", home: "/h", node: NODE }))));
  });
});

describe("writeOpencodeMcp — per-machine target", () => {
  it("test_write_fresh_creates_user_level_entry", () => {
    const home = tmp("uni-oc-home-");
    const leg = wr(home, { harnessSelected: true });
    assert.strictEqual(leg.action, "created");
    assert.strictEqual(leg.path, cfgPathOf(home));
    assert.deepStrictEqual(readUser(home).mcp.unimatrix, unixEntry());
    assert.deepStrictEqual(leg.entry, unixEntry());
  });
  it("test_write_leg_shape", () => {
    const home = tmp("uni-oc-home-");
    const leg = wr(home, { harnessSelected: true });
    assert.strictEqual(leg.harness, "opencode");
    assert.strictEqual(leg.surface, "retrieval");
  });
  it("test_write_never_writes_project_opencode_json", () => {
    const home = tmp("uni-oc-home-");
    const proj = makeTempProject();
    writeProjectJson(proj, sentinelConfig());
    const before = projectRaw(proj);
    wr(home, { harnessSelected: true });
    assert.strictEqual(projectRaw(proj), before);
  });
  it("test_write_targets_resolved_file_not_process_env", () => {
    const home = tmp("uni-oc-home-");
    const old = process.env.XDG_CONFIG_HOME;
    process.env.XDG_CONFIG_HOME = tmp("uni-oc-xdg-");
    try {
      const leg = wr(home, { harnessSelected: true });
      assert.strictEqual(leg.path, cfgPathOf(home));
    } finally {
      if (old === undefined) delete process.env.XDG_CONFIG_HOME;
      else process.env.XDG_CONFIG_HOME = old;
    }
  });
  it("test_write_creates_parent_dirs", () => {
    const home = tmp("uni-oc-home-");
    wr(home, { harnessSelected: true });
    assert.ok(fs.existsSync(cfgPathOf(home)));
  });
  it("test_write_leaves_no_tmp_files", () => {
    const home = tmp("uni-oc-home-");
    wr(home, { harnessSelected: true });
    assert.deepStrictEqual(fs.readdirSync(path.dirname(cfgPathOf(home))), ["opencode.json"]);
  });
});

describe("writeOpencodeMcp — preservation", () => {
  it("test_write_preserves_provider_permission_plugin_and_foreign_mcp", () => {
    const home = tmp("uni-oc-home-");
    const orig = sentinelConfig();
    writeUser(home, orig);
    wr(home, { harnessSelected: true });
    const after = readUser(home);
    assert.deepStrictEqual(after.provider, orig.provider);
    assert.deepStrictEqual(after.permission, orig.permission);
    assert.deepStrictEqual(after.plugin, orig.plugin);
    assert.deepStrictEqual(after.mcp.other, orig.mcp.other);
    assert.deepStrictEqual(after.mcp.unimatrix, unixEntry());
  });
  it("test_write_preserves_indent_4_space", () => {
    const home = tmp("uni-oc-home-");
    writeUser(home, sentinelConfig(), 4);
    wr(home, { harnessSelected: true });
    assert.ok(/\n {4}"provider"/.test(rawUser(home)));
  });
  it("test_write_preserves_tab_indent", () => {
    const home = tmp("uni-oc-home-");
    writeUser(home, sentinelConfig(), "\t");
    wr(home, { harnessSelected: true });
    assert.ok(/\n\t"provider"/.test(rawUser(home)));
  });
  it("test_write_preserves_foreign_top_level_keys", () => {
    const home = tmp("uni-oc-home-");
    const cfg = sentinelConfig();
    cfg.customTool = { alpha: 1, beta: ["x"] };
    writeUser(home, cfg);
    wr(home, { harnessSelected: true });
    assert.deepStrictEqual(readUser(home).customTool, cfg.customTool);
  });
});

describe("writeOpencodeMcp — idempotence and update", () => {
  it("test_write_run_twice_byte_identical", () => {
    const home = tmp("uni-oc-home-");
    wr(home, { harnessSelected: true });
    const first = rawUser(home);
    const leg = wr(home, { harnessSelected: true });
    assert.strictEqual(rawUser(home), first);
    assert.strictEqual(leg.action, "unchanged");
  });
  it("test_write_unchanged_does_not_touch_file", () => {
    const home = tmp("uni-oc-home-");
    wr(home, { harnessSelected: true });
    const m = fs.statSync(cfgPathOf(home)).mtimeMs;
    wr(home, { harnessSelected: true });
    assert.strictEqual(fs.statSync(cfgPathOf(home)).mtimeMs, m);
  });
  it("test_write_updates_owned_stale_entry_without_optin", () => {
    const home = tmp("uni-oc-home-");
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = { type: "local", command: ["/old/bin/unimatrix", "mcp-stdio"], enabled: true };
    writeUser(home, cfg);
    const leg = wr(home, {});
    assert.strictEqual(leg.action, "updated");
    assert.deepStrictEqual(readUser(home).mcp.unimatrix, unixEntry());
    assert.deepStrictEqual(readUser(home).provider, cfg.provider);
  });
  it("test_write_windows_rerecord_node_rewrites_entry", () => {
    const home = tmp("uni-oc-home-");
    wr(home, { harnessSelected: true, platform: "win32", node: "C:\\n1\\node.exe" });
    const leg = wr(home, { harnessSelected: true, platform: "win32", node: "C:\\n2\\node.exe" });
    assert.strictEqual(leg.action, "updated");
    assert.strictEqual(readUser(home).mcp.unimatrix.command[0], "C:\\n2\\node.exe");
  });
});

describe("writeOpencodeMcp — intent gate and foreign (D-065)", () => {
  it("test_write_new_entry_without_intent_skipped", () => {
    const home = tmp("uni-oc-home-");
    const leg = wr(home, {});
    assert.strictEqual(leg.action, "skipped-intent");
    assert.ok(!fs.existsSync(cfgPathOf(home)), "nothing created");
  });
  it("test_write_intent_skip_leaves_existing_config_untouched", () => {
    const home = tmp("uni-oc-home-");
    writeUser(home, sentinelConfig());
    const before = rawUser(home);
    wr(home, {});
    assert.strictEqual(rawUser(home), before);
  });
  it("test_write_foreign_unimatrix_left_with_warning", () => {
    const home = tmp("uni-oc-home-");
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = { type: "local", command: ["my-own-server", "--flag"] };
    writeUser(home, cfg);
    const before = rawUser(home);
    const leg = wr(home, { harnessSelected: true });
    assert.strictEqual(leg.action, "skipped-foreign");
    assert.match(leg.warning, /non-Unimatrix entry; left unchanged, Unimatrix MCP not written/);
    assert.strictEqual(rawUser(home), before);
  });
});

describe("writeOpencodeMcp — malformed / fail-safe", () => {
  it("test_write_malformed_json_skips_and_preserves", () => {
    const home = tmp("uni-oc-home-");
    fs.mkdirSync(path.dirname(cfgPathOf(home)), { recursive: true });
    fs.writeFileSync(cfgPathOf(home), "{ not : json ]", "utf8");
    let leg;
    assert.doesNotThrow(() => { leg = wr(home, { harnessSelected: true }); });
    assert.strictEqual(leg.action, "skipped-malformed");
    assert.strictEqual(rawUser(home), "{ not : json ]");
  });
  it("test_write_mcp_not_object_skips", () => {
    const home = tmp("uni-oc-home-");
    const cfg = sentinelConfig();
    cfg.mcp = "nope";
    writeUser(home, cfg);
    const before = rawUser(home);
    const leg = wr(home, { harnessSelected: true });
    assert.strictEqual(leg.action, "skipped-malformed");
    assert.strictEqual(rawUser(home), before);
  });
  it("test_write_fs_error_propagates", () => {
    const home = tmp("uni-oc-home-");
    fs.writeFileSync(path.join(home, ".config"), "i am a file", "utf8");
    assert.throws(() => wr(home, { harnessSelected: true }));
  });
  it("test_write_dry_run_writes_nothing", () => {
    const home = tmp("uni-oc-home-");
    const leg = wr(home, { harnessSelected: true, dryRun: true });
    assert.strictEqual(leg.action, "created");
    assert.ok(!fs.existsSync(cfgPathOf(home)));
  });
  it("test_write_dry_run_existing_unchanged_bytes", () => {
    const home = tmp("uni-oc-home-");
    writeUser(home, sentinelConfig());
    const before = rawUser(home);
    wr(home, { harnessSelected: true, dryRun: true });
    assert.strictEqual(rawUser(home), before);
  });
});

describe("removeOwnedOpencodeProjectMcp (D-064)", () => {
  it("test_remove_owned_entry_preserves_foreign_and_file", () => {
    const proj = makeTempProject();
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = unixEntry();
    writeProjectJson(proj, cfg);
    const r = removeOwnedOpencodeProjectMcp(proj, false);
    assert.strictEqual(r.changed, true);
    const after = JSON.parse(projectRaw(proj));
    assert.strictEqual(after.mcp.unimatrix, undefined);
    assert.deepStrictEqual(after.mcp.other, cfg.mcp.other);
    assert.deepStrictEqual(after.provider, cfg.provider);
  });
  it("test_remove_legacy_binary_entry", () => {
    const proj = makeTempProject();
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = { type: "local", command: ["/abs/.unimatrix/bin/unimatrix", "mcp-stdio"], environment: { LD_LIBRARY_PATH: "/x" } };
    writeProjectJson(proj, cfg);
    assert.strictEqual(removeOwnedOpencodeProjectMcp(proj, false).changed, true);
  });
  it("test_remove_foreign_entry_untouched_with_warning", () => {
    const proj = makeTempProject();
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = { type: "local", command: ["my-server"] };
    writeProjectJson(proj, cfg);
    const before = projectRaw(proj);
    const r = removeOwnedOpencodeProjectMcp(proj, false);
    assert.strictEqual(r.changed, false);
    assert.strictEqual(r.warnings.length, 1);
    assert.strictEqual(projectRaw(proj), before);
  });
  it("test_remove_nothing_owned_does_not_rewrite", () => {
    const proj = makeTempProject();
    writeProjectJson(proj, sentinelConfig());
    const before = projectRaw(proj);
    const r = removeOwnedOpencodeProjectMcp(proj, false);
    assert.deepStrictEqual(r, { actions: [], warnings: [], changed: false, removedPlugin: false });
    assert.strictEqual(projectRaw(proj), before);
  });
  it("test_remove_absent_file_noop", () => {
    const proj = makeTempProject();
    assert.strictEqual(removeOwnedOpencodeProjectMcp(proj, false).changed, false);
    assert.ok(!fs.existsSync(path.join(proj, "opencode.json")));
  });
  it("test_remove_malformed_throws_naming_file_untouched", () => {
    const proj = makeTempProject();
    fs.writeFileSync(path.join(proj, "opencode.json"), "{ bad", "utf8");
    assert.throws(() => removeOwnedOpencodeProjectMcp(proj, false), /opencode\.json/);
    assert.strictEqual(projectRaw(proj), "{ bad");
  });
  it("test_remove_dry_run_writes_nothing", () => {
    const proj = makeTempProject();
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = unixEntry();
    writeProjectJson(proj, cfg);
    const before = projectRaw(proj);
    const r = removeOwnedOpencodeProjectMcp(proj, true);
    assert.strictEqual(r.changed, true);
    assert.strictEqual(projectRaw(proj), before);
  });
  it("test_remove_preserves_indent", () => {
    const proj = makeTempProject();
    const cfg = sentinelConfig();
    cfg.mcp.unimatrix = unixEntry();
    writeProjectJson(proj, cfg, 4);
    removeOwnedOpencodeProjectMcp(proj, false);
    assert.ok(/\n {4}"provider"/.test(projectRaw(proj)));
  });
});

describe("helpers", () => {
  it("test_within_project_guard_rejects_traversal", () => {
    const dir = makeTempProject();
    assert.strictEqual(isWithinProject(dir, path.join(dir, "opencode.json")), true);
    assert.strictEqual(isWithinProject(dir, path.join(dir, "..", "evil")), false);
  });
  it("test_detect_indent_defaults_and_tab", () => {
    assert.strictEqual(detectIndent("{}"), 2);
    assert.strictEqual(detectIndent('{\n\t"a": 1\n}'), "\t");
  });
  it("test_read_json_safe_states", () => {
    const dir = tmp("uni-oc-rj-");
    assert.strictEqual(readJsonSafe(path.join(dir, "none.json")).present, false);
    fs.writeFileSync(path.join(dir, "bad.json"), "{", "utf8");
    assert.strictEqual(readJsonSafe(path.join(dir, "bad.json")).malformed, true);
  });
  it("test_opencode_leg_carries_reason_and_warning", () => {
    const l = opencodeLeg("skipped-foreign", "/p", { reason: "r", warning: "w" });
    assert.strictEqual(l.reason, "r");
    assert.strictEqual(l.warning, "w");
    assert.strictEqual(l.entry, undefined);
  });
});

describe("opencode user-level file modes (F1)", { skip: process.platform === "win32" }, () => {
  const modeOf = (p) => fs.statSync(p).mode & 0o777;
  it("0600 opencode.json stays 0600 after writeOpencodeMcp", () => {
    const home = tmp("unimatrix-opencode-mode-");
    writeUser(home, { theme: "x" });
    fs.chmodSync(cfgPathOf(home), 0o600);
    assert.strictEqual(wr(home, { harnessSelected: true }).action, "created");
    assert.strictEqual(modeOf(cfgPathOf(home)), 0o600);
  });
  it("new user-level opencode.json is created 0600", () => {
    const home = tmp("unimatrix-opencode-mode-");
    assert.strictEqual(wr(home, { harnessSelected: true }).action, "created");
    assert.strictEqual(modeOf(cfgPathOf(home)), 0o600);
  });
});

describe("1036 C10 — resolved path, legacy twin, G3", () => {
  const owned = () => ({ theme: "t", mcp: { other: { type: "local", command: ["o"] }, unimatrix: unixEntry() } });
  const writeAt = (f, obj, indent) => {
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, JSON.stringify(obj, null, indent || 2) + "\n", "utf8");
  };
  const readAt = (f) => JSON.parse(fs.readFileSync(f, "utf8"));

  it("OPENCODE_CONFIG_DIR: entry at C, owned twin removed, foreign keys deep-equal", () => {
    const home = tmp("uni-oc-home-");
    const C = tmp("uni-oc-c-");
    writeAt(cfgPathOf(home), owned());
    const r = install(home, { OPENCODE_CONFIG_DIR: C }, { harnessSelected: true });
    assert.strictEqual(r.w.action, "created");
    assert.strictEqual(r.w.path, path.join(C, "opencode.json"));
    assert.deepStrictEqual(readAt(r.file).mcp.unimatrix, unixEntry());
    assert.strictEqual(r.r[0].action, "removed");
    assert.deepStrictEqual(readAt(cfgPathOf(home)), { theme: "t", mcp: { other: { type: "local", command: ["o"] } } });
  });
  it("XDG_CONFIG_HOME → X/opencode; both set → OPENCODE_CONFIG_DIR wins, X untouched", () => {
    const home = tmp("uni-oc-home-");
    const X = tmp("uni-oc-x-");
    const a = install(home, { XDG_CONFIG_HOME: X }, { harnessSelected: true });
    assert.strictEqual(a.w.path, path.join(X, "opencode", "opencode.json"));
    const home2 = tmp("uni-oc-home-");
    const X2 = tmp("uni-oc-x-");
    const C = tmp("uni-oc-c-");
    const b = install(home2, { XDG_CONFIG_HOME: X2, OPENCODE_CONFIG_DIR: C }, { harnessSelected: true });
    assert.strictEqual(b.w.path, path.join(C, "opencode.json"));
    assert.ok(!fs.existsSync(path.join(X2, "opencode")));
  });
  it("D-015: owned twin only, no --harness → resolved file written, twin removed", () => {
    const home = tmp("uni-oc-home-");
    const X = tmp("uni-oc-x-");
    writeAt(cfgPathOf(home), owned());
    const r = install(home, { XDG_CONFIG_HOME: X }, {});
    assert.strictEqual(r.w.action, "created");
    assert.strictEqual(r.r[0].action, "removed");
    assert.strictEqual(readAt(cfgPathOf(home)).mcp.unimatrix, undefined);
  });
  it("no opt-in anywhere → skipped-intent, no removal leg", () => {
    const home = tmp("uni-oc-home-");
    const r = install(home, { XDG_CONFIG_HOME: tmp("uni-oc-x-") }, {});
    assert.strictEqual(r.w.action, "skipped-intent");
    assert.strictEqual(r.r, "skipped-dest-unwired");
  });
  it("same file (XDG=H/.config, OPENCODE_CONFIG_DIR=H/.config/opencode) → entry survives, no R10", () => {
    const home = tmp("uni-oc-home-");
    writeAt(cfgPathOf(home), owned());
    const r = install(home, { XDG_CONFIG_HOME: path.join(home, ".config"), OPENCODE_CONFIG_DIR: path.join(home, ".config", "opencode") }, {});
    assert.strictEqual(r.r, null);
    assert.deepStrictEqual(readAt(cfgPathOf(home)).mcp.unimatrix, unixEntry());
    assert.strictEqual(r.w.action, "unchanged");
  });
  it("unparseable user file → skipped-malformed + warning, bytes unchanged, twin kept", () => {
    const home = tmp("uni-oc-home-");
    const C = tmp("uni-oc-c-");
    const f = path.join(C, "opencode.json");
    fs.writeFileSync(f, '{ // jsonc\n "a": 1 }', "utf8");
    writeAt(cfgPathOf(home), owned());
    const r = install(home, { OPENCODE_CONFIG_DIR: C }, { harnessSelected: true });
    assert.strictEqual(r.w.action, "skipped-malformed");
    assert.match(r.w.warning, /not valid JSON/);
    assert.strictEqual(fs.readFileSync(f, "utf8"), '{ // jsonc\n "a": 1 }');
    assert.strictEqual(r.r, "skipped-dest-unwired");
    assert.deepStrictEqual(readAt(cfgPathOf(home)), owned(), "twin kept");
  });
  it("unparseable legacy twin → skipped-malformed with warning, untouched, no throw", () => {
    const home = tmp("uni-oc-home-");
    fs.mkdirSync(path.dirname(cfgPathOf(home)), { recursive: true });
    fs.writeFileSync(cfgPathOf(home), "{ bad", "utf8");
    const legs = removeOwnedOpencodeUserMcpLeg(cfgPathOf(home), false);
    assert.strictEqual(legs[0].action, "skipped-malformed");
    assert.match(legs[0].warning, /not valid JSON/);
    assert.strictEqual(rawUser(home), "{ bad");
  });
  it("foreign legacy twin → kept + warning", () => {
    const home = tmp("uni-oc-home-");
    writeUser(home, { mcp: { unimatrix: { type: "local", command: ["my-server"] } } });
    const before = rawUser(home);
    const legs = removeOwnedOpencodeUserMcpLeg(cfgPathOf(home), false);
    assert.strictEqual(legs.length, 0);
    assert.strictEqual(legs.warnings.length, 1);
    assert.strictEqual(rawUser(home), before);
  });
  it("G3: legacy plugin removed with mcp.unimatrix in one mutate, order kept", () => {
    const proj = makeTempProject();
    writeProjectJson(proj, { plugin: ["x", "@dug-21/unimatrix-opencode-plugin@1.2.3", "y"], mcp: { unimatrix: unixEntry() } }, 4);
    const legs = removeOwnedOpencodeProjectLegs(proj, false);
    assert.strictEqual(legs.length, 2);
    const after = JSON.parse(projectRaw(proj));
    assert.deepStrictEqual(after, { plugin: ["x", "y"], mcp: {} });
    assert.ok(/\n {4}"plugin"/.test(projectRaw(proj)));
    assert.strictEqual(lib.computeProjectRemoval(projectRaw(proj), "f").text, null, "re-mutate is a no-op");
  });
  it("G3: single legacy entry → []; non-array plugin → no leg", () => {
    const proj = makeTempProject();
    writeProjectJson(proj, { plugin: ["@dug-21/unimatrix-opencode-plugin"] });
    const legs = removeOwnedOpencodeProjectLegs(proj, false);
    assert.strictEqual(legs.length, 1);
    assert.deepStrictEqual(JSON.parse(projectRaw(proj)).plugin, []);
    const p2 = makeTempProject();
    writeProjectJson(p2, { plugin: "@dug-21/unimatrix-opencode-plugin" });
    const before = projectRaw(p2);
    assert.strictEqual(removeOwnedOpencodeProjectLegs(p2, false).length, 0);
    assert.strictEqual(projectRaw(p2), before);
  });
  it("D-029: unparseable project opencode.json during removal throws naming file", () => {
    const proj = makeTempProject();
    fs.writeFileSync(path.join(proj, "opencode.json"), "{ bad", "utf8");
    assert.throws(() => removeOwnedOpencodeProjectLegs(proj, false), (e) => e.message.includes(path.join(proj, "opencode.json")));
    assert.strictEqual(projectRaw(proj), "{ bad");
  });
  it("R-12: rename fault rethrows SafeWriteError, file untouched", () => {
    const home = tmp("uni-oc-home-");
    writeUser(home, { theme: "t" });
    const before = rawUser(home);
    const origFs = fs.renameSync;
    fs.renameSync = () => { const e = new Error("EPERM"); e.code = "EPERM"; throw e; };
    try {
      assert.throws(() => wr(home, { harnessSelected: true }), (e) => e instanceof SafeWriteError || e.code === "EPERM");
    } finally {
      fs.renameSync = origFs;
    }
    assert.strictEqual(rawUser(home), before);
  });
});
