"use strict";

// Portable hook-client test runner (vnc-026 AC-12 CI).
//
// Why this exists: `node --test <directory>` recursive discovery is only
// available on Node >= 21; the CI matrix targets Node 18/20/22/24 across
// Linux/macOS/Windows (R-14). Passing an explicit, discovered file list to
// `node --test` works identically on every targeted version and OS — no shell
// globbing (which differs between bash and PowerShell), no version-gated
// directory walking.
//
// Selection:
//   default            -> all test/hook-client/*.test.js EXCEPT the excluded set
//   --include-layer2   -> also include the parity-layer2*.test.js suites
//                         (requires a cargo-built server binary; real-server.js
//                          hard-fails — never skips — if absent, per #4452)
//   --only <substr>    -> run only files whose name contains <substr>
//
// Excluded from the default matrix run (documented in .github/workflows/ci.yml):
//   * parity-layer2*   -> Layer 2 needs the cargo server; scoped to the Linux
//                         `layer2` job (test-plan OVERVIEW Integration Plan).
//   * benchmark-spawn  -> AC-13 perf gate is machine-variant; it owns its own
//                         job/artifact and must not gate the cross-OS matrix.
//
// Exit code mirrors `node --test` (non-zero on any failure) so CI fails honestly.

const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const HOOK_CLIENT_DIR = path.join(__dirname, "hook-client");

const argv = process.argv.slice(2);
const includeLayer2 = argv.includes("--include-layer2");
const onlyIdx = argv.indexOf("--only");
const onlySubstr = onlyIdx >= 0 ? argv[onlyIdx + 1] : null;

const DEFAULT_EXCLUDE = [/^parity-layer2/, /^benchmark-spawn\.test\.js$/];

function isTestFile(name) {
  return name.endsWith(".test.js");
}

function included(name) {
  if (onlySubstr) return name.includes(onlySubstr);
  if (includeLayer2 && name.startsWith("parity-layer2")) return true;
  return !DEFAULT_EXCLUDE.some((re) => re.test(name));
}

const files = fs
  .readdirSync(HOOK_CLIENT_DIR)
  .filter(isTestFile)
  .filter(included)
  .sort()
  .map((name) => path.join(HOOK_CLIENT_DIR, name));

if (files.length === 0) {
  console.error("run-hook-client: no test files selected");
  process.exit(1);
}

console.error("run-hook-client: selected " + files.length + " suite(s):");
for (const f of files) console.error("  " + path.relative(__dirname, f));

const result = spawnSync(process.execPath, ["--test", ...files], {
  stdio: "inherit",
});

if (result.error) {
  console.error("run-hook-client: failed to spawn node --test: " + result.error.message);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
