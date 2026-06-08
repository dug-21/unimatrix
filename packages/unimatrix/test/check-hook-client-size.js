"use strict";

// ============================================================================
// C-04 hook-client size gate (vnc-027, ADR-005). Human decision 2026-06-08.
// ----------------------------------------------------------------------------
// Two independent limits over lib/hook-client/**/*.js (decimal bytes):
//
//   PRIMARY  : comment-stripped total <= 100,000 bytes
//   BACKSTOP : raw (on-disk) total    <= 160,000 bytes
//
// The PRIMARY limit measures shipped logic only — line and block comments are
// removed before counting so the oracle-citation comment style this client
// needs (hook.rs:NNN anchors, ADR refs) no longer competes with code for the
// budget (the #4780 rework driver). The BACKSTOP caps raw growth so a stripper
// miscount can never admit unbounded files.
//
// Either limit exceeded -> non-zero exit (fails CI). Both totals are always
// printed so each limit is independently auditable.
//
// CAP-CHANGE RULE: any change to either limit is a HUMAN decision recorded on
// the feature's GitHub issue. It is NEVER an agent adjustment — raising a cap
// to make a failing gate pass is a vacuous pass and is forbidden.
//
// The comment stripper below is a dependency-free, string-literal-safe
// character state machine with an embedded self-test corpus that runs on EVERY
// invocation before measuring; a self-test failure fails the gate closed (the
// gate never measures with a broken stripper). This file lives in test/ and
// does NOT count against the client byte budget.
// ============================================================================

const fs = require("fs");
const path = require("path");

const PRIMARY_LIMIT = 100000; // comment-stripped bytes (decimal)
const BACKSTOP_LIMIT = 160000; // raw bytes (decimal)
const ROOT = path.resolve(__dirname, "..", "lib", "hook-client");

// Lexer states.
const CODE = 0;
const SQ = 1; // single-quote string
const DQ = 2; // double-quote string
const TPL = 3; // template literal
const LINE = 4; // line comment
const BLOCK = 5; // block comment
const REGEX = 6; // regex literal body
const RCLASS = 7; // regex character class [...]

const IDENT = /[A-Za-z0-9_$]/;
const WS = /\s/;

// Prev-significant-char punctuation after which a `/` opens a regex (ADR-005 3).
const REGEX_PREV_PUNCT = new Set([
  "(", ",", "=", ":", "[", "!", "&", "|", "?", "{", "}",
  ";", "+", "-", "*", "%", "<", ">", "^", "~",
]);
// Keywords after which a `/` opens a regex (prev-token heuristic).
const REGEX_PREV_KEYWORDS = new Set([
  "return", "typeof", "case", "in", "of", "instanceof",
  "new", "delete", "void", "do", "else", "yield", "throw",
]);

/**
 * Decide whether a `/` in CODE state opens a regex literal (true) or is a
 * division operator (false), from the last significant code char and the
 * identifier word ending at it.
 */
function regexCanOpen(prevSig, prevWord) {
  if (prevSig === null) return true; // start of file / line
  if (REGEX_PREV_PUNCT.has(prevSig)) return true;
  if (IDENT.test(prevSig)) return REGEX_PREV_KEYWORDS.has(prevWord);
  return false; // ) ] . " ' ` / number -> division
}

/**
 * Remove line and block comments from JS source, leaving all other bytes
 * (strings, templates, regex, whitespace, newlines) verbatim. Removal-only:
 * the output is always a byte-subsequence of the input.
 */
