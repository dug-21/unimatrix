"use strict";

// init-remote.test.js — vnc-026 F3 `init --remote` branch + merge-settings
// generalization, RE-WIRED for vnc-039 Scope A+B. Covers: ownership-pattern
// spaced-path table (R-11), init matrix (AC-11), the OUT-OF-TREE credential
// store (R-12, replacing the in-tree settings.local.json write), the TOKEN-FREE
// stdio .mcp.json bridge entry (AC-01/AC-07/AC-09), the legacy bundle-only
// unsupported message (AC-10), the stale-in-tree-creds migration, Ping loud
// failure (R-18), commandSource back-compat (AC-16), and FR-21 9-event
// regression. Cumulative infra — extends the existing node:test suites; no
// isolated scaffolding. os.homedir() is overridden to a temp dir so the store
// lands out-of-tree under a temp root.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const {
  mergeSettings,
  isUnimatrixHook,
  buildHookClientCommand,
  normalizeCommandSource,
  UNIMATRIX_PATTERNS,
  HOOK_EVENTS,
  EVENT_MATCHERS,
} = require("../lib/merge-settings.js");

const initModule = require("../lib/init.js");
const { initRemote, LEGACY_MCP_UNSUPPORTED_MESSAGE } = initModule;
const credstore = require("../lib/hook-client/credstore.js");
const { computeProjectHash } = require("../lib/hook-client/config.js");
// init.js holds a reference to this exact module object; overriding the method
// here is observed by initRemote (no module-cache surgery needed).
const transport = require("../lib/hook-client/transport-http.js");

const REMOTE = "https://unimatrix.example.com";
const TOKEN = "unit-test-placeholder-token-2";

// A fixture v:2 bundle (the bundle path is the Scope A surface). 64-hex token,
// sha256:<64 hex> fp; both URLs https. Decoded verbatim by decodeBundle.
const BUNDLE_MCP_URL = "https://unimatrix.example.com/v1/myslug";
const BUNDLE_OBSERVE_URL = "https://unimatrix.example.com/v1/myslug/observe";
const BUNDLE_TOKEN = "a".repeat(64);
const BUNDLE_FP = "sha256:" + "b".repeat(64);
function makeBundle() {
  const json = JSON.stringify({
    v: 2,
    mcp_url: BUNDLE_MCP_URL,
    observe_url: BUNDLE_OBSERVE_URL,
    token: BUNDLE_TOKEN,
    fp: BUNDLE_FP,
  });
  return "unimatrix-bundle:" + Buffer.from(json, "utf8").toString("base64url");
}

// Resolve the same client path initRemote will write, so command-string
// assertions are exact regardless of the install location. index.js is owned by
// the Wave-3 entry/dispatch work and may not exist yet; mirror init.js's
// resolve-with-fallback so this suite is green before and after it lands.
let CLIENT_PATH;
try {
  CLIENT_PATH = require.resolve("../lib/hook-client/index.js");
} catch (_err) {
  CLIENT_PATH = path.join(__dirname, "..", "lib", "hook-client", "index.js");
}

function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-remote-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

/**
 * Write the SubagentStop opt-in key into {root}/.claude/settings.local.json
 * (vnc-027 ADR-004 §2). Used by tests that assert the full 9-event contract;
 * SubagentStop is default-off otherwise.
 */
function writeSubagentOptIn(projectRoot) {
  const claudeDir = path.join(projectRoot, ".claude");
  fs.mkdirSync(claudeDir, { recursive: true });
  fs.writeFileSync(
    path.join(claudeDir, "settings.local.json"),
    JSON.stringify({ unimatrix: { hooks: { subagent_stop: true } } }, null, 2),
    "utf8"
  );
}

function readSettings(projectRoot) {
  const fp = path.join(projectRoot, ".claude", "settings.json");
  return JSON.parse(fs.readFileSync(fp, "utf8"));
}

function countUnimatrixHooks(content, event) {
  let count = 0;
  for (const group of content.hooks[event] || []) {
    for (const hook of group.hooks || []) {
      if (isUnimatrixHook(hook)) count++;
    }
  }
  return count;
}

// Stub pingForInit; restore in afterEach.
let origPing;
function stubPing(fn) {
  origPing = transport.pingForInit;
  transport.pingForInit = fn;
}
function okPing() {
  return Promise.resolve({ ok: true, message: "Pong from host (server x)" });
}

// Resolve the bridge path init writes, mirroring init.js's resolve-with-fallback
// so .mcp.json command-string assertions are exact whether or not the module
// exists yet at test time.
let BRIDGE_PATH;
try {
  BRIDGE_PATH = require.resolve("../lib/hook-client/mcp-bridge.js");
} catch (_err) {
  BRIDGE_PATH = path.join(__dirname, "..", "lib", "hook-client", "mcp-bridge.js");
}

