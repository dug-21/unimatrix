"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it } = require("node:test");

// Component tests — Wire orchestrator (nan-023, ADR-001/005). These prove the
// MANIFEST CONTRACT that the C14 verifier (c14-verifier.md) depends on: every
// dispatch path yields a well-formed WireLeg (including all skipped-*), the
// intent gate is applied once per surface, dry-run == real action set, and the
// claude common-path bytes stay byte-identical (SR-07). Behavioral fire/return
// (AC-09/AC-10) is the verifier's concern, NOT exercised here.

const { wire, detectHarnesses } = require("../lib/wire.js");

// A fixed, machine-independent binary path so the golden fixtures are stable.
const FIXED_BIN = "/opt/unimatrix/bin/unimatrix";
const CLIENT = "/opt/unimatrix/lib/hook-client/index.js";
const GOLDEN_DIR = path.join(__dirname, "fixtures", "wire", "golden");

/** Temp project root with .git (matches init/codex/opencode test idiom). */
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-wire-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function touchDir(dir, rel) {
  fs.mkdirSync(path.join(dir, rel), { recursive: true });
}
function writeFile(dir, rel, content) {
  const p = path.join(dir, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, content, "utf8");
}
function read(dir, rel) {
  return fs.readFileSync(path.join(dir, rel));
}

/** Every leg is a well-formed WireLeg (OVERVIEW invariants). */
function assertWellFormedLeg(leg) {
  assert.ok(leg && typeof leg === "object", "leg is an object");
  assert.ok(["claude-code", "opencode", "codex-cli"].includes(leg.harness), "harness set: " + leg.harness);
  assert.ok(["mcp", "retrieval", "hooks", "plugin"].includes(leg.surface), "surface set: " + leg.surface);
  const actions = [
    "created", "updated", "unchanged",
    "skipped-undetected", "skipped-malformed", "skipped-intent", "skipped",
  ];
  assert.ok(actions.includes(leg.action), "action valid: " + leg.action);
  assert.strictEqual(typeof leg.path, "string", "path is a string");
  const skipped = leg.action.indexOf("skipped") === 0;
  // reason present iff skipped-*
  assert.strictEqual(!!leg.reason, skipped, "reason present iff skipped: " + JSON.stringify(leg));
  if (!skipped) {
    if (leg.surface === "hooks") {
      assert.strictEqual(typeof leg.command, "string", "hooks success leg carries command");
    }
    if (leg.surface === "mcp" || leg.surface === "retrieval") {
      assert.ok(leg.entry && typeof leg.entry === "object", "mcp/retrieval success leg carries entry");
    }
  }
}

// ── Detection (AC-13, FR-15) ─────────────────────────────────────────

describe("detectHarnesses", () => {
  it("test_detectHarnesses_all_markers", () => {
    const dir = makeTempProject();
    writeFile(dir, ".mcp.json", "{}");
    writeFile(dir, "opencode.json", "{}");
    touchDir(dir, ".codex");
    assert.deepStrictEqual(detectHarnesses(dir), {
      "claude-code": true, opencode: true, "codex-cli": true,
    });
  });

  it("test_detectHarnesses_none", () => {
    const dir = makeTempProject();
    assert.deepStrictEqual(detectHarnesses(dir), {
      "claude-code": false, opencode: false, "codex-cli": false,
    });
  });

  it("test_detectHarnesses_partial", () => {
    const dir = makeTempProject();
    touchDir(dir, ".codex");
    assert.deepStrictEqual(detectHarnesses(dir), {
      "claude-code": false, opencode: false, "codex-cli": true,
    });
  });

  it("test_detectHarnesses_ignores_gemini", () => {
    const dir = makeTempProject();
    touchDir(dir, ".gemini");
    const d = detectHarnesses(dir);
    assert.ok(!("gemini-cli" in d) && !("gemini" in d), "gemini not in the detected set");
    assert.deepStrictEqual(d, { "claude-code": false, opencode: false, "codex-cli": false });
  });
});

