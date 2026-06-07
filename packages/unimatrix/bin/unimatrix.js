#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");

function main() {
  const args = process.argv.slice(2);

  // Route "init" to JS implementation (ADR-003)
  if (args[0] === "init") {
    const { init } = require("../lib/init.js");
    const valueAfter = (flag) => {
      const idx = args.indexOf(flag);
      return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
    };
    const projectDir = valueAfter("--project-dir");
    // --remote/--token plumbed through init's own argv (interactive, user-typed).
    // RQ-3 forbids the token in the HOOK command line / checked-in files, not the
    // init invocation itself.
    const remote = valueAfter("--remote");
    const token = valueAfter("--token");
    init({ dryRun: args.includes("--dry-run"), projectDir, remote, token })
      .then(() => {
        process.exitCode = 0;
      })
      .catch((error) => {
        process.stderr.write("unimatrix init failed: " + error.message + "\n");
        process.exitCode = 1;
      });
    return;
  }

  // All other subcommands: resolve binary and exec
  let binaryPath;
  try {
    binaryPath = require("../lib/resolve-binary.js").resolveBinary();
  } catch (error) {
    process.stderr.write(error.message + "\n");
    process.exitCode = 1;
    return;
  }

  // Ensure bundled shared libraries (libonnxruntime) are found at runtime
  const binDir = require("path").dirname(binaryPath);
  const ldPath = process.env.LD_LIBRARY_PATH;
  const env = Object.assign({}, process.env, {
    LD_LIBRARY_PATH: ldPath ? binDir + ":" + ldPath : binDir,
  });

  try {
    execFileSync(binaryPath, args, { stdio: "inherit", env: env });
  } catch (error) {
    // execFileSync throws on non-zero exit code
    // error.status contains the exit code from the child process
    if (error.status !== null && error.status !== undefined) {
      process.exitCode = error.status;
    } else {
      // Signal death or spawn failure
      process.stderr.write(
        "Failed to execute unimatrix: " + error.message + "\n"
      );
      process.exitCode = 1;
    }
  }
}

main();
