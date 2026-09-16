"use strict";

// CLI routing — bin/unimatrix.js `wire` verb, `--harness`, `--dry-run`, and the
// `--force` handoff into init (nan-023, ADR-005 / #960).
//
// These tests drive the REAL user command (the shim entry point) and assert the
// observed routing outcome — per the behavioral-outcome lens, a writer unit test
// beneath the verb does NOT discharge these rows (test-plan/cli-routing.md).
//
// wire.js is mocked (it is built by a parallel Wave C agent); the mock records
// the exact opts wire() received so we can assert the routing contract:
//   - `wire` skips installSkills + DB (init() never runs on the wire path) — AC-03
//   - `--harness` is forwarded (target + opt-in signal) — AC-11
//   - unknown `--harness` → help, exit 2, wire NEVER called — AC-16
//   - `--force` is NEVER forwarded to wire() — C-12 / SR-04 / AC-02
//   - `--dry-run` routes an opts shape identical to the real path except dryRun — AC-14
//   - `init --force` threads force:true into init() (skills-installer handoff)
//
// Cumulative: reuses the Module._resolveFilename + require.cache mock pattern
// from test/bin-mcp-bridge.test.js and test/shim.test.js.

const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const shimPath = path.resolve(__dirname, "../bin/unimatrix.js");

// Run bin/unimatrix.js in a child process with lib/wire.js, lib/init.js,
// lib/resolve-binary.js, and the cloud credstore/config modules mocked. Each mock
// emits a deterministic marker to stdout so the parent can assert the route taken
// and the exact opts forwarded.
function runShim(args, env = {}) {
  const wrapperScript = `
    "use strict";
    const Module = require("module");
    const originalResolveFilename = Module._resolveFilename;

    const mockBinaryPath = process.env._TEST_BINARY_PATH || "";
    const mockBinaryError = process.env._TEST_BINARY_ERROR || "";
    const mockWireThrow = process.env._TEST_WIRE_THROW || "";
    const mockCredJson = process.env._TEST_CRED_JSON || "";

    Module._resolveFilename = function(request, parent, isMain, options) {
      if (request.endsWith("resolve-binary.js")) return "__mock_resolve_binary__";
      if (request.endsWith("wire.js")) return "__mock_wire__";
      if (request.endsWith("init.js")) return "__mock_init__";
      if (request.endsWith("hook-client/config.js")) return "__mock_config__";
      if (request.endsWith("credstore.js")) return "__mock_credstore__";
      return originalResolveFilename.call(this, request, parent, isMain, options);
    };

    require.cache["__mock_resolve_binary__"] = {
      id: "__mock_resolve_binary__", filename: "__mock_resolve_binary__",
      loaded: true,
      exports: {
        resolveBinary: function() {
          process.stdout.write("RESOLVE_BINARY_CALLED\\n");
          if (mockBinaryError) throw new Error(mockBinaryError);
          return mockBinaryPath;
        }
      }
    };

    require.cache["__mock_wire__"] = {
      id: "__mock_wire__", filename: "__mock_wire__", loaded: true,
      exports: {
        wire: function(projectRoot, opts) {
          process.stdout.write("WIRE_CALLED:" + JSON.stringify({ projectRoot: projectRoot, opts: opts }) + "\\n");
          if (mockWireThrow) throw new Error(mockWireThrow);
          return { actions: ["opencode retrieval: created " + projectRoot + "/opencode.json"], manifest: [] };
        }
      }
    };

    require.cache["__mock_init__"] = {
      id: "__mock_init__", filename: "__mock_init__", loaded: true,
      exports: {
        init: function(options) {
          process.stdout.write("INIT_CALLED:" + JSON.stringify(options) + "\\n");
          return Promise.resolve();
        },
        detectProjectRoot: function(startDir) {
          process.stdout.write("DETECT_ROOT_CALLED\\n");
          return "/detected/root";
        }
      }
    };

    require.cache["__mock_config__"] = {
      id: "__mock_config__", filename: "__mock_config__", loaded: true,
      exports: { computeProjectHash: function() { return "0123456789abcdef"; } }
    };

    require.cache["__mock_credstore__"] = {
      id: "__mock_credstore__", filename: "__mock_credstore__", loaded: true,
      exports: {
        read: function() {
          process.stdout.write("CREDSTORE_READ_CALLED\\n");
          return mockCredJson ? JSON.parse(mockCredJson) : null;
        }
      }
    };

    process.argv = ["node", "unimatrix.js"].concat(JSON.parse(process.env._TEST_ARGS || "[]"));
    require(${JSON.stringify(shimPath)});
  `;

  const mergedEnv = {
    ...process.env,
    _TEST_ARGS: JSON.stringify(args),
    ...env,
  };

  // spawnSync captures BOTH stdout and stderr regardless of exit code — the
  // `--force` usage note is written to stderr on the success (exit 0) path.
  const res = spawnSync(process.execPath, ["-e", wrapperScript], {
    env: mergedEnv,
    timeout: 5000,
    encoding: "utf8",
  });
  return {
    exitCode: res.status === null ? 1 : res.status,
    stdout: res.stdout || "",
    stderr: res.stderr || "",
  };
}

