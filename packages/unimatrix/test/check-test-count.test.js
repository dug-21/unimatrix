"use strict";
const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { count } = require("./check-test-count.js");

const SCRIPT = path.join(__dirname, "check-test-count.js");
function runWith(counts) {
  const d = fs.mkdtempSync(path.join(os.tmpdir(), "tcount-"));
  const b = path.join(d, "b.json");
  fs.writeFileSync(b, JSON.stringify({ counts }));
  const r = spawnSync(process.execPath, [SCRIPT], { env: Object.assign({}, process.env, { TEST_COUNT_BASELINE: b }), encoding: "utf8" });
  fs.rmSync(d, { recursive: true, force: true });
  return r;
}

describe("check-test-count (NFR-04)", () => {
  it("counts it/test/skip call sites", () => {
    assert.equal(count("it('a',()=>{});\n  test.skip('b');\n// it('c')\nconst x = 1;"), 2);
  });
  it("passes on equal counts and fails on a deliberately reduced suite", () => {
    const own = count(fs.readFileSync(path.join(__dirname, "check-test-count.test.js"), "utf8"));
    assert.equal(runWith({ "test/check-test-count.test.js": own }).status, 0);
    const bad = runWith({ "test/check-test-count.test.js": own + 1 });
    assert.equal(bad.status, 1);
    assert.match(bad.stderr, /test count decreased/);
  });
  it("fails when a baselined suite is deleted", () => {
    assert.equal(runWith({ "test/no-such-suite.test.js": 3 }).status, 1);
  });
});
