"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it } = require("node:test");

// Component tests — Codex installer Wave B (nan-023, ADR-001/002/003/006). These
// drive the real fs-touching writers against on-disk `.git` fixtures. The
// BYTE-PRESERVATION (R-04), COMMAND-STRING (R-05/AC-08), TOML-ESCAPE (R-10) and
// WireLeg (SR-09) assertions live here. Behavioral fire/return (AC-09/AC-10) is
// the C14 verifier's concern (c14-verifier.md), NOT exercised here.

const {
  maybeWireCodex,
  writeCodexMcpToml,
  writeCodexHooks,
  requireProviderFlag,
  resolveCodexTransport,
  CODEX_EVENTS,
  CONFIG_FILE,
  HOOKS_FILE,
} = require("../lib/codex-install.js");
const { PRETOOLUSE_CYCLE_MATCHER } = require("../lib/merge-settings.js");

/** Temp project root with .git (matches init.test.js / opencode idiom). */
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-codex-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function cfgPath(dir) {
  return path.join(dir, CONFIG_FILE);
}
function hooksPath(dir) {
  return path.join(dir, HOOKS_FILE);
}
function writeCfg(dir, raw) {
  fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
  fs.writeFileSync(cfgPath(dir), raw, "utf8");
}
function writeHooks(dir, obj) {
  fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
  fs.writeFileSync(hooksPath(dir), JSON.stringify(obj, null, 2) + "\n", "utf8");
}

const BIN = "/abs/path/unimatrix";
const CLIENT = "/abs/path/lib/hook-client/index.js";

