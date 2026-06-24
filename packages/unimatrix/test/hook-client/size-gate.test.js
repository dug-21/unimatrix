"use strict";

// vnc-027 C-04 size-gate tests (ADR-005, FR-1..FR-4, AC-09, R-02/R-03).
// Covers the comment stripper (string-literal safety, regex-vs-division,
// escapes, removal-only), the fail-closed self-test, and both byte limits
// triggering independently on synthetic fixtures.

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const gate = require("../check-hook-client-size");
const {
  stripComments,
  runSelfTest,
  runGate,
  measureTree,
  PRIMARY_LIMIT,
  BACKSTOP_LIMIT,
  ROOT,
} = gate;

// ── helpers ─────────────────────────────────────────────────────────

/** Create a temp dir with the given { name: content } files; returns the dir. */
function makeTree(files) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "size-gate-"));
  for (const [name, content] of Object.entries(files)) {
    fs.writeFileSync(path.join(dir, name), content, "utf8");
  }
  return dir;
}

/** Run the gate against a tree, capturing stdout/stderr lines. */
function captureGate(root, extra) {
  const log = [];
  const err = [];
  const opts = { log: (m) => log.push(m), err: (m) => err.push(m) };
  if (extra) Object.assign(opts, extra);
  const code = runGate(root, opts);
  return { code, log, err };
}

/** True iff sub is an in-order subsequence of sup (char level). */
function isSubsequence(sub, sup) {
  let j = 0;
  for (let k = 0; k < sup.length && j < sub.length; k++) {
    if (sup[k] === sub[j]) j++;
  }
  return j === sub.length;
}

// ── self-test ordering / fail-closed (R-03 s1) ───────────────────────

describe("size-gate self-test", function () {
  it("test_selftest_passes_on_real_corpus", function () {
    assert.deepStrictEqual(runSelfTest(), { ok: true });
  });

  it("test_selftest_runs_before_measure_and_fails_gate_on_strip_bug", function () {
    const dir = makeTree({ "a.js": "const x = 1;\n" });
    let measured = false;
    const { code, err } = captureGate(dir, {
      runSelfTest: () => ({ ok: false, name: "forced bug" }),
      measureTree: () => {
        measured = true;
        return { rows: [], rawTotal: 0, strippedTotal: 0 };
      },
    });
    assert.strictEqual(code, 1);
    assert.strictEqual(measured, false, "must not measure once self-test fails");
    assert.ok(err.some((m) => m.includes("self-test FAILED")), "self-test failure message");
  });
});

// ── string-literal safety (FR-2, R-03) ───────────────────────────────

describe("stripComments string-literal safety", function () {
  it("test_strip_preserves_double_quote_string_with_slashslash", function () {
    const out = stripComments('const s = "// not a comment"; x();');
    assert.ok(out.includes('"// not a comment"'));
    assert.ok(out.includes("x()"), "nothing after the string was dropped");
  });

  it("test_strip_preserves_template_literal_with_dollar_brace_and_backtick", function () {
    const out = stripComments("const t = `a ${x} /* still code */ b`; // gone");
    assert.ok(out.includes("`a ${x} /* still code */ b`"), "template body verbatim");
    assert.ok(!out.includes("gone"), "real comment after the closing backtick stripped");
  });

  it("test_strip_preserves_regex_with_slashslash_and_slashstar", function () {
    const out = stripComments("const a = /foo\\/\\/bar/; const b = /a\\/\\*b/;");
    assert.ok(out.includes("/foo\\/\\/bar/"));
    assert.ok(out.includes("/a\\/\\*b/"));
  });

  it("test_strip_regex_char_class_slash", function () {
    const out = stripComments("const a = /[/]/g; const b = /[\\/]/g;");
    assert.ok(out.includes("/[/]/g"));
    assert.ok(out.includes("/[\\/]/g"));
  });
});

// ── regex-vs-division heuristic (ADR-005 3, R-03) ────────────────────

