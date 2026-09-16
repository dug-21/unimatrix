"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it } = require("node:test");

// Component tests for C9 (vnc-049): the non-clobbering, additive OpenCode
// installer branch. Drives the real provisioning entry point
// (maybeProvisionOpenCode) against on-disk fixtures — the behavioral outcome is
// the provisioned plugin + the byte-for-byte-preserved retrieval sentinel.
// (The full `unimatrix init` + live `context_*` retrieval leg — AC-05c — is a
// Stage 3c integration concern and is NOT exercised here.)

const {
  detectsOpenCode,
  provisionOpenCode,
  maybeProvisionOpenCode,
  appendPluginEntry,
  mergePackageDep,
  isWithinProject,
  PLUGIN_PACKAGE,
  PLUGIN_ENTRY_FILE,
  PLUGIN_EXPORT,
} = require("../lib/opencode-install.js");

/** Temp project root with .git (matches init.test.js helper idiom). */
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-opencode-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

/** The canonical retrieval-sentinel opencode.json (C10, AC-05). */
function sentinelConfig() {
  return {
    $schema: "https://opencode.ai/config.json",
    mcp: {
      unimatrix: {
        type: "local",
        command: ["/abs/path/unimatrix", "mcp"],
        environment: { LD_LIBRARY_PATH: "/abs/path" },
      },
    },
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

/** Write opencode.json into dir with 2-space indent. */
function writeOpencodeJson(dir, obj) {
  fs.writeFileSync(
    path.join(dir, "opencode.json"),
    JSON.stringify(obj, null, 2) + "\n",
    "utf8"
  );
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

// ── Detection (FR-09, AC-05a) ───────────────────────────────────────

describe("detection", () => {
  it("test_installer_detects_opencode_by_opencode_json", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
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
    const actions = maybeProvisionOpenCode(dir, {});
    assert.deepStrictEqual(actions, []);
    // No OpenCode artifacts written.
    assert.ok(!fs.existsSync(path.join(dir, ".opencode")));
    assert.ok(!fs.existsSync(path.join(dir, "opencode.json")));
  });
});

// ── Provisioning (FR-09, AC-05a) ────────────────────────────────────

describe("provisioning", () => {
  it("test_installer_provisions_plugin_into_dot_opencode_plugins", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    maybeProvisionOpenCode(dir, {});
    const shim = path.join(dir, ".opencode", "plugins", PLUGIN_ENTRY_FILE);
    assert.ok(fs.existsSync(shim), "shim dropped into .opencode/plugins/");
    const content = fs.readFileSync(shim, "utf8");
    assert.ok(content.includes(PLUGIN_EXPORT));
    assert.ok(content.includes(PLUGIN_PACKAGE));
  });

  it("test_installer_adds_package_dep", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    maybeProvisionOpenCode(dir, {});
    const pkg = readJson(path.join(dir, ".opencode", "package.json"));
    assert.ok(
      Object.prototype.hasOwnProperty.call(pkg.dependencies, PLUGIN_PACKAGE),
      "plugin dep added to .opencode/package.json"
    );
  });

  it("test_installer_merges_dep_into_existing_package_json", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    const ocDir = path.join(dir, ".opencode");
    fs.mkdirSync(ocDir, { recursive: true });
    fs.writeFileSync(
      path.join(ocDir, "package.json"),
      JSON.stringify(
        { dependencies: { "@opencode-ai/plugin": "1.18.31" } },
        null,
        2
      ) + "\n",
      "utf8"
    );
    maybeProvisionOpenCode(dir, {});
    const pkg = readJson(path.join(ocDir, "package.json"));
    assert.strictEqual(
      pkg.dependencies["@opencode-ai/plugin"],
      "1.18.31",
      "pre-existing dep preserved"
    );
    assert.ok(pkg.dependencies[PLUGIN_PACKAGE], "plugin dep added");
  });

  it("test_installer_appends_plugin_array_entry", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    maybeProvisionOpenCode(dir, {});
    const cfg = readJson(path.join(dir, "opencode.json"));
    assert.ok(Array.isArray(cfg.plugin));
    assert.ok(cfg.plugin.includes(PLUGIN_PACKAGE));
  });

  it("test_installer_dry_run_writes_nothing", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    const before = fs.readFileSync(path.join(dir, "opencode.json"), "utf8");
    const actions = maybeProvisionOpenCode(dir, { dryRun: true });
    assert.ok(actions.every((a) => a.startsWith("[dry-run]") || a.startsWith("Note:")));
    assert.ok(!fs.existsSync(path.join(dir, ".opencode", "plugins", PLUGIN_ENTRY_FILE)));
    assert.strictEqual(fs.readFileSync(path.join(dir, "opencode.json"), "utf8"), before);
  });
});