// Override os.homedir() to a per-test temp dir so the credential store lands
// out-of-tree under a sandboxed root (never the developer's real ~/.unimatrix).
let origHomedir;
let tempHome;
function readStore(projectRoot) {
  return credstore.read(computeProjectHash(projectRoot));
}
function storePath(projectRoot) {
  return credstore.pathFor(computeProjectHash(projectRoot));
}

// Silence init's summary output during tests.
let origLog;
beforeEach(() => {
  origLog = console.log;
  console.log = () => {};
  origHomedir = os.homedir;
  tempHome = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-home-"));
  os.homedir = () => tempHome;
});
afterEach(() => {
  console.log = origLog;
  os.homedir = origHomedir;
  if (origPing) {
    transport.pingForInit = origPing;
    origPing = undefined;
  }
});

// ── detectProjectRoot worktree parity (project.rs::resolve_git_file) ─

describe("detectProjectRoot worktree parity", () => {
  const { detectProjectRoot } = initModule;

  it("resolves a worktree .git FILE to the main repo root", () => {
    // init --remote from a worktree must write settings/hooks into the MAIN
    // root — the same root the hook client resolves at spawn time (ADR-006).
    const main = fs.realpathSync(makeTempProject());
    fs.mkdirSync(path.join(main, ".git", "worktrees", "wt"), { recursive: true });
    const wt = fs.realpathSync(
      fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-remote-wt-"))
    );
    fs.writeFileSync(
      path.join(wt, ".git"),
      "gitdir: " + path.join(main, ".git", "worktrees", "wt") + "\n"
    );
    assert.strictEqual(detectProjectRoot(wt), main);
  });

  it("falls back to the containing dir on a malformed .git file", () => {
    const dir = fs.realpathSync(
      fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-remote-wt-"))
    );
    fs.writeFileSync(path.join(dir, ".git"), "no gitdir line here\n");
    assert.strictEqual(detectProjectRoot(dir), dir);
  });
});

// ── Ownership Pattern Table (R-11 — the only open gate note) ─────────

describe("ownership pattern (R-11 spaced-path table)", () => {
  function matches(cmd) {
    return UNIMATRIX_PATTERNS.some((p) => p.test(cmd));
  }

  it("test_pattern_table_positive", () => {
    const positives = [
      "node /a/b/lib/hook-client/index.js SessionStart",
      'node "/Users/d/My Projects/n/lib/hook-client/index.js" Stop',
      'node "C:\\Program Files\\n\\lib\\hook-client\\index.js" PreCompact',
      "node C:\\u\\lib\\hook-client\\index.js PostToolUse",
      "/usr/bin/env node '/a b/lib/hook-client/index.js' SubagentStart",
      "node /usr/lib/node_modules/@dug-21/unimatrix/lib/hook-client/index.js PreToolUse",
    ];
    for (const cmd of positives) {
      assert.ok(matches(cmd), "expected MATCH: " + cmd);
    }
  });

  it("test_pattern_table_negative", () => {
    const negatives = [
      "node /a/b/lib/other-client/index.js Stop",
      "node script.js /a/hook-client/index.js Stop", // path not adjacent to node
      "echo hook-client/index.js",
      "node /opt/other-tool/index.js X",
      "node /x/hook-client-extra/index.js X",
      "some-binary hook-client/index.js",
    ];
    for (const cmd of negatives) {
      assert.ok(!matches(cmd), "expected NO MATCH: " + cmd);
    }
  });

  it("test_old_style_unimatrix_still_matched", () => {
    // Mode-switch replacement relies on the legacy patterns still matching.
    assert.ok(matches("unimatrix hook SessionStart"));
    assert.ok(matches("/old/path/unimatrix hook Stop"));
    assert.ok(matches("unimatrix-server hook PreToolUse"));
  });

  it("test_unimatrix_hook_legacy_not_matched_by_node_pattern", () => {
    // `unimatrix hook X` matches a legacy pattern, NOT the node pattern.
    const nodePattern = UNIMATRIX_PATTERNS[UNIMATRIX_PATTERNS.length - 1];
    assert.ok(!nodePattern.test("unimatrix hook SessionStart"));
  });
});

// ── buildHookClientCommand (R-11 quoting) ───────────────────────────