describe("stripComments regex-vs-division", function () {
  it("test_regex_open_after_paren_eq_return", function () {
    assert.ok(stripComments("foo(/x/); // c").includes("foo(/x/)"));
    assert.ok(stripComments("const r = /x/; // c").includes("/x/"));
    assert.ok(stripComments("return /x/.test(s); // c").includes("/x/.test(s)"));
    // the trailing real comment is stripped in each case
    assert.ok(!stripComments("return /x/.test(s); // c").includes("// c"));
  });

  it("test_division_after_identifier", function () {
    // Division: the // trap must not fire and swallow the rest.
    const out = stripComments("const r = a / b / c; const keep = 1;");
    assert.ok(out.includes("a / b / c"));
    assert.ok(out.includes("keep = 1"), "division did not start a comment/regex");
  });

  it("test_division_after_paren_and_bracket", function () {
    assert.ok(stripComments("x = fn() / 2; y();").includes("fn() / 2"));
    assert.ok(stripComments("x = arr[0] / 2; y();").includes("arr[0] / 2"));
  });
});

// ── escapes (FR-2) ───────────────────────────────────────────────────

describe("stripComments escapes", function () {
  it("test_strip_escapes_in_string_template_regex", function () {
    const out = stripComments('const a = "\\""; const b = `\\``; const c = /\\//g; ok();');
    assert.ok(out.includes('"\\""'), "escaped quote does not close the string");
    assert.ok(out.includes("`\\`"), "escaped backtick does not close the template");
    assert.ok(out.includes("/\\//g"), "escaped slash does not close the regex");
    assert.ok(out.includes("ok()"), "state was correctly restored to code");
  });
});

// ── removal-only / byte-subsequence on the real tree (R-03 s2) ───────

describe("stripComments removal-only", function () {
  it("test_strip_is_removal_only_byte_subsequence", function () {
    for (const file of gate.collectJsFiles(ROOT)) {
      const raw = fs.readFileSync(file, "utf8");
      const stripped = stripComments(raw);
      assert.ok(isSubsequence(stripped, raw), "stripped is a subsequence of " + file);
      const rawBytes = Buffer.byteLength(raw, "utf8");
      const strippedBytes = Buffer.byteLength(stripped, "utf8");
      assert.ok(strippedBytes > 0, "stripped > 0 for " + file);
      assert.ok(strippedBytes < rawBytes, "stripped < raw (file has comments) for " + file);
    }
  });
});

// ── gate behavior + independent limits (FR-1, R-02 s2) ───────────────

