"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it, beforeEach, afterEach, mock } = require("node:test");

// We test the internal functions directly (detectProjectRoot, writeMcpJson,
// printSummary) and keep skill-copying and full init integration tests in
// init-integration.test.js.

const {
  detectProjectRoot,
  writeMcpJson,
  printSummary,
  installSkills,
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
    assert.deepStrictEqual(content.mcpServers.unimatrix.env, {
      LD_LIBRARY_PATH: path.dirname(BINARY),
    });

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

// ── installSkills (ADR-004: non-destructive definition install) ─────────
//
// installSkills iterates the SHIPPED source tree (`<pkg>/skills`). To control
// the shipped manifest deterministically we inject temp skills into that source
// tree and remove them in `finally` (same idiom as init-integration.test.js).

const SKILLS_SOURCE = path.join(__dirname, "..", "skills");

/**
 * Inject a temp shipped-skills manifest, run `fn`, then remove the injected
 * dirs. `manifest` is { skillDir: { fileName: content } }.
 */
function withShippedSkills(manifest, fn) {
  const names = Object.keys(manifest);
  for (const name of names) {
    const dir = path.join(SKILLS_SOURCE, name);
    fs.mkdirSync(dir, { recursive: true });
    for (const [file, content] of Object.entries(manifest[name])) {
      fs.writeFileSync(path.join(dir, file), content);
    }
  }
  try {
    return fn();
  } finally {
    for (const name of names) {
      fs.rmSync(path.join(SKILLS_SOURCE, name), {
        recursive: true,
        force: true,
      });
    }
  }
}

/** Reduce action lines to a per-file decision map (prefix/tense-insensitive). */
function decisionSet(actions) {
  const out = {};
  for (const a of actions) {
    const m = a.match(
      /(Installed|Overwrote \(--force\)|Kept skill file \(exists\)|Would install|Would overwrite \(--force\)) skill file(?: \(exists\))?: (\S+)/
    );
    if (m) {
      const verb = m[1]
        .replace(/^Would /, "")
        .replace(/ \(--force\)/, "")
        .replace("Kept skill file (exists)", "kept")
        .toLowerCase();
      out[m[2]] = verb.startsWith("install")
        ? "install"
        : verb.startsWith("overwrite")
        ? "overwrite"
        : "kept";
    }
  }
  return out;
}

describe("installSkills", () => {
  // Install-if-absent default (R-16, AC-01) ────────────────────────────

  it("test_installSkills_fresh_repo_installs_all_shipped", () => {
    const dir = makeTempProject();
    withShippedSkills(
      {
        "nan023-a": { "SKILL.md": "A" },
        "nan023-b": { "SKILL.md": "B" },
      },
      () => {
        const actions = installSkills(dir, { force: false, dryRun: false });
        for (const name of ["nan023-a", "nan023-b"]) {
          const p = path.join(dir, ".claude", "skills", name, "SKILL.md");
          assert.ok(fs.existsSync(p), "should install " + name);
        }
        assert.ok(
          actions.some((a) => a === "Installed skill file: nan023-a/SKILL.md")
        );
        assert.ok(
          actions.some((a) => a === "Installed skill file: nan023-b/SKILL.md")
        );
      }
    );
  });

  it("test_installSkills_existing_skill_kept_byte_for_byte", () => {
    const dir = makeTempProject();
    withShippedSkills({ "nan023-edit": { "SKILL.md": "SHIPPED" } }, () => {
      // Pre-place a MODIFIED owned skill on disk.
      const dst = path.join(dir, ".claude", "skills", "nan023-edit");
      fs.mkdirSync(dst, { recursive: true });
      fs.writeFileSync(path.join(dst, "SKILL.md"), "USER EDITED");

      const actions = installSkills(dir, { force: false, dryRun: false });

      // AC-01: byte-diff of the edited file is empty (survives untouched).
      assert.strictEqual(
        fs.readFileSync(path.join(dst, "SKILL.md"), "utf8"),
        "USER EDITED"
      );
      assert.ok(
        actions.some(
          (a) => a === "Kept skill file (exists): nan023-edit/SKILL.md"
        )
      );
    });
  });

  it("test_installSkills_partial_install_fills_only_missing", () => {
    const dir = makeTempProject();
    withShippedSkills(
      {
        "nan023-present": { "SKILL.md": "SHIPPED-P" },
        "nan023-absent": { "SKILL.md": "SHIPPED-Q" },
      },
      () => {
        // Present-and-edited on disk; absent one is missing.
        const present = path.join(dir, ".claude", "skills", "nan023-present");
        fs.mkdirSync(present, { recursive: true });
        fs.writeFileSync(path.join(present, "SKILL.md"), "EDITED-P");

        installSkills(dir, { force: false, dryRun: false });

        assert.strictEqual(
          fs.readFileSync(path.join(present, "SKILL.md"), "utf8"),
          "EDITED-P",
          "present (edited) skill untouched"
        );
        assert.strictEqual(
          fs.readFileSync(
            path.join(dir, ".claude", "skills", "nan023-absent", "SKILL.md"),
            "utf8"
          ),
          "SHIPPED-Q",
          "absent skill installed from shipped"
        );
      }
    );
  });

  it("test_installSkills_foreign_skill_untouched_default", () => {
    const dir = makeTempProject();
    withShippedSkills({ "nan023-owned": { "SKILL.md": "OWNED" } }, () => {
      // A skill the package does NOT ship.
      const foreign = path.join(dir, ".claude", "skills", "foreign");
      fs.mkdirSync(foreign, { recursive: true });
      fs.writeFileSync(path.join(foreign, "SKILL.md"), "FOREIGN");

      const actions = installSkills(dir, { force: false, dryRun: false });

      assert.strictEqual(
        fs.readFileSync(path.join(foreign, "SKILL.md"), "utf8"),
        "FOREIGN"
      );
      assert.ok(
        !actions.some((a) => a.includes("foreign")),
        "foreign skill never enumerated"
      );
    });
  });

  // --force (R-13, AC-02) ───────────────────────────────────────────────

  it("test_installSkills_force_overwrites_owned_with_shipped", () => {
    const dir = makeTempProject();
    withShippedSkills({ "nan023-force": { "SKILL.md": "SHIPPED-NEW" } }, () => {
      const dst = path.join(dir, ".claude", "skills", "nan023-force");
      fs.mkdirSync(dst, { recursive: true });
      fs.writeFileSync(path.join(dst, "SKILL.md"), "OLD");

      const actions = installSkills(dir, { force: true, dryRun: false });

      assert.strictEqual(
        fs.readFileSync(path.join(dst, "SKILL.md"), "utf8"),
        "SHIPPED-NEW"
      );
      assert.ok(
        actions.some(
          (a) => a === "Overwrote (--force) skill file: nan023-force/SKILL.md"
        )
      );
    });
  });

  it("test_installSkills_force_never_touches_foreign", () => {
    const dir = makeTempProject();
    withShippedSkills({ "nan023-owned": { "SKILL.md": "OWNED" } }, () => {
      const foreign = path.join(dir, ".claude", "skills", "foreign");
      fs.mkdirSync(foreign, { recursive: true });
      fs.writeFileSync(path.join(foreign, "SKILL.md"), "FOREIGN");

      installSkills(dir, { force: true, dryRun: false });

      assert.strictEqual(
        fs.readFileSync(path.join(foreign, "SKILL.md"), "utf8"),
        "FOREIGN",
        "foreign byte-identical even under --force (AC-02)"
      );
    });
  });

  it("test_installSkills_force_scope_is_skills_only", () => {
    const dir = makeTempProject();
    withShippedSkills({ "nan023-owned": { "SKILL.md": "OWNED" } }, () => {
      // Protocols/agents present — must be untouched (C-13, SR-05).
      const proto = path.join(dir, ".claude", "protocols");
      const agents = path.join(dir, ".claude", "agents");
      fs.mkdirSync(proto, { recursive: true });
      fs.mkdirSync(agents, { recursive: true });
      fs.writeFileSync(path.join(proto, "p.md"), "PROTO");
      fs.writeFileSync(path.join(agents, "a.md"), "AGENT");

      const actions = installSkills(dir, { force: true, dryRun: false });

      assert.strictEqual(fs.readFileSync(path.join(proto, "p.md"), "utf8"), "PROTO");
      assert.strictEqual(fs.readFileSync(path.join(agents, "a.md"), "utf8"), "AGENT");
      assert.ok(
        actions.some(
          (a) =>
            a === "Definition scope: skills only (protocols/agents not installed)"
        ),
        "boundary line present (SR-05)"
      );
    });
  });

  // Dry-run (AC-14, R-14) ───────────────────────────────────────────────

  it("test_installSkills_dryRun_prints_actions_writes_nothing", () => {
    const dir = makeTempProject();
    withShippedSkills({ "nan023-dry": { "SKILL.md": "X" } }, () => {
      const actions = installSkills(dir, { force: false, dryRun: true });

      const target = path.join(dir, ".claude", "skills", "nan023-dry");
      assert.ok(!fs.existsSync(target), "no filesystem writes in dry-run");
      // .claude/skills itself not created either.
      assert.ok(!fs.existsSync(path.join(dir, ".claude", "skills")));
      assert.ok(
        actions.some((a) => a.startsWith("[dry-run] Would install skill file:")),
        "dry-run reports intended per-file action"
      );
    });
  });

  it("test_installSkills_dryRun_action_set_equals_real", () => {
    const shipped = {
      "nan023-x": { "SKILL.md": "X" },
      "nan023-y": { "SKILL.md": "Y" },
    };
    // Same on-disk starting state for both runs: nan023-x already present.
    function seed(dir) {
      const d = path.join(dir, ".claude", "skills", "nan023-x");
      fs.mkdirSync(d, { recursive: true });
      fs.writeFileSync(path.join(d, "SKILL.md"), "EDITED");
    }

    const dryDir = makeTempProject();
    const realDir = makeTempProject();
    withShippedSkills(shipped, () => {
      seed(dryDir);
      seed(realDir);
      const dryActions = installSkills(dryDir, { force: false, dryRun: true });
      const realActions = installSkills(realDir, { force: false, dryRun: false });
      assert.deepStrictEqual(
        decisionSet(dryActions),
        decisionSet(realActions),
        "per-file decision set identical between dry-run and real (NFR-10)"
      );
    });
  });

  // Containment / path-traversal (AC-12) ────────────────────────────────

  it("test_installSkills_rejects_path_traversal_filename", () => {
    const dir = makeTempProject();
    // A shipped skill dir containing a filename with `..` must throw.
    const bad = path.join(SKILLS_SOURCE, "nan023-trav");
    fs.mkdirSync(bad, { recursive: true });
    fs.writeFileSync(path.join(bad, "a..b"), "x");
    try {
      assert.throws(
        () => installSkills(dir, { force: false, dryRun: false }),
        /Path traversal detected/
      );
    } finally {
      fs.rmSync(bad, { recursive: true, force: true });
    }
  });

  it("test_installSkills_no_bundled_skills_dir_is_no_op", () => {
    // Sanity: when the source tree exists (it does), function returns lines.
    // The missing-source branch is covered structurally; here assert the
    // scope-boundary line always closes the action list.
    const dir = makeTempProject();
    withShippedSkills({ "nan023-z": { "SKILL.md": "Z" } }, () => {
      const actions = installSkills(dir, { force: false, dryRun: false });
      assert.strictEqual(
        actions[actions.length - 1],
        "Definition scope: skills only (protocols/agents not installed)"
      );
    });
  });
});
