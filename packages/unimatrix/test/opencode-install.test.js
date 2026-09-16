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
  writeOpencodeMcp,
  buildOpencodeEntry,
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

// ── writeOpencodeMcp — retrieval writer (ADR-001, vnc-049 C-05, AC-05/10) ──
//
// New additive `mcp.unimatrix` retrieval entry. Byte-for-byte sentinel
// preservation (provider + foreign keys), WireLeg contract, idempotence,
// malformed fail-safe, dry-run, intent gate. The live `context_*` RETURN leg
// (AC-05/AC-10) is a Stage 3c C14-verifier concern, not exercised here.

const LOCAL_BINARY = "/opt/uni/bin/unimatrix";

/** The exact local (stdio-binary) entry the writer must emit for LOCAL_BINARY. */
function expectedLocalEntry() {
  return {
    type: "local",
    command: [LOCAL_BINARY],
    environment: { LD_LIBRARY_PATH: "/opt/uni/bin" },
    enabled: true,
  };
}

/** opencode.json fixture WITHOUT a pre-existing mcp.unimatrix (fresh case). */
function freshConfig() {
  return {
    $schema: "https://opencode.ai/config.json",
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

function readRaw(dir) {
  return fs.readFileSync(path.join(dir, "opencode.json"), "utf8");
}

function assertWireLegShape(leg, action) {
  assert.strictEqual(leg.harness, "opencode");
  assert.strictEqual(leg.surface, "retrieval");
  assert.strictEqual(leg.action, action);
  assert.strictEqual(typeof leg.path, "string");
  if (action.startsWith("skipped")) {
    assert.strictEqual(typeof leg.reason, "string", "skipped leg carries a reason");
    assert.strictEqual(leg.entry, undefined, "skipped leg has no entry");
  } else {
    assert.ok(leg.entry && typeof leg.entry === "object", "write leg carries an entry");
  }
}

describe("writeOpencodeMcp — sentinel preservation (R-08, C-05)", () => {
  it("test_writeOpencodeMcp_preserves_ollama_provider_block", () => {
    const dir = makeTempProject();
    const original = sentinelConfig();
    writeOpencodeJson(dir, original);
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY }, false);
    // Unimatrix owns mcp.unimatrix — it may be updated — but the provider block
    // and permissions are foreign and survive byte-for-byte.
    const after = readJson(path.join(dir, "opencode.json"));
    assert.strictEqual(
      JSON.stringify(after.provider, null, 2),
      JSON.stringify(original.provider, null, 2)
    );
    assert.strictEqual(
      JSON.stringify(after.permission, null, 2),
      JSON.stringify(original.permission, null, 2)
    );
    assert.ok(["created", "updated", "unchanged"].includes(leg.action));
  });

  it("test_writeOpencodeMcp_preserves_existing_mcp_unimatrix", () => {
    const dir = makeTempProject();
    // Pre-existing entry equal to what the writer would emit → unchanged, no write.
    const cfg = freshConfig();
    cfg.mcp = { unimatrix: expectedLocalEntry() };
    writeOpencodeJson(dir, cfg);
    const before = readRaw(dir);
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY }, false);
    assertWireLegShape(leg, "unchanged");
    assert.deepStrictEqual(leg.entry, expectedLocalEntry());
    assert.strictEqual(readRaw(dir), before, "no write on unchanged");
  });

  it("test_writeOpencodeMcp_preserves_foreign_keys_and_indent", () => {
    const dir = makeTempProject();
    const cfg = freshConfig();
    cfg.customTool = { alpha: 1, beta: ["x", "y"] };
    // Write with 4-space indent to prove detectIndent is honored.
    fs.writeFileSync(
      path.join(dir, "opencode.json"),
      JSON.stringify(cfg, null, 4) + "\n",
      "utf8"
    );
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    assertWireLegShape(leg, "created");
    const raw = readRaw(dir);
    const after = readJson(path.join(dir, "opencode.json"));
    assert.deepStrictEqual(after.customTool, cfg.customTool, "foreign key preserved");
    assert.deepStrictEqual(after.provider, cfg.provider, "provider preserved");
    // 4-space indent preserved (a top-level key sits at 4 spaces).
    assert.ok(/\n {4}"provider"/.test(raw), "4-space indent preserved");
    assert.deepStrictEqual(after.mcp.unimatrix, expectedLocalEntry());
  });
});

