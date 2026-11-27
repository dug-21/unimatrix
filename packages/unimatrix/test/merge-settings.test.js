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
  return path.join(dir, ".claude", "settings.local.json");
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
  let cur = {};
  try { cur = JSON.parse(fs.readFileSync(settingsPath, "utf8")); } catch (_) { /* absent */ }
  cur.unimatrix = { hooks: { subagent_stop: value } };
  fs.mkdirSync(path.dirname(settingsPath), { recursive: true });
  fs.writeFileSync(settingsPath, JSON.stringify(cur, null, 2), "utf8");
}

const BINARY = "/abs/path/to/unimatrix";

// ADR-004 §2: SubagentStop is opt-in (default off). The default registered set
// is HOOK_EVENTS minus SubagentStop unless settings.local.json opts in.
// commandSource object (the legacy string arm is deleted, D-008).
const SRC = { events: HOOK_EVENTS, commandForEvent: (e) => expectedLocalCommand(e) };

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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
      const group = result.content.hooks.SessionStart[0];
      assert.strictEqual(group.hooks.length, 1);
      assert.strictEqual(group.hooks[0].command, expectedLocalCommand("SessionStart"));
    });
  });

  describe("R-01: preserves extra top-level keys", function () {
    it("test_merge_preserves_extra_top_level_keys", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { customKey: "value", hooks: {} });
      const result = mergeSettings(fp, SRC, {});
      assert.strictEqual(result.content.customKey, "value");
    });
  });

  describe("R-01/R-04: idempotent round trip", function () {
    it("test_merge_idempotent_round_trip", function () {
      const fp = tempSettingsPath();
      const first = mergeSettings(fp, SRC, {});
      const second = mergeSettings(fp, SRC, {});
      assert.deepStrictEqual(first.content, second.content);
    });
  });

  // ── Hook Event Coverage ───────────────────────────────────────────

  describe("hook event coverage", function () {
    it("test_default_8_events_present", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, SRC, {});
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
      mergeSettings(fp, SRC, {});
      mergeSettings(fp, SRC, {});
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
      // FR-21: PreCompact joins the session-level ("") matcher group.
      for (const event of ["SessionStart", "Stop", "UserPromptSubmit", "PreCompact"]) {
        assert.strictEqual(result.content.hooks[event][0].matcher, "");
      }
    });

    it("test_tool_events_use_star_matcher", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, SRC, {});
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
        () => mergeSettings(fp, SRC, {}),
        (err) => err.message.includes("Malformed") && err.message.includes(fp)
      );
      // File NOT modified
      assert.strictEqual(fs.readFileSync(fp, "utf8"), "{invalid json");
    });

    it("test_empty_file_treated_as_empty_object", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, "");
      const result = mergeSettings(fp, SRC, {});
      assert.ok(result.content.hooks);
      // ADR-004 §2: SubagentStop opt-in default off — 8 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });

    it("test_hooks_key_not_object_errors", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { hooks: "string" });
      assert.throws(
        () => mergeSettings(fp, SRC, {}),
        (err) => err.message.includes("not an object")
      );
    });

    it("test_hooks_key_array_errors", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, { hooks: [1, 2, 3] });
      assert.throws(
        () => mergeSettings(fp, SRC, {}),
        (err) => err.message.includes("not an object")
      );
    });
  });

  // ── Output Format ─────────────────────────────────────────────────

  describe("output format", function () {
    it("test_output_uses_2_space_indentation", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, SRC, {});
      const written = fs.readFileSync(fp, "utf8");
      // Second line should start with 2 spaces (not tabs, not 4 spaces)
      const lines = written.split("\n");
      assert.ok(lines[1].startsWith("  "), "Expected 2-space indentation");
      assert.ok(!lines[1].startsWith("    "), "Should not be 4-space indentation on first nesting level");
    });

    it("test_actions_array_describes_changes", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, { dryRun: true });
      assert.ok(!fs.existsSync(fp));
      assert.ok(result.content.hooks);
      assert.ok(result.actions.every((a) => a.startsWith("[dry-run]")));
    });

    it("test_dry_run_returns_actions_and_content", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, SRC, { dryRun: true });
      assert.ok(result.actions.length > 0);
      // ADR-004 §2: SubagentStop opt-in default off — 8 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });
  });

  // ── R-04 Dedup Across Multiple Runs ───────────────────────────────

  describe("dedup across multiple runs", function () {
    it("test_three_consecutive_merges_no_growth", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, SRC, {});
      mergeSettings(fp, SRC, {});
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
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
      const result = mergeSettings(fp, SRC, {});
      // ADR-003: the ownership predicate is end-anchored, so a tee pipeline is a
      // foreign (user-authored) command: preserved, and the managed entry is added.
      const group = result.content.hooks.UserPromptSubmit[0];
      assert.strictEqual(group.hooks.length, 2);
      assert.ok(group.hooks[0].command.includes("| tee -a"));
      assert.strictEqual(group.hooks[1].command, expectedLocalCommand("UserPromptSubmit"));
    });

    it("test_file_not_exist_creates_directory_and_file", function () {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-test-"));
      const fp = path.join(dir, "deep", "nested", ".claude", "settings.local.json");
      const result = mergeSettings(fp, SRC, {});
      assert.ok(fs.existsSync(fp));
      assert.ok(result.actions.some((a) => a.includes("Created")));
    });

    it("test_whitespace_only_file_treated_as_empty", function () {
      const fp = tempSettingsPath();
      writeSettings(fp, "   \n  \t  ");
      const result = mergeSettings(fp, SRC, {});
      assert.ok(result.content.hooks);
      // ADR-004 §2: SubagentStop opt-in default off — 8 events register.
      assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    });
  });

  // ── Command Format ────────────────────────────────────────────────

  describe("command format", function () {
    it("test_all_hooks_use_backcompat_command_format", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, SRC, {});
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
    const result = mergeSettings(fp, SRC, {});
    const group = unimatrixPreToolUseGroup(result.content);
    assert.ok(group, "expected a unimatrix-owned PreToolUse matcher group");
    // EXACTLY the cycle tools — no longer "*". The hook no longer spawns for
    // ordinary tool calls. #832: anchored so the two alternatives are mutually
    // exclusive (the namespaced name CONTAINS the bare name → double-fire before).
    assert.strictEqual(group.matcher, "^context_cycle$|^mcp__unimatrix__context_cycle$");
    assert.strictEqual(group.matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(
      EVENT_MATCHERS.PreToolUse,
      "^context_cycle$|^mcp__unimatrix__context_cycle$"
    );
  });

  it("test_pretooluse_matcher_single_fires_each_cycle_tool_name (#832, R-5)", function () {
    // Claude Code matches a tool name against the matcher regex. The bug was the
    // unanchored alternation matching the NAMESPACED name on BOTH branches; the
    // anchored form must single-fire EACH name — and must NOT drop the bare
    // `context_cycle` that UDS/STDIO uses (R-5 regression guard).
    const re = new RegExp(PRETOOLUSE_CYCLE_MATCHER);
    // Count distinct alternatives a name satisfies (the double-fire fingerprint).
    function alternativesMatched(name) {
      return PRETOOLUSE_CYCLE_MATCHER.split("|").filter((alt) =>
        new RegExp(alt).test(name)
      ).length;
    }
    // R-5: bare context_cycle (UDS) still matches — exactly one alternative.
    assert.ok(re.test("context_cycle"), "bare context_cycle must still match (UDS)");
    assert.strictEqual(alternativesMatched("context_cycle"), 1, "bare → single-fire");
    // HTTP: namespaced name matches — exactly one alternative (was 2 before the fix).
    assert.ok(re.test("mcp__unimatrix__context_cycle"), "namespaced must match (HTTP)");
    assert.strictEqual(
      alternativesMatched("mcp__unimatrix__context_cycle"),
      1,
      "namespaced → single-fire (no longer double)"
    );
    // A non-cycle name still matches nothing (no spurious spawn).
    assert.ok(!re.test("Bash"), "non-cycle tool never matches");
    assert.ok(!re.test("evil_context_cycle_bypass"), "anchored: no substring bypass");
  });

  it("test_pretooluse_stays_in_hook_events", function () {
    // The matcher narrows; the event is NOT dropped from the table.
    assert.ok(HOOK_EVENTS.includes("PreToolUse"));
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, SRC, {});
    assert.ok(result.content.hooks.PreToolUse, "PreToolUse still registered");
  });

  it("test_all_other_matchers_unchanged", function () {
    // Output snapshot: every other registered event's unimatrix entry keeps its
    // F3 matcher byte-for-byte.
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, SRC, {});
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
    const result = mergeSettings(fp, SRC, {});
    assert.ok(!result.content.hooks.SubagentStop, "SubagentStop must be opt-in");
  });

  it("test_subagentstop_missing_local_file_absent", function () {
    // No settings.local.json at all → absent, no throw.
    const fp = tempSettingsPath();
    const result = mergeSettings(fp, SRC, {});
    assert.ok(!result.content.hooks.SubagentStop);
  });

  it("test_subagentstop_registered_when_key_true", function () {
    const fp = tempSettingsPath();
    writeOptIn(fp, true);
    const result = mergeSettings(fp, SRC, {});
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
    const result = mergeSettings(fp, SRC, {});
    assert.ok(!result.content.hooks.SubagentStop);
  });

  it("test_subagentstop_non_boolean_treated_as_unset", function () {
    // Type-confusion guard: only the literal boolean true enables. Each of these
    // non-boolean truthy/falsy values is treated as unset (security surface).
    for (const value of ["true", 1, 0, null, {}, [], "yes"]) {
      const fp = tempSettingsPath();
      writeOptIn(fp, value);
      const result = mergeSettings(fp, SRC, {});
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
    const result = mergeSettings(fp, SRC, {});
    assert.ok(!result.content.hooks.SubagentStop, "stale SubagentStop entry must be pruned");
    assert.ok(result.actions.some((a) => a.includes("opt-out")));
  });

  it("test_subagentstop_optout_preserves_foreign_hook", function () {
    const fp = tempSettingsPath();
    seedWithSubagentStop(fp, true);
    const result = mergeSettings(fp, SRC, {});
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
    const first = mergeSettings(fp, SRC, {});
    const second = mergeSettings(fp, SRC, {});
    assert.deepStrictEqual(first.content, second.content);
  });

  it("test_subagentstop_optin_then_optout_round_trip", function () {
    const fp = tempSettingsPath();
    // Opt in → registered.
    writeOptIn(fp, true);
    let result = mergeSettings(fp, SRC, {});
    assert.ok(result.content.hooks.SubagentStop);
    // Opt out (flip key to false) → pruned.
    writeOptIn(fp, false);
    result = mergeSettings(fp, SRC, {});
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
    const result = mergeSettings(fp, SRC, {});
    assert.strictEqual(Object.keys(result.content.hooks).length, 8);
    assert.ok(!result.content.hooks.SubagentStop);
    const group = result.content.hooks.PreToolUse.find((g) =>
      (g.hooks || []).some((h) => isUnimatrixHook(h))
    );
    assert.strictEqual(group.matcher, PRETOOLUSE_CYCLE_MATCHER);
  });
});

