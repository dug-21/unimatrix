"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it, beforeEach, afterEach } = require("node:test");

// We need to manipulate require cache and process properties for testing.
// Store originals for restoration.
const originalPlatform = Object.getOwnPropertyDescriptor(process, "platform");
const originalArch = Object.getOwnPropertyDescriptor(process, "arch");

function requireFresh() {
  const modPath = require.resolve("../lib/resolve-binary.js");
  delete require.cache[modPath];
  return require(modPath);
}

describe("resolve-binary", () => {
  let savedEnv;

  beforeEach(() => {
    savedEnv = process.env.UNIMATRIX_BINARY;
    delete process.env.UNIMATRIX_BINARY;
  });

  afterEach(() => {
    if (savedEnv !== undefined) {
      process.env.UNIMATRIX_BINARY = savedEnv;
    } else {
      delete process.env.UNIMATRIX_BINARY;
    }
    // Restore platform/arch if overridden
    if (originalPlatform) {
      Object.defineProperty(process, "platform", originalPlatform);
    }
    if (originalArch) {
      Object.defineProperty(process, "arch", originalArch);
    }
  });

  describe("Platform Map", () => {
    it("test_platform_map_contains_linux_x64", () => {
      const { PLATFORMS } = requireFresh();
      assert.deepStrictEqual(PLATFORMS, {
        "linux-x64": "@dug-21/unimatrix-linux-x64",
        "linux-arm64": "@dug-21/unimatrix-linux-arm64",
      });
    });
  });

  describe("UNIMATRIX_BINARY Env Fallback", () => {
    it("test_env_override_takes_precedence", () => {
      // Create a temporary file to use as the binary
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-test-"));
      const fakeBinary = path.join(tmpDir, "unimatrix");
      fs.writeFileSync(fakeBinary, "fake");

      try {
        process.env.UNIMATRIX_BINARY = fakeBinary;
        const { resolveBinary } = requireFresh();
        const result = resolveBinary();
        // Should return the realpath of the env var path
        assert.strictEqual(result, fs.realpathSync(fakeBinary));
        // Should be absolute
        assert.ok(path.isAbsolute(result), "Path should be absolute");
      } finally {
        fs.unlinkSync(fakeBinary);
        fs.rmdirSync(tmpDir);
      }
    });

    it("test_env_override_with_nonexistent_path_throws", () => {
      process.env.UNIMATRIX_BINARY = "/nonexistent/path/unimatrix";
      const { resolveBinary } = requireFresh();
      assert.throws(
        () => resolveBinary(),
        (err) => {
          assert.ok(
            err.message.includes("UNIMATRIX_BINARY points to non-existent file"),
            "Error should mention UNIMATRIX_BINARY"
          );
          assert.ok(
            err.message.includes("/nonexistent/path/unimatrix"),
            "Error should include the path"
          );
          return true;
        }
      );
    });
  });

  describe("Error Cases", () => {
    it("test_unsupported_platform_throws", () => {
      Object.defineProperty(process, "platform", {
        value: "win32",
        configurable: true,
      });
      Object.defineProperty(process, "arch", {
        value: "x64",
        configurable: true,
      });

      // resolve-binary uses os.platform()/os.arch(), but those read from
      // the same underlying source. We need to mock the os module.
      // Instead, we test by using a platform key that doesn't exist.
      // The module uses os.platform() and os.arch() — we need to intercept.
      // Since os.platform() calls process.platform internally on Node, our
      // override should work. But resolve-binary uses the os module.
      // Let's check: os.platform() returns process.platform on Node.js.
      // Verify this works:
      const osModule = require("os");
      assert.strictEqual(osModule.platform(), "win32");
      assert.strictEqual(osModule.arch(), "x64");

      const { resolveBinary } = requireFresh();
      assert.throws(
        () => resolveBinary(),
        (err) => {
          assert.ok(
            err.message.includes("Unsupported platform: win32-x64"),
            "Error should mention the unsupported platform"
          );
          assert.ok(
            err.message.includes("Supported platforms"),
            "Error should list supported platforms"
          );
          return true;
        }
      );
    });

    it("test_error_message_lists_all_supported_platforms", () => {
      Object.defineProperty(process, "platform", {
        value: "freebsd",
        configurable: true,
      });
      Object.defineProperty(process, "arch", {
        value: "arm64",
        configurable: true,
      });

      const { resolveBinary, PLATFORMS } = requireFresh();
      assert.throws(
        () => resolveBinary(),
        (err) => {
          // Every key from PLATFORMS map should appear in the error message
          for (const key of Object.keys(PLATFORMS)) {
            assert.ok(
              err.message.includes(key),
              "Error should include platform key: " + key
            );
          }
          return true;
        }
      );
    });

    it("test_missing_package_throws_with_platform_info", () => {
      // On linux-x64 (our CI), the platform package is not installed,
      // so require.resolve will fail. This tests the error path naturally.
      // If this test runs on linux-x64 without the package:
      const { resolveBinary } = requireFresh();

      // On the test environment, the platform package is not installed,
      // so this should throw with the install instructions.
      assert.throws(
        () => resolveBinary(),
        (err) => {
          assert.ok(
            err.message.includes("Could not find platform binary package") ||
              err.message.includes("Unsupported platform"),
            "Error should indicate missing package or unsupported platform"
          );
          return true;
        }
      );
    });
  });

  describe("Platform Detection", () => {
    it("test_linux_x64_resolves_correct_package", () => {
      // Set platform to linux-x64 to test the lookup path
      Object.defineProperty(process, "platform", {
        value: "linux",
        configurable: true,
      });
      Object.defineProperty(process, "arch", {
        value: "x64",
        configurable: true,
      });

      const { resolveBinary } = requireFresh();
      // The platform package is not installed in test, so require.resolve
      // will fail. We verify the error references the correct package name.
      assert.throws(
        () => resolveBinary(),
        (err) => {
          assert.ok(
            err.message.includes("@dug-21/unimatrix-linux-x64"),
            "Error should reference the linux-x64 package name"
          );
          return true;
        }
      );
    });

    it("test_resolved_path_is_absolute", () => {
      // Use UNIMATRIX_BINARY to test the absolute path guarantee
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-test-"));
      const fakeBinary = path.join(tmpDir, "unimatrix");
      fs.writeFileSync(fakeBinary, "fake");

      try {
        process.env.UNIMATRIX_BINARY = fakeBinary;
        const { resolveBinary } = requireFresh();
        const result = resolveBinary();
        assert.ok(
          path.isAbsolute(result),
          "Resolved path must be absolute, got: " + result
        );
      } finally {
        fs.unlinkSync(fakeBinary);
        fs.rmdirSync(tmpDir);
      }
    });

    it("test_env_override_resolves_symlinks", () => {
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-test-"));
      const realBinary = path.join(tmpDir, "unimatrix-real");
      const symlinkBinary = path.join(tmpDir, "unimatrix-link");
      fs.writeFileSync(realBinary, "fake");
      fs.symlinkSync(realBinary, symlinkBinary);

      try {
        process.env.UNIMATRIX_BINARY = symlinkBinary;
        const { resolveBinary } = requireFresh();
        const result = resolveBinary();
        // Should resolve through the symlink
        assert.strictEqual(
          result,
          fs.realpathSync(realBinary),
          "Should resolve symlinks"
        );
      } finally {
        fs.unlinkSync(symlinkBinary);
        fs.unlinkSync(realBinary);
        fs.rmdirSync(tmpDir);
      }
    });
  });
});