// ── Byte-for-byte preservation — C10 regression sentinel (AC-05b, R-08) ──

describe("preservation", () => {
  it("test_installer_preserves_mcp_unimatrix_byte_for_byte", () => {
    const dir = makeTempProject();
    const original = sentinelConfig();
    writeOpencodeJson(dir, original);
    maybeProvisionOpenCode(dir, {});
    const after = readJson(path.join(dir, "opencode.json"));
    assert.strictEqual(
      JSON.stringify(after.mcp, null, 2),
      JSON.stringify(original.mcp, null, 2)
    );
  });

  it("test_installer_preserves_local_stdio_command", () => {
    const dir = makeTempProject();
    const original = sentinelConfig();
    writeOpencodeJson(dir, original);
    maybeProvisionOpenCode(dir, {});
    const after = readJson(path.join(dir, "opencode.json"));
    assert.deepStrictEqual(
      after.mcp.unimatrix.command,
      original.mcp.unimatrix.command
    );
  });

  it("test_installer_preserves_ollama_provider_block", () => {
    const dir = makeTempProject();
    const original = sentinelConfig();
    writeOpencodeJson(dir, original);
    maybeProvisionOpenCode(dir, {});
    const after = readJson(path.join(dir, "opencode.json"));
    assert.strictEqual(
      JSON.stringify(after.provider, null, 2),
      JSON.stringify(original.provider, null, 2)
    );
  });

  it("test_installer_preserves_non_unimatrix_keys", () => {
    const dir = makeTempProject();
    const original = sentinelConfig();
    writeOpencodeJson(dir, original);
    maybeProvisionOpenCode(dir, {});
    const after = readJson(path.join(dir, "opencode.json"));
    // Every pre-existing key survives byte-for-byte; only `plugin` is additive.
    for (const key of Object.keys(original)) {
      assert.strictEqual(
        JSON.stringify(after[key], null, 2),
        JSON.stringify(original[key], null, 2),
        "preserved key drifted: " + key
      );
    }
    const addedKeys = Object.keys(after).filter((k) => !(k in original));
    assert.deepStrictEqual(addedKeys, ["plugin"], "only additive key is `plugin`");
  });
});

// ── Idempotence (NFR-06, AC-05d, R-09) ──────────────────────────────

describe("idempotence", () => {
  it("test_installer_rerun_no_duplicate_plugin_entry", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    maybeProvisionOpenCode(dir, {});
    maybeProvisionOpenCode(dir, {});
    const cfg = readJson(path.join(dir, "opencode.json"));
    const hits = cfg.plugin.filter((e) => e === PLUGIN_PACKAGE);
    assert.strictEqual(hits.length, 1);
  });

  it("test_installer_rerun_no_duplicate_package_dep", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    maybeProvisionOpenCode(dir, {});
    const first = readJson(path.join(dir, ".opencode", "package.json"));
    maybeProvisionOpenCode(dir, {});
    const second = readJson(path.join(dir, ".opencode", "package.json"));
    assert.deepStrictEqual(second.dependencies, first.dependencies);
  });

  it("test_installer_rerun_no_key_drift", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    maybeProvisionOpenCode(dir, {});
    const afterFirst = fs.readFileSync(path.join(dir, "opencode.json"), "utf8");
    maybeProvisionOpenCode(dir, {});
    const afterSecond = fs.readFileSync(path.join(dir, "opencode.json"), "utf8");
    assert.strictEqual(afterSecond, afterFirst, "second run is byte-identical");
  });
});

// ── Untrusted installer input (R-15, security) ──────────────────────