// ── vnc-031 Step 3c: cross-matcher-group stale-uni prune ─────────────
//
// For each MANAGED event, every uni-owned hook that is NOT the freshly-written
// keep-target is removed across ALL matcher groups — identity keep test
// (ADR-001), registered events only (ADR-002), Step 3b opt-out untouched.

describe("vnc-031 Step 3c cross-matcher-group prune", function () {
  // Count uni-owned hooks for an event across all matcher groups.
  function countUni(content, event) {
    let count = 0;
    for (const group of content.hooks[event] || []) {
      for (const hook of group.hooks || []) {
        if (isUnimatrixHook(hook)) {
          count++;
        }
      }
    }
    return count;
  }

  // The single uni hook command for a managed event, asserting exactly one.
  function soleUniHook(content, event) {
    const uni = [];
    for (const group of content.hooks[event] || []) {
      for (const hook of group.hooks || []) {
        if (isUnimatrixHook(hook)) {
          uni.push({ matcher: group.matcher, command: hook.command });
        }
      }
    }
    assert.strictEqual(uni.length, 1, "expected exactly one uni hook for " + event);
    return uni[0];
  }

  // Cumulative helper (#4263): derive everything from one source. Seeds the
  // fresh-shaped managed group AND an extra stale uni hook under a DIFFERENT
  // (non-managed) matcher group. staleCommand defaults to a Rust "*"-PreToolUse
  // legacy form; `foreign` optionally adds a foreign hook into the stale group.
  function seedWithCrossGroupStale(
    fp,
    { event = "PreToolUse", staleMatcher = "*", staleCommand, foreign } = {}
  ) {
    const cmd = staleCommand || "/old/path/unimatrix hook " + event;
    const staleHooks = [{ type: "command", command: cmd }];
    if (foreign) {
      staleHooks.push({ type: "command", command: foreign });
    }
    const managedMatcher = EVENT_MATCHERS[event];
    const hooks = {};
    hooks[event] = [
      {
        matcher: managedMatcher,
        hooks: [{ type: "command", command: "unimatrix-server hook " + event }],
      },
      { matcher: staleMatcher, hooks: staleHooks },
    ];
    writeSettings(fp, { hooks });
  }

  // ── AC-01: legacy "*" PreToolUse migrates clean (R-01) ──────────────

  it("test_legacy_star_pretooluse_migrates_clean", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        PreToolUse: [
          {
            matcher: "*",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook PreToolUse" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const cycleGroup = result.content.hooks.PreToolUse.find(
      (g) => g.matcher === PRETOOLUSE_CYCLE_MATCHER
    );
    assert.ok(cycleGroup, "cycle matcher group must exist");
    const uniInCycle = cycleGroup.hooks.filter(isUnimatrixHook);
    assert.strictEqual(uniInCycle.length, 1);
    assert.strictEqual(uniInCycle[0].command, expectedLocalCommand("PreToolUse"));
    // No uni hook survives under any "*" matcher.
    const star = result.content.hooks.PreToolUse.find(
      (g) => g.matcher === "*" && g.hooks.some(isUnimatrixHook)
    );
    assert.strictEqual(star, undefined, "no uni hook may survive under '*'");
    assert.strictEqual(countUni(result.content, "PreToolUse"), 1);
  });

  // ── R-01 (Critical): identity must not degrade to string compare ────
  // A `command ===` reimplementation must turn at least one of these red.

  it("test_cross_group_stale_twin_differing_only_by_shape_pruned", function () {
    const fp = tempSettingsPath();
    // No fresh managed group on input; the only uni hook is a stale near-twin
    // under a non-managed matcher whose command differs from fresh ONLY by shape
    // (collapsible whitespace / arg spacing) yet still classifies as uni.
    writeSettings(fp, {
      hooks: {
        SessionStart: [
          {
            matcher: "Foreign",
            hooks: [{ type: "command", command: "unimatrix  hook   SessionStart" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "SessionStart");
    assert.strictEqual(sole.matcher, EVENT_MATCHERS.SessionStart);
    assert.strictEqual(sole.command, expectedLocalCommand("SessionStart"));
    // The near-twin's group no longer holds a uni hook.
    const foreignGroup = result.content.hooks.SessionStart.find(
      (g) => g.matcher === "Foreign"
    );
    assert.ok(
      !foreignGroup || !foreignGroup.hooks.some(isUnimatrixHook),
      "near-twin uni hook must be pruned"
    );
  });

  it("test_cross_group_pretooluse_star_shares_prefix_with_cycle_survivor", function () {
    const fp = tempSettingsPath();
    // Stale "*" Rust uni hook sharing a long common prefix with the fresh command.
    writeSettings(fp, {
      hooks: {
        PreToolUse: [
          {
            matcher: PRETOOLUSE_CYCLE_MATCHER,
            hooks: [{ type: "command", command: "unimatrix-server hook PreToolUse" }],
          },
          {
            matcher: "*",
            hooks: [
              {
                type: "command",
                command: BINARY + " hook PreToolUse --legacy-star-suffix",
              },
            ],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(sole.command, expectedLocalCommand("PreToolUse"));
  });

  it("test_cross_group_survivor_is_exact_fresh_command_not_substring", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, {
      event: "PreToolUse",
      // Shape-varying twin: a substring/includes keep-rule would mis-handle this.
      staleCommand: "unimatrix   hook    PreToolUse",
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.command, expectedLocalCommand("PreToolUse")); // exact, not includes
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
  });

  // ── AC-02: exactly one uni hook per managed event, never zero ───────

  it("test_each_event_has_exactly_one_unimatrix_entry_cross_group", function () {
    const fp = tempSettingsPath();
    // Seed every event with the fresh-shaped managed group AND an extra stale uni
    // hook under a non-managed matcher group, then merge.
    const hooks = {};
    for (const event of DEFAULT_EVENTS) {
      hooks[event] = [
        {
          matcher: EVENT_MATCHERS[event],
          hooks: [{ type: "command", command: "unimatrix-server hook " + event }],
        },
        {
          matcher: "StaleMatcher",
          hooks: [{ type: "command", command: "/old/dir/unimatrix hook " + event }],
        },
      ];
    }
    writeSettings(fp, { hooks });
    const result = mergeSettings(fp, SRC, {});
    for (const event of DEFAULT_EVENTS) {
      const count = countUni(result.content, event);
      assert(count !== 0, "managed event " + event + " dropped to zero uni hooks");
      assert.strictEqual(count, 1, "expected exactly 1 uni hook for " + event);
      const sole = soleUniHook(result.content, event);
      assert.strictEqual(sole.matcher, EVENT_MATCHERS[event]);
      assert.strictEqual(sole.command, expectedLocalCommand(event));
    }
  });

  it("test_cross_group_only_stale_on_input_managed_entry_created_then_kept", function () {
    const fp = tempSettingsPath();
    // The ONLY uni hook pre-merge is a stale one under a non-managed matcher; no
    // uni hook in the managed group on input. Step 3 must CREATE the managed entry
    // and Step 3c must NOT prune it (guards a capture-before-create refactor bug).
    writeSettings(fp, {
      hooks: {
        Stop: [
          {
            matcher: "Foreign",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook Stop" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "Stop");
    assert.strictEqual(sole.matcher, EVENT_MATCHERS.Stop);
    assert.strictEqual(sole.command, expectedLocalCommand("Stop"));
  });

  // ── R-03: wrong-scope prune in/out managed group ────────────────────

  it("test_cross_group_in_group_dup_plus_cross_group_stale", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        SessionStart: [
          {
            matcher: "", // managed group with TWO uni entries (in-group dup)
            hooks: [
              { type: "command", command: "unimatrix-server hook SessionStart" },
              { type: "command", command: "/other/path/unimatrix hook SessionStart" },
            ],
          },
          {
            matcher: "Foreign",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook SessionStart" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "SessionStart");
    assert.strictEqual(sole.matcher, EVENT_MATCHERS.SessionStart);
    assert.strictEqual(sole.command, expectedLocalCommand("SessionStart"));
    assert.ok(result.actions.some((a) => a.includes("Removed duplicate")), "Step 3 dedup");
    assert.ok(
      result.actions.some((a) => a.includes("(cross-matcher migration)")),
      "Step 3c cross-matcher"
    );
  });

  it("test_cross_group_multiple_stale_groups_all_pruned", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        PostToolUse: [
          {
            matcher: "*", // managed
            hooks: [{ type: "command", command: "unimatrix-server hook PostToolUse" }],
          },
          {
            matcher: "StaleA",
            hooks: [{ type: "command", command: "/a/unimatrix hook PostToolUse" }],
          },
          {
            matcher: "StaleB",
            hooks: [{ type: "command", command: "/b/unimatrix hook PostToolUse" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "PostToolUse");
    assert.strictEqual(sole.matcher, "*");
    assert.strictEqual(sole.command, expectedLocalCommand("PostToolUse"));
    const crossActions = result.actions.filter((a) =>
      a.includes("PostToolUse (cross-matcher migration)")
    );
    assert.strictEqual(crossActions.length, 2, "one action per stale group");
  });

  // ── AC-03: foreign + near-miss + non-command preserved (R-07) ───────

  it("test_cross_group_preserves_foreign_star_hook", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, {
      event: "PreToolUse",
      staleMatcher: "*",
      foreign: "my-tool pre-check",
    });
    const result = mergeSettings(fp, SRC, {});
    const star = result.content.hooks.PreToolUse.find((g) => g.matcher === "*");
    assert.ok(star, "foreign-retaining group must NOT be dropped");
    assert.ok(!star.hooks.some(isUnimatrixHook), "stale uni hook gone from '*'");
    assert.strictEqual(star.hooks.length, 1);
    assert.strictEqual(star.hooks[0].command, "my-tool pre-check");
  });

  it("test_cross_group_preserves_near_miss_foreign_hook", function () {
    const fp = tempSettingsPath();
    // uni-LOOKING but isUnimatrixHook === false (no anchor match) — must survive
    // byte-for-byte (SR-02 / R-07).
    const nearMiss = "my-unimatrix-wrapper run";
    assert.ok(!isUnimatrixHook({ command: nearMiss }), "precondition: near-miss is not uni");
    seedWithCrossGroupStale(fp, {
      event: "PreToolUse",
      staleMatcher: "Foreign",
      staleCommand: nearMiss,
    });
    const result = mergeSettings(fp, SRC, {});
    const foreignGroup = result.content.hooks.PreToolUse.find((g) => g.matcher === "Foreign");
    assert.ok(foreignGroup, "near-miss foreign group must survive");
    assert.strictEqual(foreignGroup.hooks.length, 1);
    assert.strictEqual(foreignGroup.hooks[0].command, nearMiss);
  });

  it("test_cross_group_preserves_non_command_entry", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        PreToolUse: [
          {
            matcher: PRETOOLUSE_CYCLE_MATCHER,
            hooks: [{ type: "command", command: "unimatrix-server hook PreToolUse" }],
          },
          {
            matcher: "Foreign",
            hooks: [{ type: "url", url: "https://example.com/webhook" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const foreignGroup = result.content.hooks.PreToolUse.find((g) => g.matcher === "Foreign");
    assert.ok(foreignGroup, "non-command entry group must survive");
    assert.strictEqual(foreignGroup.hooks.length, 1);
    assert.strictEqual(foreignGroup.hooks[0].type, "url");
    assert.strictEqual(foreignGroup.hooks[0].url, "https://example.com/webhook");
  });

  // ── AC-04: emptied group dropped, event key retained (R-08) ─────────

  it("test_cross_group_drops_emptied_group_keeps_event", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, { event: "PreToolUse", staleMatcher: "*" });
    const result = mergeSettings(fp, SRC, {});
    assert.ok(result.content.hooks.PreToolUse, "event key retained");
    const star = result.content.hooks.PreToolUse.find((g) => g.matcher === "*");
    assert.strictEqual(star, undefined, "emptied '*' group dropped");
    const cycle = result.content.hooks.PreToolUse.find(
      (g) => g.matcher === PRETOOLUSE_CYCLE_MATCHER
    );
    assert.ok(cycle, "cycle group present");
  });

  it("test_cross_group_foreign_retaining_group_not_dropped", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, {
      event: "PreToolUse",
      staleMatcher: "*",
      foreign: "my-tool pre-check",
    });
    const result = mergeSettings(fp, SRC, {});
    const star = result.content.hooks.PreToolUse.find((g) => g.matcher === "*");
    assert.ok(star, "group with surviving foreign hook not dropped");
    assert.strictEqual(star.hooks.length, 1);
    assert.strictEqual(star.hooks[0].command, "my-tool pre-check");
  });

  // ── AC-05: idempotency incl. stale-"*"-on-first-run (R-06) ──────────

  it("test_cross_group_migration_idempotent", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, { event: "PreToolUse", staleMatcher: "*" });
    const first = mergeSettings(fp, SRC, {});
    const second = mergeSettings(fp, SRC, {});
    assert.deepStrictEqual(first.content, second.content);
  });

  it("test_cross_group_three_run_stability", function () {
    const fp = tempSettingsPath();
    // Multi-stale seed: "*" + .bak + old-dir uni hooks under one event.
    writeSettings(fp, {
      hooks: {
        PreToolUse: [
          {
            matcher: "*",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook PreToolUse" }],
          },
          {
            matcher: "BakMatcher",
            hooks: [
              {
                type: "command",
                command: "node /a/lib/hook-client/index.js.bak PreToolUse",
              },
            ],
          },
          {
            matcher: "OldDir",
            hooks: [
              {
                type: "command",
                command: "node /dogfood-client-OLD/lib/hook-client/index.js PreToolUse",
              },
            ],
          },
        ],
      },
    });
    mergeSettings(fp, SRC, {});
    const second = mergeSettings(fp, SRC, {});
    const third = mergeSettings(fp, SRC, {});
    assert.ok(
      !second.actions.some((a) => a.includes("(cross-matcher migration)")),
      "run 2 emits no cross-matcher action"
    );
    assert.deepStrictEqual(second.content, third.content);
  });

  // ── AC-06: both call arms identical (R-05, R-15) ────────────────────

  it("test_cross_group_migration_string_arm", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, { event: "PreToolUse", staleMatcher: "*" });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(sole.command, expectedLocalCommand("PreToolUse"));
  });

  it("test_cross_group_migration_object_arm", function () {
    const fp = tempSettingsPath();
    const clientPath = "/abs/lib/hook-client/index.js";
    seedWithCrossGroupStale(fp, { event: "PreToolUse", staleMatcher: "*" });
    const result = mergeSettings(
      fp,
      {
        events: HOOK_EVENTS,
        commandForEvent: (e) => buildHookClientCommand(clientPath, e),
      },
      {}
    );
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(sole.command, buildHookClientCommand(clientPath, "PreToolUse"));
  });

  // ── AC-07: opt-out path unchanged + partition seam (R-09) ───────────

  it("test_partition_combined_subagentstop_optout_and_pretooluse_cross_group", function () {
    const fp = tempSettingsPath();
    // No opt-in. Stale SubagentStop uni (non-registered → Step 3b opt-out) AND
    // stale "*" PreToolUse uni (registered → Step 3c cross-matcher) in one file.
    writeSettings(fp, {
      hooks: {
        SubagentStop: [
          {
            matcher: "*",
            hooks: [{ type: "command", command: "unimatrix hook SubagentStop" }],
          },
        ],
        PreToolUse: [
          {
            matcher: "*",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook PreToolUse" }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    assert.ok(!result.content.hooks.SubagentStop, "SubagentStop opt-out pruned");
    assert.strictEqual(countUni(result.content, "PreToolUse"), 1);
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
    // Each removal emits its OWN phrase; neither path emits the other's.
    const optOut = result.actions.filter((a) => a.includes("(opt-out)"));
    const cross = result.actions.filter((a) => a.includes("(cross-matcher migration)"));
    assert.strictEqual(optOut.length, 1, "exactly one opt-out action");
    assert.ok(optOut.every((a) => a.includes("SubagentStop")));
    assert.strictEqual(cross.length, 1, "exactly one cross-matcher action");
    assert.ok(cross.every((a) => a.includes("PreToolUse")));
    assert.ok(!cross.some((a) => a.includes("(opt-out)")), "no double emission");
  });

  // ── R-10: vnc-027 adjacency preserved ───────────────────────────────

  it("test_cross_group_pretooluse_survivor_under_cycle_matcher", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, { event: "PreToolUse", staleMatcher: "*" });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
  });

  it("test_cross_group_subagentstop_optin_composes", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        SubagentStop: [
          {
            matcher: "Foreign",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook SubagentStop" }],
          },
        ],
      },
    });
    writeOptIn(fp, true);
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "SubagentStop");
    // SubagentStop is opted in → managed "*" survivor; stale cross-group pruned.
    assert.strictEqual(sole.matcher, "*");
    assert.strictEqual(sole.command, expectedLocalCommand("SubagentStop"));
    const foreign = result.content.hooks.SubagentStop.find((g) => g.matcher === "Foreign");
    assert.ok(!foreign || !foreign.hooks.some(isUnimatrixHook), "stale cross-group pruned");
  });

  // ── AC-08: action string contract (R-11) ────────────────────────────

  it("test_cross_group_emits_action_and_dry_run_prefix", function () {
    const fp = tempSettingsPath();
    seedWithCrossGroupStale(fp, { event: "PreToolUse", staleMatcher: "*" });
    const result = mergeSettings(fp, SRC, {});
    const phrase = "Removed stale unimatrix hook: PreToolUse (cross-matcher migration)";
    assert.ok(result.actions.includes(phrase), "exact cross-matcher action present");
    // Disjoint from the other action phrases.
    assert.ok(!phrase.includes("(opt-out)"));
    assert.ok(!phrase.includes("Updated hook"));
    assert.ok(!phrase.includes("Added hook"));
    assert.ok(!phrase.includes("Removed duplicate"));

    // Dry-run arm: action prefixed; the seeded file is left exactly as seeded
    // (dry-run never re-writes). Re-read confirms no write side effect occurred.
    const fp2 = tempSettingsPath();
    seedWithCrossGroupStale(fp2, { event: "PreToolUse", staleMatcher: "*" });
    const before = fs.readFileSync(fp2, "utf8");
    const dry = mergeSettings(fp2, SRC, { dryRun: true });
    assert.ok(dry.actions.includes("[dry-run] " + phrase), "dry-run prefixed action");
    assert.strictEqual(fs.readFileSync(fp2, "utf8"), before, "dry-run must not write");
  });

  // ── P6 quoted-spaced-path keep-target (GATE C P6, unit-level) ───────

  it("test_cross_group_quoted_spaced_path_target_kept", function () {
    const fp = tempSettingsPath();
    const clientPath = "/a b/lib/hook-client/index.js"; // spaced → quoted by builder
    const freshCmd = buildHookClientCommand(clientPath, "PreToolUse");
    assert.ok(freshCmd.includes('"'), "precondition: spaced path is quoted");
    // Managed group already holds the quoted keep-target; a stale uni hook under "*".
    writeSettings(fp, {
      hooks: {
        PreToolUse: [
          {
            matcher: PRETOOLUSE_CYCLE_MATCHER,
            hooks: [{ type: "command", command: freshCmd }],
          },
          {
            matcher: "*",
            hooks: [{ type: "command", command: "/old/path/unimatrix hook PreToolUse" }],
          },
        ],
      },
    });
    const result = mergeSettings(
      fp,
      {
        events: HOOK_EVENTS,
        commandForEvent: (e) => buildHookClientCommand(clientPath, e),
      },
      {}
    );
    const sole = soleUniHook(result.content, "PreToolUse");
    assert.strictEqual(sole.matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(sole.command, freshCmd); // quoting irrelevant — object identity
  });

  // ── Edge cases (Risk Strategy §Edge Cases) ──────────────────────────

  it("test_cross_group_coincidentally_identical_command_pruned", function () {
    const fp = tempSettingsPath();
    // A uni hook in a foreign group whose command byte-equals the FRESH command is
    // still pruned (different object, FR-04) — no "two identical commands" state.
    writeSettings(fp, {
      hooks: {
        SessionStart: [
          {
            matcher: "Foreign",
            hooks: [{ type: "command", command: expectedLocalCommand("SessionStart") }],
          },
        ],
      },
    });
    const result = mergeSettings(fp, SRC, {});
    const sole = soleUniHook(result.content, "SessionStart");
    assert.strictEqual(sole.matcher, EVENT_MATCHERS.SessionStart);
    const foreign = result.content.hooks.SessionStart.find((g) => g.matcher === "Foreign");
    assert.ok(!foreign || !foreign.hooks.some(isUnimatrixHook), "coincidental twin pruned");
  });

  it("test_cross_group_malformed_entry_treated_as_foreign", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        SessionStart: [
          {
            matcher: EVENT_MATCHERS.SessionStart,
            hooks: [{ type: "command", command: "unimatrix-server hook SessionStart" }],
          },
          {
            matcher: "Foreign",
            hooks: [null, { type: "command", command: 42 }],
          },
        ],
      },
    });
    let result;
    assert.doesNotThrow(() => {
      result = mergeSettings(fp, SRC, {});
    });
    const foreign = result.content.hooks.SessionStart.find((g) => g.matcher === "Foreign");
    assert.ok(foreign, "malformed-entry group untouched");
    assert.strictEqual(foreign.hooks.length, 2);
  });

  it("test_cross_group_group_missing_hooks_key_skipped", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, {
      hooks: {
        SessionStart: [
          {
            matcher: EVENT_MATCHERS.SessionStart,
            hooks: [{ type: "command", command: "unimatrix-server hook SessionStart" }],
          },
          { matcher: "EmptyHooks", hooks: [] },
          { matcher: "NoHooksKey" },
        ],
      },
    });
    let result;
    assert.doesNotThrow(() => {
      result = mergeSettings(fp, SRC, {});
    });
    assert.strictEqual(countUni(result.content, "SessionStart"), 1);
  });
});

// nan-023 ADR-003 §2 (AC-07/AC-08, C-03): buildHookClientCommand gains an
// optional 3rd providerHint arg. The codex hook writer passes "codex-cli" so it
// emits `node <path> <EVENT> --provider codex-cli`. Every existing 2-arg call is
// byte-identical (SR-07 backward compat).
describe("nan-023 buildHookClientCommand provider hint", function () {
  const P = "/abs/pkg/lib/hook-client/index.js";

  it("test_buildHookClientCommand_no_hint_backward_compatible", function () {
    // 2-arg call unchanged — guards the claude-code golden (SR-07).
    assert.strictEqual(buildHookClientCommand(P, "Stop"), "node " + P + " Stop");
  });

  it("test_buildHookClientCommand_appends_provider_hint", function () {
    assert.strictEqual(
      buildHookClientCommand(P, "PreToolUse", "codex-cli"),
      "node " + P + " PreToolUse --provider codex-cli"
    );
  });

  it("test_buildHookClientCommand_empty_hint_omits_flag", function () {
    // Empty/whitespace-defensive: an empty-string hint appends nothing.
    assert.strictEqual(buildHookClientCommand(P, "Stop", ""), "node " + P + " Stop");
  });

  it("test_buildHookClientCommand_targets_hook_client_not_binary", function () {
    // AC-08 / C-03: invokes `node …/hook-client/index.js`, never the binary.
    const cmd = buildHookClientCommand(P, "PostToolUse", "codex-cli");
    assert.ok(/^node .*\/hook-client\/index\.js /.test(cmd));
    assert.ok(!/\bunimatrix\s+hook\b/.test(cmd));
  });

  it("test_buildHookClientCommand_matches_ownership_regex", function () {
    // AC-04: emitted command is Unimatrix-owned so re-runs are idempotent.
    const cmd = buildHookClientCommand(P, "SessionStart", "codex-cli");
    assert.ok(isUnimatrixHook({ command: cmd }));
  });

  it("test_buildHookClientCommand_path_with_space_quoted", function () {
    // R-10: a spaced path is double-quoted; the hint appends after the event.
    const spaced = "/abs/my dir/lib/hook-client/index.js";
    const cmd = buildHookClientCommand(spaced, "Stop", "codex-cli");
    assert.strictEqual(cmd, 'node "' + spaced + '" Stop --provider codex-cli');
    assert.ok(isUnimatrixHook({ command: cmd })); // quoted arm of pattern 5
  });

  it("test_every_hook_event_carries_provider_flag", function () {
    // AC-07: every codex hook command must carry --provider codex-cli (fail-loud
    // if any is missing). Mirrors the codex writer's per-event emission.
    for (const event of HOOK_EVENTS) {
      const cmd = buildHookClientCommand(P, event, "codex-cli");
      assert.ok(cmd.endsWith(" --provider codex-cli"), event + " missing provider flag");
    }
  });
});

// ── nan-024: shim command, settings.local.json, cross-scope, per-machine MCP ──

describe("nan-024 buildShimCommand (D-080)", function () {
  const { buildShimCommand, normalizeCommandSource } = require("../lib/merge-settings");
  const { isUnimatrixCommand } = require("../lib/hook-ownership");
  const cp = require("child_process");

  it("unix and windows byte forms", function () {
    assert.strictEqual(buildShimCommand("Stop", null, "linux", "/tmp/fx/h"), "'/tmp/fx/h/.unimatrix/bin/unimatrix' hook Stop");
    assert.strictEqual(buildShimCommand("Stop", "codex-cli", "linux", "/tmp/fx/my home"), "'/tmp/fx/my home/.unimatrix/bin/unimatrix' hook Stop --provider codex-cli");
    assert.strictEqual(buildShimCommand("Stop", null, "win32", "C:\\Users\\My User"), '"C:/Users/My User/.unimatrix/bin/unimatrix.cmd" hook Stop');
  });
  it("home omitted falls back to os.homedir()", function () {
    assert.strictEqual(buildShimCommand("Stop", null, "linux"), "'" + path.posix.join(os.homedir(), ".unimatrix/bin/unimatrix") + "' hook Stop");
  });
  it("refuses unsafe characters", function () {
    assert.throws(() => buildShimCommand("Stop", null, "linux", "/tmp/o'x"));
    assert.throws(() => buildShimCommand("Stop", null, "win32", "C:\\a%b"));
    assert.throws(() => buildShimCommand("Stop", null, "win32", 'C:\\a"b'));
  });
  it("pure and owned for every event x provider x platform x home", function () {
    for (const ev of HOOK_EVENTS) for (const pv of [null, "codex-cli"]) {
      for (const [pl, h] of [["linux", "/tmp/my home"], ["darwin", "/Users/a"], ["win32", "C:\\Users\\My User"]]) {
        const c = buildShimCommand(ev, pv, pl, h);
        assert.strictEqual(c, buildShimCommand(ev, pv, pl, h));
        assert.ok(isUnimatrixCommand(c), c);
      }
    }
  });
  it("space survives single quotes under sh -c", function () {
    const home = fs.mkdtempSync(path.join(os.tmpdir(), "um my home-"));
    fs.mkdirSync(path.join(home, ".unimatrix", "bin"), { recursive: true });
    const shim = path.join(home, ".unimatrix", "bin", "unimatrix");
    fs.writeFileSync(shim, '#!/bin/sh\necho "$@"\n', { mode: 0o755 });
    const out = cp.execFileSync("sh", ["-c", buildShimCommand("Stop", null, "linux", home)]).toString().trim();
    assert.strictEqual(out, "hook Stop");
  });
  it("normalizeCommandSource string arm removed", function () {
    assert.throws(() => normalizeCommandSource("/bin/unimatrix"), TypeError);
    assert.strictEqual(require("../lib/merge-settings").UNIMATRIX_PATTERNS, undefined);
  });
});

describe("nan-024 cross-scope dedupe (AC-13, D-010)", function () {
  const { buildShimCommand } = require("../lib/merge-settings");
  function fixture() {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "um-xs-"));
    return {
      local: path.join(dir, ".claude", "settings.local.json"),
      proj: path.join(dir, ".claude", "settings.json"),
      user: path.join(dir, "home", ".claude", "settings.json"),
    };
  }
  const src = (home) => ({ events: HOOK_EVENTS, commandForEvent: (e) => buildShimCommand(e, null, "linux", home) });
  const own = (cmd) => ({ matcher: "", hooks: [{ type: "command", command: cmd }, { type: "command", command: "foreign-tool" }] });

  it("exactly one owned entry per event, in settings.local.json; foreign preserved", function () {
    const f = fixture();
    const legacy = [
      "unimatrix hook Stop", "/abs/unimatrix hook Stop", "node /x/hook-client/index.js Stop",
      "npx unimatrix hook Stop", "$HOME/.unimatrix/bin/unimatrix hook Stop", "'/a/.unimatrix/bin/unimatrix' hook Stop",
    ];
    writeSettings(f.proj, { permissions: { allow: ["Read"] }, hooks: { Stop: legacy.map(own) } });
    writeSettings(f.user, { hooks: { Stop: [own("unimatrix hook Stop")] } });
    writeSettings(f.local, { hooks: { Stop: [own("unimatrix hook Stop")] } });
    for (let i = 0; i < 2; i++) mergeSettings(f.local, src("/h"), { crossScopeFiles: [f.proj, f.user] });
    const count = (file) => {
      const j = JSON.parse(fs.readFileSync(file, "utf8"));
      return Object.values(j.hooks || {}).flat().flatMap((g) => g.hooks).filter(isUnimatrixHook).length;
    };
    assert.strictEqual(count(f.proj), 0);
    assert.strictEqual(count(f.user), 0);
    const local = JSON.parse(fs.readFileSync(f.local, "utf8"));
    for (const ev of DEFAULT_EVENTS) assert.strictEqual(local.hooks[ev].flatMap((g) => g.hooks).filter(isUnimatrixHook).length, 1, ev);
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(f.proj, "utf8")).permissions, { allow: ["Read"] });
    assert.ok(JSON.parse(fs.readFileSync(f.proj, "utf8")).hooks.Stop.every((g) => g.hooks.some((h) => h.command === "foreign-tool")));
  });
  it("foreign-only cross-scope file not rewritten; malformed aborts untouched", function () {
    const f = fixture();
    writeSettings(f.proj, '{"hooks":{"Stop":[{"matcher":"","hooks":[{"type":"command","command":"x"}]}]}}');
    const old = new Date(2020, 1, 1);
    fs.utimesSync(f.proj, old, old);
    const before = fs.readFileSync(f.proj);
    mergeSettings(f.local, src("/h"), { crossScopeFiles: [f.proj] });
    assert.ok(before.equals(fs.readFileSync(f.proj)));
    assert.strictEqual(fs.statSync(f.proj).mtimeMs, old.getTime());
    writeSettings(f.user, "{ not json");
    assert.throws(() => mergeSettings(f.local, src("/h"), { crossScopeFiles: [f.user] }), /Malformed/);
    assert.strictEqual(fs.readFileSync(f.user, "utf8"), "{ not json");
  });
  it("re-merge over current entry writes nothing (mtime stable); exposes commands", function () {
    const f = fixture();
    const r = mergeSettings(f.local, src("/h"), {});
    assert.ok(r.commands.some((c) => c.event === "Stop" && c.command === "'/h/.unimatrix/bin/unimatrix' hook Stop"));
    const old = new Date(2020, 1, 1);
    fs.utimesSync(f.local, old, old);
    mergeSettings(f.local, src("/h"), {});
    assert.strictEqual(fs.statSync(f.local).mtimeMs, old.getTime());
  });
});

