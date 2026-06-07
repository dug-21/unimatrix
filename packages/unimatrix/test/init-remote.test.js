"use strict";

// init-remote.test.js — vnc-026 F3 `init --remote` branch + merge-settings
// generalization. Covers: ownership-pattern spaced-path table (R-11),
// init matrix (AC-11), settings.local.json 0600 + gitignore warning + token
// scans (R-16), Ping loud failure (R-18), commandSource back-compat (AC-16),
// and FR-21 9-event regression. Cumulative infra — extends the existing
// node:test suites; no isolated scaffolding.

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
const { initRemote } = initModule;
// init.js holds a reference to this exact module object; overriding the method
// here is observed by initRemote (no module-cache surgery needed).
const transport = require("../lib/hook-client/transport-http.js");

const REMOTE = "https://unimatrix.example.com";
const TOKEN = "unit-test-placeholder-token-2";

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

// Silence init's summary output during tests.
let origLog;
beforeEach(() => {
  origLog = console.log;
  console.log = () => {};
});
afterEach(() => {
  console.log = origLog;
  if (origPing) {
    transport.pingForInit = origPing;
    origPing = undefined;
  }
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

// ── settings.local.json (R-16 / FR-18 / ADR-006) ────────────────────

describe("settings.local.json (R-16 / FR-18)", () => {
  it("test_settings_local_json_written_0600_merge_preserving", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const slPath = path.join(dir, ".claude", "settings.local.json");
    fs.mkdirSync(path.dirname(slPath), { recursive: true });
    // Pre-existing foreign + other unimatrix keys must survive.
    fs.writeFileSync(
      slPath,
      JSON.stringify(
        { claudeOwned: "keep", unimatrix: { other: "keep-too" } },
        null,
        2
      ),
      "utf8"
    );

    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    const content = JSON.parse(fs.readFileSync(slPath, "utf8"));
    assert.strictEqual(content.claudeOwned, "keep");
    assert.strictEqual(content.unimatrix.other, "keep-too");
    assert.deepStrictEqual(content.unimatrix.remote, {
      url: REMOTE,
      token: TOKEN,
    });
    // Mode 0600 (skip the strict check on Windows where chmod is a no-op).
    if (process.platform !== "win32") {
      const mode = fs.statSync(slPath).mode & 0o777;
      assert.strictEqual(mode, 0o600, "expected mode 0600, got " + mode.toString(8));
    }
  });

  it("test_gitignore_warning_fires_when_uncovered", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    // No .gitignore -> warning expected.
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    assert.ok(
      actions.some((a) => a.includes("not gitignored")),
      "expected gitignore warning"
    );
  });

  it("test_no_gitignore_warning_when_covered", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    fs.writeFileSync(
      path.join(dir, ".gitignore"),
      ".claude/settings.local.json\n",
      "utf8"
    );
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    assert.ok(
      !actions.some((a) => a.includes("not gitignored")),
      "should not warn when covered"
    );
  });

  it("test_no_token_on_argv_or_settings_json", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    // The token must NOT appear anywhere in settings.json (only the hook
    // command lines + matcher groups live there; R-16).
    const raw = fs.readFileSync(
      path.join(dir, ".claude", "settings.json"),
      "utf8"
    );
    assert.ok(!raw.includes(TOKEN), "token leaked into settings.json");
    assert.ok(!raw.includes(REMOTE), "URL leaked into settings.json");
    // No hook command carries the token.
    const content = JSON.parse(raw);
    for (const event of HOOK_EVENTS) {
      for (const group of content.hooks[event]) {
        for (const hook of group.hooks) {
          assert.ok(
            !(hook.command || "").includes(TOKEN),
            "token in hook command for " + event
          );
        }
      }
    }
  });

  it("test_malformed_settings_local_throws", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const slPath = path.join(dir, ".claude", "settings.local.json");
    fs.mkdirSync(path.dirname(slPath), { recursive: true });
    fs.writeFileSync(slPath, "{ not json", "utf8");
    await assert.rejects(
      () => initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir }),
      (err) => err.message.includes("Malformed")
    );
    // User content NOT clobbered.
    assert.strictEqual(fs.readFileSync(slPath, "utf8"), "{ not json");
  });
});

// ── Skips + dry-run (FR-20) ─────────────────────────────────────────

describe("remote skips + dry-run", () => {
  it("test_mcp_and_binary_skipped_with_message", async () => {
    stubPing(okPing);
    const dir = makeTempProject();
    const actions = [];
    const origLogLocal = console.log;
    console.log = (...a) => actions.push(a.join(" "));
    try {
      await initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir });
    } finally {
      console.log = origLogLocal;
    }
    const joined = actions.join("\n");
    assert.ok(joined.includes("Skipped .mcp.json"));
    assert.ok(joined.includes("Skipped binary/database steps"));
    // No .mcp.json written.
    assert.ok(!fs.existsSync(path.join(dir, ".mcp.json")));
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
      remote: REMOTE,
      token: TOKEN,
      projectDir: dir,
      dryRun: true,
    });
    assert.ok(!pingCalled, "Ping should be skipped in dry-run");
    assert.ok(!fs.existsSync(path.join(dir, ".claude", "settings.json")));
    assert.ok(
      !fs.existsSync(path.join(dir, ".claude", "settings.local.json"))
    );
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
    // Documented behavior (pseudocode §Error Handling): Step 3/4 ran before the
    // Ping, so config files ARE on disk; the error says so and re-run is
    // idempotent. We assert both the loud failure and the written files.
    stubPing(() =>
      Promise.resolve({
        ok: false,
        message: "cannot reach host — check --remote URL",
      })
    );
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ remote: REMOTE, token: TOKEN, projectDir: dir }),
      (err) =>
        err.message.includes("cannot reach") &&
        err.message.includes("Configuration files were written")
    );
    assert.ok(fs.existsSync(path.join(dir, ".claude", "settings.json")));
    assert.ok(
      fs.existsSync(path.join(dir, ".claude", "settings.local.json"))
    );
  });
});