describe("buildHookClientCommand", () => {
  it("test_unquoted_when_no_space", () => {
    assert.strictEqual(
      buildHookClientCommand("/a/b/hook-client/index.js", "Stop"),
      "node /a/b/hook-client/index.js Stop"
    );
  });

  it("test_quoted_when_spaced", () => {
    assert.strictEqual(
      buildHookClientCommand("/My Projects/hook-client/index.js", "PreCompact"),
      'node "/My Projects/hook-client/index.js" PreCompact'
    );
  });

  it("test_quoted_windows_spaced", () => {
    assert.strictEqual(
      buildHookClientCommand("C:\\Program Files\\hook-client\\index.js", "Stop"),
      'node "C:\\Program Files\\hook-client\\index.js" Stop'
    );
  });

  it("test_built_command_is_recognized_by_ownership_pattern", () => {
    // The command we WRITE must round-trip through the ownership regex (so
    // re-runs recognize and replace it rather than duplicating).
    for (const p of [
      "/a/b/lib/hook-client/index.js",
      "/My Projects/lib/hook-client/index.js",
      "C:\\Program Files\\lib\\hook-client\\index.js",
    ]) {
      for (const event of HOOK_EVENTS) {
        const cmd = buildHookClientCommand(p, event);
        assert.ok(isUnimatrixHook({ command: cmd }), "not recognized: " + cmd);
      }
    }
  });
});

// ── commandSource generalization + back-compat (AC-16) ──────────────

describe("mergeSettings commandSource generalization", () => {
  function tempSettingsPath() {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-cs-test-"));
    // These tests iterate the full HOOK_EVENTS set; opt in to SubagentStop
    // (vnc-027 ADR-004 §2 default-off otherwise drops it).
    writeSubagentOptIn(dir);
    return path.join(dir, ".claude", "settings.json");
  }

  it("test_commandsource_backcompat_wrapper_byte_identical", () => {
    // The legacy string call site must produce the exact LD_LIBRARY_PATH form.
    const fp = tempSettingsPath();
    const binary = "/abs/path/to/unimatrix";
    const result = mergeSettings(fp, binary, {});
    const binDir = path.dirname(binary);
    for (const event of HOOK_EVENTS) {
      const group = result.content.hooks[event].find(
        (g) => g.matcher === EVENT_MATCHERS[event]
      );
      const uni = group.hooks.find((h) => isUnimatrixHook(h));
      assert.strictEqual(
        uni.command,
        "LD_LIBRARY_PATH=" + binDir + " " + binary + " hook " + event
      );
    }
  });

  it("test_normalize_passthrough_object", () => {
    const cs = { events: ["Stop"], commandForEvent: () => "x" };
    assert.strictEqual(normalizeCommandSource(cs), cs);
  });

  it("test_commandsource_remote", () => {
    const fp = tempSettingsPath();
    const clientPath = "/abs/lib/hook-client/index.js";
    const result = mergeSettings(
      fp,
      {
        events: HOOK_EVENTS,
        commandForEvent: (e) => buildHookClientCommand(clientPath, e),
      },
      {}
    );
    for (const event of HOOK_EVENTS) {
      const group = result.content.hooks[event].find(
        (g) => g.matcher === EVENT_MATCHERS[event]
      );
      const uni = group.hooks.find((h) => isUnimatrixHook(h));
      assert.strictEqual(uni.command, "node " + clientPath + " " + event);
    }
  });
});

// ── FR-21 / AC-16 — 9-event set + matchers ──────────────────────────

