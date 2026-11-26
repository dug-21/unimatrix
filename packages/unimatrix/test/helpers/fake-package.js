"use strict";
// Copy of the package into a throwaway "npx cache" dir, carrying a fake linux platform binary
// package (node-shebang stub; body from `stubSrc`). Used by assembled init tests (D-078/D-077/D-082).
const fs = require("fs");
const path = require("path");
const { PLATFORMS } = require("../../lib/resolve-runtime.js");

const PKG = path.join(__dirname, "..", "..");
const plat = PLATFORMS["linux-" + process.arch];

function makeCache(root, stubSrc) {
  const cache = path.join(root, "npx-cache", "unimatrix");
  fs.mkdirSync(cache, { recursive: true });
  for (const n of ["bin", "lib", "skills", "protocols", "package.json"]) {
    fs.cpSync(path.join(PKG, n), path.join(cache, n), { recursive: true, filter: (s) => !/[\\/]node_modules([\\/]|$)/.test(s) });
  }
  const d = path.join(cache, "node_modules", ...plat.split("/"));
  fs.mkdirSync(path.join(d, "bin"), { recursive: true });
  fs.writeFileSync(path.join(d, "package.json"), JSON.stringify({ name: plat, version: "0.0.0" }));
  fs.writeFileSync(path.join(d, "bin", "unimatrix"), stubSrc || "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  return { cache, cliBin: path.join(cache, "bin", "unimatrix.js"), cacheRoot: path.join(root, "npx-cache") };
}

module.exports = { makeCache, plat, supported: process.platform === "linux" && !!plat };
