#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");

function main() {
  const args = process.argv.slice(2);

  // Route "init" to JS implementation (ADR-003)
  if (args[0] === "init") {
    const { init } = require("../lib/init.js");
    init({ dryRun: args.includes("--dry-run") })
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

  try {
    execFileSync(binaryPath, args, { stdio: "inherit" });
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
