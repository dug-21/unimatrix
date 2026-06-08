# Component: size-gate (`test/check-hook-client-size.js` REWRITE)

ADR-005. FR-1..FR-4, AC-09, NFR-2. Risks R-02 (Critical), R-03.
**Merges as the literal FIRST vnc-027 commit** (SR-02; client at 99,997/100,000 raw).

## Purpose

Redefine the hook-client size gate from a single raw-byte limit to a
comment-stripped primary limit plus a raw backstop, so documented oracle-citation
comments stop competing with shipped logic for budget (the #4780 rework driver).
The stripper is a dependency-free, string-literal-safe character state machine with
an embedded self-test that runs on every invocation. The stripper lives in `test/`
and does NOT count against the client budget.

## Constants

```
PRIMARY_LIMIT = 100000      // comment-stripped bytes (decimal)
BACKSTOP_LIMIT = 160000     // raw bytes (decimal)
ROOT = resolve(__dirname, "..", "lib", "hook-client")
```

## File header contract (FR-3, ADR-005 §5)

Header comment block MUST document: the C-04 decision (2026-06-08), both limits
and their meaning, and the rule that **any cap change is a human decision recorded
on the feature's GH issue, never an agent adjustment**.

## Lexer state machine — `stripComments(source) -> string` (FR-2, ADR-005 §2-3)

Character-level, single pass. Removal-only: output is a byte-subsequence of input;
only line and block comment bytes are dropped. Strings, templates, regex, and all
whitespace pass through verbatim.

States: `CODE`, `SQ` (single-quote), `DQ` (double-quote), `TPL` (template literal),
`LINE_COMMENT`, `BLOCK_COMMENT`, `REGEX`, `REGEX_CLASS` (inside `[...]`).

```
FUNCTION stripComments(src):
  out = []                          // collected output chars (or byte length accumulator)
  state = CODE
  tplDepth = 0                      // ${ } nesting; >0 means a CODE region inside a template
  prevSig = null                    // last significant code char, for regex disambiguation
  i = 0
  WHILE i < src.length:
    c = src[i]; next = src[i+1]
    SWITCH state:
      CASE CODE:
        IF c == '/' AND next == '/':  state = LINE_COMMENT; i += 2; continue
        IF c == '/' AND next == '*':  state = BLOCK_COMMENT; i += 2; continue
        IF c == '/' AND regexCanOpen(prevSig):  state = REGEX; emit(c); prevSig='/'; i++; continue
        IF c == '"':  state = DQ; emit(c); i++; continue
        IF c == '\'': state = SQ; emit(c); i++; continue
        IF c == '`':  state = TPL; emit(c); i++; continue
        IF c == '}' AND tplDepth > 0 AND inTemplateInterp(): // closing ${...}
            // handled via a stack; see note below
        emit(c)
        IF NOT isWhitespace(c): prevSig = c
        i++
      CASE LINE_COMMENT:
        IF c == '\n':  emit(c); state = CODE; i++; continue   // newline preserved (not a comment byte)
        i++                          // drop comment byte (no emit)
      CASE BLOCK_COMMENT:
        IF c == '*' AND next == '/':  state = CODE; i += 2; continue
        IF c == '\n': emit(c); i++; continue  // preserve newlines for line-count sanity (optional)
        i++                          // drop comment byte
      CASE SQ / DQ:
        emit(c)
        IF c == '\\': emit(next); i += 2; continue            // escape: consume next verbatim
        IF (state==SQ AND c=='\'') OR (state==DQ AND c=='"'): state = CODE; prevSig = c
        i++
      CASE TPL:
        emit(c)
        IF c == '\\': emit(next); i += 2; continue
        IF c == '`': state = CODE; prevSig = c; i++; continue
        IF c == '$' AND next == '{': emit(next); pushTplContext(); state = CODE; tplDepth++; i += 2; continue
        i++
      CASE REGEX:
        emit(c)
        IF c == '\\': emit(next); i += 2; continue            // escaped metachar
        IF c == '[': state = REGEX_CLASS; i++; continue
        IF c == '/': state = CODE; prevSig = '/'; i++; continue  // end of regex (flags follow in CODE)
        i++
      CASE REGEX_CLASS:
        emit(c)
        IF c == '\\': emit(next); i += 2; continue
        IF c == ']': state = REGEX; i++; continue
        i++
  RETURN join(out)
```

Notes for the implementer:
- **Template `${}` nesting** (ADR-005 §2): track a small stack so a `}` in CODE
  state that closes an interpolation returns to TPL. A depth counter plus a stack
  of "are we in a template interpolation" booleans is sufficient; nested templates
  inside `${}` push again. Keep it readable — the embedded self-test is the proof.
- For a pure byte-count gate, `emit` may accumulate a counter instead of building a
  string; but returning the stripped string makes the self-test diffable. Choose
  the readable form (ADR-005 "auditable by reading").

## Regex-open heuristic — `regexCanOpen(prevSig)` (ADR-005 §3, R-03)

```
FUNCTION regexCanOpen(prevSig):
  IF prevSig is null: RETURN true                  // start of file
  IF prevSig in { '(', ',', '=', ':', '[', '!', '&', '|', '?', '{', '}',
                  ';', '+', '-', '*', '%', '<', '>', '^', '~' }: RETURN true
  IF lastTokenIsKeyword in { return, typeof, case, in, of, instanceof,
                             new, delete, void, do, else, yield, throw }: RETURN true
  RETURN false                                     // identifier/number/) /] before / → division
