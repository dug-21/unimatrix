"use strict";

/**
 * Wire orchestrator — `lib/wire.js` (nan-023, ADR-001 / ADR-005).
 *
 * The single wiring entry both `init` and the `wire` verb call: detect harnesses
 * by project-local markers, select the target set, dispatch the per-(harness x
 * surface) writers, aggregate every `WireLeg` into the manifest the C14 verifier
 * and the `--dry-run` printer consume. NO harness file-format logic lives here —
 * that is in the writers (`init.js` claude, `opencode-install.js`, `codex-install.js`).
 *
 * Error posture (two coexisting boundaries — ADR-001 §5): claude-code reuses the
 * loud `writeMcpJson`/`mergeSettings` writers UNCHANGED (SR-07 byte-for-byte),
 * which THROW on malformed claude config exactly as `init` does today (the throw
 * propagates to `bin`). opencode/codex dispatch is fail-safe: the writers
 * warn-and-skip and never throw (AC-15); a defensive try/catch converts any
 * unexpected throw into a `skipped-malformed` leg (paths only, no secrets).
 *
 * Intent gate (#960, ADR-005 §3): the orchestrator does NOT gate — the
 * user-owned-config writers SELF-GATE at a single site (AC-11): `writeOpencodeMcp`
 * reads `harnessSelected`, `writeCodexMcpToml` reads `harnessSel`; the
 * orchestrator only forwards the `--harness` opt-in signal. claude-code is the
 * historically-wired common path and is NEVER gated (SR-07).
 *
 * `init.js` is required LAZILY inside the claude dispatch — init.js requires this
 * module, so a top-level require would be a partial-exports cycle.
 */

const path = require("path");
const fs = require("fs");

const {
  buildHookClientCommand,
  HOOK_EVENTS,
  subagentStopEnabled,
  normalizeCommandSource,
} = require("./merge-settings.js");
const { writeOpencodeMcp, maybeProvisionOpenCode } = require("./opencode-install.js");
const { maybeWireCodex } = require("./codex-install.js");
const { computeProjectHash } = require("./hook-client/config.js");

const HARNESSES = ["claude-code", "opencode", "codex-cli"];
const SUBAGENT_STOP = "SubagentStop";

/** Best-effort existence check; never throws (mirror opencode `exists`). */
function existsSafe(p) {
  try {
    return fs.existsSync(p);
  } catch (_err) {
    return false;
  }
}

/**
 * Detect harnesses by PROJECT-LOCAL markers only (FR-15, AC-13). Best-effort;
 * never throws. `.gemini` is intentionally NOT detected (out of scope).
 */
function detectHarnesses(dir) {
  return {
    "claude-code": existsSafe(path.join(dir, ".mcp.json")) || existsSafe(path.join(dir, ".claude")),
    opencode: existsSafe(path.join(dir, "opencode.json")) || existsSafe(path.join(dir, ".opencode")),
    "codex-cli": existsSafe(path.join(dir, ".codex")),
  };
}

/** The project-local marker file that governs a harness's detection. */
function markerPathFor(h, root) {
  if (h === "opencode") return path.join(root, "opencode.json");
  if (h === "codex-cli") return path.join(root, ".codex");
  return path.join(root, ".mcp.json");
}

/** The surface a bare undetected/skipped leg is attributed to. */
function primarySurface(h) {
  return h === "opencode" ? "retrieval" : "mcp";
}

function undetectedLeg(h, root) {
  return {
    harness: h,
    surface: primarySurface(h),
    action: "skipped-undetected",
    path: markerPathFor(h, root),
    reason: h + " marker not present",
  };
}

function skippedLeg(h, surface, legPath, reason) {
  return { harness: h, surface, action: "skipped-undetected", path: legPath, reason };
}

/**
 * Resolve ONE transport descriptor (OVERVIEW Transport). `binaryPath` => local
 * stdio-binary; `mcp` => cloud token-free stdio-bridge (NEVER a `url=`/token
 * entry — Q1/Principle 8). Neither => "none" (MCP writers emit a visible skipped
 * leg; do NOT throw).
 */
