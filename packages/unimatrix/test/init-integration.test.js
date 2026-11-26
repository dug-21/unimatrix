"use strict";

const assert = require("assert");
const { linuxLocal } = require("./helpers/linux-local.js");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it, beforeEach, afterEach, mock } = require("node:test");

// Integration tests for copySkills and the full init() flow.
// Core unit tests (detectProjectRoot, per-machine MCP wiring, printSummary) are in
// init.test.js.

const { copySkills, installSkills } = require("../lib/init.js");
const { withShippedSkills } = require("./helpers/shipped-skills");

/** Isolated per-machine home + injected env: init must never touch the real HOME. */
function makeHome() {
  return fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-init-home-")));
}
const seam = (home) => ({ home, requireBinary: false, env: { PATH: process.env.PATH, HOME: home } });

/** Create a temp directory that acts as a project root with .git */
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-init-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

// ── Skill File Copying ──────────────────────────────────────────────

describe("copySkills", () => {
  // Bundled fixture skills are injected via the in-process fs overlay (never
  // written into the real package dir, which parallel test files read).

  it("test_copies_skill_dirs", () => {
    const dir = makeTempProject();
    const testSkills = ["test-skill-a", "test-skill-b"];

    withShippedSkills(
      Object.fromEntries(testSkills.map((n) => [n, { "SKILL.md": "# " + n }])),
      () => {
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

      // ADR-004: install-if-absent emits per-FILE "Installed skill file: <skill>/<file>"
      // (the retired blanket copy reported "Copied skill: <name>").
      assert.ok(
        actions.some((a) => a.includes("Installed skill file: test-skill-a")),
        "Should report installed skill files"
      );
      }
    );
  });

  // ADR-004 replaced the retired blanket-overwrite copySkills with install-if-absent
  // + `--force` (AC-01 non-clobber / AC-02 force-overwrite). Default KEEPS an existing
  // Unimatrix-owned skill byte-for-byte; only `installSkills(dir, {force:true})`
  // overwrites it with the shipped version.
  it("test_default_keeps_existing_skill_force_overwrites", () => {
    const dir = makeTempProject();
    // Pre-create the same skill in the project with edited (older) content
    const targetSkillDir = path.join(dir, ".claude", "skills", "overwrite-test");
    fs.mkdirSync(targetSkillDir, { recursive: true });
    const targetFile = path.join(targetSkillDir, "SKILL.md");
    fs.writeFileSync(targetFile, "OLD CONTENT");

    withShippedSkills({ "overwrite-test": { "SKILL.md": "NEW CONTENT" } }, () => {
      // Default (force:false) — install-if-absent: an existing edited skill SURVIVES
      // byte-for-byte (AC-01, the copySkills-clobber regression).
      const keptActions = installSkills(dir, { force: false, dryRun: false });
      assert.strictEqual(
        fs.readFileSync(targetFile, "utf8"),
        "OLD CONTENT",
        "Default install-if-absent must NOT overwrite an existing skill (AC-01)"
      );
      assert.ok(
        keptActions.some((a) => a.includes("Kept skill file (exists): overwrite-test")),
        "Should report the existing skill file was kept"
      );

      // force:true — overwrite the Unimatrix-owned skill with the shipped version (AC-02).
      const forceActions = installSkills(dir, { force: true, dryRun: false });
      assert.strictEqual(
        fs.readFileSync(targetFile, "utf8"),
        "NEW CONTENT",
        "--force must overwrite the Unimatrix-owned skill (AC-02)"
      );
      assert.ok(
        forceActions.some((a) => a.includes("Overwrote (--force) skill file: overwrite-test")),
        "Should report the --force overwrite"
      );
    });
  });

  it("test_preserves_non_unimatrix_skills", () => {
    const dir = makeTempProject();
    // Pre-create a custom (non-unimatrix) skill in the project
    const customSkillDir = path.join(dir, ".claude", "skills", "custom-skill");
    fs.mkdirSync(customSkillDir, { recursive: true });
    fs.writeFileSync(path.join(customSkillDir, "SKILL.md"), "CUSTOM");

    withShippedSkills({ "bundled-only": { "SKILL.md": "bundled" } }, () => {
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
    });
  });

  it("test_dry_run_does_not_copy_skills", () => {
    const dir = makeTempProject();
    withShippedSkills({ "dryrun-test": { "SKILL.md": "content" } }, () => {
      const actions = copySkills(dir, true);
      const targetDir = path.join(dir, ".claude", "skills", "dryrun-test");
      assert.ok(!fs.existsSync(targetDir), "Should NOT copy in dry-run");
      // ADR-004 install-if-absent dry-run wording (retired: "[dry-run] Would copy skill:").
      assert.ok(
        actions.some((a) => a.includes("[dry-run] Would install skill file: dryrun-test")),
        "Should report planned install actions"
      );
    });
  });
});

