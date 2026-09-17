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
// Selection sets (--set <name>, default "hook-client"):
//   hook-client (default) -> all test/hook-client/*.test.js EXCEPT the excluded
//                            set; parallel `node --test`. UNCHANGED behaviour.
//     --include-layer2     -> also include the parity-layer2*.test.js suites
//                             (requires a cargo-built server binary; real-server.js
//                              hard-fails — never skips — if absent, per #4452)
//     --only <substr>      -> run only files whose name contains <substr>
//   package               -> top-level test/*.test.js (NON-recursive), run
//                            SERIALLY (`--test-concurrency=1`). Serial is
//                            mandatory: several suites inject/remove dirs in the
//                            shared real skills/ tree and race under the default
//                            parallelism (#5778). Non-recursive readdir excludes
//                            c14-verifier.js, check-*.js, fixtures/, helpers/.
//   opencode-plugin       -> opencode-plugin/test/*.test.js (ESM type:module),
//                            also serial for consistency.
//   audit                 -> ORPHAN-SUITE GUARD (bug #995 / lesson #5781): walk
//                            every *.test.js under test/ and opencode-plugin/test/
//                            and assert each is claimed by a selection set or a
//                            documented exclusion. Exits 1 naming any orphan, so
//                            a suite added outside a selected directory FAILS CI
//                            instead of silently escaping it. OS/version-agnostic.
//
// Windows (durable exclusion, #5781): the top-level `package` and
// `opencode-plugin` suites hard-code Unix shapes — `#!/bin/sh` fake binaries +
// chmod 0o755, /bin/true, /bin/echo, /bin/false, bash scripts — so they are
// Windows-incompatible by construction. On win32 those two sets exit fast (0)
// with a skip message rather than run and fail; CI never schedules them on a
// Windows cell. This guard is durable — it does not rely on a ci.yml comment.
//
// Excluded from the default hook-client matrix run (documented in
// .github/workflows/ci.yml AND enumerated by the audit set below):
//   * parity-layer2*   -> Layer 2 needs the cargo server; scoped to the LOCAL
//                         Gate-3c `layer2` step (test:hook-client:layer2), not CI.
//   * benchmark-spawn  -> AC-13 perf gate is machine-variant; it owns its own
//                         job/artifact and must not gate the cross-OS matrix.
//
// Exit code mirrors `node --test` (non-zero on any failure) so CI fails honestly.

const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const HOOK_CLIENT_DIR = path.join(__dirname, "hook-client");
const PACKAGE_TEST_DIR = __dirname;
const OPENCODE_TEST_DIR = path.join(__dirname, "..", "opencode-plugin", "test");

const argv = process.argv.slice(2);
const includeLayer2 = argv.includes("--include-layer2");
const onlyIdx = argv.indexOf("--only");
const onlySubstr = onlyIdx >= 0 ? argv[onlyIdx + 1] : null;
const setIdx = argv.indexOf("--set");
const setName = setIdx >= 0 ? argv[setIdx + 1] : "hook-client";

const DEFAULT_EXCLUDE = [/^parity-layer2/, /^benchmark-spawn\.test\.js$/];

function isTestFile(name) {
  return name.endsWith(".test.js");
}

function included(name) {
  if (onlySubstr) return name.includes(onlySubstr);
  if (includeLayer2 && name.startsWith("parity-layer2")) return true;
  return !DEFAULT_EXCLUDE.some((re) => re.test(name));
}

// ── Additive branches for the non-default selection sets ─────────────────────
// These call process.exit() and never fall through to the hook-client path
// below, whose selection/invocation lines are preserved byte-for-byte.

function listTestFiles(dir) {
  // Non-recursive: top-level *.test.js only. Naturally excludes helper/fixture
  // .js files and subdirectories.
  return fs
    .readdirSync(dir)
    .filter(isTestFile)
    .sort()
    .map((name) => path.join(dir, name));
}