describe("size-gate behavior", function () {
  it("test_gate_passes_within_both_limits", function () {
    const dir = makeTree({ "a.js": "const x = 1; // ok\n", "b.js": "const y = 2;\n" });
    const { code, log, err } = captureGate(dir);
    assert.strictEqual(code, 0);
    assert.deepStrictEqual(err, []);
    assert.ok(log.some((m) => m.startsWith("OK:")));
  });

  it("test_gate_fails_on_stripped_over_100000", function () {
    // Pure code (a string literal) over the primary limit; raw stays < backstop.
    const big = 'const s = "' + "a".repeat(PRIMARY_LIMIT + 50) + '";\n';
    const dir = makeTree({ "big.js": big });
    const { code, log, err } = captureGate(dir);
    assert.strictEqual(code, 1);
    assert.ok(err.some((m) => m.includes("(OVER)") && m.includes("stripped=")));
    // both totals printed, per-file table present
    assert.ok(log.some((m) => m.includes("totals: stripped=")));
    assert.ok(log.some((m) => m.includes("big.js")));
  });

  it("test_gate_fails_on_raw_over_160000", function () {
    // Comment-heavy: raw > backstop but stripped collapses well under primary.
    const heavy = "/*" + "a".repeat(BACKSTOP_LIMIT + 50) + "*/\nconst x = 1;\n";
    const dir = makeTree({ "heavy.js": heavy });
    const { code, err } = captureGate(dir);
    assert.strictEqual(code, 1);
    const fail = err.find((m) => m.startsWith("FAIL:"));
    assert.ok(fail, "FAIL line present");
    assert.ok(fail.includes("raw=") && fail.includes("(OVER)"), "raw backstop tripped");
    // primary not the cause: stripped stayed under its limit
    assert.ok(!/stripped=\d+\/\d+ \(OVER\)/.test(fail), "primary did not trip");
  });

  it("test_both_limits_independently_triggerable", function () {
    // disjoint fixtures: one trips primary only, the other backstop only.
    const primaryOnly = makeTree({
      "p.js": 'const s = "' + "a".repeat(PRIMARY_LIMIT + 50) + '";\n',
    });
    const backstopOnly = makeTree({
      "b.js": "/*" + "a".repeat(BACKSTOP_LIMIT + 50) + "*/\nconst x = 1;\n",
    });

    const p = measureTree(primaryOnly);
    assert.ok(p.strippedTotal > PRIMARY_LIMIT, "primary fixture over stripped limit");
    assert.ok(p.rawTotal <= BACKSTOP_LIMIT, "primary fixture under raw backstop");

    const b = measureTree(backstopOnly);
    assert.ok(b.rawTotal > BACKSTOP_LIMIT, "backstop fixture over raw limit");
    assert.ok(b.strippedTotal <= PRIMARY_LIMIT, "backstop fixture under stripped primary");

    assert.strictEqual(captureGate(primaryOnly).code, 1);
    assert.strictEqual(captureGate(backstopOnly).code, 1);
  });

  it("test_backstop_fires_under_forced_strip_everything_bug", function () {
    // A stripper that drops all bytes makes stripped=0, but the raw backstop
    // still caps the tree (R-03 mitigation: a miscount cannot admit growth).
    const dir = makeTree({ "x.js": "x".repeat(BACKSTOP_LIMIT + 100) });
    const m = measureTree(dir, () => "");
    assert.strictEqual(m.strippedTotal, 0);
    assert.ok(m.rawTotal > BACKSTOP_LIMIT, "raw measured independently of stripper");
  });

  it("test_gate_exits_1_when_dir_absent", function () {
    const { code, err } = captureGate(path.join(os.tmpdir(), "size-gate-nope-" + Date.now()));
    assert.strictEqual(code, 1);
    assert.ok(err.some((m) => m.includes("directory not found")));
  });

  it("test_empty_tree_passes_no_divide_by_zero", function () {
    const dir = makeTree({});
    const { code } = captureGate(dir);
    assert.strictEqual(code, 0);
  });

  it("test_boundary_exactly_at_primary_limit_passes", function () {
    // stripped exactly == PRIMARY_LIMIT must pass (<=). All-ASCII so the
    // string-literal body is uncommented code measured byte-for-byte.
    const prefix = 'const s="';
    const suffix = '";\n';
    const body = "a".repeat(PRIMARY_LIMIT - prefix.length - suffix.length);
    const content = prefix + body + suffix;
    const dir = makeTree({ "x.js": content });
    const m = measureTree(dir);
    assert.strictEqual(m.strippedTotal, PRIMARY_LIMIT, "constructed exactly at the limit");
    assert.strictEqual(captureGate(dir).code, 0, "boundary passes");
  });
});

// ── header contract (FR-3) ───────────────────────────────────────────

describe("size-gate header", function () {
  it("test_header_documents_human_decision_rule", function () {
    const src = fs.readFileSync(path.join(__dirname, "..", "check-hook-client-size.js"), "utf8");
    assert.ok(src.includes("100,000"), "primary limit documented");
    assert.ok(src.includes("180,000"), "backstop limit documented");
    assert.ok(/HUMAN decision/.test(src), "cap-change is a human decision");
    assert.ok(/GitHub issue/.test(src), "recorded on the feature issue");
  });

  it("test_limits_are_decimal", function () {
    // PRIMARY raised 100000→101000 (TEMP) for the #839 critical availability fix
    // (transport/connect timeout + silent-eviction self-heal). Human-approved,
    // recorded on #839; reclaim to 100000 tracked in #840. Keep this
    // meta-assertion in lockstep with check-hook-client-size.js:34.
    assert.strictEqual(PRIMARY_LIMIT, 101000);
    // BACKSTOP raised 160000→180000 for the vnc-039 stdio→HTTPS MCP bridge
    // (~24KB new pure-JS); human-approved, recorded on #775. Keep this
    // meta-assertion in lockstep with check-hook-client-size.js:35.
    assert.strictEqual(BACKSTOP_LIMIT, 180000);
  });
});