describe("nan-024 per-machine Claude MCP (claude-json, mcp-entry)", function () {
  const { writeClaudeMcp, removeOwnedMcpJson, resolveProjectKey, ClaudeJsonError } = require("../lib/claude-json");
  const { buildMcpEntry, buildMcpArgv, isFallbackMcpEntry } = require("../lib/mcp-entry");
  const { isUnimatrixMcpEntry } = require("../lib/hook-ownership");
  const mk = () => fs.mkdtempSync(path.join(os.tmpdir(), "um-cj-"));
  const entry = buildMcpEntry("linux", "/h", "/n/node");

  it("buildMcpEntry shapes and ownership", function () {
    assert.deepStrictEqual(entry, { command: "sh", args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'], env: {} });
    const w = buildMcpEntry("win32", "C:\\Users\\a", "C:\\n\\node.exe");
    assert.strictEqual(w.args[0], "C:/Users/a/.unimatrix/bin/shim-entry.js");
    assert.ok(isFallbackMcpEntry(w) && !isFallbackMcpEntry(entry));
    assert.ok(isUnimatrixMcpEntry("unimatrix", w) && isUnimatrixMcpEntry("unimatrix", entry));
    assert.deepStrictEqual(buildMcpArgv("linux", "/h", "n"), ["sh", "-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio']);
  });
  it("writes only projects[root].mcpServers.unimatrix; idempotent; other keys untouched", function () {
    const home = mk(), root = mk();
    const file = path.join(home, ".claude.json");
    const other = { projects: { "/other": { mcpServers: { x: { command: "y" } }, allowedTools: [] } }, theme: "dark" };
    fs.writeFileSync(file, JSON.stringify(other, null, 4));
    assert.strictEqual(writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry }).action, "created");
    const j = JSON.parse(fs.readFileSync(file, "utf8"));
    assert.deepStrictEqual(j.projects["/other"], other.projects["/other"]);
    assert.strictEqual(j.theme, "dark");
    assert.deepStrictEqual(j.projects[fs.realpathSync(root)].mcpServers.unimatrix, entry);
    const old = new Date(2020, 1, 1);
    fs.utimesSync(file, old, old);
    assert.strictEqual(writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry }).action, "unchanged");
    assert.strictEqual(fs.statSync(file).mtimeMs, old.getTime());
    assert.strictEqual(writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry: { ...entry, args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio '] } }).action, "updated");
    assert.deepStrictEqual(fs.readdirSync(home).filter((n) => n.endsWith(".tmp")), []);
  });
  it("foreign entry left with warning; malformed aborts byte-equal", function () {
    const home = mk(), root = mk();
    const file = path.join(home, ".claude.json");
    const key = fs.realpathSync(root);
    fs.writeFileSync(file, JSON.stringify({ projects: { [key]: { mcpServers: { unimatrix: { command: "mine" } } } } }));
    const before = fs.readFileSync(file);
    const r = writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry });
    assert.strictEqual(r.action, "skipped-foreign");
    assert.ok(/non-Unimatrix/.test(r.warning));
    assert.ok(before.equals(fs.readFileSync(file)));
    fs.writeFileSync(file, "{oops");
    assert.throws(() => writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry }), ClaudeJsonError);
    assert.strictEqual(fs.readFileSync(file, "utf8"), "{oops");
    assert.deepStrictEqual(fs.readdirSync(home).filter((n) => n.endsWith(".tmp")), []);
  });
  it("project key table: reuse spelling for trailing slash, symlink; win32 case", function () {
    const root = mk();
    const link = root + "-link";
    fs.symlinkSync(root, link);
    assert.strictEqual(resolveProjectKey({ [root + "/"]: {} }, root, "linux"), root + "/");
    assert.strictEqual(resolveProjectKey({ [link]: {} }, root, "linux"), link);
    assert.strictEqual(resolveProjectKey({}, link, "linux"), fs.realpathSync(root));
    assert.strictEqual(resolveProjectKey({ "c:/Users/a": {} }, "C:\\Users\\a", "win32"), "c:/Users/a");
    assert.strictEqual(resolveProjectKey({}, "C:\\Users\\a", "win32"), "C:/Users/a");
  });
  it("dry-run writes nothing", function () {
    const home = mk(), root = mk();
    assert.strictEqual(writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry, dryRun: true }).action, "created");
    assert.ok(!fs.existsSync(path.join(home, ".claude.json")));
  });
  it("concurrent writer: result stays valid JSON", function () {
    const home = mk(), root = mk();
    const file = path.join(home, ".claude.json");
    fs.writeFileSync(file, JSON.stringify({ n: 0 }));
    for (let i = 1; i <= 50; i++) {
      writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry: { ...entry, env: { i: String(i) } } });
      const j = JSON.parse(fs.readFileSync(file, "utf8"));
      j.n = i;
      fs.writeFileSync(file, JSON.stringify(j));
    }
    const j = JSON.parse(fs.readFileSync(file, "utf8"));
    assert.strictEqual(j.n, 50);
    assert.strictEqual(j.projects[fs.realpathSync(root)].mcpServers.unimatrix.env.i, "50");
  });
  it(".mcp.json removal preserving foreign servers", function () {
    const root = mk();
    const file = path.join(root, ".mcp.json");
    const own = { command: "sh", args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'] };
    fs.writeFileSync(file, JSON.stringify({ mcpServers: { unimatrix: own, other: { command: "o" } }, extra: 1 }, null, 2));
    assert.deepStrictEqual(removeOwnedMcpJson(root, true).actions.length, 1);
    assert.ok(JSON.parse(fs.readFileSync(file, "utf8")).mcpServers.unimatrix);
    const r = removeOwnedMcpJson(root, false);
    assert.ok(r.changed);
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(file, "utf8")), { mcpServers: { other: { command: "o" } }, extra: 1 });
    fs.writeFileSync(file, JSON.stringify({ mcpServers: { unimatrix: own } }));
    removeOwnedMcpJson(root, false);
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(file, "utf8")), { mcpServers: {} });
    // foreign-only: untouched, foreign "unimatrix" warns
    fs.writeFileSync(file, '{"mcpServers":{"unimatrix":{"command":"mine"}}}');
    const old = new Date(2020, 1, 1);
    fs.utimesSync(file, old, old);
    const r2 = removeOwnedMcpJson(root, false);
    assert.ok(!r2.changed && r2.warnings.length === 1);
    assert.strictEqual(fs.statSync(file).mtimeMs, old.getTime());
    fs.writeFileSync(file, "{bad");
    assert.throws(() => removeOwnedMcpJson(root, false), ClaudeJsonError);
  });
});

