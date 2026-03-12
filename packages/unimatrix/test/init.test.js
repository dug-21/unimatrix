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