function resolveTransport(projectRoot, opts) {
  if (opts.binaryPath) {
    return { kind: "stdio-binary", binaryPath: opts.binaryPath };
  }
  if (opts.mcp) {
    let bridgePath = opts.bridgePath;
    if (!bridgePath) {
      try {
        bridgePath = require.resolve("./hook-client/mcp-bridge.js");
      } catch (_err) {
        bridgePath = path.join(__dirname, "hook-client", "mcp-bridge.js");
      }
    }
    return { kind: "stdio-bridge", bridgePath, projectHash: computeProjectHash(projectRoot) };
  }
  return { kind: "none" };
}

/**
 * claude-code dispatch — reuse `writeMcpJson`/`writeMcpBridgeEntry` (mcp) and
 * `mergeSettings` (hooks) UNCHANGED so the common-path bytes stay identical
 * (SR-07). Never intent/detection-gated (ADR-005 §3). One hooks WireLeg PER
 * managed event carries the exact command (manifest fidelity, R-02).
 */
function dispatchClaude(root, ctx, manifest) {
  const t = ctx.transport;
  const mcpPath = path.join(root, ".mcp.json");
  const settingsPath = path.join(root, ".claude", "settings.json");

  if (t.kind === "none") {
    manifest.push(skippedLeg("claude-code", "mcp", mcpPath, "no transport (binaryPath/mcp absent)"));
    manifest.push(skippedLeg("claude-code", "hooks", settingsPath, "no transport (binaryPath/mcp absent)"));
    return;
  }

  const initMod = require("./init.js"); // lazy — breaks the init<->wire cycle.
  const { mergeSettings } = require("./merge-settings.js");

  // --- MCP surface ---
  let mcpActions;
  let entry;
  if (t.kind === "stdio-bridge") {
    mcpActions = initMod.writeMcpBridgeEntry(root, t.bridgePath, t.projectHash, ctx.dryRun);
    entry = { command: "node", args: [t.bridgePath, t.projectHash], env: {} };
  } else {
    mcpActions = initMod.writeMcpJson(root, t.binaryPath, ctx.dryRun);
    entry = {
      command: t.binaryPath,
      args: [],
      env: { LD_LIBRARY_PATH: path.dirname(t.binaryPath) },
    };
  }
  const mcpAction = mcpActions.some((a) => a.includes("Created")) ? "created" : "updated";
  manifest.push({ harness: "claude-code", surface: "mcp", action: mcpAction, path: mcpPath, entry });

  // --- hooks surface (one leg per managed event) ---
  const commandSource =
    t.kind === "stdio-bridge"
      ? { events: HOOK_EVENTS, commandForEvent: (e) => buildHookClientCommand(ctx.clientPath, e) }
      : t.binaryPath; // legacy local string form → byte-identical to init Step 4.

  const existedBefore = existsSafe(settingsPath);
  const result = mergeSettings(settingsPath, commandSource, { dryRun: ctx.dryRun });

  const source = normalizeCommandSource(commandSource);
  const optInFile = path.join(path.dirname(settingsPath), "settings.local.json");
  let events = source.events;
  if (events.includes(SUBAGENT_STOP) && !subagentStopEnabled(optInFile)) {
    events = events.filter((e) => e !== SUBAGENT_STOP);
  }
  const changed = result.actions.some((a) => /Added|Updated|Removed/.test(a));
  const hookAction = existedBefore ? (changed ? "updated" : "unchanged") : "created";
  for (const event of events) {
    manifest.push({
      harness: "claude-code",
      surface: "hooks",
      action: hookAction,
      path: settingsPath,
      command: source.commandForEvent(event),
    });
  }
}

/** opencode dispatch — plugin surface (additive, NOT gated) via the reused
 * `maybeProvisionOpenCode` + retrieval surface (self-gated) via `writeOpencodeMcp`. */