// ── Full init() with mocked dependencies ────────────────────────────

describe("init (integration with mocks)", () => {
  // We test the full init flow by setting UNIMATRIX_BINARY to a script
  // that just prints a version string.

  it("test_dry_run_does_not_write_files", linuxLocal, () => {
    const dir = makeTempProject();

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

        const home = makeHome();
        await freshInit(Object.assign({ dryRun: true, projectDir: dir }, seam(home)));

        // Assert no files were created
        const mcpPath = path.join(dir, ".mcp.json");
        assert.ok(
          !fs.existsSync(mcpPath),
          ".mcp.json should NOT be created in dry-run"
        );

        assert.ok(
          !fs.existsSync(path.join(dir, ".claude", "settings.local.json")),
          "settings.local.json should NOT be created in dry-run"
        );
        assert.ok(!fs.existsSync(path.join(home, ".claude.json")), "~/.claude.json untouched");
        assert.ok(!fs.existsSync(path.join(home, ".unimatrix")), "no user-level store written");

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

  it("test_full_init_creates_mcp_and_settings", linuxLocal, () => {
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

        const home = makeHome();
        await freshInit(Object.assign({ dryRun: false, projectDir: dir }, seam(home)));

        // Per-machine MCP entry in ~/.claude.json; no committed project .mcp.json.
        assert.ok(!fs.existsSync(path.join(dir, ".mcp.json")), "no project .mcp.json");
        const cj = JSON.parse(fs.readFileSync(path.join(home, ".claude.json"), "utf8"));
        const entry = cj.projects[fs.realpathSync(dir)].mcpServers.unimatrix;
        assert.ok(JSON.stringify(entry).includes(".unimatrix/bin/unimatrix"), "shim entry");
        assert.ok(fs.existsSync(path.join(home, ".unimatrix", "bin", "unimatrix")), "shim installed");

        // .claude/settings.local.json holds the hook entries (shim commands)
        const settingsPath = path.join(dir, ".claude", "settings.local.json");
        assert.ok(fs.existsSync(settingsPath), "settings.local.json should be created");
        const settingsContent = JSON.parse(
          fs.readFileSync(settingsPath, "utf8")
        );
        assert.ok(settingsContent.hooks, "Should have hooks key");
        assert.ok(
          settingsContent.hooks.SessionStart,
          "Should have SessionStart hook"
        );
        assert.ok(
          settingsContent.hooks.SessionStart[0].hooks[0].command.includes(path.join(home, ".unimatrix", "bin", "unimatrix")),
          "hook command invokes the per-machine shim by absolute path"
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

  it("test_init_idempotent", linuxLocal, () => {
    const dir = makeTempProject();

    // vnc-027 ADR-004 §2: SubagentStop is opt-in. This dedup check covers the
    // full registered set, so opt in via settings.local.json.
    const claudeDir = path.join(dir, ".claude");
    fs.mkdirSync(claudeDir, { recursive: true });
    fs.writeFileSync(
      path.join(claudeDir, "settings.local.json"),
      JSON.stringify({ unimatrix: { hooks: { subagent_stop: true } } }, null, 2),
      "utf8"
    );

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
        const home = makeHome();
        await freshInit(Object.assign({ dryRun: false, projectDir: dir }, seam(home)));
        await freshInit(Object.assign({ dryRun: false, projectDir: dir }, seam(home)));

        // ~/.claude.json should have exactly one unimatrix entry for the project
        const cj = JSON.parse(fs.readFileSync(path.join(home, ".claude.json"), "utf8"));
        assert.strictEqual(
          Object.keys(cj.projects[fs.realpathSync(dir)].mcpServers).length,
          1,
          "Should have exactly one server entry"
        );

        // settings.local.json hooks should not be duplicated
        const settingsContent = JSON.parse(
          fs.readFileSync(
            path.join(dir, ".claude", "settings.local.json"),
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

  it("test_reports_diagnostic_on_validation_failure", linuxLocal, () => {
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
          () => freshInit(Object.assign({ dryRun: false, projectDir: dir }, seam(makeHome()))),
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

describe("init (nan-024 wiring)", () => {
  function withFakeBinary(dir, fn) {
    const fakeBinary = path.join(dir, "fake-unimatrix");
    fs.writeFileSync(fakeBinary, '#!/bin/sh\necho "unimatrix 0.5.0"\n');
    fs.chmodSync(fakeBinary, 0o755);
    const orig = process.env.UNIMATRIX_BINARY;
    process.env.UNIMATRIX_BINARY = fakeBinary;
    const logs = [];
    const origLog = console.log;
    console.log = (...a) => logs.push(a.join(" "));
    return (async () => {
      try {
        delete require.cache[require.resolve("../lib/init.js")];
        delete require.cache[require.resolve("../lib/resolve-binary.js")];
        return await fn(require("../lib/init.js"), logs);
      } finally {
        console.log = origLog;
        if (orig !== undefined) process.env.UNIMATRIX_BINARY = orig;
        else delete process.env.UNIMATRIX_BINARY;
      }
    })();
  }

  it("test_init_prints_codex_approval_step_and_no_trust_hashes", linuxLocal, () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"));
    const home = makeHome();
    return withFakeBinary(dir, async ({ init }, logs) => {
      await init(Object.assign({ dryRun: false, projectDir: dir }, seam(home)));
      const out = logs.join("\n");
      assert.ok(out.includes("run /hooks in Codex and approve"), "approval step printed");
      assert.ok(out.includes(path.join(home, ".codex", "hooks.json")), "prints the Codex path actually written");
      assert.ok(!/hooks\.state|trusted_hash|sha256:/i.test(fs.readFileSync(path.join(home, ".codex", "hooks.json"), "utf8")));
      assert.ok(!fs.existsSync(path.join(dir, ".codex", "hooks.json")), "no project-level Codex hooks");
    });
  });

  it("test_init_local_refused_off_linux", () => {
    const dir = makeTempProject();
    return withFakeBinary(dir, async ({ init }) => {
      await assert.rejects(
        () => init(Object.assign({ dryRun: false, projectDir: dir, platform: "darwin" }, seam(makeHome()))),
        /local init requires Linux/
      );
      assert.ok(!fs.existsSync(path.join(dir, ".claude")), "nothing written");
    });
  });

  it("test_init_ignores_decoy_home_env", linuxLocal, () => {
    const dir = makeTempProject();
    const home = makeHome();
    const decoy = makeHome();
    return withFakeBinary(dir, async ({ init }) => {
      await init({ dryRun: false, projectDir: dir, home, requireBinary: false, env: { PATH: process.env.PATH, HOME: decoy } });
      assert.deepStrictEqual(fs.readdirSync(decoy), [], "nothing written under the decoy HOME");
      assert.ok(fs.existsSync(path.join(home, ".unimatrix")));
    });
  });
});