function stripComments(src) {
  const n = src.length;
  const out = [];
  let state = CODE;
  let i = 0;
  let prevSig = null; // last significant code char (regex disambiguation)
  let prevWord = ""; // identifier word ending exactly at prevSig
  let lastIdent = false; // previous emitted code char was an identifier char
  const interp = []; // brace-depth stack, one entry per open ${ } interpolation

  function emitCode(ch) {
    out.push(ch);
    if (WS.test(ch)) {
      lastIdent = false; // whitespace: prevSig/prevWord unchanged
      return;
    }
    if (IDENT.test(ch)) {
      prevWord = lastIdent ? prevWord + ch : ch;
      lastIdent = true;
    } else {
      prevWord = "";
      lastIdent = false;
    }
    prevSig = ch;
  }

  // Enter a string/regex value: emit the opening delimiter, reset code context.
  function openValue(ch, nextState) {
    out.push(ch);
    prevSig = ch;
    prevWord = "";
    lastIdent = false;
    state = nextState;
  }
  // Return to CODE from a value, recording the closing delimiter as prevSig.
  function closeValue(ch) {
    prevSig = ch;
    prevWord = "";
    lastIdent = false;
    state = CODE;
  }

  while (i < n) {
    const c = src[i];
    const next = i + 1 < n ? src[i + 1] : undefined;
    switch (state) {
      case CODE:
        if (c === "/" && next === "/") { state = LINE; i += 2; continue; }
        if (c === "/" && next === "*") { state = BLOCK; i += 2; continue; }
        if (c === "/" && regexCanOpen(prevSig, prevWord)) {
          openValue(c, REGEX); i++; continue;
        }
        if (c === '"') { openValue(c, DQ); i++; continue; }
        if (c === "'") { openValue(c, SQ); i++; continue; }
        if (c === "`") { openValue(c, TPL); i++; continue; }
        if (c === "{" && interp.length > 0) {
          interp[interp.length - 1]++; emitCode(c); i++; continue;
        }
        if (c === "}" && interp.length > 0) {
          if (interp[interp.length - 1] === 0) {
            interp.pop();
            out.push(c);
            prevSig = "}";
            prevWord = "";
            lastIdent = false;
            state = TPL; // a `}` closing ${...} returns to the template
            i++;
            continue;
          }
          interp[interp.length - 1]--; emitCode(c); i++; continue;
        }
        emitCode(c); i++; continue;

      case LINE:
        if (c === "\n") { out.push(c); lastIdent = false; state = CODE; i++; continue; }
        i++; continue; // drop comment byte

      case BLOCK:
        if (c === "*" && next === "/") { lastIdent = false; state = CODE; i += 2; continue; }
        if (c === "\n") { out.push(c); i++; continue; } // keep newlines for line-count sanity
        i++; continue; // drop comment byte

      case SQ:
      case DQ:
        out.push(c);
        if (c === "\\") {
          if (next !== undefined) { out.push(next); i += 2; } else { i++; }
          continue;
        }
        if ((state === SQ && c === "'") || (state === DQ && c === '"')) closeValue(c);
        i++;
        continue;

      case TPL:
        out.push(c);
        if (c === "\\") {
          if (next !== undefined) { out.push(next); i += 2; } else { i++; }
          continue;
        }
        if (c === "`") { closeValue(c); i++; continue; }
        if (c === "$" && next === "{") {
          out.push(next);
          interp.push(0);
          prevSig = "{";
          prevWord = "";
          lastIdent = false;
          state = CODE; // interpolation body is code
          i += 2;
          continue;
        }
        i++;
        continue;

      case REGEX:
        out.push(c);
        if (c === "\\") {
          if (next !== undefined) { out.push(next); i += 2; } else { i++; }
          continue;
        }
        if (c === "[") { state = RCLASS; i++; continue; }
        if (c === "/") { closeValue("/"); i++; continue; } // flags follow in CODE
        i++;
        continue;

      case RCLASS:
        out.push(c);
        if (c === "\\") {
          if (next !== undefined) { out.push(next); i += 2; } else { i++; }
          continue;
        }
        if (c === "]") { state = REGEX; i++; continue; }
        i++;
        continue;

      default:
        emitCode(c); i++; continue;
    }
  }
  return out.join("");
}

// Embedded self-test corpus (ADR-005 4). Each case asserts that // or /*
// sequences inside a non-comment context survive and real comments are removed.
const SELF_TEST_CORPUS = [
  {
    name: "double-quote string containing //",
    input: 'const s = "// not a comment";',
    assert: (o) => o.includes('"// not a comment"'),
  },
  {
    name: "template literal with ${} and /* */ text",
    input: "const t = `a ${b}/* not */ c`;",
    assert: (o) => o.includes("/* not */") && o.includes("${b}") && o.includes(" c`"),
  },
  {
    name: "template ${} returns to code; comment after backtick stripped",
    input: "const t = `${x}` /* gone */ + 1;",
    assert: (o) => o.includes("`${x}`") && !o.includes("gone") && o.includes("+ 1"),
  },
  {
    name: "regex with char class, // and /* in body",
    input: "const re = /[/]\\/*/g;",
    assert: (o) => o.includes("/[/]\\/*/g"),
  },
  {
    name: "division, not regex",
    input: "const x = a / b / c;",
    assert: (o) => o.includes("a / b / c"),
  },
  {
    name: "regex after return; trailing line comment stripped",
    input: "return /ab/.test(x); // real",
    assert: (o) => o.includes("/ab/.test(x)") && !o.includes("real"),
  },
  {
    name: "block and line comments stripped, code survives",
    input: "/* block */ code() // line",
    assert: (o) => o.includes("code()") && !o.includes("block") && !o.includes("line"),
  },
  {
    name: "escaped delimiters do not close their state",
    input: 'const a = "\\""; const b = `\\``; const r = /\\//g;',
    assert: (o) => o.includes('"\\""') && o.includes("`\\`") && o.includes("/\\//g"),
  },
  {
    name: "regex char class with escaped slash",
    input: "const re = /[\\/]/;",
    assert: (o) => o.includes("/[\\/]/"),
  },
];

