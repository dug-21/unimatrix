"use strict";

const assert = require("assert");
const { linuxLocal } = require("./helpers/linux-local.js");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it, beforeEach, afterEach, mock } = require("node:test");
const { assertTmpHome } = require("./helpers/home-guard.js");

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

// ── 1036: spawned `init` (process boundary, real git, real harness binaries) ──
describe("init 1036 spawned acceptance", () => {
  const { spawnSync } = require("child_process");
  const { makeGitProject, gitEnv, writeJsonAt, readJson } = require("./helpers/wire-fixture.js");
  const { buildMcpArgv } = require("../lib/mcp-entry.js");
  const BIN = require("./helpers/wire-fixture.js").stagedCli();
  const tmp = (p) => fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), p || "unimatrix-1036-")));
  const onPath = (b) => spawnSync("sh", ["-c", "command -v " + b], { encoding: "utf8" }).status === 0;
  const fakeBin = () => {
    const f = path.join(tmp("unimatrix-fakebin-"), "unimatrix");
    fs.writeFileSync(f, '#!/bin/sh\necho "unimatrix 0.5.0"\n');
    fs.chmodSync(f, 0o755);
    return f;
  };
  function runInit(dir, home, extraEnv, args) {
    const env = gitEnv(Object.assign({ HOME: home, UNIMATRIX_BINARY: fakeBin() }, extraEnv || {}));
    const r = spawnSync(process.execPath, [BIN, "init"].concat(args || []), { cwd: dir, env, encoding: "utf8", timeout: 60000 });
    return { status: r.status, stdout: r.stdout || "", stderr: r.stderr || "" };
  }
  const legLines = (out) => out.split("\n").map((l) => l.trim()).filter((l) => /^(\[dry-run\] )?(unimatrix|claude-code|codex-cli|opencode) [a-z-]+: /.test(l));
  const git = (dir, args) => spawnSync("git", args, { cwd: dir, env: gitEnv(), encoding: "utf8" });
  function treeState(roots) {
    const out = {};
    const walk = (d) => {
      let es = [];
      try { es = fs.readdirSync(d, { withFileTypes: true }); } catch (_) { return; }
      for (const e of es) {
        const p = path.join(d, e.name);
        if (e.isSymbolicLink()) out[p] = "link:" + fs.readlinkSync(p);
        else if (e.isDirectory()) { if (e.name !== "objects") walk(p); out[p + "/"] = "dir"; }
        else { const s = fs.statSync(p); out[p] = s.mtimeMs + ":" + s.mode + ":" + fs.readFileSync(p).toString("base64"); }
      }
    };
    for (const r of roots) walk(r);
    return out;
  }

  it("test_ac1c_real_claude_mcp_get_reads_resolved_entry", linuxLocal, (t) => {
    if (!onPath("claude")) return t.skip("claude not on PATH; real-binary check tracked in #1053");
    const dir = makeGitProject();
    const home = makeHome();
    const D = tmp();
    const def = path.join(home, ".claude.json");
    writeJsonAt(def, { projects: { [dir]: { mcpServers: { unimatrix: { command: "sh", args: ["-c", "old"], env: {} } } } } });
    const before = fs.readFileSync(def);
    const r = runInit(dir, home, { CLAUDE_CONFIG_DIR: D });
    assert.strictEqual(r.status, 0, r.stderr);
    const g = spawnSync("claude", ["mcp", "get", "unimatrix"], {
      cwd: dir, encoding: "utf8", timeout: 30000, env: gitEnv({ HOME: home, CLAUDE_CONFIG_DIR: D }),
    });
    assert.strictEqual(g.status, 0, g.stdout + g.stderr);
    assert.ok(/unimatrix/.test(g.stdout), g.stdout);
    assert.ok(g.stdout.includes(".unimatrix/bin/unimatrix"), "reads the entry init wrote: " + g.stdout);
    assert.ok(fs.readFileSync(def).equals(before), "default profile kept (D-028)");
  });

  function runOpencodeDebugConfig(dir, home, env) {
    const argv = ["debug", "config", "--standalone"];
    assert.ok(argv.includes("--standalone"), "--standalone is mandatory");
    const r = spawnSync("opencode", argv, { cwd: dir, encoding: "utf8", timeout: 20000, env: gitEnv(Object.assign({ HOME: home }, env)) });
    let docs;
    try { docs = JSON.parse(r.stdout); } catch (e) { assert.fail("opencode debug config output not JSON: " + r.stdout + r.stderr); }
    return docs;
  }
  const ocEntry = (doc) => doc && ((doc.mcp && doc.mcp.servers && doc.mcp.servers.unimatrix) || (doc.mcp && doc.mcp.unimatrix));

  for (const [name, mkEnv, resolved] of [
    ["opencode_config_dir", (t) => ({ OPENCODE_CONFIG_DIR: t }), (t) => path.join(t, "opencode.json")],
    ["xdg_only", (t) => ({ XDG_CONFIG_HOME: t }), (t) => path.join(t, "opencode", "opencode.json")],
    ["both_set_precedence", (t, x) => ({ OPENCODE_CONFIG_DIR: t, XDG_CONFIG_HOME: x }), (t) => path.join(t, "opencode.json")],
  ]) {
    it("test_ac2b_real_opencode_debug_config_" + name, linuxLocal, (t) => {
      if (!onPath("opencode")) return t.skip("opencode not on PATH; real-binary check tracked in #1053");
      const dir = makeGitProject();
      fs.mkdirSync(path.join(dir, ".opencode"));
      const home = makeHome();
      const O = tmp();
      const X = tmp();
      const twin = path.join(home, ".config", "opencode", "opencode.json");
      writeJsonAt(twin, { mcp: { unimatrix: { type: "local", command: buildMcpArgv("linux", home) } } });
      const env = mkEnv(O, X);
      const r = runInit(dir, home, env);
      assert.strictEqual(r.status, 0, r.stderr);
      const file = resolved(O);
      const written = readJson(file).mcp.unimatrix;
      const docs = runOpencodeDebugConfig(dir, home, env);
      const body = (d) => d.info || d.content || d;
      const doc = docs.find((d) => d.path === file && d.type !== "directory");
      assert.ok(doc, "resolved file loaded by opencode: " + JSON.stringify(docs));
      assert.deepStrictEqual(ocEntry(body(doc)), written);
      const twinDoc = docs.find((d) => d.path === twin && d.type !== "directory");
      assert.ok(!twinDoc || !ocEntry(body(twinDoc)), "no owned entry at default location");
      assert.ok(!(readJson(twin).mcp || {}).unimatrix, "owned twin removed");
      if (env.XDG_CONFIG_HOME && env.OPENCODE_CONFIG_DIR) {
        assert.ok(!fs.existsSync(path.join(X, "opencode", "opencode.json")), "XDG location untouched when OPENCODE_CONFIG_DIR wins");
      }
    });
  }

  it("test_ac8_spawned_init_appends_exclude_and_rerun_adds_nothing", linuxLocal, () => {
    const dir = makeGitProject();
    const home = makeHome();
    const ex = path.join(dir, ".git", "info", "exclude");
    const dry = runInit(dir, home, {}, ["--dry-run"]);
    assert.strictEqual(dry.status, 0, dry.stderr);
    assert.ok(/\[dry-run\] claude-code git-exclude: (created|updated) /.test(dry.stdout), dry.stdout);
    assert.ok(!(fs.existsSync(ex) && fs.readFileSync(ex, "utf8").includes("/.claude/settings.local.json")), "dry-run wrote nothing");
    const r = runInit(dir, home);
    assert.strictEqual(r.status, 0, r.stderr);
    const lines = fs.readFileSync(ex, "utf8").split("\n").filter((l) => l === "/.claude/settings.local.json");
    assert.strictEqual(lines.length, 1);
    assert.ok(!git(dir, ["status", "--porcelain"]).stdout.includes("settings.local.json"), "file ignored");
    assert.strictEqual(git(dir, ["diff", "--exit-code"]).status, 0);
    assert.strictEqual(git(dir, ["diff", "--cached", "--exit-code"]).status, 0);
    const exBefore = fs.statSync(ex).mtimeMs;
    const r2 = runInit(dir, home);
    assert.ok(/claude-code git-exclude: unchanged /.test(r2.stdout), r2.stdout);
    assert.strictEqual(fs.statSync(ex).mtimeMs, exBefore);
  });

  it("test_ac8_spawned_init_outside_repo_skips_and_reports", linuxLocal, () => {
    const dir = tmp("unimatrix-norepo-");
    fs.mkdirSync(path.join(dir, ".git"));
    const r = runInit(dir, makeHome(), {}, ["--project-dir", dir]);
    assert.strictEqual(r.status, 0, r.stderr);
    assert.ok(/claude-code git-exclude: skipped-not-repo /.test(r.stdout), r.stdout);
  });

  it("test_ac5_spawned_double_init_all_unchanged_no_mtime_no_notice", linuxLocal, () => {
    for (const extra of [{}, { CLAUDE_CONFIG_DIR: tmp() }]) {
      const dir = makeGitProject();
      const home = makeHome();
      const r1 = runInit(dir, home, extra);
      assert.strictEqual(r1.status, 0, r1.stderr);
      assert.strictEqual(r1.stdout.split("If Claude Code was running").length - 1, 1, "D-013 notice once on run 1");
      const roots = [dir, home].concat(extra.CLAUDE_CONFIG_DIR ? [extra.CLAUDE_CONFIG_DIR] : []);
      const before = treeState(roots);
      const r2 = runInit(dir, home, extra);
      assert.strictEqual(r2.status, 0, r2.stderr);
      for (const l of legLines(r2.stdout)) assert.ok(/: (unchanged|skipped[a-z-]*) /.test(l), l);
      assert.ok(!r2.stdout.includes("If Claude Code was running"), "no notice on run 2");
      const after = treeState(roots);
      const diff = Object.keys(after).filter((k) => after[k] !== before[k] && !k.includes(path.sep + ".git" + path.sep) && !/\.db(-wal|-shm)?$/.test(k));
      assert.deepStrictEqual(diff, [], "no file touched on run 2");
    }
  });

  it("test_ac9_spawned_dry_run_lines_match_real_and_write_nothing", linuxLocal, () => {
    const dir = makeGitProject();
    const home = makeHome();
    const D = tmp();
    writeJsonAt(path.join(dir, ".mcp.json"), { mcpServers: { other: { command: "x" } } });
    const roots = [dir, home, D];
    const before = treeState(roots);
    const dry = runInit(dir, home, { CLAUDE_CONFIG_DIR: D }, ["--dry-run"]);
    assert.strictEqual(dry.status, 0, dry.stderr);
    assert.deepStrictEqual(treeState(roots), before, "dry-run writes nothing (FR-62)");
    const real = runInit(dir, home, { CLAUDE_CONFIG_DIR: D });
    assert.strictEqual(real.status, 0, real.stderr);
    const strip = (out) => out.split("\n").map((l) => l.trim())
      .filter((l) => /^(\[dry-run\] )?(resolved:|note:|If Claude Code|(unimatrix|claude-code|codex-cli|opencode) [a-z-]+: )/.test(l))
      .map((l) => l.replace(/^\[dry-run\] /, ""));
    assert.deepStrictEqual(strip(dry.stdout), strip(real.stdout));
    assert.ok(strip(real.stdout).some((l) => l.startsWith("resolved: claude-code mcp -> " + path.join(D, ".claude.json") + " (CLAUDE_CONFIG_DIR=")));
  });

  it("test_fr53_unparseable_resolved_claude_json_exits_1_naming_file", linuxLocal, () => {
    const dir = makeGitProject();
    const home = makeHome();
    const D = tmp();
    const f = path.join(D, ".claude.json");
    fs.writeFileSync(f, "{ nope");
    const r = runInit(dir, home, { CLAUDE_CONFIG_DIR: D });
    assert.strictEqual(r.status, 1);
    assert.ok(r.stderr.includes(f), r.stderr);
    assert.strictEqual(fs.readFileSync(f, "utf8"), "{ nope");
  });

  it("test_fr53_invalid_override_warns_exit_0", linuxLocal, () => {
    const dir = makeGitProject();
    const home = makeHome();
    // invalid value passed directly: scrubbedEnv only accepts tmp paths for overrides
    const env = Object.assign(gitEnv({ HOME: home, UNIMATRIX_BINARY: fakeBin() }), { CLAUDE_CONFIG_DIR: "relative/cfg" });
    const r = spawnSync(process.execPath, [BIN, "init"], { cwd: dir, env, encoding: "utf8", timeout: 60000 });
    assert.strictEqual(r.status, 0, r.stderr);
    assert.ok(r.stderr.includes("CLAUDE_CONFIG_DIR"), r.stderr);
    assert.ok(r.stdout.includes("resolved: claude-code mcp -> " + path.join(home, ".claude.json") + " (default)"), r.stdout);
  });

  // ── AC-5 / AC-8 / AC-9 extended: twins, bundle, mode drift, FR-42 rows, dry-run parity ──
  const bin = fakeBin();
  const ri = (dir, home, extra, args) => runInit(dir, assertTmpHome(home), Object.assign({ UNIMATRIX_BINARY: bin }, extra || {}), args);
  const exclOf = (dir) => path.resolve(dir, git(dir, ["rev-parse", "--git-path", "info/exclude"]).stdout.trim());
  const ENTRY = "/.claude/settings.local.json";
  const seedOwnedTwins = (home, dir) => {
    writeJsonAt(path.join(home, ".claude", "settings.json"), { hooks: { Stop: [{ hooks: [{ type: "command", command: "unimatrix hook Stop" }] }] } });
    writeJsonAt(path.join(home, ".config", "opencode", "opencode.json"), { mcp: { unimatrix: { type: "local", command: buildMcpArgv("linux", home) } } });
    writeJsonAt(path.join(home, ".claude.json"), { projects: { [dir]: { mcpServers: { unimatrix: { command: "sh", args: ["-c", "old"], env: {} } } } } });
  };
  const changedKeys = (a, b) => Object.keys(Object.assign({}, a, b))
    .filter((k) => a[k] !== b[k] && !/\.db(-wal|-shm)?$/.test(k) && !k.endsWith(path.join(".git", "index")));
  const assertNoChange = (a, b) => assert.deepStrictEqual(changedKeys(a, b), []);
  const manifestOf = (out) => out.split("\n").map((l) => l.trim())
    .filter((l) => /^(\[dry-run\] )?(resolved:|note:|If Claude Code|(unimatrix|claude-code|codex-cli|opencode) [a-z-]+: )/.test(l))
    .map((l) => l.replace(/^\[dry-run\] /, ""));

  it("test_ac5_run_twice_overrides_and_twins_all_unchanged", linuxLocal, () => {
    const variants = [{}, { CLAUDE_CONFIG_DIR: tmp() }, { OPENCODE_CONFIG_DIR: tmp(), XDG_CONFIG_HOME: tmp() }];
    for (const extra of variants) {
      const dir = makeGitProject();
      const home = makeHome();
      seedOwnedTwins(home, dir);
      const r1 = ri(dir, home, extra);
      assert.strictEqual(r1.status, 0, r1.stderr);
      const roots = [dir, home].concat(Object.values(extra));
      const before = treeState(roots);
      const r2 = ri(dir, home, extra);
      assert.strictEqual(r2.status, 0, r2.stderr);
      for (const l of legLines(r2.stdout)) assert.ok(/: (unchanged|skipped[a-z-]*) /.test(l), l);
      assert.ok(!r2.stdout.includes("If Claude Code was running"), "no notice on run 2");
      assertNoChange(before, treeState(roots));
    }
  });

  it("test_ac5_bundle_run_twice_all_unchanged", linuxLocal, async (t) => {
    const H = require("./helpers/e2e-harness.js");
    const cert = H.loadCert();
    if (!cert) return t.skip("no TLS cert material (openssl); bundle run needs a live stub");
    const stub = await H.startStub(cert);
    try {
      for (const overrideDir of [null, tmp()]) {
        const dir = makeGitProject();
        const home = makeHome();
        seedOwnedTwins(home, dir);
        const extra = overrideDir ? { CLAUDE_CONFIG_DIR: overrideDir } : {};
        const env = gitEnv(Object.assign({ HOME: home, UNIMATRIX_BINARY: bin, NODE_EXTRA_CA_CERTS: cert.certPath }, extra));
        const argv = [BIN, "init", "--bundle", H.bundleFor(stub, cert)];
        const r1 = await H.run(process.execPath, argv, { spawn: { cwd: dir, env }, timeoutMs: 120000 });
        assert.strictEqual(r1.code, 0, r1.out + r1.err);
        const roots = [dir, home].concat(overrideDir ? [overrideDir] : []);
        const before = treeState(roots);
        const r2 = await H.run(process.execPath, argv, { spawn: { cwd: dir, env }, timeoutMs: 120000 });
        assert.strictEqual(r2.code, 0, r2.out + r2.err);
        for (const l of legLines(r2.out)) assert.ok(/: (unchanged|skipped[a-z-]*) /.test(l), l);
        assert.ok(!r2.out.includes("If Claude Code was running"), "no notice on run 2");
        assertNoChange(before, treeState(roots));
      }
    } finally {
      await stub.close();
    }
  });

  it("test_ac5_mode_drift_repaired_then_converges_unchanged", linuxLocal, () => {
    const dir = makeGitProject();
    const home = makeHome();
    const r1 = ri(dir, home);
    assert.strictEqual(r1.status, 0, r1.stderr);
    const inst = path.join(home, ".unimatrix", "install.json");
    fs.chmodSync(inst, 0o644);
    const r2 = ri(dir, home);
    assert.strictEqual(r2.status, 0, r2.stderr);
    assert.ok(/unimatrix store: updated .*install\.json/.test(r2.stdout), r2.stdout);
    assert.strictEqual(fs.statSync(inst).mode & 0o777, 0o600);
    const before = treeState([dir, home]);
    const r3 = ri(dir, home);
    assert.strictEqual(r3.status, 0, r3.stderr);
    for (const l of legLines(r3.stdout)) assert.ok(/: (unchanged|skipped[a-z-]*) /.test(l), l);
    assertNoChange(before, treeState([dir, home]));
  });

  // FR-42 fixtures shared by the AC-8 row assertions and the AC-9 parity harness.
  const FIXTURES = [
    { name: "fresh-repo", make: () => ({ dir: makeGitProject(), home: makeHome(), expect: /claude-code git-exclude: (created|updated) / }) },
    { name: "ac1-claude-config-dir-twin", make: () => {
      const dir = makeGitProject(); const home = makeHome(); const D = tmp();
      seedOwnedTwins(home, dir);
      return { dir, home, extra: { CLAUDE_CONFIG_DIR: D }, roots: [D], expect: /claude-code git-exclude: (created|updated) / };
    } },
    { name: "ac2-opencode-xdg-twin", make: () => {
      const dir = makeGitProject(); const home = makeHome(); const X = tmp();
      seedOwnedTwins(home, dir);
      return { dir, home, extra: { XDG_CONFIG_HOME: X }, roots: [X], expect: /claude-code git-exclude: (created|updated) / };
    } },
    { name: "ac3-outside-root-link", exitOnly: true, make: () => {
      const dir = makeGitProject(); const home = makeHome(); const out = tmp("unimatrix-outside-");
      writeJsonAt(path.join(out, "settings.local.json"), { victim: true });
      fs.symlinkSync(out, path.join(dir, ".claude"));
      return { dir, home, roots: [out] };
    } },
    { name: "ac8-gitignored-gitignore", make: () => {
      const dir = makeGitProject(); fs.writeFileSync(path.join(dir, ".gitignore"), ".claude/\n");
      return { dir, home: makeHome(), expect: /claude-code git-exclude: unchanged / };
    } },
    { name: "ac8-core-excludesfile", make: () => {
      const dir = makeGitProject(); const cfg = path.join(tmp(), "global-ignore");
      fs.writeFileSync(cfg, "settings.local.json\n");
      git(dir, ["config", "core.excludesFile", cfg]);
      return { dir, home: makeHome(), expect: /claude-code git-exclude: unchanged / };
    } },
    { name: "ac8-existing-exclude-entry", make: () => {
      const dir = makeGitProject(); fs.writeFileSync(exclOf(dir), ENTRY + "\n");
      return { dir, home: makeHome(), expect: /claude-code git-exclude: unchanged / };
    } },
    { name: "ac8-tracked", gitClean: false, make: () => {
      const dir = makeGitProject(); fs.mkdirSync(path.join(dir, ".claude"));
      fs.writeFileSync(path.join(dir, ".claude", "settings.local.json"), "{}\n");
      git(dir, ["add", "-f", ".claude/settings.local.json"]);
      git(dir, ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "c"]);
      const ex = exclOf(dir);
      return { dir, home: makeHome(), expect: /claude-code git-exclude: skipped-tracked /,
        check: (s) => {
          assert.ok(!fs.existsSync(ex) || !fs.readFileSync(ex, "utf8").includes(ENTRY), "exclude not appended for tracked file");
          assert.strictEqual(git(s.dir, ["ls-files", "--stage"]).status, 0);
        } };
    } },
    { name: "ac8-bang-override", make: () => {
      const dir = makeGitProject(); fs.writeFileSync(path.join(dir, ".gitignore"), "!" + ENTRY + "\n");
      return { dir, home: makeHome(), dryExpect: /claude-code git-exclude: updated /, expect: /appended-not-effective /,
        check: (s) => {
          const again = ri(s.dir, s.home, s.extra);
          assert.strictEqual(again.status, 0, again.stderr);
          assert.strictEqual(fs.readFileSync(exclOf(s.dir), "utf8").split("\n").filter((l) => l === ENTRY).length, 1);
        } };
    } },
    { name: "ac8-path-without-git", make: () => {
      const dir = makeGitProject(); const emptyPath = tmp("unimatrix-nogit-");
      return { dir, home: makeHome(), extra: { PATH: emptyPath }, expect: /claude-code git-exclude: skipped-no-git / };
    } },
    { name: "ac8-linked-worktree", make: () => {
      const dir = makeGitProject(); git(dir, ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "c"]);
      const wt = path.join(tmp(), "wt");
      git(dir, ["worktree", "add", "-q", wt]);
      return { dir: wt, home: makeHome(), roots: [dir], args: ["--project-dir", wt], expect: /claude-code git-exclude: (created|updated) /,
        check: (s) => assert.ok(fs.readFileSync(exclOf(s.dir), "utf8").split("\n").includes(ENTRY)) };
    } },
    { name: "ac8-submodule", make: () => {
      const sub = makeGitProject(); git(sub, ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "c"]);
      const sup = makeGitProject(); git(sup, ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "c"]);
      git(sup, ["-c", "protocol.file.allow=always", "submodule", "add", "-q", sub, "mod"]);
      const dir = path.join(sup, "mod");
      return { dir, home: makeHome(), roots: [sup], args: ["--project-dir", dir], expect: /claude-code git-exclude: (created|updated) /,
        check: (s) => assert.ok(fs.readFileSync(path.join(sup, ".git", "modules", "mod", "info", "exclude"), "utf8").includes(ENTRY)) };
    } },
    { name: "ac8-subdir-escaped-entry", make: () => {
      const dir = makeGitProject(); const sub = path.join(dir, "x", "a*b[1]!");
      fs.mkdirSync(sub, { recursive: true });
      return { dir: sub, home: makeHome(), roots: [dir], args: ["--project-dir", sub], expect: /claude-code git-exclude: (created|updated) /,
        check: (s) => assert.ok(fs.readFileSync(exclOf(dir), "utf8").split("\n").includes("/x/a\\*b\\[1]\\!" + ENTRY)) };
    } },
    { name: "ac8-missing-info-dir", make: () => {
      const dir = makeGitProject(); fs.rmSync(path.join(dir, ".git", "info"), { recursive: true, force: true });
      return { dir, home: makeHome(), expect: /claude-code git-exclude: created / };
    } },
    { name: "ac8-symlinked-exclude", make: () => {
      const dir = makeGitProject(); const tgtDir = tmp(); const tgt = path.join(tgtDir, "real-exclude");
      fs.writeFileSync(tgt, "*.o\n");
      fs.rmSync(exclOf(dir), { force: true }); fs.symlinkSync(tgt, exclOf(dir));
      return { dir, home: makeHome(), roots: [tgtDir], expect: /claude-code git-exclude: (created|updated) /,
        check: () => { assert.ok(fs.lstatSync(exclOf(dir)).isSymbolicLink()); assert.ok(fs.readFileSync(tgt, "utf8").includes(ENTRY)); } };
    } },
    { name: "ac8-git-dir-env-ignored", make: () => {
      const dir = makeGitProject(); const other = makeGitProject();
      return { dir, home: makeHome(), extra: { GIT_DIR: path.join(other, ".git") }, roots: [other], expect: /claude-code git-exclude: (created|updated) / };
    } },
    { name: "ac9-mode-drift", make: () => {
      const dir = makeGitProject(); const home = makeHome();
      const pre = ri(dir, home); assert.strictEqual(pre.status, 0, pre.stderr);
      fs.chmodSync(path.join(home, ".unimatrix", "install.json"), 0o644);
      return { dir, home, expect: /unimatrix store: updated .*install\.json/ };
    } },
  ];

  it("test_ac8_fr42_fixtures_expected_action_and_clean_diff", linuxLocal, () => {
    for (const fx of FIXTURES.filter((f) => !f.exitOnly && !/^ac9-/.test(f.name))) {
      const s = fx.make();
      const dry = ri(s.dir, s.home, s.extra, ["--dry-run"].concat(s.args || []));
      assert.strictEqual(dry.status, 0, fx.name + ": " + dry.stderr);
      if (s.dryExpect || s.expect) assert.match(dry.stdout, s.dryExpect || s.expect, fx.name + " dry-run");
      const real = ri(s.dir, s.home, s.extra, s.args || []);
      assert.strictEqual(real.status, 0, fx.name + ": " + real.stderr);
      if (s.expect) assert.match(real.stdout, s.expect, fx.name + " real");
      if (fx.gitClean !== false) {
        assert.strictEqual(git(s.dir, ["diff", "--exit-code"]).status, 0, fx.name + ": git diff");
        assert.strictEqual(git(s.dir, ["diff", "--cached", "--exit-code"]).status, 0, fx.name + ": git diff --cached");
      }
      if (s.check) s.check(s);
    }
  });

  it("test_ac9_parity_dry_run_matches_real_over_every_fixture", linuxLocal, () => {
    for (const fx of FIXTURES) {
      const s = fx.make();
      const roots = [s.dir, s.home].concat(s.roots || []);
      const before = treeState(roots);
      const outBefore = treeState(s.roots || []);
      const dry = ri(s.dir, s.home, s.extra, ["--dry-run"].concat(s.args || []));
      assert.deepStrictEqual(treeState(roots), before, fx.name + ": dry-run wrote nothing (tree, modes, mtimes)");
      const real = ri(s.dir, s.home, s.extra, s.args || []);
      if (fx.exitOnly) {
        assert.strictEqual(dry.status, 1, fx.name + " dry-run must exit 1");
        assert.strictEqual(real.status, 1, fx.name + " real must exit 1");
        assert.ok(/outside project root/.test(dry.stderr) && /outside project root/.test(real.stderr), fx.name);
        assert.deepStrictEqual(treeState(s.roots), outBefore, fx.name + ": outside target untouched");
        continue;
      }
      assert.strictEqual(dry.status, 0, fx.name + ": " + dry.stderr);
      assert.strictEqual(real.status, 0, fx.name + ": " + real.stderr);
      // FR-42 step 7 is the one allowed dry/real difference: dry-run cannot yet see the effective check.
      const norm = (out) => manifestOf(out).map((l) => l.replace(/git-exclude: (appended-not-effective|updated) (\S+).*$/, "git-exclude: WRITE $2"));
      assert.deepStrictEqual(norm(real.stdout), norm(dry.stdout), fx.name + ": manifest parity");
      const after = treeState(roots);
      const diffKeys = Object.keys(Object.assign({}, before, after))
        .filter((k) => before[k] !== after[k] && !/\.db(-wal|-shm)?$/.test(k) && !k.endsWith(path.join(".git", "index")));
      const changed = diffKeys.filter((k) => !k.endsWith("/"));
      const planned = dry.stdout.split("\n")
        .map((l) => /: (created|updated|removed) (\S+)/.exec(l) || /skill file: (\S+)/.exec(l))
        .filter(Boolean)
        .map((m) => (m[0].includes("skill file") ? path.join(s.dir, ".claude", "skills", m[1]) : path.resolve(s.dir, m[2])));
      // A planned path that is a symlink is written through: its real target is the changed file.
      // A planned symlink path is written through: its real target is the changed file.
      const linkTarget = (p) => { try { return fs.realpathSync(p); } catch (_e) { return null; } };
      for (const k of changed) assert.ok(planned.some((p) => k === p || k.startsWith(p + path.sep) || k === linkTarget(p)), fx.name + ": unplanned change " + k);
      for (const p of planned) assert.ok(diffKeys.some((k) => k === p || k === p + path.sep || k.startsWith(p + path.sep) || k === linkTarget(p)), fx.name + ": planned but unchanged " + p);
    }
  });
});
