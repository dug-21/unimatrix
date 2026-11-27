"use strict";

/**
 * Wire orchestrator (nan-024, 1036 C6). The single flow shared by local init,
 * cloud attach and the `wire` verb (D-040): transport -> preflight -> store
 * install -> resolve config dirs once -> wiring plan (wire-plan.js) -> render.
 * No harness file-format logic and no user/global paths are built here (FR-01).
 * Dry-run and real runs share one plan, so their legs and lines match
 * (FR-12d, FR-60..63). Hard failures (SafeWriteError, unparseable config,
 * unsafe shim path) throw to the caller (exit 1, FR-53); warnings go to stderr
 * and the actions list and never change the exit code.
 */

const path = require("path");
const fs = require("fs");
const os = require("os");

const { installStore } = require("./install-store.js");
const { preflightRecordedNode } = require("./wire-preflight.js");
const { detectsOpenCode } = require("./opencode-install.js");
const { resolveConfigDirs } = require("./config-dirs.js");
const { buildWirePlan, runWirePlan } = require("./wire-plan.js");

const HARNESSES = ["claude-code", "opencode", "codex-cli"];
const NOTICE_ACTIONS = new Set(["created", "updated", "removed"]);

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

/** Render one manifest leg into a human summary line. */
function legToActionLine(root, leg, dryRun) {
  const rel = path.relative(root, leg.path);
  const loc = rel && !rel.startsWith("..") && !path.isAbsolute(rel) ? rel : leg.path;
  let s = leg.harness + " " + leg.surface + ": " + leg.action + " " + loc;
  if (leg.reason) s += " (" + leg.reason + ")";
  return pre(dryRun, s);
}

function pre(dryRun, s) {
  return dryRun ? "[dry-run] " + s : s;
}

function srcTag(b) {
  return b.source === "default" ? "default" : b.source + "=" + b.value;
}

/** Codex counts as processed only when its step did not report it undetected. */
function codexProcessed(steps, manifest) {
  return steps.some((s) => s.harness === "codex-cli") &&
    manifest.some((l) => l.harness === "codex-cli" && l.action !== "skipped-not-detected");
}

/** FR-50: one `resolved:` line per destination of each harness processed (selected or detected). */
function resolvedLines(dirs, steps, codex) {
  const has = (h) => steps.some((s) => s.harness === h);
  const L = [];
  const line = (h, surf, file, b) => L.push("resolved: " + h + " " + surf + " -> " + file + " (" + srcTag(b) + ")");
  if (has("claude-code")) {
    if (steps.some((s) => s.id === "W2")) line("claude-code", "mcp", dirs.claude.claudeJson, dirs.claude);
    line("claude-code", "hooks", dirs.claude.userSettings, dirs.claude);
  }
  if (codex) {
    line("codex-cli", "hooks", dirs.codex.hooksJson, dirs.codex);
    line("codex-cli", "mcp", dirs.codex.configToml, dirs.codex);
  }
  if (steps.some((s) => s.id === "W5")) line("opencode", "mcp", dirs.opencode.configJson, dirs.opencode);
  return L;
}

/** FR-51: once, when any harness in the plan resolves through an override. */
function sameEnvNote(dirs, steps, codex) {
  const vars = [];
  const add = (b) => { if (b.source !== "default" && vars.indexOf(b.source) < 0) vars.push(b.source); };
  if (steps.some((s) => s.harness === "claude-code")) add(dirs.claude);
  if (codex) add(dirs.codex);
  if (steps.some((s) => s.id === "W5")) add(dirs.opencode);
  return vars.length
    ? "note: start each harness with the same " + vars.join(", ") +
      " so it reads the files above; a harness started without them reads the default location"
    : null;
}

/** FR-52 / D-013: once, when the resolved .claude.json is written or changed. */
function claudeRunningNotice(manifest, claudeJson) {
  const hit = manifest.some((l) => l.harness === "claude-code" && l.path === claudeJson && NOTICE_ACTIONS.has(l.action));
  return hit
    ? "If Claude Code was running during this install, it may overwrite " + claudeJson +
      "; re-run 'npx unimatrix init' or check 'npx unimatrix status'"
    : null;
}

function emitWarnings(warnings) {
  for (const w of warnings) {
    try {
      process.stderr.write(w + "\n");
    } catch (_) { /* best effort */ }
  }
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
  const home = o.home || os.homedir();
  const platform = o.platform || process.platform;
  const env = o.env || process.env;
  const dryRun = o.dryRun === true;
  const harnessSel = o.harness || null;
  if (harnessSel && HARNESSES.indexOf(harnessSel) < 0) {
    return { actions: ["unknown --harness: " + harnessSel], manifest: [], warnings: [] };
  }
  const dirs = resolveConfigDirs({ env, home, platform });
  const warnings = dirs.warnings.slice();
  const transport = resolveTransport(projectRoot, o);
  const manifest = [];

  if (transport.kind === "none") {
    const reason = "no transport (binaryPath/credential absent)";
    manifest.push(skippedLeg("mcp", dirs.claude.claudeJson, reason));
    manifest.push(skippedLeg("hooks", path.join(projectRoot, ".claude", "settings.local.json"), reason));
    const actions = manifest.map((l) => legToActionLine(projectRoot, l, dryRun)).concat(warnings);
    emitWarnings(warnings);
    return { actions, manifest, warnings };
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
  manifest.push(...store.legs);

  const steps = buildWirePlan({
    root: projectRoot, home, platform, env, node, dirs, harnessSel,
    writeMcp: o.mcp !== false && !o.hooksOnly, hooksOnly: !!o.hooksOnly,
  });
  const r = runWirePlan(steps, { dryRun });
  manifest.push(...r.manifest);
  warnings.push(...r.warnings);

  const codex = codexProcessed(steps, r.manifest);
  const actions = resolvedLines(dirs, steps, codex).map((s) => pre(dryRun, s));
  const note = sameEnvNote(dirs, steps, codex);
  if (note) actions.push(pre(dryRun, note));
  actions.push(...manifest.map((l) => legToActionLine(projectRoot, l, dryRun)));
  const notice = claudeRunningNotice(manifest, dirs.claude.claudeJson);
  if (notice) actions.push(pre(dryRun, notice));
  actions.push(...warnings);
  emitWarnings(warnings);
  return { actions, manifest, warnings };
}

module.exports = { wire, detectHarnesses, resolveTransport, legToActionLine, HARNESSES };
