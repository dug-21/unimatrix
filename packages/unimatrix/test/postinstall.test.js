"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");
const { describe, it } = require("node:test");

const POSTINSTALL_PATH = path.resolve(__dirname, "..", "postinstall.js");

/**
 * Run postinstall.js as a child process with optional environment overrides.
 * Uses spawnSync to capture both stdout and stderr regardless of exit code.
 * Returns { status, stdout, stderr }.
 */
function runPostinstall(env) {
  const mergedEnv = Object.assign({}, process.env, env || {});
  const result = spawnSync(process.execPath, [POSTINSTALL_PATH], {
    env: mergedEnv,
    timeout: 15000,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  return {
    status: result.status,
    stdout: result.stdout || "",
    stderr: result.stderr || "",
  };
}

describe("postinstall", () => {
  describe("test_postinstall_with_binary_calls_model_download", () => {
    it("calls model-download and prints success when binary exists", () => {
      // Create a fake binary that exits 0
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-post-"));
      const fakeBinary = path.join(tmpDir, "unimatrix");
      fs.writeFileSync(
        fakeBinary,
        '#!/bin/sh\necho "model downloaded" >&2\nexit 0\n'
      );
      fs.chmodSync(fakeBinary, 0o755);

      try {
        const result = runPostinstall({ UNIMATRIX_BINARY: fakeBinary });
        assert.strictEqual(result.status, 0, "should exit 0");
        assert.ok(
          result.stdout.includes("ONNX model ready"),
          "should print ONNX model ready: got " + JSON.stringify(result.stdout)
        );
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      }
    });
  });

  describe("test_postinstall_network_failure_exits_0", () => {
    it("exits 0 and warns when model-download fails", () => {
      // Create a fake binary that exits 1 (simulating download failure)
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-post-"));
      const fakeBinary = path.join(tmpDir, "unimatrix");
      fs.writeFileSync(
        fakeBinary,
        '#!/bin/sh\necho "connection refused" >&2\nexit 1\n'
      );
      fs.chmodSync(fakeBinary, 0o755);

      try {
        const result = runPostinstall({ UNIMATRIX_BINARY: fakeBinary });
        assert.strictEqual(result.status, 0, "must exit 0 even on failure");
        // stderr from the child is forwarded to inherit, but our wrapper captures postinstall's own output
        assert.ok(
          result.stderr.includes("model download failed") ||
            result.stdout.includes("model download failed"),
          "should warn about model download failure"
        );
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      }
    });
  });

  describe("test_postinstall_binary_missing_exits_0", () => {
    it("exits 0 when UNIMATRIX_BINARY points to non-existent file", () => {
      const result = runPostinstall({
        UNIMATRIX_BINARY: "/nonexistent/path/to/unimatrix",
      });
      assert.strictEqual(result.status, 0, "must exit 0 when binary missing");
      assert.ok(
        result.stderr.includes("platform binary not available") ||
          result.stderr.includes("postinstall"),
        "should warn about missing binary"
      );
    });
  });

  describe("test_postinstall_disk_full_exits_0", () => {
    it("exits 0 when binary execution fails with an error", () => {
      // Create a binary that exits with code 2 (simulating disk full or other IO error)
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-post-"));
      const fakeBinary = path.join(tmpDir, "unimatrix");
      fs.writeFileSync(
        fakeBinary,
        '#!/bin/sh\necho "No space left on device" >&2\nexit 2\n'
      );
      fs.chmodSync(fakeBinary, 0o755);

      try {
        const result = runPostinstall({ UNIMATRIX_BINARY: fakeBinary });
        assert.strictEqual(result.status, 0, "must exit 0 on disk full");
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      }
    });
  });

  describe("test_postinstall_model_already_cached_succeeds", () => {
    it("exits 0 when model-download succeeds quickly (already cached)", () => {
      // Create a binary that exits immediately (model already present)
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-post-"));
      const fakeBinary = path.join(tmpDir, "unimatrix");
      fs.writeFileSync(fakeBinary, "#!/bin/sh\nexit 0\n");
      fs.chmodSync(fakeBinary, 0o755);

      try {
        const result = runPostinstall({ UNIMATRIX_BINARY: fakeBinary });
        assert.strictEqual(result.status, 0, "should exit 0");
        assert.ok(
          result.stdout.includes("ONNX model ready"),
          "should print ONNX model ready"
        );
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      }
    });
  });

  describe("test_all_code_paths_wrapped_in_try_catch", () => {
    it("source has outer try/catch and process.exit(0)", () => {
      const source = fs.readFileSync(POSTINSTALL_PATH, "utf8");
      // Verify the outer try/catch structure exists
      assert.ok(
        source.includes("} catch (outerError)"),
        "must have outer catch block"
      );
      // Verify process.exit(0) is the final exit path
      assert.ok(
        source.includes("process.exit(0)"),
        "must call process.exit(0)"
      );
      // Verify no process.exit(1) exists anywhere
      assert.ok(
        !source.includes("process.exit(1)"),
        "must never call process.exit(1)"
      );
    });
  });
});
