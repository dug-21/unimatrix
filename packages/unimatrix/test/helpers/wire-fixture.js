"use strict";
// wire()/init fixtures (nan-024): temp git project + isolated HOME so no test writes
// under the real home. wireOpts() skips the real platform-binary copy (requireBinary false).
const fs = require("fs");
const os = require("os");
const path = require("path");
const { assertTmpHome } = require("./home-guard");

function makeProject(prefix) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), prefix || "unimatrix-wire-test-")));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function makeHome() {
  return assertTmpHome(fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-wire-home-"))));
}

/** Options for wire(): isolated home, no platform-binary copy, local (binaryPath) or cloud. */
function wireOpts(home, extra) {
  assertTmpHome(home);
  return Object.assign({ home, requireBinary: false, env: { PATH: process.env.PATH, HOME: home } }, extra || {});
}

/** Sorted relative file list of a tree (for dry-run == real comparisons). */
function listTree(dir) {
  const out = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else out.push(path.relative(dir, p));
    }
  };
  try { walk(dir); } catch (_) { /* absent */ }
  return out.sort();
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

function writeJsonAt(p, obj) {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, JSON.stringify(obj, null, 2) + "\n");
}

/** Temp project with a real `git init` (hermetic: no global/system config). */
function makeGitProject(prefix) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), prefix || "unimatrix-wire-git-")));
  require("child_process").execFileSync("git", ["init", "-q"], { cwd: dir, env: gitEnv(), stdio: "ignore" });
  return dir;
}

/** Env for git/spawned init that ignores the developer's global and system git config. */
function gitEnv(extra) {
  const { scrubbedEnv } = require("./home-guard");
  return Object.assign(scrubbedEnv(), { GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1" }, extra || {});
}

/** Owned Claude hooks object for `events` (default: all) as the shim would write them. */
function ownedHooks(home, platform, events) {
  const { HOOK_EVENTS, buildShimCommand } = require("../../lib/merge-settings.js");
  const hooks = {};
  for (const e of events || HOOK_EVENTS) {
    hooks[e] = [{ hooks: [{ type: "command", command: buildShimCommand(e, undefined, platform || "linux", home) }] }];
  }
  return hooks;
}

/**
 * Seed owned + foreign entries at nan-024 legacy / RT locations (AC-4, AC-6).
 * Returns {file: seededObject} so callers can assert foreign survivors.
 */
function seedLegacy(home, root, o) {
  const opt = o || {};
  const { buildMcpEntry, buildMcpArgv } = require("../../lib/mcp-entry.js");
  const plat = opt.platform || "linux";
  const entry = buildMcpEntry(plat, home, opt.node || process.execPath);
  const foreignHook = { hooks: [{ type: "command", command: "echo foreign" }] };
  const settings = (h) => ({ theme: "dark", hooks: Object.assign({}, h, { Notification: [foreignHook] }) });
  const out = {};
  const put = (p, obj) => { writeJsonAt(p, obj); out[p] = obj; };
  const own = ownedHooks(home, opt.platform);
  put(path.join(root, ".claude", "settings.json"), settings(own));
  put(path.join(home, ".claude", "settings.json"), settings(own));
  if (opt.claudeConfigDir) put(path.join(opt.claudeConfigDir, "settings.json"), settings(own));
  put(path.join(root, ".mcp.json"), { mcpServers: { unimatrix: entry, other: { command: "x" } } });
  const cj = path.join(opt.claudeConfigDir || home, ".claude.json");
  put(cj, { numStartups: 3, mcpServers: { unimatrix: entry, other: { command: "y" } } });
  put(path.join(root, "opencode.json"), {
    $schema: "https://opencode.ai/config.json",
    mcp: { unimatrix: { type: "local", command: buildMcpArgv(plat, home, opt.node || process.execPath) } },
    plugin: ["@dug-21/unimatrix-opencode-plugin@1.0.0", "foreign-plugin"],
  });
  return out;
}

/** Replace `file` with a symlink to a copy in another temp dir; returns the target path. */
function seedSymlinked(file, kind) {
  const other = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-link-target-")));
  const target = path.join(other, path.basename(file) + ".real");
  if (fs.existsSync(file)) fs.renameSync(file, target);
  else fs.writeFileSync(target, "{}\n");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  if (kind === "relative") fs.symlinkSync(path.relative(path.dirname(file), target), file);
  else if (kind === "chain") {
    const hop = path.join(other, "hop");
    fs.symlinkSync(target, hop);
    fs.symlinkSync(hop, file);
  } else fs.symlinkSync(target, file);
  return target;
}

/**
 * Spawnable CLI entry for real `init` subprocess tests. Stages the published files into a
 * tmp package root with a fixture platform package beside it, so init's required
 * binary-package copy resolves even where the optional @dug-21 platform package is absent (CI).
 */
let stagedRoot = null;
function stagedCli() {
  if (!stagedRoot) {
    const src = path.join(__dirname, "..", "..");
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-staged-pkg-")));
    for (const n of ["bin", "lib", "skills", "protocols", "package.json"]) if (fs.existsSync(path.join(src, n))) fs.cpSync(path.join(src, n), path.join(root, n), { recursive: true, dereference: true });
    const { PLATFORMS } = require("../../lib/resolve-runtime.js");
    for (const name of Object.values(PLATFORMS)) {
      const d = path.join(root, "node_modules", ...name.split("/"));
      fs.mkdirSync(path.join(d, "bin"), { recursive: true });
      fs.writeFileSync(path.join(d, "package.json"), JSON.stringify({ name, version: "0.0.0-fixture" }));
      fs.writeFileSync(path.join(d, "bin", "unimatrix"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
    }
    process.on("exit", () => { try { fs.rmSync(root, { recursive: true, force: true }); } catch (_) {} });
    stagedRoot = root;
  }
  return path.join(stagedRoot, "bin", "unimatrix.js");
}

module.exports = {
  stagedCli, makeProject, makeHome, wireOpts, listTree, readJson,
  makeGitProject, gitEnv, ownedHooks, seedLegacy, seedSymlinked, writeJsonAt,
};