// Minimal TOML BASIC-string decoder — R-10 read-back oracle (mirrors the Wave A
// test). Proves an escaped value re-parses to the intended value.
function parseTomlBasicString(literal) {
  assert.ok(literal.startsWith('"') && literal.endsWith('"'), "not a basic string: " + literal);
  const inner = literal.slice(1, -1);
  let out = "";
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i];
    if (ch !== "\\") {
      out += ch;
      continue;
    }
    const esc = inner[i + 1];
    i += 1;
    switch (esc) {
      case "\\": out += "\\"; break;
      case '"': out += '"'; break;
      case "b": out += "\b"; break;
      case "t": out += "\t"; break;
      case "n": out += "\n"; break;
      case "f": out += "\f"; break;
      case "r": out += "\r"; break;
      case "u":
        out += String.fromCodePoint(parseInt(inner.slice(i + 1, i + 5), 16));
        i += 4;
        break;
      default: assert.fail("unknown escape \\" + esc);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// writeCodexMcpToml — MCP TOML (AC-06, R-04)
// ---------------------------------------------------------------------------

describe("writeCodexMcpToml", () => {
  it("test_writeCodexMcpToml_fresh_creates_owned_table", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const l = writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: BIN }, harnessSel: "codex-cli" },
      false
    );
    assert.strictEqual(l.surface, "mcp");
    assert.strictEqual(l.action, "created");
    assert.strictEqual(l.path, cfgPath(dir));
    assert.deepStrictEqual(l.entry, { command: BIN });
    const disk = fs.readFileSync(cfgPath(dir), "utf8");
    assert.ok(disk.includes("[mcp_servers.unimatrix]"));
    assert.ok(disk.includes('command = "' + BIN + '"'));
  });

  it("test_writeCodexMcpToml_local_command_shape", () => {
    const dir = makeTempProject();
    writeCfg(dir, "");
    writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: BIN }, harnessSel: "codex-cli" },
      false
    );
    const disk = fs.readFileSync(cfgPath(dir), "utf8");
    assert.ok(disk.includes('command = "' + BIN + '"'));
    assert.ok(!/^\s*url\s*=/m.test(disk));
  });

  it("test_writeCodexMcpToml_cloud_bridge_shape (no url, no token — Q1/Principle 8)", () => {
    const dir = makeTempProject();
    writeCfg(dir, "");
    const secret = "SECRET-BEARER-TOKEN";
    const l = writeCodexMcpToml(
      dir,
      {
        transport: { kind: "stdio-bridge", bridgePath: "/abs/mcp-bridge.js", projectHash: "abc123" },
        harnessSel: "codex-cli",
      },
      false
    );
    assert.deepStrictEqual(l.entry, {
      command: "node",
      args: ["/abs/mcp-bridge.js", "abc123"],
    });
    const disk = fs.readFileSync(cfgPath(dir), "utf8");
    assert.ok(disk.includes('command = "node"'));
    assert.ok(disk.includes('args = ["/abs/mcp-bridge.js", "abc123"]'));
    assert.ok(!/^\s*url\s*=/m.test(disk), "must NEVER emit url =");
    assert.ok(!disk.includes(secret));
  });

  it("test_writeCodexMcpToml_preserves_foreign_tables_comments_order", () => {
    const dir = makeTempProject();
    const raw = [
      "# header",
      "[tools]",
      "zeta = 1",
      "alpha = 2",
      "",
      "[mcp_servers.other]",
      'command = "other-bin"',
      "",
    ].join("\n");
    writeCfg(dir, raw);
    const l = writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: BIN }, harnessSel: "codex-cli" },
      false
    );
    assert.strictEqual(l.action, "created"); // owned absent → created
    const disk = fs.readFileSync(cfgPath(dir), "utf8");
    assert.ok(disk.startsWith(raw)); // every foreign byte survives as a prefix
    assert.ok(disk.includes("[mcp_servers.other]"));
    assert.ok(disk.includes("zeta = 1"));
    assert.ok(disk.includes("[mcp_servers.unimatrix]"));
  });

  it("test_writeCodexMcpToml_update_in_place_stale_value", () => {
    const dir = makeTempProject();
    const raw =
      '[tools]\na = 1\n\n[mcp_servers.unimatrix]\ncommand = "STALE"\n\n[mcp_servers.other]\ncommand = "keep"\n';
    writeCfg(dir, raw);
    const l = writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: BIN }, harnessSel: undefined },
      false
    );
    // Present → intent gate does not fire even without harnessSel; updated.
    assert.strictEqual(l.action, "updated");
    const disk = fs.readFileSync(cfgPath(dir), "utf8");
    assert.ok(disk.includes('command = "' + BIN + '"'));
    assert.ok(!disk.includes("STALE"));
    assert.ok(disk.endsWith('[mcp_servers.other]\ncommand = "keep"\n'));
  });

  it("test_writeCodexMcpToml_idempotent", () => {
    const dir = makeTempProject();
    writeCfg(dir, "");
    const opts = {
      transport: { kind: "stdio-binary", binaryPath: BIN },
      harnessSel: "codex-cli",
    };
    writeCodexMcpToml(dir, opts, false);
    const first = fs.readFileSync(cfgPath(dir), "utf8");
    const l2 = writeCodexMcpToml(dir, opts, false);
    assert.strictEqual(l2.action, "unchanged");
    const second = fs.readFileSync(cfgPath(dir), "utf8");
    assert.strictEqual(first, second); // byte-identical (AC-04)
  });

  it("test_writeCodexMcpToml_intent_gate_blocks_new_entry_without_harness (AC-11)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const l = writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: BIN }, harnessSel: undefined },
      false
    );
    assert.strictEqual(l.action, "skipped-intent");
    assert.ok(/--harness codex-cli/.test(l.reason));
    assert.ok(!fs.existsSync(cfgPath(dir)), "must not write on intent-skip");
  });

  it("test_codex_toml_quote_metacharacter_path (R-10)", () => {
    const dir = makeTempProject();
    writeCfg(dir, "");
    const evil = '/opt/my apps/uni"; command = "pwned';
    writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: evil }, harnessSel: "codex-cli" },
      false
    );
    const disk = fs.readFileSync(cfgPath(dir), "utf8");
    const m = /^command\s*=\s*(".*?")\s*$/m.exec(disk.split("[mcp_servers.unimatrix]")[1]);
    assert.ok(m, "command line not found");
    assert.strictEqual(parseTomlBasicString(m[1]), evil); // re-parses to intended
    // The injected `command = "pwned` fragment is NOT a second real key.
    assert.strictEqual(
      (disk.match(/^command\s*=/gm) || []).length,
      1
    );
  });

  it("test_maybeWireCodex_malformed_toml_skips_preserves (AC-15)", () => {
    const dir = makeTempProject();
    const raw = "[tools]\nnote = \"\"\"\nunclosed\n"; // unterminated multi-line string
    writeCfg(dir, raw);
    let threw = false;
    let legs;
    try {
      legs = maybeWireCodex(dir, {
        clientPath: CLIENT,
        binaryPath: BIN,
        harnessSel: "codex-cli",
        dryRun: false,
      });
    } catch (_e) {
      threw = true;
    }
    assert.strictEqual(threw, false, "never throws");
    const mcpLeg = legs.find((l) => l.surface === "mcp");
    assert.strictEqual(mcpLeg.action, "skipped-malformed");
    assert.strictEqual(fs.readFileSync(cfgPath(dir), "utf8"), raw); // byte-preserved
  });

  it("dry-run writes nothing (AC-14)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const l = writeCodexMcpToml(
      dir,
      { transport: { kind: "stdio-binary", binaryPath: BIN }, harnessSel: "codex-cli" },
      true
    );
    assert.strictEqual(l.action, "created");
    assert.deepStrictEqual(l.entry, { command: BIN });
    assert.ok(!fs.existsSync(cfgPath(dir)), "dry-run must not write");
  });
});

