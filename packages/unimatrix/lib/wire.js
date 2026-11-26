"use strict";

/**
 * Wire orchestrator (nan-024). The single flow shared by local init, cloud
 * attach and the `wire` verb (D-040): install store -> Claude hooks + MCP ->
 * Codex -> OpenCode -> removal of owned project-file MCP entries -> warnings.
 * Every writer takes `dryRun` and returns the same legs as a real run, so the
 * dry-run action list equals the real changes (FR-12d). No harness file-format
 * logic lives here. Hard failures (unsafe shim path, unparseable config) throw
 * to the caller (exit 1); warnings go to stderr and never change the exit code.
 */

const path = require("path");
const fs = require("fs");
const { isDeepStrictEqual } = require("util");
const { execFileSync } = require("child_process");

const { mergeSettings, HOOK_EVENTS, buildShimCommand, removeOwnedHooks } = require("./merge-settings.js");
const { installStore } = require("./install-store.js");
const { preflightRecordedNode } = require("./wire-preflight.js");
const { buildMcpEntry } = require("./mcp-entry.js");
const { writeClaudeMcp, removeOwnedMcpJson } = require("./claude-json.js");
const { writeOpencodeMcp, removeOwnedOpencodeProjectMcp, detectsOpenCode } = require("./opencode-install.js");
const { maybeWireCodex } = require("./codex-install.js");

const HARNESSES = ["claude-code", "opencode", "codex-cli"];
const GITIGNORE_WARNING =
  "warning: .claude/settings.local.json is not git-ignored; add it to .gitignore so local hook wiring is not committed";

function existsSafe(p) {
  try {
    return fs.existsSync(p);
  } catch (_err) {
    return false;
  }
}

/** Project-local OpenCode/Claude markers. Codex is detected by detectsCodex (D-081). */
function detectHarnesses(dir) {
  return {
    "claude-code": existsSafe(path.join(dir, ".mcp.json")) || existsSafe(path.join(dir, ".claude")),
    opencode: detectsOpenCode(dir),
  };
}

/** {kind: "local"|"cloud"|"none"}: binary present => local; credential => cloud. */
function resolveTransport(_projectRoot, opts) {
  if (opts.binaryPath) return { kind: "local", binaryPath: opts.binaryPath };
  if (opts.mcp || opts.cloud) return { kind: "cloud" };
  return { kind: "none" };
}

function skippedLeg(surface, legPath, reason) {
  return { harness: "claude-code", surface, action: "skipped-undetected", path: legPath, reason };
}

/** git check-ignore: 0 ignored, 1 not ignored (warn); anything else => no warning. */
function gitIgnored(root, rel) {
  try {
    execFileSync("git", ["check-ignore", "-q", rel], { cwd: root, stdio: "ignore" });
    return true;
  } catch (e) {
    return !(e && e.status === 1);
  }
}

function storeLegs(result) {
  return result.actions.map((a) => {
    const m = /(?:would write|wrote) (.*)$/.exec(a);
    return { harness: "unimatrix", surface: "store", action: "written", path: m ? m[1] : a };
  });
}

function dispatchClaude(root, c, manifest) {
  const settings = path.join(root, ".claude", "settings.local.json");
  let before = null;
  try {
    before = fs.readFileSync(settings, "utf8");
  } catch (_) { /* absent */ }
  const r = mergeSettings(
    settings,
    { events: HOOK_EVENTS, commandForEvent: (e) => buildShimCommand(e, undefined, c.platform, c.home) },
    { dryRun: c.dryRun }
  );
  let unchanged = false;
  if (before !== null) {
    try {
      unchanged = isDeepStrictEqual(JSON.parse(before), r.content);
    } catch (_) { /* treated as changed */ }
  }
  const action = before === null ? "created" : unchanged ? "unchanged" : "updated";
  for (const { command } of r.commands) {
    manifest.push({ harness: "claude-code", surface: "hooks", action, path: settings, command });
  }
  for (const file of [path.join(root, ".claude", "settings.json"), path.join(c.home, ".claude", "settings.json")]) {
    const rm = removeOwnedHooks(file, { dryRun: c.dryRun });
    if (rm.changed) {
      manifest.push({ harness: "claude-code", surface: "hooks", action: "removed", path: file, reason: "owned hooks removed (cross-scope)" });
    }
  }
  if (!c.writeMcp) return;
  const entry = buildMcpEntry(c.platform, c.home, c.node);
  const w = writeClaudeMcp({ home: c.home, projectRoot: root, entry, dryRun: c.dryRun, platform: c.platform });
  if (w.action === "skipped-foreign") {
    c.warnings.push(w.warning);
    manifest.push({ harness: "claude-code", surface: "mcp", action: "skipped-foreign", path: w.file, reason: w.warning });
  } else {
    manifest.push({ harness: "claude-code", surface: "mcp", action: w.action, path: w.file, entry });
  }
}

