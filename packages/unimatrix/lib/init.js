"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");
const { resolveBinary } = require("./resolve-binary.js");
const { mergeSettings } = require("./merge-settings.js");

const HOOK_EVENTS = [
  "SessionStart",
  "Stop",
  "UserPromptSubmit",
  "PreToolUse",
  "PostToolUse",
  "SubagentStart",
  "SubagentStop",
];

/**
 * Detect project root by walking up from startDir to find .git directory.
 * Mirrors the Rust detect_project_root algorithm (ADR-003).
 *
 * @param {string} startDir - Directory to start searching from.
 * @returns {string} Absolute path to the project root.
 */
function detectProjectRoot(startDir) {
  let current = path.resolve(startDir);
  for (;;) {
    if (fs.existsSync(path.join(current, ".git"))) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      throw new Error(
        "Could not find project root (.git directory).\n" +
          "Run this command from within a git repository."
      );
    }
    current = parent;
  }
}

/**
 * Write or merge .mcp.json with the unimatrix server entry.
 * Preserves existing servers. Malformed JSON causes an error (ADR-004).
 *
 * @param {string} projectRoot - Absolute path to project root.
 * @param {string} binaryPath - Absolute path to the unimatrix binary.
 * @param {boolean} dryRun - If true, do not write the file.
 * @returns {string[]} Actions taken.
 */
function writeMcpJson(projectRoot, binaryPath, dryRun) {
  const mcpPath = path.join(projectRoot, ".mcp.json");
  const actions = [];
  let existing = {};

  if (fs.existsSync(mcpPath)) {
    try {
      existing = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    } catch (parseError) {
      throw new Error(
        "Malformed .mcp.json at " +
          mcpPath +
          ": " +
          parseError.message +
          "\nFix the JSON syntax and re-run 'npx unimatrix init'."
      );
    }
    actions.push("Updated .mcp.json (preserved existing servers)");
  } else {
    actions.push("Created .mcp.json");
  }

  if (!existing.mcpServers) {
    existing.mcpServers = {};
  }

  existing.mcpServers.unimatrix = {
    command: binaryPath,
    args: [],
    env: {
      LD_LIBRARY_PATH: path.dirname(binaryPath),
    },
  };

  if (!dryRun) {
    fs.writeFileSync(
      mcpPath,
      JSON.stringify(existing, null, 2) + "\n",
      "utf8"
    );
  } else {
    actions[actions.length - 1] = "[dry-run] " + actions[actions.length - 1];
  }

  return actions;
}

/**
 * Copy bundled skill files from the package's skills/ directory
 * into the project's .claude/skills/ directory. Overwrites existing
 * unimatrix skills, preserves non-unimatrix skills.
 *
 * @param {string} projectRoot - Absolute path to project root.
 * @param {boolean} dryRun - If true, do not copy files.
 * @returns {string[]} Actions taken.
 */
function copySkills(projectRoot, dryRun) {
  const actions = [];
  const targetDir = path.join(projectRoot, ".claude", "skills");
  const sourceDir = path.join(__dirname, "..", "skills");

  if (!fs.existsSync(sourceDir)) {
    actions.push("No bundled skills found (skipped)");
    return actions;
  }

  if (!dryRun) {
    fs.mkdirSync(targetDir, { recursive: true });
  }

  const skillDirs = fs
    .readdirSync(sourceDir, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name);

  for (const skillDir of skillDirs) {
    const src = path.join(sourceDir, skillDir);
    const dst = path.join(targetDir, skillDir);

    if (!dryRun) {
      fs.mkdirSync(dst, { recursive: true });

      const files = fs.readdirSync(src);
      for (const file of files) {
        if (file.includes("..")) {
          throw new Error(
            "Path traversal detected in skill file: " + file
          );
        }

        const srcFile = path.join(src, file);
        const dstFile = path.join(dst, file);

        // Only copy files, not subdirectories
        const stat = fs.statSync(srcFile);
        if (stat.isFile()) {
          fs.copyFileSync(srcFile, dstFile);
        }
      }

      actions.push("Copied skill: " + skillDir);
    } else {
      actions.push("[dry-run] Would copy skill: " + skillDir);
    }
  }

  return actions;
}