function runSerial(label, dir) {
  if (process.platform === "win32") {
    // Durable Windows exclusion (#5781): these suites hard-code Unix shapes
    // (#!/bin/sh, chmod, /bin/true, bash). Skip cleanly instead of failing.
    console.error(
      "run-hook-client: skipping '" + label + "' set on win32 — top-level " +
        "suites are Unix-only (#5781); this set runs on ubuntu/macos cells only."
    );
    process.exit(0);
  }
  let files;
  try {
    files = listTestFiles(dir);
  } catch (err) {
    console.error("run-hook-client: cannot read " + dir + ": " + err.message);
    process.exit(1);
  }
  if (files.length === 0) {
    console.error("run-hook-client: no test files selected for set '" + label + "'");
    process.exit(1);
  }
  console.error(
    "run-hook-client: set '" + label + "' selected " + files.length + " suite(s):"
  );
  for (const f of files) console.error("  " + path.relative(__dirname, f));
  const result = spawnSync(
    process.execPath,
    ["--test", "--test-concurrency=1", ...files],
    { stdio: "inherit" }
  );
  if (result.error) {
    console.error("run-hook-client: failed to spawn node --test: " + result.error.message);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
}

// Orphan-suite guard (bug #995, lesson #5781). Every *.test.js under test/ and
// opencode-plugin/test/ MUST be claimed by a selection set or a documented
// exclusion; otherwise it escapes CI silently. This converts the Gate-3b/3c
// human check into a CI failure.
function walkTestFiles(dir) {
  const out = [];
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch (err) {
    return out;
  }
  for (const ent of entries) {
    if (ent.name === "node_modules" || ent.name.startsWith(".")) continue;
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) out.push(...walkTestFiles(full));
    else if (isTestFile(ent.name)) out.push(full);
  }
  return out;
}

function runAudit() {
  // Documented exclusions: real *.test.js files intentionally NOT in a running
  // selection set. Each needs a one-line reason.
  const EXCLUSIONS = [
    {
      file: path.join(HOOK_CLIENT_DIR, "benchmark-spawn.test.js"),
      reason: "AC-13 spawn-latency perf gate — machine-variant, owns its own local job/artifact; must not gate the cross-OS matrix.",
    },
    // parity-layer2*.test.js are covered by the layer2 selection mode
    // (test:hook-client:layer2, a LOCAL Gate-3c step); enumerated below.
  ];

  // Files claimed by a running selection set.
  const claimed = new Set();
  for (const f of listTestFiles(PACKAGE_TEST_DIR)) claimed.add(f); // --set package
  for (const f of fs.readdirSync(HOOK_CLIENT_DIR).filter(isTestFile)) {
    // --set hook-client (default) + parity-layer2 via --only/--include-layer2.
    claimed.add(path.join(HOOK_CLIENT_DIR, f));
  }
  if (fs.existsSync(OPENCODE_TEST_DIR)) {
    for (const f of listTestFiles(OPENCODE_TEST_DIR)) claimed.add(f); // --set opencode-plugin
  }
  for (const ex of EXCLUSIONS) claimed.add(ex.file);

  const all = [
    ...walkTestFiles(PACKAGE_TEST_DIR),
    ...walkTestFiles(OPENCODE_TEST_DIR),
  ];
  const orphans = all.filter((f) => !claimed.has(f)).sort();

  console.error(
    "run-hook-client: orphan-suite audit — " + all.length + " suite(s) found, " +
      claimed.size + " claimed by a selection set or documented exclusion."
  );
  if (orphans.length > 0) {
    console.error(
      "run-hook-client: FAIL — " + orphans.length + " suite(s) run by NO CI " +
        "selection set (add them to a --set or the documented exclusion list):"
    );
    for (const f of orphans) console.error("  " + path.relative(__dirname, f));
    process.exit(1);
  }
  console.error("run-hook-client: audit PASS — every suite is selected or excluded.");
  process.exit(0);
}

if (setName === "package") runSerial("package", PACKAGE_TEST_DIR);
if (setName === "opencode-plugin") runSerial("opencode-plugin", OPENCODE_TEST_DIR);
if (setName === "audit") runAudit();
if (setName !== "hook-client") {
  console.error("run-hook-client: unknown --set '" + String(setName) + "'");
  process.exit(1);
}

// ── Default set: hook-client. Selection/invocation preserved byte-for-byte. ──

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
