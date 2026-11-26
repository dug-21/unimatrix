"use strict";
// NFR-04 / AC-12 test-count guard (nan-024): per-file test counts must not decrease against the
// committed baseline (test/fixtures/test-counts.json). Old-string tests are updated, never deleted.
//   node test/check-test-count.js                       check the working tree against the baseline
//   node test/check-test-count.js --write-baseline <rev>  regenerate the baseline from git <rev>
// The baseline changes only in a commit that states why. Counts are static `it(`/`test(` call sites.
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const PKG = path.join(__dirname, "..");
const BASELINE = process.env.TEST_COUNT_BASELINE || path.join(__dirname, "fixtures", "test-counts.json");
const DIRS = ["test", "test/hook-client", "opencode-plugin/test"];
const RE = /^[ \t]*(?:it|test)(?:\.(?:skip|only|todo))?\s*\(/gm;

function count(src) {
  return (src.match(RE) || []).length;
}

function main(argv) {
  const wi = argv.indexOf("--write-baseline");
  if (wi >= 0) {
    const rev = argv[wi + 1];
    if (!rev) { console.error("check-test-count: --write-baseline needs a git rev"); return 2; }
    const git = (a) => execFileSync("git", a, { cwd: path.join(PKG, "..", ".."), encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
    const out = { _comment: "per-file it()/test() counts at " + rev + "; update only in a commit that states why", counts: {} };
    for (const d of DIRS) {
      const ls = git(["ls-tree", "--name-only", rev + ":packages/unimatrix/" + d]).split("\n").filter((n) => n.endsWith(".test.js"));
      for (const n of ls) {
        out.counts[d + "/" + n] = count(git(["show", rev + ":packages/unimatrix/" + d + "/" + n]));
      }
    }
    fs.writeFileSync(BASELINE, JSON.stringify(out, null, 2) + "\n");
    console.error("check-test-count: wrote " + Object.keys(out.counts).length + " entries");
    return 0;
  }
  const baseline = JSON.parse(fs.readFileSync(BASELINE, "utf8")).counts;
  const bad = [];
  let total = 0;
  for (const rel of Object.keys(baseline).sort()) {
    let now = 0;
    try { now = count(fs.readFileSync(path.join(PKG, rel), "utf8")); } catch (_e) { /* deleted */ }
    total += now;
    if (now < baseline[rel]) bad.push(rel + ": " + baseline[rel] + " -> " + now);
  }
  if (bad.length) {
    console.error("check-test-count: FAIL — test count decreased (update tests, do not delete them):");
    for (const b of bad) console.error("  " + b);
    return 1;
  }
  console.error("check-test-count: PASS — " + Object.keys(baseline).length + " suites, no decrease (" + total + " tests).");
  return 0;
}

if (require.main === module) process.exit(main(process.argv.slice(2)));
module.exports = { count, main };
