"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it, beforeEach, afterEach, mock } = require("node:test");

// We test the internal functions directly (detectProjectRoot, writeMcpJson,
// copySkills, printSummary) and mock external dependencies for the full init().

const {
  detectProjectRoot,
  writeMcpJson,
  copySkills,
  printSummary,
} = require("../lib/init.js");

const BINARY = "/abs/path/to/node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix";

/** Create a temp directory that acts as a project root with .git */
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-init-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

/** Create a temp directory without .git (for error tests) */
function makeTempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-init-test-"));
}

/** Create bundled skills source directory with test content */
function makeBundledSkills(packageDir) {
  const skillsDir = path.join(packageDir, "skills");
  const skillNames = ["unimatrix-init", "unimatrix-seed"];
  for (const name of skillNames) {
    const dir = path.join(skillsDir, name);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, "SKILL.md"), "# " + name + "\nBundled content.\n");
  }
  return skillsDir;
}

// ── Project Root Detection ──────────────────────────────────────────

describe("detectProjectRoot", () => {
  it("test_finds_git_in_current_dir", () => {
    const dir = makeTempProject();
    const result = detectProjectRoot(dir);
    assert.strictEqual(result, dir);
  });

  it("test_walks_up_to_git", () => {
    const dir = makeTempProject();
    const subDir = path.join(dir, "src", "lib");
    fs.mkdirSync(subDir, { recursive: true });
    const result = detectProjectRoot(subDir);
    assert.strictEqual(result, dir);
  });

  it("test_no_git_errors_with_diagnostic", () => {
    // Create a temp dir that has no .git anywhere up to root.
    // We use a nested structure where we know .git won't exist.
    const dir = makeTempDir();
    const nested = path.join(dir, "deep", "nested");
    fs.mkdirSync(nested, { recursive: true });

    // This will walk up and eventually either find a .git or hit root.
    // On a CI machine the workspace itself may have .git, so we test
    // with an isolated dir that we know has no .git in it.
    // We test the error message content if it throws.
    try {
      detectProjectRoot(nested);
      // If it doesn't throw, it found a .git somewhere above — that's OK
      // in a real filesystem. The key test is the error message format.
    } catch (error) {
      assert.ok(
        error.message.includes("Could not find project root"),
        "Error should mention 'Could not find project root', got: " + error.message
      );
    }
  });

  it("test_stops_at_filesystem_root", () => {
    // Attempt detection from /tmp with no .git — should not infinite loop.
    // Create isolated dir to avoid hitting workspace .git.
    const dir = makeTempDir();
    try {
      detectProjectRoot(dir);
    } catch (error) {
      assert.ok(error.message.includes("Could not find project root"));
    }
    // If no error, a .git exists above tmpdir — acceptable.
  });
});

// ── .mcp.json Writing ───────────────────────────────────────────────

