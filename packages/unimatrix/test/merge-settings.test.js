"use strict";

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const {
  mergeSettings,
  isUnimatrixHook,
  buildHookClientCommand,
  subagentStopEnabled,
  HOOK_EVENTS,
  EVENT_MATCHERS,
  PRETOOLUSE_CYCLE_MATCHER,
} = require("../lib/merge-settings");

/** Create a temp directory and return a settings.json path inside it. */
function tempSettingsPath() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-test-"));
  return path.join(dir, ".claude", "settings.json");
}

/** Write content to a settings file, creating parent dirs. */
function writeSettings(filePath, content) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, typeof content === "string" ? content : JSON.stringify(content, null, 2), "utf8");
}

/**
 * Write the SubagentStop opt-in key into the settings.local.json sibling of a
 * settings.json path (ADR-004 §2). `value` is written verbatim so non-boolean
 * type-confusion cases can be exercised.
 */
function writeOptIn(settingsPath, value) {
  const localPath = path.join(path.dirname(settingsPath), "settings.local.json");
  fs.mkdirSync(path.dirname(localPath), { recursive: true });
  fs.writeFileSync(
    localPath,
    JSON.stringify({ unimatrix: { hooks: { subagent_stop: value } } }, null, 2),
    "utf8"
  );
}

const BINARY = "/abs/path/to/unimatrix";

// ADR-004 §2: SubagentStop is opt-in (default off). The default registered set
// is HOOK_EVENTS minus SubagentStop unless settings.local.json opts in.
const DEFAULT_EVENTS = HOOK_EVENTS.filter((e) => e !== "SubagentStop");

// Byte-exact local-mode command the back-compat wrapper produces (AC-16).
// Mirrors normalizeCommandSource(string): LD_LIBRARY_PATH=<binDir> <binary> hook <event>.
const BIN_DIR = path.dirname(BINARY);
function expectedLocalCommand(event) {
  return "LD_LIBRARY_PATH=" + BIN_DIR + " " + BINARY + " hook " + event;
}

// ── R-01 Scenarios ──────────────────────────────────────────────────