/**
 * Run the embedded self-test corpus. Returns { ok: true } on success, or
 * { ok: false, name, stripped?, error? } on the first failing case.
 */
function runSelfTest() {
  for (const c of SELF_TEST_CORPUS) {
    let stripped;
    try {
      stripped = stripComments(c.input);
    } catch (e) {
      return { ok: false, name: c.name, error: String(e && e.message ? e.message : e) };
    }
    if (!c.assert(stripped)) return { ok: false, name: c.name, stripped };
  }
  return { ok: true };
}

/** Collect every *.js file under dir, recursively. */
function collectJsFiles(dir) {
  const files = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) files.push(...collectJsFiles(full));
    else if (entry.isFile() && entry.name.endsWith(".js")) files.push(full);
  }
  return files;
}

/**
 * Measure raw and comment-stripped byte totals over the *.js tree at root.
 * stripFn is injectable for tests (default: the real stripComments).
 */
function measureTree(root, stripFn) {
  const strip = stripFn || stripComments;
  const rows = [];
  let rawTotal = 0;
  let strippedTotal = 0;
  for (const file of collectJsFiles(root)) {
    const raw = fs.readFileSync(file);
    const rawBytes = raw.length;
    const strippedBytes = Buffer.byteLength(strip(raw.toString("utf8")), "utf8");
    rawTotal += rawBytes;
    strippedTotal += strippedBytes;
    rows.push({ file: path.relative(root, file), raw: rawBytes, stripped: strippedBytes });
  }
  rows.sort((a, b) => b.stripped - a.stripped);
  return { rows, rawTotal, strippedTotal };
}

/**
 * Run the gate against root. opts allows dependency injection for tests:
 * { log, err, runSelfTest, measureTree }. Returns the process exit code.
 */
function runGate(root, opts) {
  opts = opts || {};
  const out = opts.log || console.log;
  const err = opts.err || console.error;
  const selfTest = opts.runSelfTest || runSelfTest;
  const measure = opts.measureTree || measureTree;

  if (!fs.existsSync(root)) {
    err("check-hook-client-size: directory not found: " + root);
    return 1;
  }

  const self = selfTest();
  if (!self.ok) {
    err(
      "check-hook-client-size: stripper self-test FAILED (gate fails closed): " +
        self.name +
        (self.error ? " -- " + self.error : "")
    );
    return 1;
  }

  let measured;
  try {
    measured = measure(root);
  } catch (e) {
    err("check-hook-client-size: failed to measure: " + (e && e.message ? e.message : e));
    return 1;
  }
  const { rows, rawTotal, strippedTotal } = measured;

  out("  stripped       raw  file");
  for (const r of rows) {
    out(String(r.stripped).padStart(10) + String(r.raw).padStart(10) + "  " + r.file);
  }
  out("-".repeat(48));
  out(
    "totals: stripped=" + strippedTotal + "/" + PRIMARY_LIMIT +
      "  raw=" + rawTotal + "/" + BACKSTOP_LIMIT
  );

  const failedPrimary = strippedTotal > PRIMARY_LIMIT;
  const failedBackstop = rawTotal > BACKSTOP_LIMIT;
  if (failedPrimary || failedBackstop) {
    err(
      "FAIL: stripped=" + strippedTotal + "/" + PRIMARY_LIMIT + (failedPrimary ? " (OVER)" : "") +
        ", raw=" + rawTotal + "/" + BACKSTOP_LIMIT + (failedBackstop ? " (OVER)" : "")
    );
    return 1;
  }
  out(
    "OK: within both budgets (stripped " + strippedTotal + " <= " + PRIMARY_LIMIT +
      ", raw " + rawTotal + " <= " + BACKSTOP_LIMIT + ")"
  );
  return 0;
}

function main() {
  return runGate(ROOT);
}

if (require.main === module) {
  process.exit(main());
}

module.exports = {
  PRIMARY_LIMIT,
  BACKSTOP_LIMIT,
  ROOT,
  regexCanOpen,
  stripComments,
  SELF_TEST_CORPUS,
  runSelfTest,
  collectJsFiles,
  measureTree,
  runGate,
};