describe("FR-21 9-event set", () => {
  it("test_event_list_is_nine", () => {
    assert.strictEqual(HOOK_EVENTS.length, 9);
    assert.ok(HOOK_EVENTS.includes("PreCompact"));
    assert.ok(HOOK_EVENTS.includes("PostToolUseFailure"));
  });

  it("test_new_event_matchers", () => {
    assert.strictEqual(EVENT_MATCHERS.PreCompact, "");
    assert.strictEqual(EVENT_MATCHERS.PostToolUseFailure, "*");
  });

  it("test_blast_radius_confined_to_new_events", () => {
    // Diff of local-mode output vs the pre-change 7-event set is EXACTLY the
    // two new event entries — nothing else changes (SR-07 / C-10 gate).
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-blast-"));
    writeSubagentOptIn(dir); // assert the full 9-event blast radius (ADR-004 §2)
    const fp = path.join(dir, ".claude", "settings.json");
    const binary = "/abs/path/to/unimatrix";
    const result = mergeSettings(fp, binary, {});
    const SEVEN = [
      "SessionStart", "Stop", "UserPromptSubmit",
      "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop",
    ];
    const keys = Object.keys(result.content.hooks).sort();
    const expected = SEVEN.concat(["PreCompact", "PostToolUseFailure"]).sort();
    assert.deepStrictEqual(keys, expected);
  });

  it("test_local_9_events_rerun_recognized", () => {
    // Re-run over a PRE-EXISTING 7-event local config: new events added,
    // existing recognized, no duplicates.
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-rerun-"));
    writeSubagentOptIn(dir); // retain the seeded SubagentStop entry (ADR-004 §2)
    const fp = path.join(dir, ".claude", "settings.json");
    const binary = "/abs/path/to/unimatrix";
    const binDir = path.dirname(binary);
    // Seed a legacy 7-event config (pre-FR-21).
    const seed = { hooks: {} };
    for (const e of [
      "SessionStart", "Stop", "UserPromptSubmit",
      "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop",
    ]) {
      seed.hooks[e] = [
        {
          matcher: EVENT_MATCHERS[e],
          hooks: [
            {
              type: "command",
              command:
                "LD_LIBRARY_PATH=" + binDir + " " + binary + " hook " + e,
            },
          ],
        },
      ];
    }
    fs.mkdirSync(path.dirname(fp), { recursive: true });
    fs.writeFileSync(fp, JSON.stringify(seed, null, 2), "utf8");

    const result = mergeSettings(fp, binary, {});
    assert.strictEqual(Object.keys(result.content.hooks).length, 9);
    for (const event of HOOK_EVENTS) {
      assert.strictEqual(
        countUnimatrixHooks(result.content, event),
        1,
        "expected 1 entry for " + event
      );
    }
  });
});

// ── Init Matrix (AC-11) ─────────────────────────────────────────────

describe("initRemote matrix (AC-11)", () => {
  it("test_fresh_config", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    writeSubagentOptIn(dir); // full 9-event contract (ADR-004 §2 default-off)
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const content = readSettings(dir);
    assert.strictEqual(Object.keys(content.hooks).length, 9);
    for (const event of HOOK_EVENTS) {
      const group = content.hooks[event].find(
        (g) => g.matcher === EVENT_MATCHERS[event]
      );
      assert.ok(group, "matcher group for " + event);
      const uni = group.hooks.find((h) => isUnimatrixHook(h));
      assert.strictEqual(
        uni.command,
        buildHookClientCommand(CLIENT_PATH, event)
      );
    }
    // Matchers spot-check.
    assert.strictEqual(content.hooks.PreCompact[0].matcher, "");
    assert.strictEqual(content.hooks.PostToolUseFailure[0].matcher, "*");
  });

  it("test_rerun_idempotent_double_fire", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    writeSubagentOptIn(dir); // full 9-event contract (ADR-004 §2 default-off)
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const content = readSettings(dir);
    for (const event of HOOK_EVENTS) {
      assert.strictEqual(
        countUnimatrixHooks(content, event),
        1,
        "double-fire: expected 1 entry per event for " + event
      );
    }
  });

  it("test_foreign_hooks_preserved_incl_foreign_node", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const fp = path.join(dir, ".claude", "settings.json");
    fs.mkdirSync(path.dirname(fp), { recursive: true });
    const foreign = {
      hooks: {
        PreToolUse: [
          {
            matcher: "*",
            hooks: [
              { type: "command", command: "my-linter check" },
              // A foreign `node` command that must NOT be claimed.
              { type: "command", command: "node /opt/tool/index.js PreToolUse" },
            ],
          },
        ],
      },
    };
    fs.writeFileSync(fp, JSON.stringify(foreign, null, 2), "utf8");

    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const content = readSettings(dir);
    const star = content.hooks.PreToolUse.find((g) => g.matcher === "*");
    const commands = star.hooks.map((h) => h.command);
    assert.ok(commands.includes("my-linter check"));
    assert.ok(commands.includes("node /opt/tool/index.js PreToolUse"));
    // Plus exactly one unimatrix entry.
    assert.strictEqual(countUnimatrixHooks(content, "PreToolUse"), 1);
  });

  it("test_mode_switch_replaces_old_style", async () => {
    // SR-08: config with old-style `unimatrix hook` entries -> replaced.
    stubPing(okPing);
    const dir = makeTempProject();
    const fp = path.join(dir, ".claude", "settings.json");
    fs.mkdirSync(path.dirname(fp), { recursive: true });
    const old = {
      hooks: {
        SessionStart: [
          {
            matcher: "",
            hooks: [
              { type: "command", command: "unimatrix hook SessionStart" },
            ],
          },
        ],
      },
    };
    fs.writeFileSync(fp, JSON.stringify(old, null, 2), "utf8");

    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const content = readSettings(dir);
    assert.strictEqual(countUnimatrixHooks(content, "SessionStart"), 1);
    const group = content.hooks.SessionStart.find((g) => g.matcher === "");
    const uni = group.hooks.find((h) => isUnimatrixHook(h));
    assert.strictEqual(uni.command, buildHookClientCommand(CLIENT_PATH, "SessionStart"));
    assert.ok(!uni.command.includes("unimatrix hook"));
  });

  it("test_mode_switch_old_node_style_entries_replaced", async () => {
    // SR-08: a prior node-client entry (different install path) is recognized
    // and replaced, not duplicated.
    stubPing(okPing);
    const dir = makeTempProject();
    const fp = path.join(dir, ".claude", "settings.json");
    fs.mkdirSync(path.dirname(fp), { recursive: true });
    const old = {
      hooks: {
        Stop: [
          {
            matcher: "",
            hooks: [
              {
                type: "command",
                command: "node /old/install/lib/hook-client/index.js Stop",
              },
            ],
          },
        ],
      },
    };
    fs.writeFileSync(fp, JSON.stringify(old, null, 2), "utf8");

    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const content = readSettings(dir);
    assert.strictEqual(countUnimatrixHooks(content, "Stop"), 1);
    const group = content.hooks.Stop.find((g) => g.matcher === "");
    const uni = group.hooks.find((h) => isUnimatrixHook(h));
    assert.strictEqual(uni.command, buildHookClientCommand(CLIENT_PATH, "Stop"));
  });
});

