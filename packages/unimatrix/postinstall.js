#!/usr/bin/env node
"use strict";

function main() {
  try {
    var resolveBinary;
    try {
      resolveBinary = require("./lib/resolve-binary").resolveBinary;
    } catch (_err) {
      console.warn(
        "[unimatrix] postinstall: could not load resolve-binary module, skipping model download"
      );
      process.exit(0);
    }

    var binaryPath;
    try {
      binaryPath = resolveBinary();
    } catch (error) {
      console.warn(
        "[unimatrix] postinstall: platform binary not available (" +
          error.message +
          "), skipping model download"
      );
      process.exit(0);
    }

    var execFileSync = require("child_process").execFileSync;
    try {
      execFileSync(binaryPath, ["model-download"], {
        stdio: ["ignore", "inherit", "inherit"],
        timeout: 300000,
      });
      console.log("[unimatrix] postinstall: ONNX model ready");
    } catch (execError) {
      console.warn(
        "[unimatrix] postinstall: model download failed, will retry on first server start"
      );
      if (execError.stderr) {
        console.warn(
          "[unimatrix]   " + execError.stderr.toString().trim()
        );
      }
    }
  } catch (outerError) {
    console.warn(
      "[unimatrix] postinstall: unexpected error: " + outerError.message
    );
  }

  process.exit(0);
}

main();