// ---------------------------------------------------------------------------
// writeCodexHooks — hooks JSON (AC-07, AC-08, R-05)
// ---------------------------------------------------------------------------

describe("writeCodexHooks", () => {
  it("test_writeCodexHooks_writes_seven_event_set", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const legs = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    assert.strictEqual(legs.length, 7);
    const disk = JSON.parse(fs.readFileSync(hooksPath(dir), "utf8"));
    const keys = Object.keys(disk.hooks).sort();
    assert.deepStrictEqual(keys, CODEX_EVENTS.slice().sort());
    assert.ok(!("PostToolUseFailure" in disk.hooks));
    assert.ok(!("SubagentStop" in disk.hooks));
    // PreToolUse carries the cycle matcher; PostToolUse/SubagentStart carry "*".
    assert.strictEqual(disk.hooks.PreToolUse[0].matcher, PRETOOLUSE_CYCLE_MATCHER);
    assert.strictEqual(disk.hooks.PostToolUse[0].matcher, "*");
    assert.strictEqual(disk.hooks.SubagentStart[0].matcher, "*");
    assert.strictEqual(disk.hooks.SessionStart[0].matcher, "");
  });

  it("test_writeCodexHooks_every_command_targets_hook_client_not_binary (AC-08/C-03)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    const disk = JSON.parse(fs.readFileSync(hooksPath(dir), "utf8"));
    for (const event of CODEX_EVENTS) {
      const cmd = disk.hooks[event][0].hooks[0].command;
      assert.ok(cmd.includes("hook-client/index.js"), event + " must target the JS client");
      assert.ok(!/\bunimatrix\s+hook\b/.test(cmd), event + " must NOT target the binary");
      assert.ok(cmd.startsWith("node "), event + " must invoke node");
    }
  });

  it("test_writeCodexHooks_every_command_carries_provider_flag (AC-07/NFR-07)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const legs = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    const disk = JSON.parse(fs.readFileSync(hooksPath(dir), "utf8"));
    for (const event of CODEX_EVENTS) {
      assert.ok(
        disk.hooks[event][0].hooks[0].command.includes("--provider codex-cli"),
        event + " missing --provider codex-cli"
      );
    }
    for (const l of legs) {
      assert.ok(l.command.includes("--provider codex-cli"));
    }
  });

  it("test_writeCodexHooks_command_is_exact_wireleg_command", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const legs = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    const disk = JSON.parse(fs.readFileSync(hooksPath(dir), "utf8"));
    // Every leg's command equals the byte-exact string on disk for its event.
    const onDisk = new Set();
    for (const event of CODEX_EVENTS) {
      onDisk.add(disk.hooks[event][0].hooks[0].command);
    }
    for (const l of legs) {
      assert.ok(onDisk.has(l.command), "leg command not found on disk: " + l.command);
    }
  });

  it("test_writeCodexHooks_provider_flag_fail_loud (NFR-07)", () => {
    // The mandatory-flag invariant throws on a flag-less command.
    assert.throws(
      () => requireProviderFlag("node /x/index.js PreToolUse"),
      /missing --provider codex-cli/
    );
    assert.doesNotThrow(() =>
      requireProviderFlag("node /x/index.js PreToolUse --provider codex-cli")
    );
  });

  it("test_writeCodexHooks_preserves_foreign_hooks", () => {
    const dir = makeTempProject();
    const foreign = {
      hooks: {
        PostToolUse: [
          { matcher: "*", hooks: [{ type: "command", command: "/usr/bin/foreign-tool" }] },
        ],
        Notification: [{ matcher: "", hooks: [{ type: "command", command: "beep" }] }],
      },
    };
    writeHooks(dir, foreign);
    writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    const disk = JSON.parse(fs.readFileSync(hooksPath(dir), "utf8"));
    // Foreign entries preserved.
    assert.ok(
      disk.hooks.PostToolUse.some((g) =>
        g.hooks.some((h) => h.command === "/usr/bin/foreign-tool")
      )
    );
    assert.ok(disk.hooks.Notification[0].hooks[0].command === "beep");
    // Uni entry added alongside in the "*" group.
    assert.ok(
      disk.hooks.PostToolUse.some((g) =>
        g.hooks.some((h) => h.command.includes("hook-client/index.js"))
      )
    );
  });

  it("test_writeCodexHooks_idempotent_unchanged", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    const first = fs.readFileSync(hooksPath(dir), "utf8");
    const legs2 = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    assert.ok(legs2.every((l) => l.action === "unchanged"));
    const second = fs.readFileSync(hooksPath(dir), "utf8");
    assert.strictEqual(first, second); // byte-identical (AC-04)
  });

  it("test_writeCodexHooks_spaced_clientPath_is_quoted (R-10)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const spaced = "/opt/my apps/lib/hook-client/index.js";
    const legs = writeCodexHooks(dir, { clientPath: spaced, dryRun: false });
    const disk = JSON.parse(fs.readFileSync(hooksPath(dir), "utf8"));
    const cmd = disk.hooks.PreToolUse[0].hooks[0].command;
    assert.ok(cmd.includes('"' + spaced + '"'), "spaced path must be double-quoted");
    // WireLeg carries the exact string written.
    assert.ok(legs.some((l) => l.command === cmd));
  });

  it("test_writeCodexHooks_malformed_json_skips_preserves (AC-15)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const raw = "{ not valid json ";
    fs.writeFileSync(hooksPath(dir), raw, "utf8");
    let threw = false;
    let legs;
    try {
      legs = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    } catch (_e) {
      threw = true;
    }
    assert.strictEqual(threw, false);
    assert.strictEqual(legs.length, 1);
    assert.strictEqual(legs[0].action, "skipped-malformed");
    assert.strictEqual(fs.readFileSync(hooksPath(dir), "utf8"), raw);
  });

  it("skips when `hooks` key is not an object", () => {
    const dir = makeTempProject();
    writeHooks(dir, { hooks: "nope" });
    const legs = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: false });
    assert.strictEqual(legs[0].action, "skipped-malformed");
  });

  it("dry-run writes nothing", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const legs = writeCodexHooks(dir, { clientPath: CLIENT, dryRun: true });
    assert.strictEqual(legs.length, 7);
    assert.strictEqual(legs[0].action, "created");
    assert.ok(!fs.existsSync(hooksPath(dir)), "dry-run must not write");
  });
});

