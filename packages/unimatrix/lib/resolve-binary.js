"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");

const PLATFORMS = {
  "linux-x64": "@dug-21/unimatrix-linux-x64",
};

/**
 * Resolve the absolute path to the platform-specific Unimatrix binary.
 *
 * Resolution order:
 * 1. UNIMATRIX_BINARY env var (development/testing override)
 * 2. Platform package via require.resolve (optionalDependencies)
 *
 * @returns {string} Absolute real path to the binary
 * @throws {Error} If binary cannot be found or env override points to missing file
 */
function resolveBinary() {
  // 1. Check environment variable override (development/testing)
  const envPath = process.env.UNIMATRIX_BINARY;
  if (envPath) {
    if (!fs.existsSync(envPath)) {
      throw new Error(
        "UNIMATRIX_BINARY points to non-existent file: " + envPath
      );
    }
    return fs.realpathSync(envPath);
  }

  // 2. Determine current platform key
  const platformKey = os.platform() + "-" + os.arch();

  // 3. Look up package name for this platform
  const packageName = PLATFORMS[platformKey];
  if (!packageName) {
    throw new Error(
      "Unsupported platform: " +
        platformKey +
        "\n" +
        "Supported platforms: " +
        Object.keys(PLATFORMS).join(", ") +
        "\n" +
        "Set UNIMATRIX_BINARY environment variable to use a custom binary path."
    );
  }

  // 4. Resolve binary path via require.resolve
  let binaryPath;
  try {
    binaryPath = require.resolve(packageName + "/bin/unimatrix");
  } catch (_err) {
    throw new Error(
      "Could not find platform binary package '" +
        packageName +
        "'.\n" +
        "Run 'npm install @dug-21/unimatrix' to install.\n" +
        "If using pnpm or yarn, set UNIMATRIX_BINARY to the binary path."
    );
  }

  // 5. Resolve through symlinks to get the real absolute path (ADR-001)
  return fs.realpathSync(binaryPath);
}

module.exports = { resolveBinary, PLATFORMS };
