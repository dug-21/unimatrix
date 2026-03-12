"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { mergeSettings, isUnimatrixHook, HOOK_EVENTS } = require("../lib/merge-settings");

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

const BINARY = "/abs/path/to/unimatrix";

// ── R-01 Scenarios ──────────────────────────────────────────────────

describe("mergeSettings", function () {
  describe("R-01: merge into empty file", function () {
    it("test_merge_into_empty_file", function () {
      const fp = tempSettingsPath();
      // File does not exist
      const result = mergeSettings(fp, BINARY, {});
      assert.ok(result.content.hooks);
      for (const event of HOOK_EVENTS) {
        assert.ok(result.content.hooks[event], "Missing event: " + event);
        const groups = result.content.hooks[event];
        assert.strictEqual(groups.length, 1);
        assert.strictEqual(groups[0].hooks.length, 1);
        assert.strictEqual(groups[0].hooks[0].type, "command");
        assert.ok(groups[0].hooks[0].command.includes(BINARY + " hook " + event));
      }
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
      assert.strictEqual(Object.keys(result.content.hooks).length, 7);
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
      // The existing matcher group "*" should have both the custom hook and the unimatrix hook
      const starGroup = preToolUse.find((g) => g.matcher === "*");
      assert.ok(starGroup);
      assert.strictEqual(starGroup.hooks.length, 2);
      assert.strictEqual(starGroup.hooks[0].command, "my-tool pre-check");
      assert.ok(starGroup.hooks[1].command.includes(BINARY));
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
      assert.strictEqual(group.hooks[0].command, BINARY + " hook SessionStart");
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
      assert.strictEqual(group.hooks[0].command, BINARY + " hook SessionStart");
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
    it("test_all_7_events_present", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      const expected = [
        "SessionStart", "Stop", "UserPromptSubmit",
        "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop",
      ];
      for (const e of expected) {
        assert.ok(result.content.hooks[e], "Missing event: " + e);
      }
    });

    it("test_each_event_has_exactly_one_unimatrix_entry", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, BINARY, {});
      mergeSettings(fp, BINARY, {});
      const result = mergeSettings(fp, BINARY, {});
      for (const event of HOOK_EVENTS) {
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
      for (const event of ["SessionStart", "Stop", "UserPromptSubmit"]) {
        assert.strictEqual(result.content.hooks[event][0].matcher, "");
      }
    });

    it("test_tool_events_use_star_matcher", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      for (const event of ["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]) {
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
      assert.strictEqual(Object.keys(result.content.hooks).length, 7);
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
      assert.strictEqual(Object.keys(result.content.hooks).length, 7);
    });
  });

  // ── R-04 Dedup Across Multiple Runs ───────────────────────────────

  describe("dedup across multiple runs", function () {
    it("test_three_consecutive_merges_no_growth", function () {
      const fp = tempSettingsPath();
      mergeSettings(fp, BINARY, {});
      mergeSettings(fp, BINARY, {});
      const result = mergeSettings(fp, BINARY, {});
      for (const event of HOOK_EVENTS) {
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
      assert.strictEqual(uniHooks[0].command, BINARY + " hook SessionStart");
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
      // Unimatrix hook should be in the "*" matcher group
      const starGroup = result.content.hooks.PreToolUse.find((g) => g.matcher === "*");
      assert.ok(starGroup);
      assert.ok(starGroup.hooks.some((h) => h.command.includes(BINARY)));
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
      // No tee pipeline - plain command format
      assert.strictEqual(group.hooks[0].command, BINARY + " hook UserPromptSubmit");
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
      assert.strictEqual(Object.keys(result.content.hooks).length, 7);
    });
  });

  // ── Command Format ────────────────────────────────────────────────

  describe("command format", function () {
    it("test_all_hooks_use_plain_command_format", function () {
      const fp = tempSettingsPath();
      const result = mergeSettings(fp, BINARY, {});
      for (const event of HOOK_EVENTS) {
        for (const group of result.content.hooks[event]) {
          for (const hook of group.hooks) {
            if (isUnimatrixHook(hook)) {
              assert.strictEqual(
                hook.command,
                BINARY + " hook " + event,
                "Hook for " + event + " should use plain command format"
              );
              assert.ok(!hook.command.includes("|"), "No pipe in hook command for " + event);
            }
          }
        }
      }
    });
  });
});