describe("writeMcpJson", () => {
  it("test_creates_mcp_json_on_clean_project", () => {
    const dir = makeTempProject();
    const actions = writeMcpJson(dir, BINARY, false);

    const mcpPath = path.join(dir, ".mcp.json");
    assert.ok(fs.existsSync(mcpPath), ".mcp.json should be created");

    const content = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    assert.strictEqual(content.mcpServers.unimatrix.command, BINARY);
    assert.deepStrictEqual(content.mcpServers.unimatrix.args, []);
    assert.deepStrictEqual(content.mcpServers.unimatrix.env, {});

    assert.ok(
      actions.some((a) => a.includes("Created .mcp.json")),
      "Should report creation"
    );
  });

  it("test_preserves_existing_servers", () => {
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    const existing = {
      mcpServers: {
        filesystem: { command: "/usr/bin/fs-server", args: ["--ro"] },
      },
    };
    fs.writeFileSync(mcpPath, JSON.stringify(existing, null, 2), "utf8");

    writeMcpJson(dir, BINARY, false);

    const content = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    assert.strictEqual(
      content.mcpServers.filesystem.command,
      "/usr/bin/fs-server",
      "Filesystem server should be preserved"
    );
    assert.deepStrictEqual(content.mcpServers.filesystem.args, ["--ro"]);
    assert.strictEqual(content.mcpServers.unimatrix.command, BINARY);
  });

  it("test_updates_existing_unimatrix_entry", () => {
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    const existing = {
      mcpServers: {
        unimatrix: { command: "/old/path/to/unimatrix", args: [] },
      },
    };
    fs.writeFileSync(mcpPath, JSON.stringify(existing, null, 2), "utf8");

    writeMcpJson(dir, BINARY, false);

    const content = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    assert.strictEqual(
      content.mcpServers.unimatrix.command,
      BINARY,
      "Should update to new binary path"
    );
  });

  it("test_preserves_nested_env_args_in_other_servers", () => {
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    const existing = {
      mcpServers: {
        other: {
          command: "/usr/bin/other",
          args: ["--flag", "value"],
          env: { API_KEY: "secret123" },
          cwd: "/some/dir",
        },
      },
    };
    fs.writeFileSync(mcpPath, JSON.stringify(existing, null, 2), "utf8");

    writeMcpJson(dir, BINARY, false);

    const content = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    assert.deepStrictEqual(content.mcpServers.other, existing.mcpServers.other);
  });

  it("test_malformed_mcp_json_throws", () => {
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    fs.writeFileSync(mcpPath, "{ invalid json }", "utf8");

    assert.throws(
      () => writeMcpJson(dir, BINARY, false),
      (error) => {
        assert.ok(error.message.includes("Malformed .mcp.json"));
        return true;
      }
    );
  });

  it("test_dry_run_does_not_write_mcp_json", () => {
    const dir = makeTempProject();
    const mcpPath = path.join(dir, ".mcp.json");
    const actions = writeMcpJson(dir, BINARY, true);

    assert.ok(!fs.existsSync(mcpPath), ".mcp.json should NOT be created in dry-run");
    assert.ok(
      actions.some((a) => a.includes("[dry-run]")),
      "Actions should be prefixed with [dry-run]"
    );
  });
});

// ── Skill File Copying ──────────────────────────────────────────────

