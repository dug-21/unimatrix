"use strict";

const fs = require("fs");
const path = require("path");

const PLATFORMS = {
  "linux-x64": "@dug-21/unimatrix-linux-x64",
  "linux-arm64": "@dug-21/unimatrix-linux-arm64",
};
const CLIENT_ENTRY = path.join("lib", "hook-client", "index.js");

class RuntimeResolutionError extends Error {
  constructor(code, source, p, message) {
    super(message);
    this.name = "RuntimeResolutionError";
    this.code = code;
    this.source = source;
    this.path = p;
  }
}

function fail(code, source, p, msg) {
  return new RuntimeResolutionError(
    code,
    source,
    p,
    "unimatrix: " + msg + " (source: " + source + ", path: " + p + ")"
  );
}

function readJsonFile(file) {
  let raw;
  try {
    raw = fs.readFileSync(file, "utf8");
  } catch (e) {
    if (e && (e.code === "ENOENT" || e.code === "ENOTDIR")) return undefined;
    throw e;
  }
  return JSON.parse(raw);
}

function readRedirect(home) {
  const file = path.join(home, ".unimatrix", "redirect.json");
  let r;
  try {
    r = readJsonFile(file);
  } catch (_e) {
    throw fail("redirect_malformed", "redirect", file, "redirect.json is unreadable or not valid JSON");
  }
  if (r === undefined) return null;
  const bad = (v) => v !== undefined && v !== null && typeof v !== "string";
  if (!r || typeof r !== "object" || Array.isArray(r) || bad(r.binary) || bad(r.client)) {
    throw fail("redirect_malformed", "redirect", file, "redirect.json must be an object with string binary/client");
  }
  return r;
}

function readInstallJson(home) {
  const file = path.join(home, ".unimatrix", "install.json");
  let i;
  try {
    i = readJsonFile(file);
  } catch (_e) {
    throw fail("install_invalid", "install", file, "install.json is unreadable or not valid JSON");
  }
  if (i === undefined) return { file, inst: null };
  if (!i || typeof i !== "object" || typeof i.packageRoot !== "string") {
    throw fail("install_invalid", "install", file, "install.json lacks a packageRoot");
  }
  return { file, inst: i };
}

function validateClientRoot(root, source, code) {
  if (typeof root !== "string" || !path.isAbsolute(root)) {
    throw fail(code, source, String(root), "client root must be an absolute path");
  }
  let ok = false;
  try {
    ok =
      fs.statSync(root).isDirectory() &&
      fs.statSync(path.join(root, CLIENT_ENTRY)).isFile();
  } catch (_e) {
    ok = false;
  }
  if (!ok) {
    throw fail(code, source, root, "client root is not a directory containing " + CLIENT_ENTRY);
  }
  return root;
}

function validateBinary(p, source, code, platform) {
  if (typeof p !== "string" || !path.isAbsolute(p)) {
    throw fail(code, source, String(p), "binary path must be absolute");
  }
  try {
    if (!fs.statSync(p).isFile()) throw new Error("not a file");
    if (platform !== "win32") fs.accessSync(p, fs.constants.X_OK);
    return fs.realpathSync(p);
  } catch (e) {
    const missing = e && (e.code === "ENOENT" || e.code === "ENOTDIR");
    throw fail(
      code,
      source,
      p,
      source === "env"
        ? "UNIMATRIX_BINARY points to " + (missing ? "non-existent file" : "a non-executable or non-file path")
        : "binary is " + (missing ? "missing" : "not an executable file")
    );
  }
}

function installedBinary(base, platform, arch) {
  const pkg = PLATFORMS[platform + "-" + arch];
  if (!pkg) {
    throw fail(
      "unsupported_platform_local",
      "install",
      platform + "-" + arch,
      "Unsupported platform " + platform + "-" + arch + "; supported: " + Object.keys(PLATFORMS).join(", ")
    );
  }
  if (!base) return { pkg, binary: null };
  try {
    return { pkg, binary: fs.realpathSync(require.resolve(pkg + "/bin/unimatrix", { paths: [base] })) };
  } catch (_e) {
    return { pkg, binary: null };
  }
}

/**
 * Single runtime resolver (ADR-002).
 * client: redirect.client > install.json packageRoot
 * binary (local transport on linux only): UNIMATRIX_BINARY > redirect.binary > installed platform package
 */
function resolveRuntime(p) {
  const env = p.env || {};
  const arch = p.arch || process.arch;
  const redirect = readRedirect(p.home);
  const selfRoot = path.resolve(__dirname, "..");
  let installRoot = null;
  let clientRoot;
  let clientSource;

  if (redirect && redirect.client != null) {
    clientRoot = validateClientRoot(redirect.client, "redirect", "redirect_client_invalid");
    clientSource = "redirect";
  } else {
    const { file, inst } = readInstallJson(p.home);
    if (!inst) {
      if (!p.allowSelf) {
        throw fail("install_missing", "install", file, "no install found; run 'npx unimatrix init'");
      }
      clientRoot = validateClientRoot(selfRoot, "install", "client_invalid");
    } else {
      installRoot = inst.packageRoot;
      if (!path.isAbsolute(installRoot) || !fs.existsSync(installRoot)) {
        throw fail("install_invalid", "install", String(installRoot), "install packageRoot is missing");
      }
      clientRoot = validateClientRoot(installRoot, "install", "client_invalid");
    }
    clientSource = "install";
  }

  const out = { binary: null, clientRoot, binarySource: null, clientSource };
  if (p.transport !== "local" || p.platform !== "linux") return out;

  const envBin = env.UNIMATRIX_BINARY;
  if (typeof envBin === "string" && envBin !== "") {
    out.binary = validateBinary(envBin, "env", "env_binary_invalid", p.platform);
    out.binarySource = "env";
    return out;
  }
  if (redirect && redirect.binary != null) {
    out.binary = validateBinary(redirect.binary, "redirect", "redirect_binary_invalid", p.platform);
    out.binarySource = "redirect";
    return out;
  }
  if (installRoot === null && clientSource === "redirect") {
    try {
      installRoot = readInstallJson(p.home).inst.packageRoot;
    } catch (_e) {
      installRoot = null;
    }
  }
  const { pkg, binary } = installedBinary(installRoot || (p.allowSelf ? selfRoot : null), p.platform, arch);
  if (!binary) {
    throw fail(
      "binary_unresolvable",
      "install",
      pkg,
      "Could not find platform binary package '" + pkg + "'; sources tried: UNIMATRIX_BINARY, redirect, install (" +
        pkg + "). Run 'npx unimatrix init' or set UNIMATRIX_BINARY"
    );
  }
  out.binary = binary;
  out.binarySource = "install";
  return out;
}

module.exports = { resolveRuntime, RuntimeResolutionError, PLATFORMS };