// ── AC-01 — .mcp.json stdio bridge entry (Scope A, R-10) ─────────────

describe("AC-01 — .mcp.json stdio bridge entry", () => {
  it("test_initBundle_writesStdioUnimatrixEntry", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const mcp = JSON.parse(
      fs.readFileSync(path.join(dir, ".mcp.json"), "utf8")
    );
    assert.deepStrictEqual(mcp.mcpServers.unimatrix, {
      command: "node",
      args: [BRIDGE_PATH, computeProjectHash(dir)],
      env: {},
    });
  });

  it("test_initBundle_bridgeCommandNotRustBinary", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const mcp = JSON.parse(
      fs.readFileSync(path.join(dir, ".mcp.json"), "utf8")
    );
    const entry = mcp.mcpServers.unimatrix;
    // The command invokes node + the resolved JS bridge module, never the Rust
    // platform binary (no LD_LIBRARY_PATH env, no binary path as command).
    assert.strictEqual(entry.command, "node");
    assert.ok(entry.args[0].endsWith("mcp-bridge.js"));
    assert.deepStrictEqual(entry.env, {});
  });

  it("test_initBundle_noSkippedMcpJsonLine", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ bundle: makeBundle(), projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    const joined = actions.join("\n");
    assert.ok(
      !joined.includes("Skipped .mcp.json"),
      "the prior Skipped .mcp.json line must be gone on the bundle path"
    );
  });
});

// ── AC-07 — idempotent + merge-preserving + dry-run (R-10) ──────────

describe("AC-07 — .mcp.json idempotent + merge-preserving + dry-run", () => {
  it("test_initBundle_reInit_doesNotDuplicateUnimatrix", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    // Pre-seed a co-resident MCP server that must survive verbatim.
    fs.writeFileSync(
      mcpPath,
      JSON.stringify(
        { mcpServers: { other: { command: "other-bin", args: [] } } },
        null,
        2
      ),
      "utf8"
    );
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const mcp = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    // Co-resident preserved; unimatrix present exactly once (object key).
    assert.deepStrictEqual(mcp.mcpServers.other, {
      command: "other-bin",
      args: [],
    });
    assert.ok(mcp.mcpServers.unimatrix);
    assert.strictEqual(Object.keys(mcp.mcpServers).length, 2);
  });

  it("test_initBundle_dryRun_noMcpJsonWrite", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ bundle: makeBundle(), projectDir: dir, dryRun: true });
    } finally {
      console.log = origLogLocal;
    }
    assert.ok(!fs.existsSync(path.join(dir, ".mcp.json")), "no write in dry-run");
    assert.ok(
      actions.some((a) => a.includes("[dry-run]") && a.includes(".mcp.json")),
      "intended .mcp.json change reported in dry-run"
    );
  });

  it("test_initBundle_malformedExistingMcpJson_throws", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    fs.writeFileSync(mcpPath, "{ not json", "utf8");
    await assert.rejects(
      () => initRemote({ bundle: makeBundle(), projectDir: dir }),
      (err) => err.message.includes("Malformed .mcp.json")
    );
    // Not silently overwritten.
    assert.strictEqual(fs.readFileSync(mcpPath, "utf8"), "{ not json");
  });
});

// ── AC-08 / AC-08b — out-of-tree store, 0600, per-project (R-12, R-07)