// ---------------------------------------------------------------------------
// maybeWireCodex — fan-out + trust surface
// ---------------------------------------------------------------------------

describe("maybeWireCodex", () => {
  it("fans out to mcp + 7 hook legs; every leg surfaces the trust note (NFR-09)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const legs = maybeWireCodex(dir, {
      clientPath: CLIENT,
      binaryPath: BIN,
      harnessSel: "codex-cli",
      dryRun: false,
    });
    assert.strictEqual(legs.length, 8); // 1 mcp + 7 hooks
    assert.strictEqual(legs.filter((l) => l.surface === "mcp").length, 1);
    assert.strictEqual(legs.filter((l) => l.surface === "hooks").length, 7);
    for (const l of legs) {
      assert.ok(/trusted \.codex\/ layer/.test(l.note), "trust note missing on a leg");
    }
  });

  it("test_maybeWireCodex_untrusted_surfaces_precondition (R-06)", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    const legs = maybeWireCodex(dir, {
      clientPath: CLIENT,
      binaryPath: BIN,
      harnessSel: "codex-cli",
      dryRun: true,
    });
    // Wiring cannot establish trust — the precondition is surfaced, never silent.
    assert.ok(legs.every((l) => typeof l.note === "string" && l.note.length > 0));
  });

  it("resolveCodexTransport: binaryPath → stdio-binary; bridge → stdio-bridge; else none", () => {
    assert.strictEqual(resolveCodexTransport({ binaryPath: BIN }).kind, "stdio-binary");
    assert.strictEqual(
      resolveCodexTransport({ bridgePath: "/b.js", projectHash: "h" }).kind,
      "stdio-bridge"
    );
    assert.strictEqual(resolveCodexTransport({}).kind, "none");
    // url is intentionally NOT a transport source (never emits url=).
    assert.strictEqual(resolveCodexTransport({ url: "https://x" }).kind, "none");
  });

  it("mcp WireLeg invariants: skipped-* carry reason, success carry entry", () => {
    const dir = makeTempProject();
    fs.mkdirSync(path.join(dir, ".codex"), { recursive: true });
    // No transport → skipped-undetected with reason, no entry.
    const legs = maybeWireCodex(dir, { clientPath: CLIENT, harnessSel: "codex-cli", dryRun: true });
    const mcp = legs.find((l) => l.surface === "mcp");
    assert.strictEqual(mcp.action, "skipped-undetected");
    assert.ok(typeof mcp.reason === "string");
    assert.strictEqual(mcp.entry, undefined);
    assert.strictEqual(mcp.command, undefined);
  });
});