describe("file-mode preservation (F1)", { skip: process.platform === "win32" }, function () {
  const modeOf = (p) => fs.statSync(p).mode & 0o777;
  it("mergeSettings keeps an existing 0600 settings file at 0600", function () {
    const fp = tempSettingsPath();
    writeSettings(fp, { theme: "dark" });
    fs.chmodSync(fp, 0o600);
    mergeSettings(fp, SRC, {});
    assert.ok(fs.readFileSync(fp, "utf8").includes("hooks"));
    assert.strictEqual(modeOf(fp), 0o600);
  });
  it("claude-json write keeps 0600 on ~/.claude.json and removeOwnedMcpJson keeps .mcp.json mode", function () {
    const { writeClaudeMcp, removeOwnedMcpJson } = require("../lib/claude-json.js");
    const home = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-cj-mode-"));
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-cj-mode-"));
    const file = path.join(home, ".claude.json");
    fs.writeFileSync(file, "{}\n");
    fs.chmodSync(file, 0o600);
    writeClaudeMcp({ home, file: path.join(home, ".claude.json"), projectRoot: root, entry: { command: "sh", args: ["-c", "x"], env: {} } });
    assert.strictEqual(modeOf(file), 0o600);
    const mcp = path.join(root, ".mcp.json");
    fs.writeFileSync(mcp, JSON.stringify({ mcpServers: { unimatrix: { command: "sh", args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'], env: {} } } }));
    fs.chmodSync(mcp, 0o640);
    removeOwnedMcpJson(root, false);
    assert.strictEqual(modeOf(mcp), 0o640);
  });
});

describe("1036 C8 updateFileSync + semantic unchanged (FR-16/D-022)", function () {
  const ms = require("../lib/merge-settings");
  const root = () => fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "c8-")));
  const local = (r) => { fs.mkdirSync(path.join(r, ".claude"), { recursive: true }); return path.join(r, ".claude", "settings.local.json"); };
  const canon = (r) => { const f = local(r); ms.mergeSettings(f, SRC, { containWithin: r }); return JSON.parse(fs.readFileSync(f, "utf8")); };

  it("created then unchanged with equal mtime", function () {
    const r = root(); const f = local(r);
    assert.strictEqual(ms.mergeSettings(f, SRC, { containWithin: r }).action, "created");
    const m = fs.statSync(f).mtimeMs;
    assert.strictEqual(ms.mergeSettings(f, SRC, { containWithin: r }).action, "unchanged");
    assert.strictEqual(fs.statSync(f).mtimeMs, m);
  });
  for (const [name, fmt] of [
    ["no trailing newline", (j) => JSON.stringify(j, null, 2)],
    ["CRLF", (j) => JSON.stringify(j, null, 2).replace(/\n/g, "\r\n") + "\r\n"],
    ["compact", (j) => JSON.stringify(j)],
    ["tab indent", (j) => JSON.stringify(j, null, "\t") + "\n"],
  ]) {
    it("non-canonical correct file is unchanged, bytes+inode kept: " + name, function () {
      const r = root(); const f = local(r);
      const bytes = fmt(canon(r)); fs.writeFileSync(f, bytes);
      const ino = fs.statSync(f).ino;
      assert.strictEqual(ms.mergeSettings(f, SRC, { containWithin: r }).action, "unchanged");
      assert.strictEqual(fs.readFileSync(f, "utf8"), bytes);
      assert.strictEqual(fs.statSync(f).ino, ino);
    });
  }
  it("update keeps existing 0644 mode and reuses indent", function () {
    const r = root(); const f = local(r);
    fs.writeFileSync(f, JSON.stringify({ hooks: { Stop: [{ matcher: "", hooks: [{ type: "command", command: "foreign" }] }] } }, null, 4) + "\n", { mode: 0o644 });
    fs.chmodSync(f, 0o644);
    assert.strictEqual(ms.mergeSettings(f, SRC, { containWithin: r }).action, "updated");
    assert.strictEqual(fs.statSync(f).mode & 0o777, 0o644);
    assert.match(fs.readFileSync(f, "utf8"), /^\{\n {4}"hooks"/);
    assert.strictEqual(JSON.parse(fs.readFileSync(f, "utf8")).hooks.Stop[0].hooks[0].command, "foreign");
  });
  it("symlink outside root -> outside-project; inside root -> written through", function () {
    const r = root(); const f = local(r); const out = root();
    const tgt = path.join(out, "x.json"); fs.writeFileSync(tgt, "{}");
    fs.symlinkSync(tgt, f);
    assert.throws(() => ms.mergeSettings(f, SRC, { containWithin: r }), (e) => e.code === "outside-project");
    assert.strictEqual(fs.readFileSync(tgt, "utf8"), "{}");
    fs.unlinkSync(f); const inTgt = path.join(r, "real.json"); fs.writeFileSync(inTgt, "{}"); fs.symlinkSync(inTgt, f);
    ms.mergeSettings(f, SRC, { containWithin: r });
    assert.ok(fs.lstatSync(f).isSymbolicLink());
    assert.ok(JSON.parse(fs.readFileSync(inTgt, "utf8")).hooks.Stop);
  });
  it("malformed file throws and is untouched", function () {
    const r = root(); const f = local(r); fs.writeFileSync(f, "{bad");
    assert.throws(() => ms.mergeSettings(f, SRC, { containWithin: r }), /Malformed/);
    assert.strictEqual(fs.readFileSync(f, "utf8"), "{bad");
  });
  it("SubagentStop opt-in decided on the same bytes (fault flips opt-in, retry recomputes)", function () {
    const r = root(); const f = local(r); fs.writeFileSync(f, "{}\n"); let once = false;
    const fault = (pt) => { if (pt === "after-read" && !once) { once = true; writeOptIn(f, true); } };
    const out = ms.mergeSettings(f, SRC, { containWithin: r, fault });
    assert.ok(out.commands.some((c) => c.event === "SubagentStop"));
    assert.ok(JSON.parse(fs.readFileSync(f, "utf8")).hooks.SubagentStop);
  });
  it("removeOwnedHooks: absent unchanged; foreign order kept; leg reports removed", function () {
    const r = root(); const f = path.join(r, "settings.json");
    assert.deepStrictEqual(ms.removeOwnedHooksLeg(f, { reason: "x" }), []);
    assert.strictEqual(fs.existsSync(f), false);
    writeSettings(f, { hooks: { Stop: [{ matcher: "", hooks: [{ type: "command", command: "a" }, { type: "command", command: "unimatrix hook Stop" }, { type: "command", command: "b" }] }] } });
    const leg = ms.removeOwnedHooksLeg(f, { reason: "legacy" });
    assert.deepStrictEqual(leg, [{ harness: "claude-code", surface: "hooks", action: "removed", path: f, reason: "legacy" }]);
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(f, "utf8")).hooks.Stop[0].hooks.map((h) => h.command), ["a", "b"]);
    assert.deepStrictEqual(ms.removeOwnedHooksLeg(f, {}), []);
  });
  it("wireProjectHooks returns one leg per event with writer action", function () {
    const r = root();
    const legs = ms.wireProjectHooks({ root: r, platform: "linux", home: "/h" });
    assert.strictEqual(legs.length, DEFAULT_EVENTS.length);
    assert.ok(legs.every((l) => l.action === "created" && l.surface === "hooks"));
    assert.ok(ms.wireProjectHooks({ root: r, platform: "linux", home: "/h" }).every((l) => l.action === "unchanged"));
  });
});