```

The token-boundary keyword check requires tracking the trailing identifier word in
CODE state (accumulate `[A-Za-z_$][A-Za-z0-9_$]*` runs; compare on the char that
ends the run). Covers at least `(`, `=`, `return`, and identifier-prev (division)
per R-03 coverage requirement.

## Embedded self-test — `runSelfTest() -> boolean` (FR-2, ADR-005 §4, R-03 s1)

Runs BEFORE measuring on every invocation. Each case: `{ input, expectComment }`
where the assertion is that the `//` or `/*` sequence inside a non-comment context
is NOT stripped, and real comments ARE stripped. Corpus MUST include:
- `const s = "// not a comment";`  → string content survives.
- `` const t = `a ${b}/* not */ c`; `` → template + interpolation survive.
- `const re = /[/]\/*/g;` → regex with `//` and `/*` survives (char class + body).
- `const x = a / b / c;` → division, not regex; the `/ c` part is not a comment.
- `return /ab/.test(x); // real` → regex after `return`; trailing `// real` stripped.
- `/* block */ code // line` → both comments stripped, `code` survives.
- escape cases: `"\""`, `` `\`` ``, `/\//` — escaped delimiters do not close state.

```
FUNCTION runSelfTest():
  FOR each case in SELF_TEST_CORPUS:
    stripped = stripComments(case.input)
    IF NOT case.assert(stripped): RETURN false      // e.g. stripped includes the survivor token
  RETURN true
```

## Main flow

```
FUNCTION main():
  IF NOT existsSync(ROOT): error("directory not found"); exit(1)
  IF NOT runSelfTest():
    error("stripper self-test FAILED — gate fails closed (never measures with a broken stripper)")
    exit(1)                                          // R-03 / fail-closed
  rows = []   ; rawTotal = 0 ; strippedTotal = 0
  FOR each *.js file under ROOT (recursive):
    raw = readFileSync(file)
    rawBytes = byteLength(raw)
    strippedBytes = byteLength(stripComments(raw.toString("utf8")))
    rawTotal += rawBytes ; strippedTotal += strippedBytes
    rows.push({ file: relpath, raw: rawBytes, stripped: strippedBytes })
  sort rows by stripped desc
  print per-file table (file, stripped, raw)
  print totals + both limits
  failedPrimary  = strippedTotal > PRIMARY_LIMIT
  failedBackstop = rawTotal      > BACKSTOP_LIMIT
  IF failedPrimary OR failedBackstop:
    error("FAIL: stripped=<strippedTotal>/<PRIMARY_LIMIT>, raw=<rawTotal>/<BACKSTOP_LIMIT>")
    exit(1)
  print "OK: within both budgets"
```

Either breach fails non-zero (FR-1). Both totals always reported (R-02 s2 needs
each limit independently triggerable).

## Error handling

- Self-test failure → exit 1 (fail closed). Never measure with a broken stripper.
- Unreadable file / missing dir → exit 1 with a clear message.
- The gate throws to no host (it is a CI script): a non-zero exit is the signal.

## Key test scenarios (hints for tester)

1. Self-test corpus green on every invocation; a deliberately broken stripper makes
   the gate exit 1 (fail-closed) — R-03 s1.
2. Synthetic over-limit fixture: stripped > 100,000 fails; separately, raw > 160,000
   with stripped ≤ 100,000 fails (backstop independently triggerable) — R-02 s2.
3. String-literal cases (`"// not a comment"`, template `${}`+backticks, regex with
   `//` and `/*`, division-vs-regex) measured correctly — R-02 s1 / R-03.
4. Differential: stripped size of the real client tree is 0 < stripped < raw, and
   stripped output is a byte-subsequence of input (removal-only) — R-03 s2.
5. Escape sequences `\"`, `` \` ``, `\/`, `[/]` do not prematurely close their state
   — R-03 s3.
6. Process/Gate check: `git log` shows this rewrite as the first vnc-027 commit
   touching the client tree (auditable merge order) — R-02 s3.
