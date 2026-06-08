## ADR-005: C-04 Size Gate — Comment-Stripped ≤ 100,000 B Primary + Raw ≤ 160,000 B Backstop, Dependency-Free State-Machine Stripper

### Context

`test/check-hook-client-size.js` gates raw bytes of `lib/hook-client/**/*.js` at
100,000 (decimal) and the client sits at 99,997 — 3 bytes of headroom. vnc-026
already took a Gate-3b rework on this gate (Unimatrix #4780): the budget punishes
documentation, which this codebase's oracle-citation comment style needs. The
human C-04 decision (2026-06-08) redefines the gate: comment-stripped ≤ 100 KB
primary + raw ≤ 160 KB backstop, trivially auditable stripper, cap changes are
human decisions recorded on the feature issue. This must merge FIRST (SR-02) —
every F4a client addition competes for the stripped budget.

### Decision

1. **Limits**: comment-stripped total ≤ 100,000 bytes (primary), raw total
   ≤ 160,000 bytes (backstop). Decimal interpretation retained from the existing
   gate (stricter of the two readings, continuity with vnc-026). Either limit
   exceeded → exit non-zero with both totals and a per-file table.
2. **Stripper contract** (dependency-free, single function, in the gate script):
   a character-level state machine over each file with states: code,
   single-quote string, double-quote string, template literal (with `${}` nesting
   back to code), line comment, block comment, regex literal (with character-class
   sub-state). Escape sequences honored in all string/regex states. Only line and
   block comments are removed; ALL other bytes (strings, whitespace, newlines)
   pass through verbatim — stripping is removal-only, never rewriting.
3. **Regex-literal discipline**: a `/` in code state opens a regex iff the last
   significant (non-whitespace, non-stripped) character is one of
   `( , = : [ ! & | ? { } ; + - * % < > ^ ~` or the start of file/line keyword
   boundary (`return`, `typeof`, `case`, etc. — the standard prev-token
   heuristic). This prevents the two classic miscounts: `//` inside a regex
   swallowing the rest of the line, and `/*` inside a regex swallowing code.
4. **Auditable by test, not by trust**: the gate script ships a small embedded
   self-test corpus (string containing `//`, template literal with `${}` and
   backticks, regex containing `//` and `/*`, division vs regex ambiguity) run on
   every gate invocation before measuring; self-test failure fails the gate. The
   stripper itself lives in `test/` and does NOT count against the client budget.
5. **Header contract**: the script header documents the C-04 decision, both
   limits, and the rule that any cap change is a human decision recorded on the
   feature's GH issue — never an agent adjustment.
6. **Merge order**: this rewrite is the first merged change of vnc-027 (SR-02);
   no client file may grow before it lands.

### Consequences

Easier: ~60 KB of effective headroom for documented code (raw backstop), with the
stripped limit still bounding shipped logic; comment-quality regressions driven by
byte pressure (the #4780 failure mode) stop; transport-uds.js and config/index
diffs fit comfortably.

Harder: the stripper is ~80 lines of state machine that must itself be correct —
mitigated by the embedded self-test and the raw backstop (a stripper bug cannot
admit unbounded growth past 160,000); minified-style code could game the stripped
limit (out of scope — this is a budget gate for a hand-written client, and review
catches style regressions); two numbers to communicate instead of one.

Cross-references: SR-02, lesson #4780 (vnc-026 gate rework), C-04 human decision
(#680, 2026-06-08).