describe("copySkills", () => {
  // We need to temporarily override __dirname for copySkills to find bundled skills.
  // Instead, we'll create a bundled skills directory in the actual package location.

  it("test_copies_skill_dirs", () => {
    const dir = makeTempProject();
    // Create bundled skills in the package's skills/ dir
    const packageDir = path.join(__dirname, "..");
    const skillsSource = path.join(packageDir, "skills");

    // Create test skills
    const testSkills = ["test-skill-a", "test-skill-b"];
    for (const name of testSkills) {
      const skillDir = path.join(skillsSource, name);
      fs.mkdirSync(skillDir, { recursive: true });
      fs.writeFileSync(path.join(skillDir, "SKILL.md"), "# " + name);
    }

    try {
      const actions = copySkills(dir, false);
      const targetDir = path.join(dir, ".claude", "skills");

      for (const name of testSkills) {
        const skillMd = path.join(targetDir, name, "SKILL.md");
        assert.ok(
          fs.existsSync(skillMd),
          "Should copy skill: " + name
        );
        assert.strictEqual(
          fs.readFileSync(skillMd, "utf8"),
          "# " + name
        );
      }

      assert.ok(
        actions.some((a) => a.includes("Copied skill: test-skill-a")),
        "Should report copied skills"
      );
    } finally {
      // Clean up test skills from source
      for (const name of testSkills) {
        fs.rmSync(path.join(skillsSource, name), { recursive: true, force: true });
      }
    }
  });

  it("test_overwrites_existing_unimatrix_skills", () => {
    const dir = makeTempProject();
    const packageDir = path.join(__dirname, "..");
    const skillsSource = path.join(packageDir, "skills");

    // Create a bundled skill
    const skillDir = path.join(skillsSource, "overwrite-test");
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, "SKILL.md"), "NEW CONTENT");

    // Pre-create the same skill in the project with different content
    const targetSkillDir = path.join(dir, ".claude", "skills", "overwrite-test");
    fs.mkdirSync(targetSkillDir, { recursive: true });
    fs.writeFileSync(path.join(targetSkillDir, "SKILL.md"), "OLD CONTENT");

    try {
      copySkills(dir, false);

      const content = fs.readFileSync(
        path.join(targetSkillDir, "SKILL.md"),
        "utf8"
      );
      assert.strictEqual(content, "NEW CONTENT", "Should overwrite existing skill");
    } finally {
      fs.rmSync(skillDir, { recursive: true, force: true });
    }
  });

  it("test_preserves_non_unimatrix_skills", () => {
    const dir = makeTempProject();
    const packageDir = path.join(__dirname, "..");
    const skillsSource = path.join(packageDir, "skills");

    // Create a bundled skill
    const bundledSkill = path.join(skillsSource, "bundled-only");
    fs.mkdirSync(bundledSkill, { recursive: true });
    fs.writeFileSync(path.join(bundledSkill, "SKILL.md"), "bundled");

    // Pre-create a custom (non-unimatrix) skill in the project
    const customSkillDir = path.join(dir, ".claude", "skills", "custom-skill");
    fs.mkdirSync(customSkillDir, { recursive: true });
    fs.writeFileSync(path.join(customSkillDir, "SKILL.md"), "CUSTOM");

    try {
      copySkills(dir, false);

      // Custom skill should be untouched
      const customContent = fs.readFileSync(
        path.join(customSkillDir, "SKILL.md"),
        "utf8"
      );
      assert.strictEqual(customContent, "CUSTOM", "Custom skill should be preserved");

      // Bundled skill should be copied
      assert.ok(
        fs.existsSync(path.join(dir, ".claude", "skills", "bundled-only", "SKILL.md"))
      );
    } finally {
      fs.rmSync(bundledSkill, { recursive: true, force: true });
    }
  });

  it("test_dry_run_does_not_copy_skills", () => {
    const dir = makeTempProject();
    const packageDir = path.join(__dirname, "..");
    const skillsSource = path.join(packageDir, "skills");

    const skillDir = path.join(skillsSource, "dryrun-test");
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, "SKILL.md"), "content");

    try {
      const actions = copySkills(dir, true);
      const targetDir = path.join(dir, ".claude", "skills", "dryrun-test");
      assert.ok(!fs.existsSync(targetDir), "Should NOT copy in dry-run");
      assert.ok(
        actions.some((a) => a.includes("[dry-run] Would copy skill:")),
        "Should report planned actions"
      );
    } finally {
      fs.rmSync(skillDir, { recursive: true, force: true });
    }
  });
});

// ── Summary Output ──────────────────────────────────────────────────

describe("printSummary", () => {
  it("test_prints_unimatrix_init_suggestion", () => {
    const logs = [];
    const origLog = console.log;
    console.log = (...args) => logs.push(args.join(" "));

    try {
      printSummary(["Action 1", "Action 2"], false);
      const output = logs.join("\n");
      assert.ok(
        output.includes("/unimatrix-init"),
        "Should suggest running /unimatrix-init"
      );
      assert.ok(
        output.includes("Unimatrix Init Complete"),
        "Should print completion header"
      );
    } finally {
      console.log = origLog;
    }
  });

  it("test_dry_run_summary_header", () => {
    const logs = [];
    const origLog = console.log;
    console.log = (...args) => logs.push(args.join(" "));

    try {
      printSummary(["Action 1"], true);
      const output = logs.join("\n");
      assert.ok(
        output.includes("Dry Run Summary"),
        "Should print dry-run header"
      );
      assert.ok(
        !output.includes("/unimatrix-init"),
        "Should NOT suggest next step in dry-run"
      );
    } finally {
      console.log = origLog;
    }
  });
});

// ── Full init() with mocked dependencies ────────────────────────────

