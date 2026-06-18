"use strict";

// C3 — bin/unimatrix.js `mcp-bridge` subcommand routing (vnc-039, Scope A, AC-13).
//
// Proves the `mcp-bridge` subcommand early-returns to the JS bridge (C2) BEFORE
// the generic Rust execFileSync block — mirroring the existing `init` early
// branch. The load-bearing ADR-002 correctness point: remote-only / non-Linux
// clients ship NO Rust binary, so a mcp-bridge that fell through to
// execFileSync would throw on exactly the population the bridge serves.
//
// Cumulative: reuses the child-process + Module._resolveFilename mock pattern
// from test/shim.test.js. We mock BOTH mcp-bridge.js (so the real C2 module and
// its store read are never exercised — pure routing) and resolve-binary.js (to
// prove the Rust path is never reached even when the binary is absent).

const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const shimPath = path.resolve(__dirname, "../bin/unimatrix.js");

// Run bin/unimatrix.js in a child process with mocked resolve-binary and
// mcp-bridge modules. The mocks emit deterministic markers to stdout so the
// parent can assert which route was taken.
function runShim(args, env = {}) {
  const wrapperScript = `
    "use strict";
    const Module = require("module");
    const originalResolveFilename = Module._resolveFilename;

    // resolve-binary mock. If invoked at all it records a marker, then either
    // throws a missing-binary error (simulating a host with no Rust binary) or
    // returns a configured path. AC-13: for mcp-bridge this must NEVER run.
    const mockBinaryPath = process.env._TEST_BINARY_PATH || "";
    const mockBinaryError = process.env._TEST_BINARY_ERROR || "";

    Module._resolveFilename = function(request, parent, isMain, options) {
      if (request.endsWith("resolve-binary.js")) {
        return "__mock_resolve_binary__";
      }
      if (request.endsWith("mcp-bridge.js")) {
        return "__mock_mcp_bridge__";
      }
      if (request.endsWith("init.js")) {
        return "__mock_init__";
      }
      return originalResolveFilename.call(this, request, parent, isMain, options);
    };

    require.cache["__mock_resolve_binary__"] = {
      id: "__mock_resolve_binary__",
      filename: "__mock_resolve_binary__",
      loaded: true,
      exports: {
        resolveBinary: function() {
          process.stdout.write("RESOLVE_BINARY_CALLED\\n");
          if (mockBinaryError) {
            throw new Error(mockBinaryError);
          }
          return mockBinaryPath;
        }
      }
    };

    require.cache["__mock_mcp_bridge__"] = {
      id: "__mock_mcp_bridge__",
      filename: "__mock_mcp_bridge__",
      loaded: true,
      exports: {
        main: function(argv) {
          // Record the argv-shaped array the shim forwards so the parent can
          // assert projectHash is at argv[2] (the store key / direct-spawn shape).
          process.stdout.write("BRIDGE_MAIN_CALLED:" + JSON.stringify(argv) + "\\n");
        }
      }
    };

    require.cache["__mock_init__"] = {
      id: "__mock_init__",
      filename: "__mock_init__",
      loaded: true,
      exports: {
        init: function(options) {
          process.stdout.write("INIT_CALLED:" + JSON.stringify(options) + "\\n");
          return Promise.resolve();
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

  try {
    const stdout = execFileSync(process.execPath, ["-e", wrapperScript], {
      env: mergedEnv,
      timeout: 5000,
      encoding: "utf8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    return { exitCode: 0, stdout, stderr: "" };
  } catch (error) {
    return {
      exitCode: error.status || 1,
      stdout: error.stdout || "",
      stderr: error.stderr || "",
    };
  }
}

describe("bin/unimatrix.js — mcp-bridge subcommand (AC-13)", () => {
  it("test_binMcpBridge_routesToJsBridge_noMissingBinaryThrow", () => {
    // Host with NO Rust binary: resolveBinary configured to throw a
    // missing-binary error. The mcp-bridge route must reach the JS bridge and
    // must NOT trip the missing-binary throw.
    const result = runShim(["mcp-bridge", "abc123def456"], {
      _TEST_BINARY_ERROR: "No platform binary found. Supported platforms: linux-x64",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(
      result.stdout.includes("BRIDGE_MAIN_CALLED:"),
      "JS bridge main() should have been called"
    );
    assert.ok(
      !result.stdout.includes("RESOLVE_BINARY_CALLED"),
      "resolveBinary must NOT be called for mcp-bridge"
    );
    assert.ok(
      !result.stderr.includes("No platform binary found"),
      "missing-binary error must not surface for mcp-bridge"
    );
  });

  it("test_binMcpBridge_earlyReturnsBeforeRustExecBlock", () => {
    // Even with a perfectly resolvable binary path, the mcp-bridge branch must
    // early-return before the Rust exec block — assert the Rust path (resolveBinary)
    // is never reached for this subcommand.
    const result = runShim(["mcp-bridge", "deadbeefcafe0001"], {
      _TEST_BINARY_PATH: "/bin/true",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("BRIDGE_MAIN_CALLED:"));
    assert.ok(
      !result.stdout.includes("RESOLVE_BINARY_CALLED"),
      "early-return must precede the Rust execFileSync block"
    );
  });

  it("test_binMcpBridge_passesProjectHashArg", () => {
    // The projectHash (the bridge's store key) is forwarded at argv[2], the
    // same slot a direct `node <bridge> <projectHash>` spawn uses.
    const result = runShim(["mcp-bridge", "0123456789abcdef"], {
      _TEST_BINARY_PATH: "/bin/true",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    const match = result.stdout.match(/BRIDGE_MAIN_CALLED:(.+)/);
    assert.ok(match, "bridge main() should record its argv");
    const argv = JSON.parse(match[1]);
    assert.equal(argv[2], "0123456789abcdef", "projectHash forwarded at argv[2]");
  });

  it("test_binMcpBridge_missingHash_usageExit2_noExec", () => {
    // No projectHash: usage to stderr, exit code 2, no bridge call, no Rust exec.
    const result = runShim(["mcp-bridge"], { _TEST_BINARY_PATH: "/bin/true" });
    assert.equal(result.exitCode, 2, `stderr: ${result.stderr}`);
    assert.ok(
      result.stderr.includes("usage: unimatrix mcp-bridge <projectHash>"),
      `stderr should show usage, got: ${result.stderr}`
    );
    assert.ok(
      !result.stdout.includes("BRIDGE_MAIN_CALLED:"),
      "bridge must not run without a projectHash"
    );
    assert.ok(
      !result.stdout.includes("RESOLVE_BINARY_CALLED"),
      "missing-hash path must not fall through to the Rust exec block"
    );
  });

  it("test_binMcpBridge_mirrorsInitEarlyBranchPattern", () => {
    // Parity audit: like `init`, `mcp-bridge` is handled by an early JS branch
    // that never reaches resolveBinary. Run both and assert neither touches the
    // Rust path, so a future non-init subcommand audit stays coherent.
    const bridge = runShim(["mcp-bridge", "feed0000feed0000"], {
      _TEST_BINARY_PATH: "/bin/true",
    });
    const init = runShim(["init", "--dry-run"], { _TEST_BINARY_PATH: "/bin/true" });

    assert.ok(bridge.stdout.includes("BRIDGE_MAIN_CALLED:"));
    assert.ok(!bridge.stdout.includes("RESOLVE_BINARY_CALLED"));

    assert.ok(init.stdout.includes("INIT_CALLED:"));
    assert.ok(!init.stdout.includes("RESOLVE_BINARY_CALLED"));
  });
});

describe("bin/unimatrix.js — non-regression of existing routing", () => {
  it("test_bin_otherSubcommands_stillExecRustBinary", () => {
    // A non-mcp-bridge, non-init subcommand still routes to the Rust exec block
    // (the early-return is scoped to mcp-bridge, not a blanket bypass).
    const result = runShim(["hook", "SessionStart"], {
      _TEST_BINARY_PATH: "/bin/echo",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(
      result.stdout.includes("RESOLVE_BINARY_CALLED"),
      "non-mcp-bridge subcommand must reach the Rust exec block"
    );
    assert.ok(
      !result.stdout.includes("BRIDGE_MAIN_CALLED:"),
      "bridge must not capture other subcommands"
    );
  });

  it("test_bin_init_branchUnchanged", () => {
    const result = runShim(["init"], { _TEST_BINARY_PATH: "/bin/true" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("INIT_CALLED:"));
    assert.ok(
      !result.stdout.includes("RESOLVE_BINARY_CALLED"),
      "init must not reach the Rust exec block"
    );
    assert.ok(!result.stdout.includes("BRIDGE_MAIN_CALLED:"));
  });
});