describe("AC-08 — out-of-tree credential store", () => {
  it("test_initBundle_writesStoreOutOfTree0600", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const sp = storePath(dir);
    // The store path is under the (temp) home, NOT inside the project tree.
    assert.ok(sp.startsWith(tempHome), "store must live under home, out-of-tree");
    assert.ok(!sp.startsWith(dir), "store must not be inside the repo tree");
    assert.ok(fs.existsSync(sp), "store file written");
    const stored = readStore(dir);
    assert.strictEqual(stored.token, BUNDLE_TOKEN);
    assert.strictEqual(stored.mcp_url, BUNDLE_MCP_URL);
    assert.strictEqual(stored.observe_url, BUNDLE_OBSERVE_URL);
    assert.strictEqual(stored.fingerprint, BUNDLE_FP);
    if (process.platform !== "win32") {
      const mode = fs.statSync(sp).mode & 0o777;
      assert.strictEqual(mode, 0o600, "expected 0600, got " + mode.toString(8));
    }
  });

  it("test_initBundle_repoTreeFreeOfTokenBearingPath", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    // No file in the repo tree carries the token (the commit-leak this closes).
    const found = [];
    (function walk(d) {
      for (const ent of fs.readdirSync(d, { withFileTypes: true })) {
        const full = path.join(d, ent.name);
        if (ent.isDirectory()) {
          if (ent.name === ".git") continue;
          walk(full);
        } else if (ent.isFile()) {
          if (fs.readFileSync(full, "utf8").includes(BUNDLE_TOKEN)) {
            found.push(full);
          }
        }
      }
    })(dir);
    assert.deepStrictEqual(found, [], "token-bearing path(s) in repo tree: " + found);
  });

  it("test_initBundle_noUnimatrixRemoteCredInSettingsLocal", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const slPath = path.join(dir, ".claude", "settings.local.json");
    if (fs.existsSync(slPath)) {
      const raw = fs.readFileSync(slPath, "utf8");
      assert.ok(!raw.includes(BUNDLE_TOKEN), "token in settings.local.json");
      const parsed = JSON.parse(raw);
      assert.ok(
        !(parsed.unimatrix && parsed.unimatrix.remote),
        "unimatrix.remote credential key present in settings.local.json"
      );
    }
  });

  it("test_initBundle_twoProjects_twoDistinctStores", async () => {
    stubPing(okPing);
    const dirA = makeTempProject();
    const dirB = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dirA });
    await initRemote({ bundle: makeBundle(), projectDir: dirB });
    assert.notStrictEqual(storePath(dirA), storePath(dirB));
    assert.ok(fs.existsSync(storePath(dirA)));
    assert.ok(fs.existsSync(storePath(dirB)));
    // Re-init A updates A; B untouched (directory separation, AC-08b).
    const bMtimeBefore = fs.statSync(storePath(dirB)).mtimeMs;
    await initRemote({ bundle: makeBundle(), projectDir: dirA });
    assert.strictEqual(fs.statSync(storePath(dirB)).mtimeMs, bMtimeBefore);
  });

  it("test_initBundle_storeWriteFailure_initExitsNonZero_noPartialInTree", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    // Force a store-write failure: point home at a path that cannot be a dir.
    const notADir = path.join(
      fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-notdir-")),
      "file"
    );
    fs.writeFileSync(notADir, "x");
    os.homedir = () => notADir; // mkdirSync under a file → throws
    await assert.rejects(() =>
      initRemote({ bundle: makeBundle(), projectDir: dir })
    );
    // No token-bearing partial left in the repo tree.
    const slPath = path.join(dir, ".claude", "settings.local.json");
    if (fs.existsSync(slPath)) {
      assert.ok(!fs.readFileSync(slPath, "utf8").includes(BUNDLE_TOKEN));
    }
  });
});

// ── AC-08 (migration) — stale in-tree creds cleanup (R-12, FR-27) ────

