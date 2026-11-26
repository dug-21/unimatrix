"use strict";

const os = require("os");
const { resolveRuntime, PLATFORMS } = require("./resolve-runtime.js");

/**
 * Thin wrapper over resolveRuntime(): absolute real path to the Unimatrix
 * binary, or null on macOS/Windows. Throws RuntimeResolutionError otherwise.
 */
function resolveBinary() {
  return resolveRuntime({
    env: process.env,
    home: os.homedir(),
    platform: process.platform,
    arch: process.arch,
    transport: "local",
    allowSelf: true,
  }).binary;
}

module.exports = { resolveBinary, PLATFORMS };
