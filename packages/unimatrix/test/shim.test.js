"use strict";

const { describe, it, beforeEach, afterEach, mock } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const shimPath = path.resolve(__dirname, "../bin/unimatrix.js");

// Helper: run the shim in a child process with controlled argv and mocked modules
function runShim(args, env = {}) {
  // We run the shim as a subprocess so we can capture exit codes and stderr
  // without polluting our own process state.
  const wrapperScript = `
    "use strict";
    const Module = require("module");
    const originalResolveFilename = Module._resolveFilename;

    // Mock resolve-binary.js
    const mockBinaryPath = process.env._TEST_BINARY_PATH || "";
    const mockBinaryError = process.env._TEST_BINARY_ERROR || "";

    // Mock init.js
    const mockInitError = process.env._TEST_INIT_ERROR || "";
    const mockInitCalled = [];

    Module._resolveFilename = function(request, parent, isMain, options) {
      if (request.endsWith("resolve-binary.js") || request === "../lib/resolve-binary.js") {
        return "__mock_resolve_binary__";
      }
      if (request.endsWith("init.js") || request === "../lib/init.js") {
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
          if (mockBinaryError) {
            const err = new Error(mockBinaryError);
            throw err;
          }
          return mockBinaryPath;
        }
      }
    };

    require.cache["__mock_init__"] = {
      id: "__mock_init__",
      filename: "__mock_init__",
      loaded: true,
      exports: {
        init: function(options) {
          // Write to stdout so the test can verify init was called
          process.stdout.write("INIT_CALLED:" + JSON.stringify(options) + "\\n");
          if (mockInitError) {
            return Promise.reject(new Error(mockInitError));
          }
          return Promise.resolve();
        }
      }
    };

    // Override argv
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

describe("JS Shim — Argument Routing", () => {
  it("test_init_arg_routes_to_js_init", () => {
    const result = runShim(["init"], { _TEST_BINARY_PATH: "/fake/binary" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(
      result.stdout.includes("INIT_CALLED:"),
      "init() should have been called"
    );
    const match = result.stdout.match(/INIT_CALLED:(.+)/);
    const options = JSON.parse(match[1]);
    assert.equal(options.dryRun, false);
  });

  it("test_init_with_dry_run_routes_to_js", () => {
    const result = runShim(["init", "--dry-run"], {
      _TEST_BINARY_PATH: "/fake/binary",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("INIT_CALLED:"));
    const match = result.stdout.match(/INIT_CALLED:(.+)/);
    const options = JSON.parse(match[1]);
    assert.equal(options.dryRun, true);
  });

  it("test_hook_arg_routes_to_binary", () => {
    // Use 'echo' as the mock binary — it will succeed and print args
    const result = runShim(["hook", "SessionStart"], {
      _TEST_BINARY_PATH: "/bin/echo",
    });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    // echo prints its args to stdout
    assert.ok(
      result.stdout.includes("hook SessionStart"),
      "Binary should receive forwarded args"
    );
  });

  it("test_export_arg_routes_to_binary", () => {
    const result = runShim(["export"], { _TEST_BINARY_PATH: "/bin/echo" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("export"));
  });

  it("test_no_args_routes_to_binary", () => {
    // 'true' binary exits 0 with no output
    const result = runShim([], { _TEST_BINARY_PATH: "/bin/true" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
  });

  it("test_version_arg_routes_to_binary", () => {
    const result = runShim(["version"], { _TEST_BINARY_PATH: "/bin/echo" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    assert.ok(result.stdout.includes("version"));
    // Should NOT have called init
    assert.ok(!result.stdout.includes("INIT_CALLED:"));
  });

  it("test_dash_dash_version_routes_to_binary", () => {
    // /bin/echo with --version may not print the flag literally.
    // Use /bin/true to confirm the binary is invoked (not init), and verify
    // init was NOT called.
    const result = runShim(["--version"], { _TEST_BINARY_PATH: "/bin/true" });
    assert.equal(result.exitCode, 0, `stderr: ${result.stderr}`);
    // Should NOT have called init
    assert.ok(!result.stdout.includes("INIT_CALLED:"));
  });
});

describe("JS Shim — Exit Code Passthrough", () => {
  it("test_binary_exit_0_propagates", () => {
    const result = runShim([], { _TEST_BINARY_PATH: "/bin/true" });
    assert.equal(result.exitCode, 0);
  });

  it("test_binary_exit_1_propagates", () => {
    const result = runShim([], { _TEST_BINARY_PATH: "/bin/false" });
    assert.equal(result.exitCode, 1);
  });

  it("test_binary_exit_signal_propagates", () => {
    // Use a script that exits with a specific code to simulate signal-like failure
    // We can't easily send a signal to a sync child, so test spawn failure instead
    const result = runShim([], {
      _TEST_BINARY_PATH: "/nonexistent/binary/path",
    });
    assert.equal(result.exitCode, 1);
    assert.ok(
      result.stderr.includes("Failed to execute unimatrix"),
      `stderr should contain error message, got: ${result.stderr}`
    );
  });
});

describe("JS Shim — Error Handling", () => {
  it("test_binary_not_found_exits_1", () => {
    const result = runShim(["version"], {
      _TEST_BINARY_ERROR: "No platform binary found. Supported platforms: linux-x64",
    });
    assert.equal(result.exitCode, 1);
  });

  it("test_binary_not_found_prints_platforms", () => {
    const errorMsg =
      "No platform binary found. Supported platforms: linux-x64";
    const result = runShim(["version"], { _TEST_BINARY_ERROR: errorMsg });
    assert.equal(result.exitCode, 1);
    assert.ok(
      result.stderr.includes("Supported platforms"),
      `stderr should list platforms, got: ${result.stderr}`
    );
    assert.ok(result.stderr.includes("linux-x64"));
  });

  it("test_init_failure_prints_error_exits_1", () => {
    const result = runShim(["init"], {
      _TEST_INIT_ERROR: "something went wrong",
    });
    assert.equal(result.exitCode, 1);
    assert.ok(
      result.stderr.includes("unimatrix init failed:"),
      `stderr should contain init error, got: ${result.stderr}`
    );
    assert.ok(result.stderr.includes("something went wrong"));
  });
});