function wirePayload(stdout) {
  const match = stdout.match(/WIRE_CALLED:(.+)/);
  return match ? JSON.parse(match[1]) : null;
}

function wireOpts(stdout) {
  const match = stdout.match(/WIRE_CALLED:(.+)/);
  if (!match) return null;
  return JSON.parse(match[1]).opts;
}

describe("bin/unimatrix.js — `wire` verb routing (ADR-005, AC-03)", () => {
  it("routes `wire` to the wiring layer and NOT to init (skips skills + DB)", () => {
    const result = runShim(["wire", "--project-dir", "/proj"], {
      _TEST_BINARY_PATH: "/fake/binary",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const opts = wireOpts(result.stdout);
    assert.ok(opts, "wire() should have been called");
    // AC-03: the wire path runs the wiring layer ONLY — init() (installSkills +
    // DB/validate) must never be invoked.
    assert.ok(
      !result.stdout.includes("INIT_CALLED:"),
      "wire must not call init() (no installSkills, no DB)"
    );
    assert.equal(opts.clientPath.endsWith("hook-client/index.js"), true);
    assert.equal(opts.binaryPath, "/fake/binary", "local transport forwarded");
    assert.equal(opts.dryRun, false);
    assert.ok(!("harness" in opts), "no --harness → harness omitted (all detected)");
  });

  it("prints the wire manifest actions and a wire-specific header", () => {
    const result = runShim(["wire", "--project-dir", "/proj"], {
      _TEST_BINARY_PATH: "/fake/binary",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("--- Unimatrix Wire Complete ---"));
    assert.ok(result.stdout.includes("opencode retrieval: created"));
  });

  it("forwards a valid --harness value (target + opt-in signal, AC-11)", () => {
    const result = runShim(
      ["wire", "--project-dir", "/proj", "--harness", "codex-cli"],
      { _TEST_BINARY_PATH: "/fake/binary" }
    );
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const opts = wireOpts(result.stdout);
    assert.equal(opts.harness, "codex-cli", "--harness forwarded to wire()");
  });
});

describe("bin/unimatrix.js — unknown/ambiguous invocation → help (AC-16)", () => {
  it("unknown --harness value prints help, exits 2, never calls wire()", () => {
    const result = runShim(
      ["wire", "--project-dir", "/proj", "--harness", "bogus"],
      { _TEST_BINARY_PATH: "/fake/binary" }
    );
    assert.equal(result.exitCode, 2, `stderr: ${result.stderr}`);
    assert.ok(
      result.stderr.includes("unknown --harness: bogus"),
      `stderr should name the bad value, got: ${result.stderr}`
    );
    assert.ok(
      result.stdout.includes("usage: unimatrix wire"),
      "help/usage must print to stdout"
    );
    assert.ok(
      !result.stdout.includes("WIRE_CALLED:"),
      "wire() must NOT run on an unknown harness (no fall-through write)"
    );
  });

  it("dangling --harness consuming another flag is treated as unknown → help", () => {
    // `--harness --dry-run` makes "--dry-run" the harness value → ambiguous.
    const result = runShim(
      ["wire", "--project-dir", "/proj", "--harness", "--dry-run"],
      { _TEST_BINARY_PATH: "/fake/binary" }
    );
    assert.equal(result.exitCode, 2, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("usage: unimatrix wire"));
    assert.ok(!result.stdout.includes("WIRE_CALLED:"));
  });

  it("help output states the skills-only definition boundary (C-13)", () => {
    const result = runShim(
      ["wire", "--project-dir", "/proj", "--harness", "bogus"],
      { _TEST_BINARY_PATH: "/fake/binary" }
    );
    assert.ok(
      result.stdout.includes("definition scope is skills only"),
      "help must state protocols/agents are not installed"
    );
  });
});

describe("bin/unimatrix.js — `--force` never reaches wire (C-12, SR-04)", () => {
  it("`wire --force` prints a usage note and does NOT forward force", () => {
    const result = runShim(
      ["wire", "--project-dir", "/proj", "--force"],
      { _TEST_BINARY_PATH: "/fake/binary" }
    );
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(
      result.stderr.includes("--force has no effect on `wire`"),
      `expected a --force usage note, got: ${result.stderr}`
    );
    const opts = wireOpts(result.stdout);
    assert.ok(opts, "wire still runs (additive) despite --force");
    assert.ok(
      !("force" in opts),
      "force must NEVER appear in the opts forwarded to wire()"
    );
  });

  it("`wire --force --harness codex-cli` forwards harness but not force", () => {
    const result = runShim(
      ["wire", "--project-dir", "/proj", "--force", "--harness", "codex-cli"],
      { _TEST_BINARY_PATH: "/fake/binary" }
    );
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const opts = wireOpts(result.stdout);
    assert.equal(opts.harness, "codex-cli");
    assert.ok(!("force" in opts), "force must not be forwarded");
  });
});

describe("bin/unimatrix.js — `--dry-run` (AC-14)", () => {
  it("`wire --dry-run` forwards dryRun:true and shows the dry-run header", () => {
    const result = runShim(["wire", "--project-dir", "/proj", "--dry-run"], {
      _TEST_BINARY_PATH: "/fake/binary",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const opts = wireOpts(result.stdout);
    assert.equal(opts.dryRun, true);
    assert.ok(result.stdout.includes("--- Wire Dry Run Summary ---"));
  });

  it("dry-run routes an opts shape identical to the real path except dryRun", () => {
    const real = wireOpts(
      runShim(["wire", "--project-dir", "/proj", "--harness", "opencode"], {
        _TEST_BINARY_PATH: "/fake/binary",
      }).stdout
    );
    const dry = wireOpts(
      runShim(
        ["wire", "--project-dir", "/proj", "--harness", "opencode", "--dry-run"],
        { _TEST_BINARY_PATH: "/fake/binary" }
      ).stdout
    );
    assert.equal(real.dryRun, false);
    assert.equal(dry.dryRun, true);
    // Everything the routing feeds wire() is identical apart from dryRun, so the
    // action SET cannot diverge between the two paths (parity is wire()'s job;
    // this proves the CLI does not inject a divergence).
    assert.deepEqual(
      Object.assign({}, real, { dryRun: null }),
      Object.assign({}, dry, { dryRun: null })
    );
  });
});

describe("bin/unimatrix.js — wire transport resolution", () => {
  it("falls back to the cloud credential when no local binary exists", () => {
    const result = runShim(["wire", "--project-dir", "/proj"], {
      _TEST_BINARY_ERROR: "No platform binary found",
      _TEST_CRED_JSON: JSON.stringify({
        schema_version: 1,
        mcp_url: "https://cloud.example/mcp",
        token: "SECRET-TOKEN",
      }),
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const opts = wireOpts(result.stdout);
    assert.ok(opts.mcp, "cloud transport descriptor forwarded");
    assert.equal(opts.mcp.url, "https://cloud.example/mcp");
    assert.ok(!("binaryPath" in opts), "no local binaryPath in cloud mode");
  });

  it("no binary and no credential → empty transport, wire still runs (no throw)", () => {
    const result = runShim(["wire", "--project-dir", "/proj"], {
      _TEST_BINARY_ERROR: "No platform binary found",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const opts = wireOpts(result.stdout);
    assert.ok(opts, "wire() still called with an empty transport");
    assert.ok(!("binaryPath" in opts) && !("mcp" in opts));
  });
});

describe("bin/unimatrix.js — wire error posture (SR-07 parity)", () => {
  it("a reused claude loud-checkpoint throw surfaces on stderr, exit 1", () => {
    const result = runShim(["wire", "--project-dir", "/proj"], {
      _TEST_BINARY_PATH: "/fake/binary",
      _TEST_WIRE_THROW: "Malformed .mcp.json at /proj/.mcp.json",
    });
    assert.equal(result.exitCode, 1, `stderr: ${result.stderr}`);
    assert.ok(result.stderr.includes("unimatrix wire failed:"));
    assert.ok(result.stderr.includes("Malformed .mcp.json"));
  });

  it("no --project-dir resolves the root via detectProjectRoot", () => {
    const result = runShim(["wire"], { _TEST_BINARY_PATH: "/fake/binary" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(
      result.stdout.includes("DETECT_ROOT_CALLED"),
      "root resolution goes through the reused detectProjectRoot"
    );
    const payload = wirePayload(result.stdout);
    assert.equal(payload.projectRoot, "/detected/root");
  });
});

describe("bin/unimatrix.js — `init --force` handoff (skills-installer)", () => {
  it("`init --force` threads force:true into init()", () => {
    const result = runShim(["init", "--force"], {
      _TEST_BINARY_PATH: "/fake/binary",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const match = result.stdout.match(/INIT_CALLED:(.+)/);
    assert.ok(match, "init() should have been called");
    const options = JSON.parse(match[1]);
    assert.equal(options.force, true, "--force threaded into init options");
  });

  it("`init` without --force threads force:false", () => {
    const result = runShim(["init"], { _TEST_BINARY_PATH: "/fake/binary" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const match = result.stdout.match(/INIT_CALLED:(.+)/);
    const options = JSON.parse(match[1]);
    assert.equal(options.force, false);
  });

  it("wire path never reaches init — `--force` blast radius stays off wiring", () => {
    const result = runShim(["wire", "--project-dir", "/proj", "--force"], {
      _TEST_BINARY_PATH: "/fake/binary",
    });
    assert.ok(!result.stdout.includes("INIT_CALLED:"));
  });
});