// ── Backward-compat golden (SR-07 / C-10, AC-03/AC-10 claude leg) ─────

describe("claude common-path golden (SR-07)", () => {
  it("test_wire_claude_mcp_settings_byte_identical_to_golden", () => {
    const dir = makeTempProject();
    const res = wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });

    const goldMcp = fs.readFileSync(path.join(GOLDEN_DIR, "mcp.json"));
    const goldSettings = fs.readFileSync(path.join(GOLDEN_DIR, "settings.json"));
    assert.ok(read(dir, ".mcp.json").equals(goldMcp), ".mcp.json byte-identical to golden");
    assert.ok(
      read(dir, path.join(".claude", "settings.json")).equals(goldSettings),
      "settings.json byte-identical to golden"
    );

    // claude is dispatched on a fresh repo even with no marker (ADR-005 §3).
    const claudeMcp = res.manifest.find((l) => l.harness === "claude-code" && l.surface === "mcp");
    assert.ok(claudeMcp && claudeMcp.action === "created", "claude mcp created");
  });
});

// ── Manifest contract (R-11, R-01) ───────────────────────────────────

describe("manifest contract", () => {
  /** Multi-harness fixture with additive (pre-seeded) opencode + codex entries. */
  function multiHarnessFixture() {
    const dir = makeTempProject();
    // opencode detected + already has mcp.unimatrix → additive (not intent-gated).
    writeFile(dir, "opencode.json", JSON.stringify({ mcp: { unimatrix: { type: "local", command: ["old"], enabled: true } } }, null, 2) + "\n");
    // codex detected + owned table present → additive.
    writeFile(dir, ".codex/config.toml", "[mcp_servers.unimatrix]\ncommand = \"old\"\n");
    return dir;
  }

  it("test_wire_every_dispatch_returns_wellformed_leg", () => {
    const dir = multiHarnessFixture();
    const res = wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    for (const leg of res.manifest) assertWellFormedLeg(leg);

    const surfaces = res.manifest.map((l) => l.harness + "/" + l.surface);
    assert.ok(surfaces.includes("claude-code/mcp"), "claude mcp leg present");
    assert.ok(surfaces.includes("claude-code/hooks"), "claude hooks leg present");
    assert.ok(surfaces.includes("opencode/retrieval"), "opencode retrieval leg present");
    assert.ok(surfaces.includes("opencode/plugin"), "opencode plugin leg present");
    assert.ok(surfaces.includes("codex-cli/mcp"), "codex mcp leg present");
    assert.ok(surfaces.includes("codex-cli/hooks"), "codex hooks leg present");
  });

  it("test_wire_actions_and_manifest_consistent", () => {
    const dir = multiHarnessFixture();
    const res = wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    // actions are derived 1:1 from the manifest (ADR-001 §4).
    assert.strictEqual(res.actions.length, res.manifest.length, "one action line per leg");
  });

  it("test_wire_undetected_harness_yields_skipped_undetected_leg", () => {
    const dir = makeTempProject(); // no .codex marker
    const res = wire(dir, { harness: "codex-cli", binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    assert.strictEqual(res.manifest.length, 1, "single leg");
    const leg = res.manifest[0];
    assertWellFormedLeg(leg);
    assert.strictEqual(leg.action, "skipped-undetected");
    assert.ok(!fs.existsSync(path.join(dir, ".codex")), "no .codex written");
  });

  it("test_wire_skip_reasons_are_distinct", () => {
    // undetected (opencode) — explicit harness, no marker.
    const d1 = makeTempProject();
    const undetected = wire(d1, { harness: "opencode", binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false }).manifest[0];
    // malformed (opencode) — explicit harness, marker present, bad JSON.
    const d2 = makeTempProject();
    writeFile(d2, "opencode.json", "{ not json");
    const malformed = wire(d2, { harness: "opencode", binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false })
      .manifest.find((l) => l.surface === "retrieval");
    // intent (codex) — detected, new entry, no opt-in.
    const d3 = makeTempProject();
    touchDir(d3, ".codex");
    const intent = wire(d3, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false })
      .manifest.find((l) => l.harness === "codex-cli" && l.surface === "mcp");

    assert.strictEqual(undetected.action, "skipped-undetected");
    assert.strictEqual(malformed.action, "skipped-malformed");
    assert.strictEqual(intent.action, "skipped-intent");
    // three DISTINCT visible outcomes — a malformed skip is never a success.
    const kinds = new Set([undetected.action, malformed.action, intent.action]);
    assert.strictEqual(kinds.size, 3, "three distinct skip reasons");
  });
});

// ── Intent gate dispatch (AC-11, R-12) ───────────────────────────────

describe("intent gate", () => {
  it("test_wire_no_harness_gates_new_entry", () => {
    const dir = makeTempProject();
    writeFile(dir, "opencode.json", "{}"); // detected, no mcp.unimatrix
    touchDir(dir, ".codex"); // detected, no owned table
    const res = wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });

    const ocRetrieval = res.manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval");
    const cxMcp = res.manifest.find((l) => l.harness === "codex-cli" && l.surface === "mcp");
    assert.strictEqual(ocRetrieval.action, "skipped-intent", "opencode new entry gated");
    assert.strictEqual(cxMcp.action, "skipped-intent", "codex new entry gated");
    assert.ok(/--harness opencode/.test(ocRetrieval.reason), "help line names the exact command");
    assert.ok(/--harness codex-cli/.test(cxMcp.reason), "help line names the exact command");
  });

  it("test_wire_harness_opencode_opts_in_new_entry", () => {
    const dir = makeTempProject();
    writeFile(dir, "opencode.json", "{}");
    const res = wire(dir, { harness: "opencode", binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    const ocRetrieval = res.manifest.find((l) => l.surface === "retrieval");
    assert.strictEqual(ocRetrieval.action, "created", "explicit --harness opencode writes the new entry");
    // Only opencode targeted — no claude/codex legs.
    assert.ok(res.manifest.every((l) => l.harness === "opencode"), "only opencode dispatched");
  });

  it("test_wire_harness_selects_single", () => {
    const dir = makeTempProject();
    touchDir(dir, ".codex");
    const res = wire(dir, { harness: "codex-cli", binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    assert.ok(res.manifest.every((l) => l.harness === "codex-cli"), "only codex dispatched");
    const cxMcp = res.manifest.find((l) => l.surface === "mcp");
    assert.strictEqual(cxMcp.action, "created", "--harness codex-cli opts in the new MCP entry");
  });

  it("test_wire_claude_common_path_not_gated", () => {
    const dir = makeTempProject();
    const res = wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    const claudeMcp = res.manifest.find((l) => l.harness === "claude-code" && l.surface === "mcp");
    assert.ok(claudeMcp && claudeMcp.action !== "skipped-intent", "claude mcp never intent-gated");
    assert.ok(fs.existsSync(path.join(dir, ".mcp.json")), "claude .mcp.json written without opt-in");
  });
});

// ── Fail-safe posture (AC-15) ────────────────────────────────────────

describe("fail-safe posture", () => {
  it("test_wire_opencode_malformed_never_throws", () => {
    const dir = makeTempProject();
    writeFile(dir, "opencode.json", "{ not valid json");
    let res;
    assert.doesNotThrow(() => {
      res = wire(dir, { harness: "opencode", binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    });
    const leg = res.manifest.find((l) => l.surface === "retrieval");
    assert.strictEqual(leg.action, "skipped-malformed");
    // Preserved unchanged (no partial write).
    assert.strictEqual(read(dir, "opencode.json").toString(), "{ not valid json");
  });

  it("test_wire_claude_malformed_throws_backward_compat", () => {
    // SR-07: claude reused writers keep their loud-throw posture in the wire path.
    const dir = makeTempProject();
    writeFile(dir, ".mcp.json", "{ broken");
    assert.throws(
      () => wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false }),
      /Malformed \.mcp\.json/
    );
  });
});

// ── Idempotence + dry-run + containment ──────────────────────────────

describe("idempotence + dry-run", () => {
  function seededFixture() {
    const dir = makeTempProject();
    writeFile(dir, "opencode.json", JSON.stringify({ mcp: { unimatrix: { type: "local", command: ["x"], enabled: true } } }, null, 2) + "\n");
    writeFile(dir, ".codex/config.toml", "[mcp_servers.unimatrix]\ncommand = \"x\"\n");
    return dir;
  }

  it("test_wire_run_twice_files_byte_identical", () => {
    const dir = seededFixture();
    wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false }); // normalize
    const snap = {
      mcp: read(dir, ".mcp.json"),
      settings: read(dir, path.join(".claude", "settings.json")),
      opencode: read(dir, "opencode.json"),
      codexToml: read(dir, path.join(".codex", "config.toml")),
      codexHooks: read(dir, path.join(".codex", "hooks.json")),
    };
    wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false }); // re-run
    assert.ok(read(dir, ".mcp.json").equals(snap.mcp), ".mcp.json stable");
    assert.ok(read(dir, path.join(".claude", "settings.json")).equals(snap.settings), "settings.json stable");
    assert.ok(read(dir, "opencode.json").equals(snap.opencode), "opencode.json stable");
    assert.ok(read(dir, path.join(".codex", "config.toml")).equals(snap.codexToml), "config.toml stable");
    assert.ok(read(dir, path.join(".codex", "hooks.json")).equals(snap.codexHooks), "hooks.json stable");
  });

  it("test_wire_second_run_reports_unchanged_for_writer_surfaces", () => {
    const dir = seededFixture();
    wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    const res = wire(dir, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    const ocRetrieval = res.manifest.find((l) => l.harness === "opencode" && l.surface === "retrieval");
    const cxMcp = res.manifest.find((l) => l.harness === "codex-cli" && l.surface === "mcp");
    assert.strictEqual(ocRetrieval.action, "unchanged", "opencode retrieval idempotent");
    assert.strictEqual(cxMcp.action, "unchanged", "codex mcp idempotent");
  });

  it("test_wire_dryRun_manifest_equals_real_and_writes_nothing", () => {
    const real = seededFixture();
    const dry = seededFixture();
    const resReal = wire(real, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: false });
    // A fresh dry-run fixture (identical starting state) must yield the same
    // (harness, surface, action) set (NFR-10 — same manifest drives both).
    const dryFixture = makeTempProject();
    writeFile(dryFixture, "opencode.json", JSON.stringify({ mcp: { unimatrix: { type: "local", command: ["x"], enabled: true } } }, null, 2) + "\n");
    writeFile(dryFixture, ".codex/config.toml", "[mcp_servers.unimatrix]\ncommand = \"x\"\n");

    const before = {
      opencode: read(dryFixture, "opencode.json").toString(),
      codexToml: read(dryFixture, path.join(".codex", "config.toml")).toString(),
    };
    const resDry = wire(dryFixture, { binaryPath: FIXED_BIN, clientPath: CLIENT, dryRun: true });

    const tuple = (l) => l.harness + "/" + l.surface + "/" + l.action;
    assert.deepStrictEqual(
      resDry.manifest.map(tuple).sort(),
      resReal.manifest.map(tuple).sort(),
      "dry-run action set == real action set"
    );
    // Dry-run wrote nothing (existing files unchanged, no new claude files).
    assert.strictEqual(read(dryFixture, "opencode.json").toString(), before.opencode, "opencode untouched");
    assert.strictEqual(read(dryFixture, path.join(".codex", "config.toml")).toString(), before.codexToml, "config.toml untouched");
    assert.ok(!fs.existsSync(path.join(dryFixture, ".mcp.json")), "no .mcp.json written in dry-run");
    // Every dry-run action line carries the [dry-run] prefix.
    assert.ok(resDry.actions.every((a) => a.startsWith("[dry-run] ")), "dry-run prefix on every line");
    void dry;
  });
});