/**
 * Print summary of all actions taken during init.
 *
 * @param {string[]} actions - List of action descriptions.
 * @param {boolean} dryRun - Whether this was a dry run.
 */
function printSummary(actions, dryRun) {
  if (dryRun) {
    console.log("\n--- Dry Run Summary ---\n");
  } else {
    console.log("\n--- Unimatrix Init Complete ---\n");
  }

  for (const action of actions) {
    console.log("  " + action);
  }

  console.log("");
  if (!dryRun) {
    console.log("Next step: start a Claude Code session and run /unimatrix-init");
  }
}

/**
 * Deterministic, non-interactive, idempotent project wiring.
 * Configures MCP server, hooks, skills, and pre-creates the database.
 * Implemented in JavaScript per ADR-003.
 *
 * @param {object} options
 * @param {boolean} [options.dryRun=false] - Print actions without modifying files.
 * @param {string} [options.projectDir] - Override project root (skip .git walk).
 */
async function init(options) {
  const dryRun = (options && options.dryRun) || false;
  const actions = [];

  // Step 1: Resolve project root
  let projectRoot;
  if (options && options.projectDir) {
    projectRoot = path.resolve(options.projectDir);
  } else {
    projectRoot = detectProjectRoot(process.cwd());
  }
  actions.push("Project root: " + projectRoot);

  // Step 2: Resolve binary path
  const binaryPath = resolveBinary();
  actions.push("Binary: " + binaryPath);

  // Step 3: Write/merge .mcp.json
  const mcpActions = writeMcpJson(projectRoot, binaryPath, dryRun);
  actions.push(...mcpActions);

  // Step 4: Merge hooks into .claude/settings.json
  const settingsPath = path.join(projectRoot, ".claude", "settings.json");
  const settingsResult = mergeSettings(settingsPath, binaryPath, { dryRun });
  actions.push(...settingsResult.actions);

  // Step 5: Copy skill files
  const skillActions = copySkills(projectRoot, dryRun);
  actions.push(...skillActions);

  // Shared env for all binary invocations: libonnxruntime lives next to the binary
  const binDir = path.dirname(binaryPath);
  const ldPath = process.env.LD_LIBRARY_PATH;
  const binaryEnv = Object.assign({}, process.env, {
    LD_LIBRARY_PATH: ldPath ? binDir + ":" + ldPath : binDir,
  });

  // Step 6: Pre-create database (exec Rust binary)
  if (!dryRun) {
    try {
      execFileSync(binaryPath, ["version", "--project-dir", projectRoot], {
        stdio: "pipe",
        env: binaryEnv,
      });
      actions.push("Database: pre-created at ~/.unimatrix/{hash}/");
    } catch (error) {
      const stderr =
        error.stderr ? error.stderr.toString() : error.message;
      throw new Error("Database creation failed: " + stderr);
    }
  } else {
    actions.push(
      "[dry-run] Would pre-create database via: unimatrix version --project-dir " +
        projectRoot
    );
  }

  // Step 7: Validate binary
  if (!dryRun) {
    try {
      const versionOutput = execFileSync(binaryPath, ["version"], {
        stdio: "pipe",
        encoding: "utf8",
        env: binaryEnv,
      }).trim();
      actions.push("Validation: " + versionOutput);
    } catch (error) {
      const stderr =
        error.stderr ? error.stderr.toString() : error.message;
      throw new Error("Binary validation failed: " + stderr);
    }
  } else {
    actions.push("[dry-run] Would validate binary via: unimatrix version");
  }

  // Step 8: Print summary
  printSummary(actions, dryRun);
}

module.exports = {
  init,
  detectProjectRoot,
  writeMcpJson,
  copySkills,
  printSummary,
};