function dispatchOpencode(root, c, manifest) {
  try {
    const l = writeOpencodeMcp({
      home: c.home, platform: c.platform, node: c.node,
      harnessSelected: c.harnessSel === "opencode", dryRun: c.dryRun,
    });
    if (l.warning) c.warnings.push(l.warning);
    manifest.push(l);
  } catch (e) {
    manifest.push({
      harness: "opencode", surface: "retrieval", action: "skipped-malformed",
      path: path.join(c.home, ".config", "opencode", "opencode.json"),
      reason: "opencode wiring error: " + e.message,
    });
  }
}

function dispatchCodex(root, c, manifest) {
  const legs = maybeWireCodex({
    home: c.home, env: c.env, platform: c.platform, node: c.node,
    projectRoot: root, harnessSel: c.harnessSel, dryRun: c.dryRun,
  });
  for (const l of legs) manifest.push(l);
}

/** Owned project-file MCP entries are always migrated away (D-060/D-062/D-064). */
function removeProjectFileMcp(root, c, manifest) {
  const rm = [
    ["claude-code", path.join(root, ".mcp.json"), () => removeOwnedMcpJson(root, c.dryRun)],
    ["opencode", path.join(root, "opencode.json"), () => removeOwnedOpencodeProjectMcp(root, c.dryRun)],
  ];
  for (const [harness, file, fn] of rm) {
    const r = fn();
    for (const w of r.warnings) c.warnings.push(w);
    if (r.changed) manifest.push({ harness, surface: "mcp", action: "removed", path: file, reason: "owned project MCP entry removed" });
  }
}

/** Render one manifest leg into a human summary line. */
function legToActionLine(root, leg, dryRun) {
  const rel = path.relative(root, leg.path);
  const loc = rel && !rel.startsWith("..") && !path.isAbsolute(rel) ? rel : leg.path;
  let s = leg.harness + " " + leg.surface + ": " + leg.action + " " + loc;
  if (leg.reason) s += " (" + leg.reason + ")";
  return dryRun ? "[dry-run] " + s : s;
}

/**
 * @param {string} projectRoot
 * @param {object} opts {harness?, dryRun, home?, platform?, node?, env?, binaryPath?, mcp?,
 *   cloud?, hooksOnly?, rerecord?, requireBinary?, sourcePackageRoot?, installStoreOpts?}
 *   `rerecord` (init) skips the recorded-node preflight and re-records node.
 * @returns {{actions: string[], manifest: object[], warnings: string[]}}
 */
function wire(projectRoot, opts) {
  const o = opts || {};
  const os = require("os");
  const home = o.home || os.homedir();
  const platform = o.platform || process.platform;
  const env = o.env || process.env;
  const dryRun = o.dryRun === true;
  const harnessSel = o.harness || null;
  if (harnessSel && HARNESSES.indexOf(harnessSel) < 0) {
    return { actions: ["unknown --harness: " + harnessSel], manifest: [], warnings: [] };
  }
  const transport = resolveTransport(projectRoot, o);
  const manifest = [];
  const warnings = [];
  const settings = path.join(projectRoot, ".claude", "settings.local.json");

  if (transport.kind === "none") {
    const reason = "no transport (binaryPath/credential absent)";
    manifest.push(skippedLeg("mcp", path.join(home, ".claude.json"), reason));
    manifest.push(skippedLeg("hooks", settings, reason));
    return { actions: manifest.map((l) => legToActionLine(projectRoot, l, dryRun)), manifest, warnings };
  }

  let node = o.node || process.execPath;
  if (!o.rerecord) {
    const pf = preflightRecordedNode({ home, platform, env });
    if (pf.node) node = pf.node;
    if (pf.warning) warnings.push(pf.warning);
  }
  const store = installStore(Object.assign({
    home, platform, nodePath: node,
    sourcePackageRoot: o.sourcePackageRoot || path.join(__dirname, ".."),
    projectRoot, dryRun,
    requireBinary: o.requireBinary !== undefined ? o.requireBinary : transport.kind === "local" && platform === "linux",
  }, o.installStoreOpts || {}));
  manifest.push(...storeLegs(store));

  const c = { home, platform, env, node, dryRun, harnessSel, warnings, writeMcp: o.mcp !== false && !o.hooksOnly };
  const want = (h) => !harnessSel || harnessSel === h;
  if (want("claude-code")) dispatchClaude(projectRoot, c, manifest);
  if (!o.hooksOnly) {
    if (want("codex-cli")) dispatchCodex(projectRoot, c, manifest);
    if (want("opencode") && (harnessSel === "opencode" || detectsOpenCode(projectRoot))) {
      dispatchOpencode(projectRoot, c, manifest);
    }
    removeProjectFileMcp(projectRoot, c, manifest);
  }
  const actions = manifest.map((l) => legToActionLine(projectRoot, l, dryRun));
  for (const w of warnings) {
    try {
      process.stderr.write(w + "\n");
    } catch (_) { /* best effort */ }
  }
  if (want("claude-code") && !gitIgnored(projectRoot, ".claude/settings.local.json")) {
    warnings.push(GITIGNORE_WARNING);
    actions.push(GITIGNORE_WARNING);
  }
  return { actions, manifest, warnings };
}

module.exports = { wire, detectHarnesses, resolveTransport, legToActionLine, HARNESSES, GITIGNORE_WARNING };