function dispatchOpencode(root, ctx, manifest) {
  try {
    const pluginActions = maybeProvisionOpenCode(root, { dryRun: ctx.dryRun });
    if (pluginActions.length > 0) {
      const wrote = pluginActions.some((a) => /Provision|Updated|Added|Append|Would/.test(a));
      manifest.push({
        harness: "opencode",
        surface: "plugin",
        action: wrote ? "updated" : "unchanged",
        path: path.join(root, ".opencode"),
      });
    }
    manifest.push(
      writeOpencodeMcp(
        root,
        { transport: ctx.transport, harnessSelected: ctx.harnessSel === "opencode" },
        ctx.dryRun
      )
    );
  } catch (e) {
    manifest.push({
      harness: "opencode",
      surface: "retrieval",
      action: "skipped-malformed",
      path: path.join(root, "opencode.json"),
      reason: "opencode wiring error: " + e.message,
    });
  }
}

/** codex dispatch — `maybeWireCodex` fans out to MCP TOML (self-gated) + one
 * hooks leg per event, returning `WireLeg[]` (already fail-safe internally). */
function dispatchCodex(root, ctx, manifest) {
  try {
    const legs = maybeWireCodex(root, {
      clientPath: ctx.clientPath,
      transport: ctx.transport,
      harnessSel: ctx.harnessSel,
      dryRun: ctx.dryRun,
    });
    for (const l of legs) manifest.push(l);
  } catch (e) {
    manifest.push({
      harness: "codex-cli",
      surface: "mcp",
      action: "skipped-malformed",
      path: path.join(root, ".codex", "config.toml"),
      reason: "codex wiring error: " + e.message,
    });
  }
}

/** Render one manifest leg into a human summary line (dry-run parity, AC-14). */
function legToActionLine(root, leg, dryRun) {
  const rel = path.relative(root, leg.path);
  const loc = rel && !rel.startsWith("..") ? rel : leg.path;
  let s = leg.harness + " " + leg.surface + ": " + leg.action + " " + loc;
  if (leg.reason) s += " (" + leg.reason + ")";
  return dryRun ? "[dry-run] " + s : s;
}

/**
 * Detect → select → dispatch → aggregate. Returns `{ actions, manifest }` where
 * the manifest (`WireLeg[]`) is the single source of truth for the C14 verifier,
 * the dry-run printer, and the summary (ADR-001 §4). Never throws for an
 * opencode/codex leg; a claude loud-checkpoint throw propagates (SR-07).
 *
 * @param {string} projectRoot - Absolute project root (caller resolves it).
 * @param {object} opts - { harness?, clientPath, binaryPath?, mcp?, bridgePath?, dryRun }
 */
function wire(projectRoot, opts) {
  const options = opts || {};
  const dryRun = options.dryRun === true;
  const harnessSel = options.harness || null;
  const transport = resolveTransport(projectRoot, options);
  const manifest = [];

  // Validate an explicit --harness value (ADR-005 §2): unknown => no writes.
  if (harnessSel && HARNESSES.indexOf(harnessSel) < 0) {
    return { actions: ["unknown --harness: " + harnessSel], manifest: [] };
  }

  const detected = detectHarnesses(projectRoot);
  const targets = harnessSel ? [harnessSel] : HARNESSES.slice();
  const ctx = { transport, clientPath: options.clientPath, harnessSel, dryRun };

  for (const h of targets) {
    if (h === "claude-code") {
      // Common path: always ensured, never detection/intent-gated (ADR-005 §3).
      dispatchClaude(projectRoot, ctx, manifest);
      continue;
    }
    if (!detected[h]) {
      // Explicit --harness on an undetected harness => a VISIBLE skip (AC-13).
      // In the process-all-detected path an undetected harness contributes none.
      if (harnessSel) manifest.push(undetectedLeg(h, projectRoot));
      continue;
    }
    if (h === "opencode") dispatchOpencode(projectRoot, ctx, manifest);
    else if (h === "codex-cli") dispatchCodex(projectRoot, ctx, manifest);
  }

  const actions = manifest.map((leg) => legToActionLine(projectRoot, leg, dryRun));
  return { actions, manifest };
}

module.exports = {
  wire,
  detectHarnesses,
  resolveTransport,
  legToActionLine,
  HARNESSES,
};