describe("AC-08 migration — stale in-tree creds cleanup", () => {
  it("test_initBundle_deletesStaleSettingsLocalRemoteSubtree", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const slPath = path.join(dir, ".claude", "settings.local.json");
    fs.mkdirSync(path.dirname(slPath), { recursive: true });
    // A stale in-tree credential + an unrelated co-resident key.
    fs.writeFileSync(
      slPath,
      JSON.stringify(
        {
          claudeOwned: "keep",
          unimatrix: {
            other: "keep-too",
            remote: { token: "stale-leaked-token-xyz", url: REMOTE },
          },
        },
        null,
        2
      ),
      "utf8"
    );
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const parsed = JSON.parse(fs.readFileSync(slPath, "utf8"));
    assert.strictEqual(parsed.claudeOwned, "keep");
    assert.strictEqual(parsed.unimatrix.other, "keep-too");
    assert.ok(!parsed.unimatrix.remote, "stale unimatrix.remote not deleted");
    assert.ok(
      !fs.readFileSync(slPath, "utf8").includes("stale-leaked-token-xyz"),
      "stale token still in tree"
    );
  });

  it("test_initBundle_migrationBestEffort_doesNotAbortInit", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const slPath = path.join(dir, ".claude", "settings.local.json");
    fs.mkdirSync(path.dirname(slPath), { recursive: true });
    // Malformed settings.local.json: the migration clean must NOT abort init.
    fs.writeFileSync(slPath, "{ not json", "utf8");
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    // init completed: hooks + .mcp.json written despite the unclean file.
    assert.ok(fs.existsSync(path.join(dir, ".claude", "settings.json")));
    assert.ok(fs.existsSync(path.join(dir, ".mcp.json")));
  });

  it("test_initBundle_gitignoreWarningRemoved", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ bundle: makeBundle(), projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    assert.ok(
      !actions.some((a) => a.includes("gitignored")),
      "gitignore warning output path must be gone (no in-tree creds file)"
    );
  });
});

// ── AC-09 — no token in any init surface (R-09) ─────────────────────

describe("AC-09 — no token in any init surface", () => {
  it("test_initBundle_printSummary_noToken", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const out = [];
    const origLogLocal = console.log;
    console.log = (...a) => out.push(a.join(" "));
    try {
      await initRemote({ bundle: makeBundle(), projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    assert.ok(
      !out.join("\n").includes(BUNDLE_TOKEN),
      "token leaked into init summary output"
    );
  });

  it("test_initBundle_stdoutStderr_noToken", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const out = [];
    const origLogLocal = console.log;
    const origErr = console.error;
    console.log = (...a) => out.push(a.join(" "));
    console.error = (...a) => out.push(a.join(" "));
    try {
      await initRemote({ bundle: makeBundle(), projectDir: dir });
    } finally {
      console.log = origLogLocal;
      console.error = origErr;
    }
    assert.ok(!out.join("\n").includes(BUNDLE_TOKEN), "token in stdout/stderr");
  });

  it("test_mcpJson_noTokenNoMcpUrlNoFp", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    const raw = fs.readFileSync(path.join(dir, ".mcp.json"), "utf8");
    assert.ok(!raw.includes(BUNDLE_TOKEN), "token in .mcp.json");
    assert.ok(!raw.includes(BUNDLE_MCP_URL), "mcp_url in .mcp.json");
    assert.ok(!raw.includes(BUNDLE_FP), "fp in .mcp.json");
    const mcp = JSON.parse(raw);
    // Only command/args:[bridge,hash]/env:{} — AC-09 / FR-17.
    assert.deepStrictEqual(Object.keys(mcp.mcpServers.unimatrix).sort(), [
      "args",
      "command",
      "env",
    ]);
  });
});

// ── AC-10 — legacy path loud, deterministic, no bridge (R-11, R-15) ─

describe("AC-10 — legacy bundle-only boundary", () => {
  it("test_initLegacy_noUnimatrixMcpEntry", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    // No .mcp.json written, or if present, no unimatrix MCP server entry.
    const mcpPath = path.join(dir, ".mcp.json");
    if (fs.existsSync(mcpPath)) {
      const mcp = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
      assert.ok(
        !(mcp.mcpServers && mcp.mcpServers.unimatrix),
        "legacy path must NOT wire a unimatrix MCP entry"
      );
    } else {
      assert.ok(true);
    }
  });

  it("test_initLegacy_exactUnsupportedMessageAndExit", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const out = [];
    const origLogLocal = console.log;
    console.log = (...a) => out.push(a.join(" "));
    let threw = false;
    try {
      await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    } catch (_err) {
      threw = true;
    } finally {
      console.log = origLogLocal;
    }
    // EXACT message text emitted (SR-06) ...
    assert.ok(
      out.some((line) => line.includes(LEGACY_MCP_UNSUPPORTED_MESSAGE)),
      "exact legacy-unsupported message not emitted"
    );
    assert.ok(
      LEGACY_MCP_UNSUPPORTED_MESSAGE.includes("v:2 bundle"),
      "message must name the v:2 bundle path forward"
    );
    // ... and init does NOT hard-fail on the legacy path (exit 0, not a throw).
    assert.ok(!threw, "legacy path must not throw — observe still works");
  });

  it("test_initLegacy_observePathUnchanged", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    // Legacy observe path is preserved: the store entry is written so the hook
    // client can post; hooks are wired into settings.json.
    assert.ok(fs.existsSync(path.join(dir, ".claude", "settings.json")));
    const stored = readStore(dir);
    assert.strictEqual(stored.observe_url, REMOTE.replace(/\/+$/, "") + "/observe");
    assert.strictEqual(stored.token, TOKEN);
  });

  it("test_initLegacy_storeWrittenWithNullFingerprint", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const stored = readStore(dir);
    // WARN-1: universal relocation, but legacy stays unpinned (fingerprint:null,
    // present not omitted) — and bundle path writes the real fingerprint.
    assert.strictEqual(stored.fingerprint, null);
  });

  it("test_initBundle_storeWrittenWithRealFingerprint", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ bundle: makeBundle(), projectDir: dir });
    assert.strictEqual(readStore(dir).fingerprint, BUNDLE_FP);
  });
});