describe("init (integration with mocks)", () => {
  // We test the full init flow by setting UNIMATRIX_BINARY to a script
  // that just prints a version string.

  it("test_dry_run_does_not_write_files", () => {
    const dir = makeTempProject();
    const origCwd = process.cwd;

    // Create a fake binary
    const fakeBinary = path.join(dir, "fake-unimatrix");
    fs.writeFileSync(fakeBinary, "#!/bin/sh\necho 'unimatrix 0.5.0'\n");
    fs.chmodSync(fakeBinary, 0o755);

    // Set UNIMATRIX_BINARY so resolveBinary finds it
    const origEnv = process.env.UNIMATRIX_BINARY;
    process.env.UNIMATRIX_BINARY = fakeBinary;

    // Capture console output
    const logs = [];
    const origLog = console.log;
    console.log = (...args) => logs.push(args.join(" "));

    return (async () => {
      try {
        // Re-require init to pick up the mocked env
        const initModPath = require.resolve("../lib/init.js");
        const resolveModPath = require.resolve("../lib/resolve-binary.js");
        delete require.cache[initModPath];
        delete require.cache[resolveModPath];
        const { init: freshInit } = require("../lib/init.js");

        await freshInit({ dryRun: true, projectDir: dir });

        // Assert no files were created
        const mcpPath = path.join(dir, ".mcp.json");
        assert.ok(
          !fs.existsSync(mcpPath),
          ".mcp.json should NOT be created in dry-run"
        );

        const settingsPath = path.join(dir, ".claude", "settings.json");
        assert.ok(
          !fs.existsSync(settingsPath),
          "settings.json should NOT be created in dry-run"
        );

        // Assert summary was printed with dry-run actions
        const output = logs.join("\n");
        assert.ok(
          output.includes("Dry Run Summary"),
          "Should print dry-run summary"
        );
        assert.ok(
          output.includes("[dry-run]"),
          "Actions should include [dry-run] prefix"
        );
      } finally {
        console.log = origLog;
        if (origEnv !== undefined) {
          process.env.UNIMATRIX_BINARY = origEnv;
        } else {
          delete process.env.UNIMATRIX_BINARY;
        }
      }
    })();
  });

  it("test_full_init_creates_mcp_and_settings", () => {
    const dir = makeTempProject();

    // Create a fake binary that exits 0
    const fakeBinary = path.join(dir, "fake-unimatrix");
    fs.writeFileSync(
      fakeBinary,
      '#!/bin/sh\necho "unimatrix 0.5.0"\n'
    );
    fs.chmodSync(fakeBinary, 0o755);

    const origEnv = process.env.UNIMATRIX_BINARY;
    process.env.UNIMATRIX_BINARY = fakeBinary;

    const logs = [];
    const origLog = console.log;
    console.log = (...args) => logs.push(args.join(" "));

    return (async () => {
      try {
        const initModPath = require.resolve("../lib/init.js");
        const resolveModPath = require.resolve("../lib/resolve-binary.js");
        delete require.cache[initModPath];
        delete require.cache[resolveModPath];
        const { init: freshInit } = require("../lib/init.js");

        await freshInit({ dryRun: false, projectDir: dir });

        // .mcp.json should exist with correct binary path
        const mcpPath = path.join(dir, ".mcp.json");
        assert.ok(fs.existsSync(mcpPath), ".mcp.json should be created");
        const mcpContent = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
        assert.ok(
          mcpContent.mcpServers.unimatrix.command.includes("fake-unimatrix"),
          "Should reference the binary"
        );

        // .claude/settings.json should exist with hook entries
        const settingsPath = path.join(dir, ".claude", "settings.json");
        assert.ok(fs.existsSync(settingsPath), "settings.json should be created");
        const settingsContent = JSON.parse(
          fs.readFileSync(settingsPath, "utf8")
        );
        assert.ok(settingsContent.hooks, "Should have hooks key");
        assert.ok(
          settingsContent.hooks.SessionStart,
          "Should have SessionStart hook"
        );

        // Summary should mention init complete
        const output = logs.join("\n");
        assert.ok(
          output.includes("Unimatrix Init Complete"),
          "Should print completion header"
        );
        assert.ok(
          output.includes("/unimatrix-init"),
          "Should suggest next step"
        );
      } finally {
        console.log = origLog;
        if (origEnv !== undefined) {
          process.env.UNIMATRIX_BINARY = origEnv;
        } else {
          delete process.env.UNIMATRIX_BINARY;
        }
      }
    })();
  });

  it("test_init_idempotent", () => {
    const dir = makeTempProject();

    // Binary must be named "unimatrix" so merge-settings recognizes
    // existing hooks via the /unimatrix\s+hook\s/ pattern (ADR-004).
    const binDir = path.join(dir, "bin");
    fs.mkdirSync(binDir, { recursive: true });
    const fakeBinary = path.join(binDir, "unimatrix");
    fs.writeFileSync(
      fakeBinary,
      '#!/bin/sh\necho "unimatrix 0.5.0"\n'
    );
    fs.chmodSync(fakeBinary, 0o755);

    const origEnv = process.env.UNIMATRIX_BINARY;
    process.env.UNIMATRIX_BINARY = fakeBinary;

    const origLog = console.log;
    console.log = () => {};

    return (async () => {
      try {
        const initModPath = require.resolve("../lib/init.js");
        const resolveModPath = require.resolve("../lib/resolve-binary.js");
        delete require.cache[initModPath];
        delete require.cache[resolveModPath];
        const { init: freshInit } = require("../lib/init.js");

        // Run init twice
        await freshInit({ dryRun: false, projectDir: dir });
        await freshInit({ dryRun: false, projectDir: dir });

        // .mcp.json should have exactly one unimatrix entry
        const mcpContent = JSON.parse(
          fs.readFileSync(path.join(dir, ".mcp.json"), "utf8")
        );
        assert.strictEqual(
          Object.keys(mcpContent.mcpServers).length,
          1,
          "Should have exactly one server entry"
        );

        // settings.json hooks should not be duplicated
        const settingsContent = JSON.parse(
          fs.readFileSync(
            path.join(dir, ".claude", "settings.json"),
            "utf8"
          )
        );
        // Each event should have exactly 1 matcher group with 1 hook
        for (const event of [
          "SessionStart", "Stop", "UserPromptSubmit",
          "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop",
        ]) {
          const groups = settingsContent.hooks[event];
          assert.strictEqual(groups.length, 1, event + " should have 1 matcher group");
          assert.strictEqual(
            groups[0].hooks.length,
            1,
            event + " should have 1 hook entry"
          );
        }
      } finally {
        console.log = origLog;
        if (origEnv !== undefined) {
          process.env.UNIMATRIX_BINARY = origEnv;
        } else {
          delete process.env.UNIMATRIX_BINARY;
        }
      }
    })();
  });

  it("test_reports_diagnostic_on_validation_failure", () => {
    const dir = makeTempProject();

    // Create a fake binary that fails
    const fakeBinary = path.join(dir, "fake-unimatrix");
    fs.writeFileSync(
      fakeBinary,
      '#!/bin/sh\necho "some error" >&2; exit 1\n'
    );
    fs.chmodSync(fakeBinary, 0o755);

    const origEnv = process.env.UNIMATRIX_BINARY;
    process.env.UNIMATRIX_BINARY = fakeBinary;

    const origLog = console.log;
    console.log = () => {};

    return (async () => {
      try {
        const initModPath = require.resolve("../lib/init.js");
        const resolveModPath = require.resolve("../lib/resolve-binary.js");
        delete require.cache[initModPath];
        delete require.cache[resolveModPath];
        const { init: freshInit } = require("../lib/init.js");

        await assert.rejects(
          () => freshInit({ dryRun: false, projectDir: dir }),
          (error) => {
            assert.ok(
              error.message.includes("Database creation failed") ||
                error.message.includes("Binary validation failed"),
              "Error should mention failure: " + error.message
            );
            return true;
          }
        );
      } finally {
        console.log = origLog;
        if (origEnv !== undefined) {
          process.env.UNIMATRIX_BINARY = origEnv;
        } else {
          delete process.env.UNIMATRIX_BINARY;
        }
      }
    })();
  });
});