describe("untrusted input (R-15)", () => {
  it("test_installer_follows_no_injected_path", () => {
    const dir = makeTempProject();
    const cfg = sentinelConfig();
    // Crafted pre-existing entries: traversal + absolute path. They must be
    // preserved (non-clobber) but NEVER followed/acted upon.
    cfg.plugin = ["../../../etc/evil.js", "/tmp/attacker/plugin.js"];
    writeOpencodeJson(dir, cfg);
    maybeProvisionOpenCode(dir, {});

    // Nothing written outside the project's .opencode/ surface.
    assert.ok(!fs.existsSync("/tmp/attacker/plugin.js"));
    const outside = path.resolve(dir, "..", "..", "..", "etc", "evil.js");
    assert.ok(!fs.existsSync(outside));

    // Crafted entries preserved; our entry appended additively.
    const after = readJson(path.join(dir, "opencode.json"));
    assert.ok(after.plugin.includes("../../../etc/evil.js"));
    assert.ok(after.plugin.includes("/tmp/attacker/plugin.js"));
    assert.ok(after.plugin.includes(PLUGIN_PACKAGE));
  });

  it("test_installer_within_project_guard_rejects_traversal", () => {
    const dir = makeTempProject();
    assert.strictEqual(isWithinProject(dir, path.join(dir, ".opencode", "x")), true);
    assert.strictEqual(isWithinProject(dir, path.join(dir, "..", "escape")), false);
    assert.strictEqual(isWithinProject(dir, "/etc/passwd"), false);
  });

  it("test_installer_does_not_execute_untrusted_content", () => {
    const dir = makeTempProject();
    // A poisoned config that would blow up if `require`d/eval'd. Treated as data.
    const poisoned =
      '{ "mcp": { "unimatrix": {} }, "x": "throw new Error(\'pwned\')" }';
    fs.writeFileSync(path.join(dir, "opencode.json"), poisoned, "utf8");
    const actions = maybeProvisionOpenCode(dir, {});
    // Parsed as JSON data; provisioning proceeds without executing content.
    assert.ok(Array.isArray(actions));
    const after = readJson(path.join(dir, "opencode.json"));
    assert.strictEqual(after.x, "throw new Error('pwned')");
  });

  it("test_installer_partial_preexisting_plugin_array", () => {
    const dir = makeTempProject();
    const cfg = sentinelConfig();
    cfg.plugin = ["some-other-plugin"];
    writeOpencodeJson(dir, cfg);
    maybeProvisionOpenCode(dir, {});
    const after = readJson(path.join(dir, "opencode.json"));
    assert.deepStrictEqual(after.plugin, ["some-other-plugin", PLUGIN_PACKAGE]);
  });
});

// ── Malformed config + edge cases (R-15, ADR-006 §Error handling) ────

describe("fail-safe / edge cases", () => {
  it("test_installer_malformed_opencode_json_no_clobber", () => {
    const dir = makeTempProject();
    const malformed = "{ this is not : json ]";
    fs.writeFileSync(path.join(dir, "opencode.json"), malformed, "utf8");
    const actions = maybeProvisionOpenCode(dir, {});
    // File preserved verbatim; array leg skipped with a note; never throws.
    assert.strictEqual(fs.readFileSync(path.join(dir, "opencode.json"), "utf8"), malformed);
    assert.ok(actions.some((a) => a.includes("malformed")));
    // Mode A (plugin dir) still provisions — it does not touch opencode.json.
    assert.ok(fs.existsSync(path.join(dir, ".opencode", "plugins", PLUGIN_ENTRY_FILE)));
  });

  it("test_installer_opencode_json_present_dot_opencode_absent", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, sentinelConfig());
    assert.ok(!fs.existsSync(path.join(dir, ".opencode")));
    maybeProvisionOpenCode(dir, {});
    assert.ok(fs.existsSync(path.join(dir, ".opencode", "plugins", PLUGIN_ENTRY_FILE)));
    assert.ok(fs.existsSync(path.join(dir, ".opencode", "package.json")));
  });

  it("test_installer_dot_opencode_present_opencode_json_absent", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".opencode"), { recursive: true });
    maybeProvisionOpenCode(dir, {});
    // Mode A provisions; no opencode.json to append to (and none created).
    assert.ok(fs.existsSync(path.join(dir, ".opencode", "plugins", PLUGIN_ENTRY_FILE)));
    assert.ok(!fs.existsSync(path.join(dir, "opencode.json")));
  });

  it("test_installer_provision_never_throws_on_bad_dir", () => {
    // Non-existent dir: detection is false → no-op, no throw.
    const actions = maybeProvisionOpenCode("/nonexistent/path/xyz", {});
    assert.deepStrictEqual(actions, []);
  });
});

// ── Pure merge helpers (nan-004 principle) ──────────────────────────

describe("merge helpers", () => {
  it("test_append_plugin_entry_idempotent", () => {
    const cfg = {};
    assert.strictEqual(appendPluginEntry(cfg, "x"), true);
    assert.strictEqual(appendPluginEntry(cfg, "x"), false);
    assert.deepStrictEqual(cfg.plugin, ["x"]);
  });

  it("test_append_plugin_entry_never_clobbers_non_array", () => {
    const cfg = { plugin: "not-an-array" };
    assert.strictEqual(appendPluginEntry(cfg, "x"), false);
    assert.strictEqual(cfg.plugin, "not-an-array");
  });

  it("test_merge_package_dep_preserves_existing_version", () => {
    const pkg = { dependencies: { foo: "1.0.0" } };
    assert.strictEqual(mergePackageDep(pkg, "foo", "^2.0.0"), false);
    assert.strictEqual(pkg.dependencies.foo, "1.0.0");
    assert.strictEqual(mergePackageDep(pkg, "bar", "^2.0.0"), true);
    assert.strictEqual(pkg.dependencies.bar, "^2.0.0");
  });
});