describe("writeOpencodeMcp — fresh additive write (AC-05)", () => {
  it("test_writeOpencodeMcp_fresh_creates_mcp_unimatrix", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, freshConfig());
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    assertWireLegShape(leg, "created");
    assert.deepStrictEqual(leg.entry, expectedLocalEntry());
    const after = readJson(path.join(dir, "opencode.json"));
    assert.deepStrictEqual(after.mcp.unimatrix, expectedLocalEntry());
    // Foreign keys preserved; only mcp is additive.
    assert.deepStrictEqual(after.provider, freshConfig().provider);
  });

  it("test_writeOpencodeMcp_entry_shape_local_vs_cloud", () => {
    // Local (stdio-binary).
    const local = buildOpencodeEntry({ binaryPath: LOCAL_BINARY });
    assert.deepStrictEqual(local, expectedLocalEntry());

    // Cloud (token-free stdio-bridge). NEVER a token-bearing url (Q1).
    const cloud = buildOpencodeEntry({
      transport: { kind: "stdio-bridge", bridgePath: "/proj/.unimatrix/mcp-bridge.js", projectHash: "abc123" },
    });
    assert.deepStrictEqual(cloud, {
      type: "local",
      command: ["node", "/proj/.unimatrix/mcp-bridge.js", "abc123"],
      enabled: true,
    });
    const cloudJson = JSON.stringify(cloud);
    assert.ok(!/token|url|bearer/i.test(cloudJson), "no secret/url in cloud entry");
  });

  it("test_writeOpencodeMcp_bare_url_never_emits_token_entry", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, freshConfig());
    const before = readRaw(dir);
    // Only a bare url (no bridge descriptor) → cannot emit token-free → skip.
    const leg = writeOpencodeMcp(dir, { url: "https://cloud/mcp?token=SECRET", harnessSelected: true }, false);
    assert.strictEqual(leg.action, "skipped");
    // The writer never reads or echoes the url, so neither the secret nor the
    // url appears anywhere in the leg (the reason mentions "token-free" only).
    assert.ok(!/SECRET|https:\/\//.test(JSON.stringify(leg)), "no secret/url leaked into the leg");
    assert.strictEqual(readRaw(dir), before, "no write");
  });
});

describe("writeOpencodeMcp — idempotence (AC-04)", () => {
  it("test_writeOpencodeMcp_run_twice_byte_identical", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, freshConfig());
    writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    const afterFirst = readRaw(dir);
    const leg2 = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    const afterSecond = readRaw(dir);
    assert.strictEqual(afterSecond, afterFirst, "second run byte-identical");
    assert.strictEqual(leg2.action, "unchanged");
  });

  it("test_writeOpencodeMcp_updates_stale_command", () => {
    const dir = makeTempProject();
    const cfg = freshConfig();
    cfg.mcp = { unimatrix: { type: "local", command: ["/old/path/unimatrix"], enabled: true } };
    writeOpencodeJson(dir, cfg);
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY }, false);
    assertWireLegShape(leg, "updated");
    const after = readJson(path.join(dir, "opencode.json"));
    assert.deepStrictEqual(after.mcp.unimatrix, expectedLocalEntry());
    // Surrounding keys byte-identical.
    assert.deepStrictEqual(after.provider, cfg.provider);
  });
});

describe("writeOpencodeMcp — malformed / fail-safe (R-11, AC-15)", () => {
  it("test_writeOpencodeMcp_malformed_json_skips_and_preserves", () => {
    const dir = makeTempProject();
    const malformed = "{ this is not : json ]";
    fs.writeFileSync(path.join(dir, "opencode.json"), malformed, "utf8");
    let leg;
    assert.doesNotThrow(() => {
      leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    });
    assert.strictEqual(leg.action, "skipped-malformed");
    assert.strictEqual(readRaw(dir), malformed, "malformed file byte-preserved");
  });

  it("test_writeOpencodeMcp_mcp_not_object_skips", () => {
    const dir = makeTempProject();
    const cfg = freshConfig();
    cfg.mcp = "not-an-object";
    writeOpencodeJson(dir, cfg);
    const before = readRaw(dir);
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    assert.strictEqual(leg.action, "skipped-malformed");
    assert.strictEqual(readRaw(dir), before, "user value not clobbered");
  });
});

describe("writeOpencodeMcp — intent gate (AC-11)", () => {
  it("test_writeOpencodeMcp_new_entry_without_intent_skipped", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, freshConfig());
    const before = readRaw(dir);
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY }, false);
    assertWireLegShape(leg, "skipped-intent");
    assert.strictEqual(readRaw(dir), before, "no write without opt-in");
  });

  it("test_writeOpencodeMcp_additive_into_existing_no_optin_needed", () => {
    const dir = makeTempProject();
    const cfg = freshConfig();
    cfg.mcp = { unimatrix: { type: "local", command: ["/old/path/unimatrix"], enabled: true } };
    writeOpencodeJson(dir, cfg);
    // No harnessSelected — an existing entry proceeds (updated), not skipped.
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY }, false);
    assert.strictEqual(leg.action, "updated");
  });
});

describe("writeOpencodeMcp — dry-run + containment (AC-12, AC-14)", () => {
  it("test_writeOpencodeMcp_dry_run_writes_nothing", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, freshConfig());
    const before = readRaw(dir);
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, true);
    assertWireLegShape(leg, "created");
    assert.deepStrictEqual(leg.entry, expectedLocalEntry());
    assert.strictEqual(readRaw(dir), before, "dry-run wrote nothing");
  });

  it("test_writeOpencodeMcp_within_project_guard", () => {
    const dir = makeTempProject();
    writeOpencodeJson(dir, freshConfig());
    const leg = writeOpencodeMcp(dir, { binaryPath: LOCAL_BINARY, harnessSelected: true }, false);
    // The only target is <dir>/opencode.json — always inside the project.
    assert.ok(isWithinProject(dir, leg.path), "target within project");
    assert.strictEqual(leg.path, path.join(dir, "opencode.json"));
  });
});