describe("mergeSettings", function () {
  describe("R-01: merge into empty file", function () {
    it("test_merge_into_empty_file", function () {
      const fp = tempSettingsPath();
      // File does not exist
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(result.content.hooks);
      // ADR-004 §2: SubagentStop opt-in default off — registered set is 8 events.
      for (const event of DEFAULT_EVENTS) {
        assert.ok(result.content.hooks[event], "Missing event: " + event);
        const groups = result.content.hooks[event];
        assert.strictEqual(groups.length, 1);
        assert.strictEqual(groups[0].hooks.length, 1);
        assert.strictEqual(groups[0].hooks[0].type, "command");
        assert.ok(groups[0].hooks[0].command.includes(BINARY + " hook " + event));
      }
      assert.ok(!result.content.hooks.SubagentStop, "SubagentStop should be opt-in");
      // File was written
      assert.ok(fs.existsSync(fp));
    });
  });

  describe("R-01: preserves permissions block", function () {
    it("test_merge_preserves_permissions_block", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { permissions: { allow: ["Read"], deny: [] } });
      const result = mergeSettings(fp, BINARY, {});
      assert.deepStrictEqual(result.content.permissions, { allow: ["Read"], deny: [] });
      assert.ok(result.content.hooks);
      // ADR-004 §2: SubagentStop opt-in default off — 8 of the 9 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });
  });

  describe("R-01: preserves non-unimatrix hooks", function () {
    it("test_merge_preserves_non_unimatrix_hooks", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, {
        hooks: {
          PreToolUse: [
            {
              matcher: "*",
              hooks: [{ type: "command", command: "my-tool pre-check" }],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const preToolUse = result.content.hooks.PreToolUse;
      // ADR-004 §1: the unimatrix PreToolUse hook lands in the narrowed cycle
      // matcher group, NOT the foreign "*" group. The "*" group keeps only the
      // foreign hook, untouched.
      const starGroup = preToolUse.find((g) => g.matcher === "*");
      assert.ok(starGroup);
      assert.strictEqual(starGroup.hooks.length, 1);
      assert.strictEqual(starGroup.hooks[0].command, "my-tool pre-check");
      const cycleGroup = preToolUse.find((g) => g.matcher === PRETOOLUSE_CYCLE_MATCHER);
      assert.ok(cycleGroup, "unimatrix PreToolUse hook should be in the cycle matcher group");
      assert.strictEqual(cycleGroup.hooks.length, 1);
      assert.ok(cycleGroup.hooks[0].command.includes(BINARY));
    });
  });

  describe("R-01: updates pre-rename hooks", function () {
    it("test_merge_updates_pre_rename_hooks", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, {
        hooks: {
          SessionStart: [
            {
              matcher: "",
              hooks: [{ type: "command", command: "unimatrix-server hook SessionStart" }],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const group = result.content.hooks.SessionStart[0];
      assert.strictEqual(group.hooks.length, 1);
      assert.strictEqual(group.hooks[0].command, expectedLocalCommand("SessionStart"));
    });
  });

  describe("R-01: updates absolute path hooks", function () {
    it("test_merge_updates_absolute_path_hooks", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, {
        hooks: {
          SessionStart: [
            {
              matcher: "",
              hooks: [{ type: "command", command: "/old/path/unimatrix hook SessionStart" }],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const group = result.content.hooks.SessionStart[0];
      assert.strictEqual(group.hooks.length, 1);
      assert.strictEqual(group.hooks[0].command, expectedLocalCommand("SessionStart"));
    });
  });

  describe("R-01: preserves extra top-level keys", function () {
    it("test_merge_preserves_extra_top_level_keys", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { customKey: "value", hooks: {} });
      const result = mergeSettings(fp, BINARY, {});
      assert.strictEqual(result.content.customKey, "value");
    });
  });

  describe("R-01/R-04: idempotent round trip", function () {
    it("test_merge_idempotent_round_trip", function () {
      const fp = tempSettingsPath();
      const first = mergeSettings(fp, BINARY, {});
      const second = mergeSettings(fp, BINARY, {});
      assert.deepStrictEqual(first.content, second.content);
    });
  });

  // ── Hook Event Coverage ───────────────────────────────────────────

  describe("hook event coverage", function () {
    it("test_default_8_events_present", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      // ADR-004 §2: SubagentStop opt-in default off; the other 8 register.
      const expected = [
        "SessionStart", "Stop", "UserPromptSubmit",
        "PreToolUse", "PostToolUse", "PostToolUseFailure", "PreCompact",
        "SubagentStart",
      ];
      for (const e of expected) {
        assert.ok(result.content.hooks[e], "Missing event: " + e);
      }
      assert.ok(!result.content.hooks.SubagentStop, "SubagentStop should be opt-in");
    });

    it("test_each_event_has_exactly_one_unimatrix_entry", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, BINARY, {});
      mergeSettings(fp, BINARY, {});
      const result = mergeSettings(fp, BINARY, {});
      for (const event of DEFAULT_EVENTS) {
        let count = 0;
        for (const group of result.content.hooks[event]) {
          for (const hook of group.hooks) {
            if (isUnimatrixHook(hook)) {
              count++;
            }
          }
        }
        assert.strictEqual(count, 1, "Expected exactly 1 unimatrix hook for " + event + ", got " + count);
      }
    });
  });

  // ── Matcher values ────────────────────────────────────────────────

  describe("matcher values", function () {
    it("test_session_events_use_empty_matcher", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      // FR-21: PreCompact joins the session-level ("") matcher group.
      for (const event of ["SessionStart", "Stop", "UserPromptSubmit", "PreCompact"]) {
        assert.strictEqual(result.content.hooks[event][0].matcher, "");
      }
    });

    it("test_tool_events_use_star_matcher", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      // ADR-004 §1: PreToolUse narrowed to the cycle matcher (asserted
      // separately); SubagentStop is opt-in. The remaining tool-level events keep
      // the "*" matcher unchanged.
      for (const event of ["PostToolUse", "PostToolUseFailure", "SubagentStart"]) {
        assert.strictEqual(result.content.hooks[event][0].matcher, "*");
      }
    });
  });

  // ── Identification Patterns (ADR-004) ─────────────────────────────

  describe("identification patterns", function () {
    it("test_identifies_bare_unimatrix_hook", function () {
      assert.ok(isUnimatrixHook({ command: "unimatrix hook SessionStart" }));
    });

    it("test_identifies_bare_unimatrix_server_hook", function () {
      assert.ok(isUnimatrixHook({ command: "unimatrix-server hook SessionStart" }));
    });

    it("test_identifies_absolute_path_unimatrix", function () {
      assert.ok(isUnimatrixHook({ command: "/path/to/unimatrix hook SessionStart" }));
    });

    it("test_identifies_absolute_path_unimatrix_server", function () {
      assert.ok(isUnimatrixHook({ command: "/old/path/unimatrix-server hook SessionStart" }));
    });

    it("test_does_not_identify_custom_hook", function () {
      assert.ok(!isUnimatrixHook({ command: "my-tool hook SessionStart" }));
    });

    it("test_does_not_identify_null_entry", function () {
      assert.ok(!isUnimatrixHook(null));
      assert.ok(!isUnimatrixHook({}));
      assert.ok(!isUnimatrixHook({ command: 42 }));
    });
  });

  // ── R-14 Error Handling ───────────────────────────────────────────

  describe("error handling", function () {
    it("test_malformed_json_errors_with_diagnostic", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, "{invalid json");
      assert.throws(
        () => mergeSettings(fp, BINARY, {}),
        (err) => err.message.includes("Malformed") && err.message.includes(fp)
      );
      // File NOT modified
      assert.strictEqual(fs.readFileSync(fp, "utf8"), "{invalid json");
    });

    it("test_empty_file_treated_as_empty_object", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, "");
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(result.content.hooks);
      // ADR-004 §2: SubagentStop opt-in default off — 8 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });

    it("test_hooks_key_not_object_errors", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { hooks: "string" });
      assert.throws(
        () => mergeSettings(fp, BINARY, {}),
        (err) => err.message.includes("not an object")
      );
    });

    it("test_hooks_key_array_errors", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { hooks: [1, 2, 3] });
      assert.throws(
        () => mergeSettings(fp, BINARY, {}),
        (err) => err.message.includes("not an object")
      );
    });
  });

  // ── Output Format ─────────────────────────────────────────────────

  describe("output format", function () {
    it("test_output_uses_2_space_indentation", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, BINARY, {});
      const written = fs.readFileSync(fp, "utf8");
      // Second line should start with 2 spaces (not tabs, not 4 spaces)
      const lines = written.split("\n");
      assert.ok(lines[1].startsWith("  "), "Expected 2-space indentation");
      assert.ok(!lines[1].startsWith("    "), "Should not be 4-space indentation on first nesting level");
    });

    it("test_actions_array_describes_changes", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(Array.isArray(result.actions));
      assert.ok(result.actions.length > 0);
      // Should mention creating or adding hooks
      assert.ok(result.actions.some((a) => a.includes("Added") || a.includes("Created")));
    });
  });

  // ── Dry Run ───────────────────────────────────────────────────────

  describe("dry run", function () {
    it("test_dry_run_does_not_write_file", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, { dryRun: true });
      assert.ok(!fs.existsSync(fp));
      assert.ok(result.content.hooks);
      assert.ok(result.actions.every((a) => a.startsWith("[dry-run]")));
    });

    it("test_dry_run_returns_actions_and_content", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, { dryRun: true });
      assert.ok(result.actions.length > 0);
      // ADR-004 §2: SubagentStop opt-in default off — 8 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });
  });

  // ── R-04 Dedup Across Multiple Runs ───────────────────────────────

  describe("dedup across multiple runs", function () {
    it("test_three_consecutive_merges_no_growth", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, BINARY, {});
      mergeSettings(fp, BINARY, {});
      const result = mergeSettings(fp, BINARY, {});
      for (const event of DEFAULT_EVENTS) {
        let uniCount = 0;
        for (const group of result.content.hooks[event]) {
          for (const hook of group.hooks) {
            if (isUnimatrixHook(hook)) {
              uniCount++;
            }
          }
        }
        assert.strictEqual(uniCount, 1, "Expected 1 unimatrix hook for " + event + " after 3 merges, got " + uniCount);
      }
    });

    it("test_dedup_removes_extra_unimatrix_hooks", function () {
      const fp = tempSettingsPath();
      // Manually create a file with duplicate unimatrix hooks
      writeSettings(fp, {
        hooks: {
          SessionStart: [
            {
              matcher: "",
              hooks: [
                { type: "command", command: "unimatrix-server hook SessionStart" },
                { type: "command", command: "/other/path/unimatrix hook SessionStart" },
              ],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const group = result.content.hooks.SessionStart[0];
      const uniHooks = group.hooks.filter((h) => isUnimatrixHook(h));
      assert.strictEqual(uniHooks.length, 1);
      assert.strictEqual(uniHooks[0].command, expectedLocalCommand("SessionStart"));
      assert.ok(result.actions.some((a) => a.includes("Removed duplicate")));
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────

  describe("edge cases", function () {
    it("test_preserves_non_unimatrix_hooks_with_different_matcher", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, {
        hooks: {
          PreToolUse: [
            {
              matcher: "Write",
              hooks: [{ type: "command", command: "my-linter check" }],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const writeGroup = result.content.hooks.PreToolUse.find((g) => g.matcher === "Write");
      assert.ok(writeGroup, "Write matcher group should be preserved");
      assert.strictEqual(writeGroup.hooks[0].command, "my-linter check");
      // ADR-004 §1: the unimatrix hook lands in the narrowed cycle matcher group.
      const cycleGroup = result.content.hooks.PreToolUse.find(
        (g) => g.matcher === PRETOOLUSE_CYCLE_MATCHER
      );
      assert.ok(cycleGroup);
      assert.ok(cycleGroup.hooks.some((h) => h.command.includes(BINARY)));
    });

    it("test_hook_entry_without_type_command_is_preserved", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, {
        hooks: {
          SessionStart: [
            {
              matcher: "",
              hooks: [{ type: "url", url: "https://example.com/webhook" }],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const group = result.content.hooks.SessionStart[0];
      // URL hook preserved, unimatrix appended
      assert.strictEqual(group.hooks.length, 2);
      assert.strictEqual(group.hooks[0].type, "url");
      assert.ok(group.hooks[1].command.includes(BINARY));
    });

    it("test_handles_tee_pipeline_as_unimatrix_hook", function () {
      // The old tee pipeline for UserPromptSubmit should be identified and replaced
      const fp = tempSettingsPath();
      writeSettings(fp, {
        hooks: {
          UserPromptSubmit: [
            {
              matcher: "",
              hooks: [
                {
                  type: "command",
                  command: "unimatrix-server hook UserPromptSubmit | tee -a ~/.unimatrix/injections/hooks.log",
                },
              ],
            },
          ],
        },
      });
      const result = mergeSettings(fp, BINARY, {});
      const group = result.content.hooks.UserPromptSubmit[0];
      assert.strictEqual(group.hooks.length, 1);
      // No tee pipeline — back-compat local command format (AC-16)
      assert.strictEqual(group.hooks[0].command, expectedLocalCommand("UserPromptSubmit"));
    });

    it("test_file_not_exist_creates_directory_and_file", function () {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-test-"));
      const fp = path.join(dir, "deep", "nested", ".claude", "settings.json");
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(fs.existsSync(fp));
      assert.ok(result.actions.some((a) => a.includes("Created")));
    });

    it("test_whitespace_only_file_treated_as_empty", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, "   \n  \t  ");
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(result.content.hooks);
      // ADR-004 §2: SubagentStop opt-in default off — 8 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });
  });

  // ── Command Format ────────────────────────────────────────────────

  describe("command format", function () {
    it("test_all_hooks_use_backcompat_command_format", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      for (const event of DEFAULT_EVENTS) {
        for (const group of result.content.hooks[event]) {
          for (const hook of group.hooks) {
            if (isUnimatrixHook(hook)) {
              // AC-16 byte-identical local command (LD_LIBRARY_PATH-prefixed).
              assert.strictEqual(
                hook.command,
                expectedLocalCommand(event),
                "Hook for " + event + " should use back-compat command format"
              );
              assert.ok(!hook.command.includes("|"), "No pipe in hook command for " + event);
            }
          }
        }
      }
    });
  });
});

// ── ADR-004 §1: PreToolUse matcher narrowing (R-11 s1, AC-08) ────────

describe("ADR-004 PreToolUse matcher narrowing", function () {
  // Find the matcher group that owns the unimatrix PreToolUse hook.
  function unimatrixPreToolUseGroup(content) {
    return (content.hooks.PreToolUse || []).find((g) =>
      (g.hooks || []).some((h) => isUnimatrixHook(h))
    );
  }

  it("test_pretooluse_matcher_exactly_cycle_tools", function () {
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, BINARY, {});
    const group = unimatrixPreToolUseGroup(result.content);
    assert.ok(group, "expected a unimatrix-owned PreToolUse matcher group");
    // EXACTLY the cycle tools — no longer "*". The hook no longer spawns for
    // ordinary tool calls.
    assert.strictEqual(group.matcher, "context_cycle|mcp__unimatrix__context_cycle");
    assert.strictEqual(group.matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(EVENT_MATCHERS.PreToolUse, "context_cycle|mcp__unimatrix__context_cycle");
  });

  it("test_pretooluse_stays_in_hook_events", function () {
    // The matcher narrows; the event is NOT dropped from the table.
    assert.ok(HOOK_EVENTS.includes("PreToolUse"));
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, BINARY, {});
    assert.ok(result.content.hooks.PreToolUse, "PreToolUse still registered");
  });

  it("test_all_other_matchers_unchanged", function () {
    // Output snapshot: every other registered event's unimatrix entry keeps its
    // F3 matcher byte-for-byte.
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, BINARY, {});
    const expectedMatchers = {
      SessionStart: "",
      Stop: "",
      UserPromptSubmit: "",
      PreCompact: "",
      PostToolUse: "*",
      PostToolUseFailure: "*",
      SubagentStart: "*",
    };
    for (const [event, matcher] of Object.entries(expectedMatchers)) {
      const group = result.content.hooks[event].find((g) =>
        (g.hooks || []).some((h) => isUnimatrixHook(h))
      );
      assert.ok(group, "missing unimatrix group for " + event);
      assert.strictEqual(group.matcher, matcher, "matcher changed for " + event);
    }
  });
});

// ── ADR-004 §2: SubagentStop opt-in matrix (AC-08, R-12) ─────────────

describe("ADR-004 SubagentStop opt-in matrix", function () {
  it("test_subagentstop_absent_by_default", function () {
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, BINARY, {});
    assert.ok(!result.content.hooks.SubagentStop, "SubagentStop must be opt-in");
  });

  it("test_subagentstop_missing_local_file_absent", function () {
    // No settings.local.json at all → absent, no throw.
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, BINARY, {});
    assert.ok(!result.content.hooks.SubagentStop);
  });

  it("test_subagentstop_registered_when_key_true", function () {
    const fp = tempSettingsPath();
    writeOptIn(fp, true);
    const result = mergeSettings(fp, BINARY, {});
    assert.ok(result.content.hooks.SubagentStop, "SubagentStop should register when opted in");
    assert.strictEqual(result.content.hooks.SubagentStop[0].matcher, "*");
    const uni = result.content.hooks.SubagentStop[0].hooks.find((h) => isUnimatrixHook(h));
    assert.ok(uni, "expected a unimatrix SubagentStop entry");
    // Opted in → full 9-event set.
    assert.strictEqual(Object.keys(result.content.hooks).length, 9);
  });

  it("test_subagentstop_key_false_absent", function () {
    const fp = tempSettingsPath();
    writeOptIn(fp, false);
    const result = mergeSettings(fp, BINARY, {});
    assert.ok(!result.content.hooks.SubagentStop);
  });

  it("test_subagentstop_non_boolean_treated_as_unset", function () {
    // Type-confusion guard: only the literal boolean true enables. Each of these
    // non-boolean truthy/falsy values is treated as unset (security surface).
    for (const value of ["true", 1, 0, null, {}, [], "yes"]) {
      const fp = tempSettingsPath();
      writeOptIn(fp, value);
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(
        !result.content.hooks.SubagentStop,
        "SubagentStop must be absent for non-boolean value: " + JSON.stringify(value)
      );
    }
  });

  it("test_subagentstop_enabled_helper_matrix", function () {
    const fp = tempSettingsPath();
    const localPath = path.join(path.dirname(fp), "settings.local.json");
    // Unreadable / missing → false.
    assert.strictEqual(subagentStopEnabled(localPath), false);
    writeOptIn(fp, true);
    assert.strictEqual(subagentStopEnabled(localPath), true);
    writeOptIn(fp, "true");
    assert.strictEqual(subagentStopEnabled(localPath), false);
    // Malformed JSON → fail-open false (no throw).
    fs.writeFileSync(localPath, "{ not json", "utf8");
    assert.strictEqual(subagentStopEnabled(localPath), false);
  });
});

// ── ADR-004 §2: SubagentStop opt-out pruning (AC-08) ─────────────────

describe("ADR-004 SubagentStop opt-out pruning", function () {
  // Seed a settings.json that already has a unimatrix-owned SubagentStop entry.
  function seedWithSubagentStop(fp, extraForeign) {
    const hooks = {
      SubagentStop: [
        {
          matcher: "*",
          hooks: [{ type: "command", command: "unimatrix hook SubagentStop" }],
        },
      ],
    };
    if (extraForeign) {
      hooks.SubagentStop[0].hooks.unshift({
        type: "command",
        command: "my-tool subagent-stop",
      });
    }
    writeSettings(fp, { hooks });
  }

  it("test_subagentstop_pruned_on_opt_out", function () {
    const fp = tempSettingsPath();
    seedWithSubagentStop(fp, false);
    // No opt-in key → re-init must strip the previously-registered entry, and the
    // now-empty event key is removed.
    const result = mergeSettings(fp, BINARY, {});
    assert.ok(!result.content.hooks.SubagentStop, "stale SubagentStop entry must be pruned");
    assert.ok(result.actions.some((a) => a.includes("opt-out")));
  });

  it("test_subagentstop_optout_preserves_foreign_hook", function () {
    const fp = tempSettingsPath();
    seedWithSubagentStop(fp, true);
    const result = mergeSettings(fp, BINARY, {});
    // The foreign hook survives; only the unimatrix entry is stripped.
    assert.ok(result.content.hooks.SubagentStop, "foreign SubagentStop group must remain");
    const cmds = result.content.hooks.SubagentStop.flatMap((g) =>
      g.hooks.map((h) => h.command)
    );
    assert.ok(cmds.includes("my-tool subagent-stop"));
    assert.ok(!cmds.some((c) => isUnimatrixHook({ command: c })));
  });

  it("test_subagentstop_optout_idempotent", function () {
    const fp = tempSettingsPath();
    seedWithSubagentStop(fp, false);
    const first = mergeSettings(fp, BINARY, {});
    const second = mergeSettings(fp, BINARY, {});
    assert.deepStrictEqual(first.content, second.content);
  });

  it("test_subagentstop_optin_then_optout_round_trip", function () {
    const fp = tempSettingsPath();
    // Opt in → registered.
    writeOptIn(fp, true);
    let result = mergeSettings(fp, BINARY, {});
    assert.ok(result.content.hooks.SubagentStop);
    // Opt out (flip key to false) → pruned.
    writeOptIn(fp, false);
    result = mergeSettings(fp, BINARY, {});
    assert.ok(!result.content.hooks.SubagentStop);
  });
});

// ── ADR-004 §5: shared install surface (node-client + legacy Rust) ───

describe("ADR-004 reduced set applies to both command shapes", function () {
  it("test_reduced_set_applies_to_node_client_reinit", function () {
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
    // Same reduction: PreToolUse narrowed, SubagentStop opt-in (default off).
    assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    assert.ok(!result.content.hooks.SubagentStop);
    const group = result.content.hooks.PreToolUse.find((g) =>
      (g.hooks || []).some((h) => isUnimatrixHook(h))
    );
    assert.strictEqual(group.matcher, PRETOOLUSE_CYCLE_MATCHER);
  });

  it("test_reduced_set_applies_to_rust_hook_reinit", function () {
    // Legacy local-binary (string commandSource) gets the same reduced set.
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, BINARY, {});
    assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    assert.ok(!result.content.hooks.SubagentStop);
    const group = result.content.hooks.PreToolUse.find((g) =>
      (g.hooks || []).some((h) => isUnimatrixHook(h))
    );
    assert.strictEqual(group.matcher, PRETOOLUSE_CYCLE_MATCHER);
  });
});
