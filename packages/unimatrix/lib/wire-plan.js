"use strict";

/**
 * Wiring plan (1036 C4, ADR-004 #5959). One ordered step table drives writes,
 * removals, gating and dry-run: both modes call the same `step.run(dryRun)`, so
 * the dry-run manifest equals the real one (FR-60). Order is ARCHITECTURE §3.3
 * (store runs before, in wire.js): W1, W2(+R1), R2, R3, R4, R6, Codex, W5, R10,
 * R11(+R12), X1. Errors are never caught here: a thrown SafeWriteError or parse
 * abort ends the run, so no later removal executes (SR-11).
 */

const path = require("path");
const { wireProjectHooks, removeOwnedHooksLeg } = require("./merge-settings.js");
const { claudeMcpLegs, removeOwnedMcpJsonLeg } = require("./claude-json.js");
const { buildMcpEntry } = require("./mcp-entry.js");
const { maybeWireCodex } = require("./codex-install.js");
const {
  writeOpencodeMcp, removeOwnedOpencodeUserMcpLeg, removeOwnedOpencodeProjectLegs, detectsOpenCode,
} = require("./opencode-install.js");
const { ensureSettingsLocalIgnored } = require("./git-exclude.js");
const { sameRealFile } = require("./config-dirs.js");

const OK_ACTIONS = new Set(["created", "updated", "unchanged"]);
const CROSS = "owned hooks removed (cross-scope)";
const LEGACY = "owned hooks removed (legacy location)";

/**
 * @param {object} ctx {root, home, platform, env, node, dirs, harnessSel, writeMcp, hooksOnly, gitRun?}
 * @returns {object[]} Step[] — {id, harness, surface, file, source, gate?, run(dryRun) -> WireLeg[]}
 */
function buildWirePlan(ctx) {
  const { root, home, platform, env, node, dirs: d } = ctx;
  const sel = ctx.harnessSel || null;
  const want = (h) => !sel || sel === h;
  const same = (a, b) => sameRealFile(a, b, platform);
  const steps = [];
  const add = (s) => steps.push(Object.assign({ source: undefined }, s));

  if (want("claude-code")) {
    const cc = { harness: "claude-code", surface: "hooks" };
    add({ ...cc, id: "W1", file: path.join(root, ".claude", "settings.local.json"),
      run: (dr) => wireProjectHooks({ root, platform, home, dryRun: dr }) });
    if (ctx.writeMcp) {
      const file = d.claude.claudeJson;
      add({ harness: "claude-code", surface: "mcp", id: "W2", file, source: d.claude.source,
        run: (dr) => claudeMcpLegs({ file, projectRoot: root, entry: buildMcpEntry(platform, home, node), platform, dryRun: dr }) });
    }
    const projSettings = path.join(root, ".claude", "settings.json");
    add({ ...cc, id: "R2", file: projSettings, gate: "W1",
      run: (dr) => removeOwnedHooksLeg(projSettings, { dryRun: dr, containWithin: root, reason: CROSS }) });
    const us = d.claude.userSettings;
    if (!same(projSettings, us)) {
      add({ ...cc, id: "R3", file: us, gate: "W1", source: d.claude.source,
        run: (dr) => removeOwnedHooksLeg(us, { dryRun: dr, reason: CROSS }) });
    }
    const lus = d.legacy.userSettings;
    if (d.claude.source !== "default" && !same(lus, us) && !same(lus, projSettings)) {
      add({ ...cc, id: "R4", file: lus, gate: "W1",
        run: (dr) => removeOwnedHooksLeg(lus, { dryRun: dr, reason: LEGACY }) });
    }
    if (!ctx.hooksOnly) {
      add({ harness: "claude-code", surface: "mcp", id: "R6", file: path.join(root, ".mcp.json"),
        run: (dr) => removeOwnedMcpJsonLeg(root, dr) });
    }
  }

  if (!ctx.hooksOnly && want("codex-cli")) {
    add({ harness: "codex-cli", surface: "mcp", id: "CX", file: d.codex.dir, source: d.codex.source,
      run: (dr) => maybeWireCodex({ home, env, platform, node, projectRoot: root, harnessSel: sel, dryRun: dr, dirs: d,
        pathLookup: ctx.pathLookup }) });
  }

  if (!ctx.hooksOnly && want("opencode") && (sel === "opencode" || detectsOpenCode(root))) {
    const file = d.opencode.configJson;
    const legacyOc = d.opencode.source !== "default" && !same(d.legacy.opencodeJson, file) ? d.legacy.opencodeJson : null;
    add({ harness: "opencode", surface: "retrieval", id: "W5", file, source: d.opencode.source,
      run: (dr) => [writeOpencodeMcp({ file, legacyFile: legacyOc, platform, home, node, harnessSelected: sel === "opencode", dryRun: dr })] });
    if (legacyOc) {
      add({ harness: "opencode", surface: "retrieval", id: "R10", file: legacyOc, gate: "W5",
        run: (dr) => removeOwnedOpencodeUserMcpLeg(legacyOc, dr) });
    }
  }

  if (!ctx.hooksOnly) {
    add({ harness: "opencode", surface: "mcp", id: "R11", file: path.join(root, "opencode.json"),
      run: (dr) => removeOwnedOpencodeProjectLegs(root, dr) });
  }

  if (want("claude-code")) {
    add({ harness: "claude-code", surface: "git-exclude", id: "X1", file: root, gate: "W1",
      run: (dr) => [ensureSettingsLocalIgnored(root, { dryRun: dr, run: ctx.gitRun })] });
  }
  return steps;
}

/**
 * Execute steps in order. Gated steps whose destination did not end ok emit
 * `skipped-dest-unwired` without running (both modes). Throws propagate.
 * @returns {{manifest: object[], warnings: string[]}}
 */
function runWirePlan(steps, opts) {
  const dryRun = !!(opts && opts.dryRun);
  const manifest = [];
  const warnings = [];
  const byId = {};
  for (const s of steps) {
    if (s.gate) {
      const g = byId[s.gate];
      if (!g || !g.length || !g.every((l) => OK_ACTIONS.has(l.action))) {
        const dest = g && g[0] ? g[0].path : s.gate;
        const legs = [{ harness: s.harness, surface: s.surface, action: "skipped-dest-unwired", path: s.file,
          reason: "destination " + dest + " not wired" }];
        byId[s.id] = legs;
        manifest.push(...legs);
        continue;
      }
    }
    const legs = s.run(dryRun) || [];
    for (const w of legs.warnings || []) warnings.push(w);
    for (const l of legs) {
      if (l.warning) warnings.push(l.warning);
      if (s.source && s.source !== "default" && !l.source) l.source = s.source;
    }
    byId[s.id] = legs;
    manifest.push(...legs);
  }
  return { manifest, warnings };
}

module.exports = { buildWirePlan, runWirePlan, OK_ACTIONS };