// ── Skips + dry-run (FR-20) ─────────────────────────────────────────

describe("remote skips + dry-run", () => {
  it("test_binary_skipped_with_message", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ bundle: makeBundle(), projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    const joined = actions.join("\n");
    assert.ok(joined.includes("Skipped binary/database steps"));
    // The bundle path DOES write .mcp.json now (the bridge entry).
    assert.ok(fs.existsSync(path.join(dir, ".mcp.json")));
  });

  it("test_dry_run_writes_nothing_and_skips_network", async () => {
    // Ping must NOT be called in dry-run.
    let pingCalled = false;
    stubPing(() => {
      pingCalled = true;
      return okPing();
    });
    const dir = makeTempProject();
    await initRemote({
      bundle: makeBundle(),
      projectDir: dir,
      dryRun: true,
    });
    assert.ok(!pingCalled, "Ping should be skipped in dry-run");
    assert.ok(!fs.existsSync(path.join(dir, ".claude", "settings.json")));
    assert.ok(!fs.existsSync(path.join(dir, ".mcp.json")));
    // No store file written in dry-run.
    assert.ok(!fs.existsSync(storePath(dir)), "no store write in dry-run");
  });
});

// ── Argument validation (loud) ──────────────────────────────────────

describe("remote argument validation", () => {
  it("test_missing_token_throws", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ remote: REMOTE, projectDir: dir }),
      (err) => err.message.includes("both required")
    );
  });

  it("test_missing_remote_throws", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ token: TOKEN, projectDir: dir }),
      (err) => err.message.includes("both required")
    );
  });

  it("test_invalid_url_throws", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ remote: "not a url", token: TOKEN, projectDir: dir }),
      (err) => err.message.includes("invalid --remote URL")
    );
  });

  it("test_non_http_scheme_throws", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () =>
        initRemote({
          remote: "ftp://host/path",
          token: TOKEN,
          projectDir: dir,
        }),
      (err) => err.message.includes("http: or https:")
    );
  });
});

// ── Ping validation (R-18 / FR-19 — the ONE loud path) ──────────────

describe("Ping validation (R-18)", () => {
  it("test_ping_happy", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    // No throw == success; settings written.
    assert.ok(fs.existsSync(path.join(dir, ".claude", "settings.json")));
  });

  it("test_ping_wrong_token_loud_auth_failure", async () => {
    stubPing(() =>
      Promise.resolve({
        ok: false,
        message: "token rejected (HTTP 401) — check --token",
      })
    );
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ remote: REMOTE, token: "wrong", projectDir: dir }),
      (err) =>
        err.message.includes("Remote validation failed") &&
        err.message.includes("token rejected")
    );
  });

  it("test_ping_non_pong_200_fails", async () => {
    stubPing(() =>
      Promise.resolve({
        ok: false,
        message: "unexpected response type: NotPong",
      })
    );
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir }),
      (err) => err.message.includes("unexpected response type")
    );
  });

  it("test_ping_unreachable_fails_files_already_written", async () => {
    // Documented behavior (pseudocode §Error Handling): Steps 3/4 ran before the
    // Ping, so config IS on disk; the error says so and re-run is idempotent. We
    // assert both the loud failure and the written artifacts (store out-of-tree,
    // settings.json + .mcp.json in-tree).
    stubPing(() =>
      Promise.resolve({
        ok: false,
        message: "cannot reach host — check --remote URL",
      })
    );
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ bundle: makeBundle(), projectDir: dir }),
      (err) =>
        err.message.includes("cannot reach") &&
        err.message.includes("Configuration files were written")
    );
    assert.ok(fs.existsSync(path.join(dir, ".claude", "settings.json")));
    assert.ok(fs.existsSync(path.join(dir, ".mcp.json")));
    assert.ok(fs.existsSync(storePath(dir)), "store written before Ping");
  });
});
